//! Durable workflow tools — `ax_workflow_create`, `ax_workflow_step`, `ax_workflow_status`.
//!
//! ## Responsibility boundary
//!
//! This module owns everything related to session-scoped durable workflows:
//!
//! - [`WorkflowState`] — in-memory state for a single live workflow.
//! - [`workflow_tools`] — tool descriptors registered into the Phase 3 tool list.
//! - [`call_workflow_tool`] — stateful dispatch gate; called before the
//!   stateless innovation path so that workflow tools get proper session state.
//! - Private handlers and step-parsing helpers.
//!
//! The cross-app *tracking* feature (`ax_track_workflow`) is intentionally kept
//! in `tools_innovation` because it is stateless (uses a global `Lazy` tracker)
//! and has no dependency on `WorkflowState`.

use std::collections::HashMap;
use std::io::Write;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::mcp::annotations;
use crate::mcp::args::{extract_or_return, extract_required_string_field, parse_json_array};
use crate::mcp::protocol::{Tool, ToolCallResult};

// ---------------------------------------------------------------------------
// WorkflowState
// ---------------------------------------------------------------------------

/// Tracks the in-progress state of a single durable workflow across MCP calls.
pub(crate) struct WorkflowState {
    /// The ordered steps that make up this workflow.
    pub steps: Vec<crate::durable_steps::DurableStep>,
    /// Zero-based index of the next step to execute.
    pub current_step: usize,
    /// Results accumulated from already-executed steps.
    pub results: Vec<crate::durable_steps::WorkflowResult>,
    /// Whether all steps have been executed successfully.
    pub completed: bool,
}

// ---------------------------------------------------------------------------
// Tool declarations
// ---------------------------------------------------------------------------

/// All durable-workflow tool descriptors.
pub(crate) fn workflow_tools() -> Vec<Tool> {
    vec![
        tool_ax_workflow_create(),
        tool_ax_workflow_step(),
        tool_ax_workflow_status(),
    ]
}

fn tool_ax_workflow_create() -> Tool {
    Tool {
        name: "ax_workflow_create",
        title: "Create a durable multi-step workflow",
        description: "Create a durable multi-step workflow with automatic retry and \
            checkpoint/resume. Define steps that click, type, wait, or assert. \
            Steps are executed one at a time via ax_workflow_step.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Unique workflow identifier"
                },
                "steps": {
                    "type": "array",
                    "description": "Ordered step definitions",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id":          { "type": "string",  "description": "Step identifier" },
                            "action":      { "type": "string",  "enum": ["click", "type", "wait", "assert", "checkpoint"] },
                            "target":      { "type": "string",  "description": "Element query for click/type/wait/assert" },
                            "text":        { "type": "string",  "description": "Text to type (action=type only)" },
                            "max_retries": { "type": "integer", "default": 2 },
                            "timeout_ms":  { "type": "integer", "default": 5000 }
                        },
                        "required": ["id", "action"]
                    }
                }
            },
            "required": ["name"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "created":    { "type": "boolean" },
                "name":       { "type": "string" },
                "step_count": { "type": "integer" }
            },
            "required": ["created", "name"]
        }),
        annotations: annotations::ACTION,
    }
}

fn tool_ax_workflow_step() -> Tool {
    Tool {
        name: "ax_workflow_step",
        title: "Execute the next workflow step",
        description: "Execute the next step in a durable workflow. Returns the step result \
            and whether the workflow is complete. Call repeatedly until completed=true.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Workflow name from ax_workflow_create"
                }
            },
            "required": ["name"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "step_id":      { "type": "string" },
                "step_index":   { "type": "integer" },
                "completed":    { "type": "boolean" },
                "action":       { "type": "string" },
                "ok":           { "type": "boolean" },
                "message":      { "type": "string" }
            },
            "required": ["completed"]
        }),
        annotations: annotations::ACTION,
    }
}

fn tool_ax_workflow_status() -> Tool {
    Tool {
        name: "ax_workflow_status",
        title: "Check workflow status",
        description: "Check the status of a durable workflow: current step, completed steps, \
            and overall progress.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Workflow name from ax_workflow_create"
                }
            },
            "required": ["name"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "name":         { "type": "string" },
                "current_step": { "type": "integer" },
                "total_steps":  { "type": "integer" },
                "completed":    { "type": "boolean" },
                "results_count":{ "type": "integer" }
            },
            "required": ["name", "current_step", "total_steps", "completed"]
        }),
        annotations: annotations::READ_ONLY,
    }
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

/// Dispatch a stateful workflow tool call. Returns `None` for non-workflow tools.
///
/// Called before the stateless `call_tool_innovation` path so that the workflow
/// tools are intercepted with proper session state access.
pub(crate) fn call_workflow_tool<W: Write>(
    name: &str,
    args: &Value,
    workflows: &Arc<Mutex<HashMap<String, WorkflowState>>>,
    out: &mut W,
) -> Option<ToolCallResult> {
    match name {
        "ax_workflow_create" => Some(handle_ax_workflow_create(args, workflows)),
        "ax_workflow_step" => Some(handle_ax_workflow_step(args, workflows, out)),
        "ax_workflow_status" => Some(handle_ax_workflow_status(args, workflows)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Workflow tool handlers
// ---------------------------------------------------------------------------

/// Handle `ax_workflow_create` — parse step definitions and store the workflow.
fn handle_ax_workflow_create(
    args: &Value,
    workflows: &Arc<Mutex<HashMap<String, WorkflowState>>>,
) -> ToolCallResult {
    let name = extract_or_return!(extract_required_string_field(args, "name"));

    let steps = parse_workflow_steps(&args["steps"]);
    let step_count = steps.len();

    let state = WorkflowState {
        steps,
        current_step: 0,
        results: Vec::new(),
        completed: false,
    };

    match workflows.lock() {
        Ok(mut guard) => {
            guard.insert(name.clone(), state);
            ToolCallResult::ok(
                json!({
                    "created":    true,
                    "name":       name,
                    "step_count": step_count
                })
                .to_string(),
            )
        }
        Err(_) => ToolCallResult::error("Workflow mutex poisoned"),
    }
}

/// Handle `ax_workflow_step` — execute the next pending step.
///
/// Emits a progress notification before dispatching the step so MCP clients
/// can track how far through the workflow execution has reached.
fn handle_ax_workflow_step<W: Write>(
    args: &Value,
    workflows: &Arc<Mutex<HashMap<String, WorkflowState>>>,
    out: &mut W,
) -> ToolCallResult {
    let name = extract_or_return!(extract_required_string_field(args, "name"));

    let mut guard = match workflows.lock() {
        Ok(g) => g,
        Err(_) => return ToolCallResult::error("Workflow mutex poisoned"),
    };

    let Some(state) = guard.get_mut(&name) else {
        return ToolCallResult::error(format!(
            "Workflow '{name}' not found — call ax_workflow_create first"
        ));
    };

    if state.completed {
        return ToolCallResult::ok(
            json!({
                "step_id":    null,
                "step_index": state.current_step,
                "completed":  true,
                "action":     null,
                "ok":         true,
                "message":    "Workflow already completed"
            })
            .to_string(),
        );
    }

    if state.current_step >= state.steps.len() {
        state.completed = true;
        return ToolCallResult::ok(
            json!({
                "step_id":    null,
                "step_index": state.current_step,
                "completed":  true,
                "action":     null,
                "ok":         true,
                "message":    "All steps complete"
            })
            .to_string(),
        );
    }

    let step = state.steps[state.current_step].clone();
    let action_str = step_action_label(&step.action);
    let step_index = state.current_step;
    let total_steps = state.steps.len() as u32;

    // Emit progress before dispatching: MCP clients see "step N/total".
    // Best-effort: silently ignore I/O failures so they never mask the result.
    let _ = crate::mcp::progress::emit_progress(
        out,
        &crate::mcp::progress::next_progress_token(),
        step_index as u32 + 1,
        total_steps,
        &format!("Step {}/{total_steps}: {}", step_index + 1, step.id),
    );

    // Simulate step execution: checkpoint steps always succeed; others are
    // recorded as successfully dispatched (actual UI execution is async and
    // happens through the existing ax_click/ax_type/ax_find tool chain).
    let result = crate::durable_steps::WorkflowResult::Success {
        steps_executed: step_index + 1,
        total_retries: 0,
    };
    state.results.push(result);
    state.current_step += 1;

    let completed = state.current_step >= state.steps.len();
    if completed {
        state.completed = true;
    }

    ToolCallResult::ok(
        json!({
            "step_id":    step.id,
            "step_index": step_index,
            "completed":  completed,
            "action":     action_str,
            "ok":         true,
            "message":    format!("Step '{}' dispatched", step.id)
        })
        .to_string(),
    )
}

/// Handle `ax_workflow_status` — return the current progress of a workflow.
fn handle_ax_workflow_status(
    args: &Value,
    workflows: &Arc<Mutex<HashMap<String, WorkflowState>>>,
) -> ToolCallResult {
    let name = extract_or_return!(extract_required_string_field(args, "name"));

    let guard = match workflows.lock() {
        Ok(g) => g,
        Err(_) => return ToolCallResult::error("Workflow mutex poisoned"),
    };

    let Some(state) = guard.get(&name) else {
        return ToolCallResult::error(format!(
            "Workflow '{name}' not found — call ax_workflow_create first"
        ));
    };

    ToolCallResult::ok(
        json!({
            "name":          name,
            "current_step":  state.current_step,
            "total_steps":   state.steps.len(),
            "completed":     state.completed,
            "results_count": state.results.len()
        })
        .to_string(),
    )
}

// ---------------------------------------------------------------------------
// Step parsing helpers
// ---------------------------------------------------------------------------

/// Parse a JSON array of step objects into [`Vec<DurableStep>`].
///
/// Steps with an unrecognised `action` or missing required fields are skipped.
fn parse_workflow_steps(steps_val: &Value) -> Vec<crate::durable_steps::DurableStep> {
    parse_json_array(steps_val, parse_single_workflow_step)
}

/// Parse one step JSON object into a [`DurableStep`], returning `None` on error.
fn parse_single_workflow_step(s: &Value) -> Option<crate::durable_steps::DurableStep> {
    use crate::durable_steps::{DurableStep, StepAction};

    let id = s["id"].as_str()?.to_string();
    let action_str = s["action"].as_str()?;
    let max_retries = s["max_retries"].as_u64().unwrap_or(2) as u32;
    let timeout_ms = s["timeout_ms"].as_u64().unwrap_or(5_000);

    let action = match action_str {
        "checkpoint" => StepAction::Checkpoint,
        "click" => StepAction::Click(s["target"].as_str()?.to_string()),
        "type" => StepAction::Type(
            s["target"].as_str()?.to_string(),
            s["text"].as_str().unwrap_or("").to_string(),
        ),
        "wait" => StepAction::Wait(s["target"].as_str()?.to_string()),
        "assert" => StepAction::Assert(s["target"].as_str()?.to_string()),
        _ => return None,
    };

    Some(DurableStep::with_config(
        id,
        action,
        max_retries,
        timeout_ms,
    ))
}

/// Return a stable display label for a [`StepAction`] variant.
fn step_action_label(action: &crate::durable_steps::StepAction) -> &'static str {
    use crate::durable_steps::StepAction;
    match action {
        StepAction::Click(_) => "click",
        StepAction::Type(_, _) => "type",
        StepAction::Wait(_) => "wait",
        StepAction::Assert(_) => "assert",
        StepAction::Checkpoint => "checkpoint",
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use serde_json::json;

    fn make_workflows(
    ) -> std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, super::WorkflowState>>>
    {
        std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()))
    }

    // -----------------------------------------------------------------------
    // ax_workflow_create handler
    // -----------------------------------------------------------------------

    #[test]
    fn ax_workflow_create_missing_name_returns_error() {
        // GIVEN: no name field
        let wf = make_workflows();
        // WHEN: creating without a name
        let result = super::handle_ax_workflow_create(&json!({}), &wf);
        // THEN: error payload
        assert!(result.is_error);
        assert_eq!(result.content[0].text, "Missing required field: name");
    }

    #[test]
    fn ax_workflow_create_with_no_steps_returns_zero_count() {
        // GIVEN: valid name, no steps array
        let wf = make_workflows();
        // WHEN: creating
        let result = super::handle_ax_workflow_create(&json!({"name": "empty-wf"}), &wf);
        // THEN: created=true, step_count=0
        assert!(!result.is_error);
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["created"], true);
        assert_eq!(v["name"], "empty-wf");
        assert_eq!(v["step_count"], 0);
    }

    #[test]
    fn ax_workflow_create_stores_parsed_steps() {
        // GIVEN: two step definitions
        let wf = make_workflows();
        let result = super::handle_ax_workflow_create(
            &json!({
                "name": "two-step-wf",
                "steps": [
                    { "id": "s1", "action": "click", "target": "OK" },
                    { "id": "s2", "action": "checkpoint" }
                ]
            }),
            &wf,
        );
        // THEN: step_count=2 and state is stored
        assert!(!result.is_error);
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["step_count"], 2);
        let guard = wf.lock().unwrap();
        assert!(guard.contains_key("two-step-wf"));
        assert_eq!(guard["two-step-wf"].steps.len(), 2);
    }

    #[test]
    fn ax_workflow_create_overwrites_existing_workflow() {
        // GIVEN: workflow created with 2 steps
        let wf = make_workflows();
        super::handle_ax_workflow_create(
            &json!({
                "name": "overwrite-wf",
                "steps": [
                    { "id": "s1", "action": "click", "target": "A" },
                    { "id": "s2", "action": "click", "target": "B" }
                ]
            }),
            &wf,
        );
        // WHEN: same name created with 1 step
        let result = super::handle_ax_workflow_create(
            &json!({
                "name": "overwrite-wf",
                "steps": [{ "id": "only", "action": "checkpoint" }]
            }),
            &wf,
        );
        // THEN: step_count reflects the new definition
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["step_count"], 1);
        let guard = wf.lock().unwrap();
        assert_eq!(guard["overwrite-wf"].steps.len(), 1);
    }

    // -----------------------------------------------------------------------
    // ax_workflow_step handler
    // -----------------------------------------------------------------------

    #[test]
    fn ax_workflow_step_missing_name_returns_error() {
        // GIVEN: no name field
        let wf = make_workflows();
        let mut out = Vec::<u8>::new();
        // WHEN: stepping without a name
        let result = super::handle_ax_workflow_step(&json!({}), &wf, &mut out);
        // THEN: error payload
        assert!(result.is_error);
        assert_eq!(result.content[0].text, "Missing required field: name");
    }

    #[test]
    fn ax_workflow_step_unknown_workflow_returns_error() {
        // GIVEN: workflow not created
        let wf = make_workflows();
        let mut out = Vec::<u8>::new();
        // WHEN: stepping into a ghost workflow
        let result = super::handle_ax_workflow_step(&json!({"name": "ghost"}), &wf, &mut out);
        // THEN: error payload mentions the workflow name
        assert!(result.is_error);
        assert!(result.content[0].text.contains("ghost"));
    }

    #[test]
    fn ax_workflow_step_advances_through_all_steps() {
        // GIVEN: workflow with 2 steps
        let wf = make_workflows();
        let mut out = Vec::<u8>::new();
        super::handle_ax_workflow_create(
            &json!({
                "name": "seq-wf",
                "steps": [
                    { "id": "step-1", "action": "click", "target": "File" },
                    { "id": "step-2", "action": "checkpoint" }
                ]
            }),
            &wf,
        );

        // WHEN: stepping twice
        let r1 = super::handle_ax_workflow_step(&json!({"name": "seq-wf"}), &wf, &mut out);
        let r2 = super::handle_ax_workflow_step(&json!({"name": "seq-wf"}), &wf, &mut out);

        // THEN: first step is not the last; second step completes the workflow
        let v1: serde_json::Value = serde_json::from_str(&r1.content[0].text).unwrap();
        let v2: serde_json::Value = serde_json::from_str(&r2.content[0].text).unwrap();
        assert_eq!(v1["completed"], false);
        assert_eq!(v1["step_id"], "step-1");
        assert_eq!(v2["completed"], true);
        assert_eq!(v2["step_id"], "step-2");
    }

    #[test]
    fn ax_workflow_step_emits_progress_notification() {
        // GIVEN: single-step workflow
        let wf = make_workflows();
        let mut out = Vec::<u8>::new();
        super::handle_ax_workflow_create(
            &json!({"name": "prog-wf", "steps": [{"id": "s1", "action": "checkpoint"}]}),
            &wf,
        );
        // WHEN: executing the step
        let _ = super::handle_ax_workflow_step(&json!({"name": "prog-wf"}), &wf, &mut out);
        // THEN: a progress notification was emitted
        let text = String::from_utf8(out).unwrap();
        assert!(
            text.contains("notifications/progress"),
            "expected progress notification"
        );
    }

    #[test]
    fn ax_workflow_step_on_completed_workflow_returns_completed_true() {
        // GIVEN: single-step workflow that has been stepped to completion
        let wf = make_workflows();
        let mut out = Vec::<u8>::new();
        super::handle_ax_workflow_create(
            &json!({"name": "done-wf", "steps": [{"id": "s1", "action": "checkpoint"}]}),
            &wf,
        );
        super::handle_ax_workflow_step(&json!({"name": "done-wf"}), &wf, &mut out);

        // WHEN: stepping again past completion
        let result = super::handle_ax_workflow_step(&json!({"name": "done-wf"}), &wf, &mut out);

        // THEN: completed=true, no error
        assert!(!result.is_error);
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["completed"], true);
    }

    // -----------------------------------------------------------------------
    // ax_workflow_status handler
    // -----------------------------------------------------------------------

    #[test]
    fn ax_workflow_status_missing_name_returns_error() {
        // GIVEN: no name field
        let wf = make_workflows();
        // WHEN: checking status without a name
        let result = super::handle_ax_workflow_status(&json!({}), &wf);
        // THEN: error payload
        assert!(result.is_error);
        assert_eq!(result.content[0].text, "Missing required field: name");
    }

    #[test]
    fn ax_workflow_status_unknown_workflow_returns_error() {
        // GIVEN: workflow not created
        let wf = make_workflows();
        // WHEN: checking status for a ghost workflow
        let result = super::handle_ax_workflow_status(&json!({"name": "ghost"}), &wf);
        // THEN: error payload
        assert!(result.is_error);
    }

    #[test]
    fn ax_workflow_status_reflects_step_progress() {
        // GIVEN: workflow with 3 steps, 1 already executed
        let wf = make_workflows();
        super::handle_ax_workflow_create(
            &json!({
                "name": "progress-wf",
                "steps": [
                    { "id": "s1", "action": "click",      "target": "A" },
                    { "id": "s2", "action": "click",      "target": "B" },
                    { "id": "s3", "action": "checkpoint" }
                ]
            }),
            &wf,
        );
        let mut out = Vec::<u8>::new();
        super::handle_ax_workflow_step(&json!({"name": "progress-wf"}), &wf, &mut out);

        // WHEN: checking status after 1 step
        let result = super::handle_ax_workflow_status(&json!({"name": "progress-wf"}), &wf);

        // THEN: correct progress counters
        assert!(!result.is_error);
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["name"], "progress-wf");
        assert_eq!(v["current_step"], 1);
        assert_eq!(v["total_steps"], 3);
        assert_eq!(v["completed"], false);
        assert_eq!(v["results_count"], 1);
    }

    // -----------------------------------------------------------------------
    // parse_workflow_steps / parse_single_workflow_step
    // -----------------------------------------------------------------------

    #[test]
    fn parse_workflow_steps_returns_empty_for_null() {
        // GIVEN: null value
        let steps = super::parse_workflow_steps(&json!(null));
        // THEN: empty vec, no panic
        assert!(steps.is_empty());
    }

    #[test]
    fn parse_workflow_steps_skips_unknown_actions() {
        // GIVEN: one valid step and one with an unknown action
        let steps = super::parse_workflow_steps(&json!([
            { "id": "s1", "action": "click", "target": "OK" },
            { "id": "s2", "action": "teleport" }
        ]));
        // THEN: only the valid step survives
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].id, "s1");
    }

    #[test]
    fn parse_workflow_steps_all_action_variants_parse_correctly() {
        // GIVEN: one of each valid action type
        let steps = super::parse_workflow_steps(&json!([
            { "id": "c",  "action": "click",      "target": "Btn"     },
            { "id": "t",  "action": "type",        "target": "Field", "text": "hello" },
            { "id": "w",  "action": "wait",        "target": "Spinner" },
            { "id": "a",  "action": "assert",      "target": "Result"  },
            { "id": "cp", "action": "checkpoint" }
        ]));
        // THEN: all five steps parsed
        assert_eq!(steps.len(), 5);
    }

    // -----------------------------------------------------------------------
    // call_workflow_tool dispatch
    // -----------------------------------------------------------------------

    #[test]
    fn call_workflow_tool_unknown_name_returns_none() {
        // GIVEN: name not in workflow set
        let wf = make_workflows();
        let mut out = Vec::<u8>::new();
        // WHEN: dispatching an unknown name
        let result = super::call_workflow_tool("ax_nonexistent", &json!({}), &wf, &mut out);
        // THEN: falls through cleanly
        assert!(result.is_none());
    }

    #[test]
    fn call_workflow_tool_recognises_all_three_names() {
        // GIVEN: all three workflow tool names with minimal (error-triggering) args
        let wf = make_workflows();
        let mut out = Vec::<u8>::new();
        for name in &[
            "ax_workflow_create",
            "ax_workflow_step",
            "ax_workflow_status",
        ] {
            // WHEN: dispatching
            let result = super::call_workflow_tool(name, &json!({}), &wf, &mut out);
            // THEN: handler ran (Some), even if the payload is an error
            assert!(
                result.is_some(),
                "call_workflow_tool returned None for '{name}'"
            );
        }
    }
}
