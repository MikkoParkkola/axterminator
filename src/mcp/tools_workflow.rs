//! Session-scoped workflow tools — `ax_workflow_create`, `ax_workflow_step`,
//! `ax_workflow_status`.
//!
//! ## Responsibility boundary
//!
//! This module owns everything related to session-scoped workflows:
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

use std::collections::{hash_map::Entry, HashMap};
use std::io::Write;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::durable_steps::{DurableRunner, DurableStep, StepAction, StepExecutor, WorkflowResult};
use crate::mcp::action_safety::{is_element_destructive, require_destructive_confirmation};
use crate::mcp::annotations;
use crate::mcp::args::{extract_or_return, extract_required_string_field, reject_unknown_fields};
use crate::mcp::protocol::{ContentItem, Tool, ToolCallResult};
use crate::mcp::tools::AppRegistry;

// ---------------------------------------------------------------------------
// WorkflowState
// ---------------------------------------------------------------------------

/// Tracks the in-progress state of a single workflow across MCP calls.
pub(crate) struct WorkflowState {
    /// Connected app alias used for step execution. When absent, the runner
    /// falls back to the only connected app in the session.
    pub app_name: Option<String>,
    /// The ordered steps that make up this workflow.
    pub steps: Vec<DurableStep>,
    /// Zero-based index of the next step to execute.
    pub current_step: usize,
    /// Results accumulated from already-executed steps.
    pub results: Vec<WorkflowResult>,
    /// Step indices where checkpoint steps completed successfully.
    pub checkpoint_indices: Vec<usize>,
    /// Whether all steps have been executed successfully.
    pub completed: bool,
    /// Latest workflow error, if any.
    pub last_error: Option<String>,
}

// ---------------------------------------------------------------------------
// Tool declarations
// ---------------------------------------------------------------------------

/// All workflow tool descriptors.
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
        title: "Create a session-scoped workflow",
        description: "Create a session-scoped multi-step workflow. Each call to \
            ax_workflow_step executes the next step with its configured retry policy \
            and records checkpoint markers for status reporting within the current \
            MCP session.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Unique workflow identifier"
                },
                "app": {
                    "type": "string",
                    "description": "App alias from ax_connect. Optional only when exactly one app is connected."
                },
                "steps": {
                    "type": "array",
                    "description": "Ordered step definitions",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id":          { "type": "string",  "description": "Step identifier" },
                            "action":      { "type": "string",  "enum": ["click", "type", "wait", "assert", "checkpoint"] },
                            "target":      { "type": "string",  "description": "Element query. Required for click/type/wait/assert. wait/assert currently verify that the query resolves successfully." },
                            "text":        { "type": "string",  "description": "Text to type. Required for action=type." },
                            "max_retries": { "type": "integer", "minimum": 0, "maximum": 4294967295u64, "default": 2 },
                            "timeout_ms":  { "type": "integer", "minimum": 0, "default": 5000 }
                        },
                        "required": ["id", "action"],
                        "allOf": [
                            {
                                "if": { "properties": { "action": { "const": "click" } } },
                                "then": { "required": ["id", "action", "target"] }
                            },
                            {
                                "if": { "properties": { "action": { "const": "type" } } },
                                "then": { "required": ["id", "action", "target", "text"] }
                            },
                            {
                                "if": { "properties": { "action": { "const": "wait" } } },
                                "then": { "required": ["id", "action", "target"] }
                            },
                            {
                                "if": { "properties": { "action": { "const": "assert" } } },
                                "then": { "required": ["id", "action", "target"] }
                            }
                        ],
                        "additionalProperties": false
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
                "app":        { "type": ["string", "null"] },
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
        description: "Execute the next step in a session-scoped workflow through the real \
            automation path. Returns success/failure details, retry usage, and whether the \
            workflow is complete. Call repeatedly until completed=true.",
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
                "step_id":      { "type": ["string", "null"] },
                "step_index":   { "type": "integer" },
                "completed":    { "type": "boolean" },
                "action":       { "type": ["string", "null"] },
                "ok":           { "type": "boolean" },
                "retries_used": { "type": "integer" },
                "last_checkpoint_step": { "type": ["integer", "null"] },
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
        description: "Check the status of a session-scoped workflow: current step, completed \
            steps, recorded checkpoints, and the latest error (if any).",
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
                "name":                 { "type": "string" },
                "app":                  { "type": ["string", "null"] },
                "current_step":         { "type": "integer" },
                "total_steps":          { "type": "integer" },
                "completed":            { "type": "boolean" },
                "results_count":        { "type": "integer" },
                "checkpoint_count":     { "type": "integer" },
                "last_checkpoint_step": { "type": ["integer", "null"] },
                "last_error":           { "type": ["string", "null"] }
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
    registry: &Arc<AppRegistry>,
    workflows: &Arc<Mutex<HashMap<String, WorkflowState>>>,
    out: &mut W,
) -> Option<ToolCallResult> {
    match name {
        "ax_workflow_create" => Some(handle_ax_workflow_create(args, workflows)),
        "ax_workflow_step" => Some(handle_ax_workflow_step(args, registry, workflows, out)),
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
    extract_or_return!(reject_unknown_fields(args, &["name", "app", "steps"]));
    let app_name = extract_or_return!(parse_optional_workflow_app(args));
    let steps = match args.get("steps") {
        Some(steps) => extract_or_return!(parse_workflow_steps(steps)),
        None => Vec::new(),
    };
    let step_count = steps.len();

    let state = WorkflowState {
        app_name: app_name.clone(),
        steps,
        current_step: 0,
        results: Vec::new(),
        checkpoint_indices: Vec::new(),
        completed: false,
        last_error: None,
    };

    match workflows.lock() {
        Ok(mut guard) => {
            match guard.entry(name.clone()) {
                Entry::Vacant(entry) => {
                    entry.insert(state);
                    ToolCallResult::ok(
                        json!({
                            "created":    true,
                            "name":       name,
                            "app":        app_name,
                            "step_count": step_count
                        })
                        .to_string(),
                    )
                }
                Entry::Occupied(_) => ToolCallResult::error(format!(
                    "Workflow '{name}' already exists — use ax_workflow_step / ax_workflow_status or choose a different name"
                )),
            }
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
    registry: &Arc<AppRegistry>,
    workflows: &Arc<Mutex<HashMap<String, WorkflowState>>>,
    out: &mut W,
) -> ToolCallResult {
    let name = extract_or_return!(extract_required_string_field(args, "name"));
    extract_or_return!(reject_unknown_fields(args, &["name"]));

    let mut guard = match workflows.lock() {
        Ok(g) => g,
        Err(_) => return ToolCallResult::error("Workflow mutex poisoned"),
    };

    let Some(state) = guard.get_mut(&name) else {
        return ToolCallResult::error(format!(
            "Workflow '{name}' not found — call ax_workflow_create first"
        ));
    };

    if state.completed || state.current_step >= state.steps.len() {
        state.completed = true;
        state.last_error = None;
        return ToolCallResult::ok_json(workflow_step_payload(WorkflowStepPayload {
            step_id: None,
            step_index: state.current_step,
            completed: true,
            action: None,
            ok: true,
            retries_used: 0,
            last_checkpoint_step: state.checkpoint_indices.last().copied(),
            message: "All steps complete".to_owned(),
        }));
    }

    let step = state.steps[state.current_step].clone();
    let action_str = step_action_label(&step.action);
    let step_index = state.current_step;

    if step.action == StepAction::Checkpoint {
        let mut executor = NoopWorkflowExecutor;
        return execute_current_step_with_executor(state, out, &mut executor);
    }

    let app_name = match resolve_workflow_app(&name, state, registry) {
        Ok(app_name) => app_name,
        Err(message) => {
            state.last_error = Some(message.clone());
            return workflow_step_error(WorkflowStepPayload {
                step_id: Some(step.id.as_str()),
                step_index,
                completed: false,
                action: Some(action_str),
                ok: false,
                retries_used: 0,
                last_checkpoint_step: state.checkpoint_indices.last().copied(),
                message,
            });
        }
    };

    let mut executor = LiveWorkflowExecutor::new(Arc::clone(registry), app_name, step.timeout_ms);
    execute_current_step_with_executor(state, out, &mut executor)
}

/// Handle `ax_workflow_status` — return the current progress of a workflow.
fn handle_ax_workflow_status(
    args: &Value,
    workflows: &Arc<Mutex<HashMap<String, WorkflowState>>>,
) -> ToolCallResult {
    let name = extract_or_return!(extract_required_string_field(args, "name"));
    extract_or_return!(reject_unknown_fields(args, &["name"]));

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
            "name":                 name,
            "app":                  state.app_name.clone(),
            "current_step":         state.current_step,
            "total_steps":          state.steps.len(),
            "completed":            state.completed,
            "results_count":        state.results.len(),
            "checkpoint_count":     state.checkpoint_indices.len(),
            "last_checkpoint_step": state.checkpoint_indices.last().copied(),
            "last_error":           state.last_error.clone()
        })
        .to_string(),
    )
}

// ---------------------------------------------------------------------------
// Step execution helpers
// ---------------------------------------------------------------------------

fn execute_current_step_with_executor<W: Write>(
    state: &mut WorkflowState,
    out: &mut W,
    executor: &mut dyn StepExecutor,
) -> ToolCallResult {
    let step = state.steps[state.current_step].clone();
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

    let mut runner = DurableRunner::new();
    let step_slice = std::slice::from_ref(&step);

    match runner.run(step_slice, executor) {
        WorkflowResult::Success {
            steps_executed,
            total_retries,
        } => {
            state.results.push(WorkflowResult::Success {
                steps_executed,
                total_retries,
            });
            state.last_error = None;
            if step.action == StepAction::Checkpoint {
                state.checkpoint_indices.push(step_index);
            }
            state.current_step += steps_executed;
            state.completed = state.current_step >= state.steps.len();
            ToolCallResult::ok_json(workflow_step_payload(WorkflowStepPayload {
                step_id: Some(step.id.as_str()),
                step_index,
                completed: state.completed,
                action: Some(step_action_label(&step.action)),
                ok: true,
                retries_used: total_retries,
                last_checkpoint_step: state.checkpoint_indices.last().copied(),
                message: workflow_success_message(&step, total_retries),
            }))
        }
        WorkflowResult::Failed { failure, .. } => {
            let last_checkpoint_step = state.checkpoint_indices.last().copied();
            let retries_used = failure.attempts.saturating_sub(1);
            let message = format!(
                "Step '{}' failed after {} attempt(s): {}",
                step.id, failure.attempts, failure.reason
            );
            state.last_error = Some(message.clone());
            state.results.push(WorkflowResult::Failed {
                failure: crate::durable_steps::StepFailure {
                    step_id: step.id.clone(),
                    step_index,
                    attempts: failure.attempts,
                    reason: failure.reason,
                },
                last_checkpoint: last_checkpoint_step,
            });
            workflow_step_error(WorkflowStepPayload {
                step_id: Some(step.id.as_str()),
                step_index,
                completed: false,
                action: Some(step_action_label(&step.action)),
                ok: false,
                retries_used,
                last_checkpoint_step,
                message,
            })
        }
    }
}

fn resolve_workflow_app(
    workflow_name: &str,
    state: &mut WorkflowState,
    registry: &Arc<AppRegistry>,
) -> Result<String, String> {
    if let Some(app_name) = state.app_name.clone() {
        return Ok(app_name);
    }

    let connected_apps = registry.connected_names();
    match connected_apps.as_slice() {
        [app_name] => {
            let app_name = app_name.clone();
            state.app_name = Some(app_name.clone());
            Ok(app_name)
        }
        [] => Err(format!(
            "Workflow '{workflow_name}' has no app context — pass app to ax_workflow_create or connect the target app first"
        )),
        _ => Err(format!(
            "Workflow '{workflow_name}' has no app context and multiple apps are connected ({}) — recreate it with app=\"...\"",
            connected_apps.join(", ")
        )),
    }
}

fn workflow_success_message(step: &DurableStep, retries_used: u32) -> String {
    match &step.action {
        StepAction::Checkpoint => format!("Checkpoint '{}' recorded", step.id),
        _ if retries_used == 0 => format!("Step '{}' executed successfully", step.id),
        _ => format!(
            "Step '{}' executed successfully after {} retr{}",
            step.id,
            retries_used,
            if retries_used == 1 { "y" } else { "ies" }
        ),
    }
}

fn workflow_step_payload(payload: WorkflowStepPayload<'_>) -> Value {
    json!({
        "step_id": payload.step_id,
        "step_index": payload.step_index,
        "completed": payload.completed,
        "action": payload.action,
        "ok": payload.ok,
        "retries_used": payload.retries_used,
        "last_checkpoint_step": payload.last_checkpoint_step,
        "message": payload.message
    })
}

struct WorkflowStepPayload<'a> {
    step_id: Option<&'a str>,
    step_index: usize,
    completed: bool,
    action: Option<&'a str>,
    ok: bool,
    retries_used: u32,
    last_checkpoint_step: Option<usize>,
    message: String,
}

fn workflow_step_error(payload: WorkflowStepPayload<'_>) -> ToolCallResult {
    ToolCallResult {
        content: vec![ContentItem::text(
            workflow_step_payload(payload).to_string(),
        )],
        is_error: true,
    }
}

struct LiveWorkflowExecutor {
    registry: Arc<AppRegistry>,
    app_name: String,
    timeout_ms: u64,
}

struct NoopWorkflowExecutor;

impl LiveWorkflowExecutor {
    fn new(registry: Arc<AppRegistry>, app_name: String, timeout_ms: u64) -> Self {
        Self {
            registry,
            app_name,
            timeout_ms,
        }
    }

    fn with_app<F>(&self, f: F) -> Result<(), String>
    where
        F: FnOnce(&crate::AXApp) -> Result<(), String>,
    {
        self.registry
            .with_app(&self.app_name, f)
            .and_then(std::convert::identity)
    }
}

impl StepExecutor for LiveWorkflowExecutor {
    fn execute(&mut self, action: &StepAction) -> Result<(), String> {
        match action {
            StepAction::Click(query) => self.with_app(|app| {
                let element = app
                    .find_native(query, Some(self.timeout_ms))
                    .map_err(|e| format!("Could not find '{query}': {e}"))?;
                let destructive = is_element_destructive(&element);
                require_destructive_confirmation(query, destructive, false, "ax_click", "clicking")
                    .map_err(|err| err.content[0].text.clone())?;
                element
                    .click_native(crate::ActionMode::Background)
                    .map_err(|e| format!("Click failed: {e}"))
            }),
            StepAction::Type(query, text) => self.with_app(|app| {
                let element = app
                    .find_native(query, Some(self.timeout_ms))
                    .map_err(|e| format!("Could not find '{query}': {e}"))?;
                element
                    .type_text_native(text, crate::ActionMode::Focus)
                    .map_err(|e| format!("Type failed: {e}"))
            }),
            StepAction::Wait(query) => self.with_app(|app| {
                app.find_native(query, Some(self.timeout_ms))
                    .map(|_| ())
                    .map_err(|e| format!("Wait condition '{query}' not satisfied: {e}"))
            }),
            StepAction::Assert(query) => self.with_app(|app| {
                app.find_native(query, Some(self.timeout_ms))
                    .map(|_| ())
                    .map_err(|e| format!("Assertion failed for '{query}': {e}"))
            }),
            StepAction::Checkpoint => Ok(()),
        }
    }
}

impl StepExecutor for NoopWorkflowExecutor {
    fn execute(&mut self, action: &StepAction) -> Result<(), String> {
        match action {
            StepAction::Checkpoint => Ok(()),
            _ => Err("NoopWorkflowExecutor only supports checkpoint steps".to_owned()),
        }
    }
}

// ---------------------------------------------------------------------------
// Step parsing helpers
// ---------------------------------------------------------------------------

/// Parse a JSON array of step objects into [`Vec<DurableStep>`].
///
/// Missing `steps` still means "create an empty workflow", but a present malformed
/// step definition is rejected instead of being silently dropped.
fn parse_workflow_steps(
    steps_val: &Value,
) -> Result<Vec<crate::durable_steps::DurableStep>, String> {
    match steps_val {
        Value::Null => Err("Field 'steps' must be an array".to_owned()),
        Value::Array(steps) => steps
            .iter()
            .enumerate()
            .map(|(index, step)| {
                parse_single_workflow_step(step)
                    .map_err(|err| format!("Invalid workflow step at index {index}: {err}"))
            })
            .collect(),
        _ => Err("Field 'steps' must be an array".to_owned()),
    }
}

fn parse_optional_workflow_app(args: &Value) -> Result<Option<String>, String> {
    match args.get("app") {
        None => Ok(None),
        Some(Value::String(app)) => Ok(Some(app.clone())),
        Some(_) => Err("Field 'app' must be a string".to_owned()),
    }
}

/// Parse one step JSON object into a [`DurableStep`].
fn parse_single_workflow_step(s: &Value) -> Result<crate::durable_steps::DurableStep, String> {
    use crate::durable_steps::{DurableStep, StepAction};

    if let Some(step) = s.as_object() {
        for field in step.keys() {
            match field.as_str() {
                "id" | "action" | "target" | "text" | "max_retries" | "timeout_ms" => {}
                _ => return Err(format!("unknown field: {field}")),
            }
        }
    }

    let get_required_str = |field: &str| {
        s.get(field)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("missing required field: {field}"))
    };

    let id = get_required_str("id")?.to_string();
    let action_str = get_required_str("action")?;
    let max_retries = parse_optional_u32_field(s, "max_retries", 2)?;
    let timeout_ms = parse_optional_u64_field(s, "timeout_ms", 5_000)?;

    let action = match action_str {
        "checkpoint" => StepAction::Checkpoint,
        "click" => StepAction::Click(get_required_str("target")?.to_string()),
        "type" => StepAction::Type(
            get_required_str("target")?.to_string(),
            get_required_str("text")?.to_string(),
        ),
        "wait" => StepAction::Wait(get_required_str("target")?.to_string()),
        "assert" => StepAction::Assert(get_required_str("target")?.to_string()),
        _ => return Err(format!("unknown action: {action_str}")),
    };

    Ok(DurableStep::with_config(
        id,
        action,
        max_retries,
        timeout_ms,
    ))
}

fn parse_optional_u32_field(s: &Value, field: &str, default: u32) -> Result<u32, String> {
    let Some(value) = s.get(field) else {
        return Ok(default);
    };
    if value.is_null() {
        return Err(format!("field '{field}' must be a non-negative integer"));
    }

    let raw = value
        .as_u64()
        .ok_or_else(|| format!("field '{field}' must be a non-negative integer"))?;
    u32::try_from(raw).map_err(|_| format!("field '{field}' must be between 0 and {}", u32::MAX))
}

fn parse_optional_u64_field(s: &Value, field: &str, default: u64) -> Result<u64, String> {
    let Some(value) = s.get(field) else {
        return Ok(default);
    };
    if value.is_null() {
        return Err(format!("field '{field}' must be a non-negative integer"));
    }

    value
        .as_u64()
        .ok_or_else(|| format!("field '{field}' must be a non-negative integer"))
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
    use std::sync::Arc;

    use serde_json::{json, Value};

    use crate::durable_steps::{DurableStep, MockExecutor, StepAction};
    use crate::mcp::tools::AppRegistry;

    fn make_registry() -> Arc<AppRegistry> {
        Arc::new(AppRegistry::default())
    }

    fn make_workflows(
    ) -> std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, super::WorkflowState>>>
    {
        std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()))
    }

    fn state_with_steps(app_name: Option<&str>, steps: Vec<DurableStep>) -> super::WorkflowState {
        super::WorkflowState {
            app_name: app_name.map(str::to_owned),
            steps,
            current_step: 0,
            results: Vec::new(),
            checkpoint_indices: Vec::new(),
            completed: false,
            last_error: None,
        }
    }

    fn parse_payload(result: &crate::mcp::protocol::ToolCallResult) -> Value {
        serde_json::from_str(&result.content[0].text).expect("valid JSON payload")
    }

    // -----------------------------------------------------------------------
    // ax_workflow_create handler
    // -----------------------------------------------------------------------

    #[test]
    fn ax_workflow_create_missing_name_returns_error() {
        let wf = make_workflows();
        let result = super::handle_ax_workflow_create(&json!({}), &wf);
        assert!(result.is_error);
        assert_eq!(result.content[0].text, "Missing required field: name");
    }

    #[test]
    fn ax_workflow_create_with_no_steps_returns_zero_count() {
        let wf = make_workflows();
        let result = super::handle_ax_workflow_create(&json!({"name": "empty-wf"}), &wf);
        assert!(!result.is_error);
        let v = parse_payload(&result);
        assert_eq!(v["created"], true);
        assert_eq!(v["name"], "empty-wf");
        assert_eq!(v["step_count"], 0);
        assert_eq!(v["app"], Value::Null);
    }

    #[test]
    fn ax_workflow_create_rejects_null_steps() {
        let wf = make_workflows();
        let result = super::handle_ax_workflow_create(
            &json!({
                "name": "null-steps-wf",
                "steps": null
            }),
            &wf,
        );
        assert!(result.is_error);
        assert_eq!(result.content[0].text, "Field 'steps' must be an array");
        let guard = wf.lock().unwrap();
        assert!(!guard.contains_key("null-steps-wf"));
    }

    #[test]
    fn ax_workflow_create_rejects_non_string_app() {
        let wf = make_workflows();
        let result = super::handle_ax_workflow_create(
            &json!({
                "name": "bad-app-wf",
                "app": 123,
                "steps": [{ "id": "cp", "action": "checkpoint" }]
            }),
            &wf,
        );
        assert!(result.is_error);
        assert_eq!(result.content[0].text, "Field 'app' must be a string");
        let guard = wf.lock().unwrap();
        assert!(!guard.contains_key("bad-app-wf"));
    }

    #[test]
    fn ax_workflow_create_rejects_unknown_top_level_fields() {
        let wf = make_workflows();
        let result = super::handle_ax_workflow_create(
            &json!({
                "name": "extra-field-wf",
                "steps": [{ "id": "cp", "action": "checkpoint" }],
                "surprise": true
            }),
            &wf,
        );
        assert!(result.is_error);
        assert_eq!(result.content[0].text, "unknown field: surprise");
        let guard = wf.lock().unwrap();
        assert!(!guard.contains_key("extra-field-wf"));
    }

    #[test]
    fn ax_workflow_create_stores_parsed_steps() {
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
        assert!(!result.is_error);
        let v = parse_payload(&result);
        assert_eq!(v["step_count"], 2);
        let guard = wf.lock().unwrap();
        assert!(guard.contains_key("two-step-wf"));
        assert_eq!(guard["two-step-wf"].steps.len(), 2);
    }

    #[test]
    fn ax_workflow_create_rejects_invalid_step_definitions() {
        let wf = make_workflows();
        let result = super::handle_ax_workflow_create(
            &json!({
                "name": "invalid-wf",
                "steps": [
                    { "id": "s1", "action": "click", "target": "OK" },
                    { "id": "s2", "action": "teleport" }
                ]
            }),
            &wf,
        );
        assert!(result.is_error);
        assert_eq!(
            result.content[0].text,
            "Invalid workflow step at index 1: unknown action: teleport"
        );
        let guard = wf.lock().unwrap();
        assert!(!guard.contains_key("invalid-wf"));
    }

    #[test]
    fn ax_workflow_create_stores_app_name_when_present() {
        let wf = make_workflows();
        let result = super::handle_ax_workflow_create(
            &json!({
                "name": "app-bound-wf",
                "app": "Safari",
                "steps": [{ "id": "cp", "action": "checkpoint" }]
            }),
            &wf,
        );
        assert!(!result.is_error);
        let v = parse_payload(&result);
        assert_eq!(v["app"], "Safari");
        let guard = wf.lock().unwrap();
        assert_eq!(guard["app-bound-wf"].app_name.as_deref(), Some("Safari"));
    }

    #[test]
    fn ax_workflow_create_rejects_duplicate_workflow_names() {
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
        let result = super::handle_ax_workflow_create(
            &json!({
                "name": "overwrite-wf",
                "steps": [{ "id": "only", "action": "checkpoint" }]
            }),
            &wf,
        );
        assert!(result.is_error);
        assert_eq!(
            result.content[0].text,
            "Workflow 'overwrite-wf' already exists — use ax_workflow_step / ax_workflow_status or choose a different name"
        );
        let guard = wf.lock().unwrap();
        assert_eq!(guard["overwrite-wf"].steps.len(), 2);
    }

    // -----------------------------------------------------------------------
    // step execution helpers
    // -----------------------------------------------------------------------

    #[test]
    fn execute_current_step_with_executor_reports_retry_count() {
        let mut state = state_with_steps(
            Some("MockApp"),
            vec![DurableStep::with_retries(
                "retry-step",
                StepAction::Click("Save".into()),
                1,
            )],
        );
        let mut out = Vec::<u8>::new();
        let mut executor = MockExecutor::from_results(vec![Err("transient".into()), Ok(())]);

        let result = super::execute_current_step_with_executor(&mut state, &mut out, &mut executor);

        assert!(!result.is_error);
        let v = parse_payload(&result);
        assert_eq!(v["step_id"], "retry-step");
        assert_eq!(v["retries_used"], 1);
        assert_eq!(state.current_step, 1);
        assert!(state.completed);
        assert_eq!(state.last_error, None);
    }

    #[test]
    fn execute_current_step_with_executor_records_failure_without_advancing() {
        let mut state = state_with_steps(
            Some("MockApp"),
            vec![DurableStep::with_retries(
                "failing-step",
                StepAction::Click("Save".into()),
                1,
            )],
        );
        let mut out = Vec::<u8>::new();
        let mut executor = MockExecutor::from_results(vec![Err("boom".into()), Err("boom".into())]);

        let result = super::execute_current_step_with_executor(&mut state, &mut out, &mut executor);

        assert!(result.is_error);
        let v = parse_payload(&result);
        assert_eq!(v["ok"], false);
        assert_eq!(v["retries_used"], 1);
        assert_eq!(state.current_step, 0);
        assert!(state
            .last_error
            .as_deref()
            .is_some_and(|msg| msg.contains("boom")));
    }

    #[test]
    fn execute_current_step_with_executor_records_checkpoint_index() {
        let mut state = state_with_steps(Some("MockApp"), vec![DurableStep::checkpoint("cp-1")]);
        let mut out = Vec::<u8>::new();
        let mut executor = MockExecutor::always_ok();

        let result = super::execute_current_step_with_executor(&mut state, &mut out, &mut executor);

        assert!(!result.is_error);
        let v = parse_payload(&result);
        assert_eq!(v["last_checkpoint_step"], 0);
        assert_eq!(state.checkpoint_indices, vec![0]);
    }

    // -----------------------------------------------------------------------
    // ax_workflow_step handler
    // -----------------------------------------------------------------------

    #[test]
    fn ax_workflow_step_missing_name_returns_error() {
        let registry = make_registry();
        let wf = make_workflows();
        let mut out = Vec::<u8>::new();
        let result = super::handle_ax_workflow_step(&json!({}), &registry, &wf, &mut out);
        assert!(result.is_error);
        assert_eq!(result.content[0].text, "Missing required field: name");
    }

    #[test]
    fn ax_workflow_step_rejects_unknown_top_level_fields() {
        let registry = make_registry();
        let wf = make_workflows();
        let mut out = Vec::<u8>::new();
        super::handle_ax_workflow_create(
            &json!({"name": "known-wf", "steps": [{"id": "s1", "action": "checkpoint"}]}),
            &wf,
        );

        let result = super::handle_ax_workflow_step(
            &json!({"name": "known-wf", "extra": true}),
            &registry,
            &wf,
            &mut out,
        );
        assert!(result.is_error);
        assert_eq!(result.content[0].text, "unknown field: extra");

        let guard = wf.lock().unwrap();
        assert_eq!(guard["known-wf"].current_step, 0);
    }

    #[test]
    fn ax_workflow_step_unknown_workflow_returns_error() {
        let registry = make_registry();
        let wf = make_workflows();
        let mut out = Vec::<u8>::new();
        let result =
            super::handle_ax_workflow_step(&json!({"name": "ghost"}), &registry, &wf, &mut out);
        assert!(result.is_error);
        assert!(result.content[0].text.contains("ghost"));
    }

    #[test]
    fn ax_workflow_step_advances_through_all_steps() {
        let registry = make_registry();
        let wf = make_workflows();
        let mut out = Vec::<u8>::new();
        super::handle_ax_workflow_create(
            &json!({
                "name": "seq-wf",
                "steps": [
                    { "id": "step-1", "action": "checkpoint" },
                    { "id": "step-2", "action": "checkpoint" }
                ]
            }),
            &wf,
        );

        let r1 =
            super::handle_ax_workflow_step(&json!({"name": "seq-wf"}), &registry, &wf, &mut out);
        let r2 =
            super::handle_ax_workflow_step(&json!({"name": "seq-wf"}), &registry, &wf, &mut out);

        let v1 = parse_payload(&r1);
        let v2 = parse_payload(&r2);
        assert_eq!(v1["completed"], false);
        assert_eq!(v1["step_id"], "step-1");
        assert_eq!(v2["completed"], true);
        assert_eq!(v2["step_id"], "step-2");
    }

    #[test]
    fn ax_workflow_step_requires_app_context_for_action_steps() {
        let registry = make_registry();
        let wf = make_workflows();
        let mut out = Vec::<u8>::new();
        super::handle_ax_workflow_create(
            &json!({
                "name": "needs-app",
                "steps": [{ "id": "click", "action": "click", "target": "Save" }]
            }),
            &wf,
        );

        let result =
            super::handle_ax_workflow_step(&json!({"name": "needs-app"}), &registry, &wf, &mut out);

        assert!(result.is_error);
        let v = parse_payload(&result);
        assert_eq!(v["ok"], false);
        assert!(v["message"]
            .as_str()
            .unwrap()
            .contains("has no app context"));
    }

    #[test]
    fn ax_workflow_step_emits_progress_notification() {
        let registry = make_registry();
        let wf = make_workflows();
        let mut out = Vec::<u8>::new();
        super::handle_ax_workflow_create(
            &json!({"name": "prog-wf", "steps": [{"id": "s1", "action": "checkpoint"}]}),
            &wf,
        );
        let _ =
            super::handle_ax_workflow_step(&json!({"name": "prog-wf"}), &registry, &wf, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(
            text.contains("notifications/progress"),
            "expected progress notification"
        );
    }

    #[test]
    fn ax_workflow_step_on_completed_workflow_returns_completed_true() {
        let registry = make_registry();
        let wf = make_workflows();
        let mut out = Vec::<u8>::new();
        super::handle_ax_workflow_create(
            &json!({"name": "done-wf", "steps": [{"id": "s1", "action": "checkpoint"}]}),
            &wf,
        );
        super::handle_ax_workflow_step(&json!({"name": "done-wf"}), &registry, &wf, &mut out);

        let result =
            super::handle_ax_workflow_step(&json!({"name": "done-wf"}), &registry, &wf, &mut out);

        assert!(!result.is_error);
        let v = parse_payload(&result);
        assert_eq!(v["completed"], true);
        assert_eq!(v["step_id"], Value::Null);
    }

    // -----------------------------------------------------------------------
    // ax_workflow_status handler
    // -----------------------------------------------------------------------

    #[test]
    fn ax_workflow_status_missing_name_returns_error() {
        let wf = make_workflows();
        let result = super::handle_ax_workflow_status(&json!({}), &wf);
        assert!(result.is_error);
        assert_eq!(result.content[0].text, "Missing required field: name");
    }

    #[test]
    fn ax_workflow_status_rejects_unknown_top_level_fields() {
        let wf = make_workflows();
        super::handle_ax_workflow_create(
            &json!({"name": "status-wf", "steps": [{"id": "s1", "action": "checkpoint"}]}),
            &wf,
        );
        let result =
            super::handle_ax_workflow_status(&json!({"name": "status-wf", "extra": true}), &wf);
        assert!(result.is_error);
        assert_eq!(result.content[0].text, "unknown field: extra");
    }

    #[test]
    fn ax_workflow_status_unknown_workflow_returns_error() {
        let wf = make_workflows();
        let result = super::handle_ax_workflow_status(&json!({"name": "ghost"}), &wf);
        assert!(result.is_error);
    }

    #[test]
    fn ax_workflow_status_reflects_step_progress() {
        let registry = make_registry();
        let wf = make_workflows();
        super::handle_ax_workflow_create(
            &json!({
                "name": "progress-wf",
                "steps": [
                    { "id": "s1", "action": "checkpoint" },
                    { "id": "s2", "action": "checkpoint" },
                    { "id": "s3", "action": "checkpoint" }
                ]
            }),
            &wf,
        );
        let mut out = Vec::<u8>::new();
        super::handle_ax_workflow_step(&json!({"name": "progress-wf"}), &registry, &wf, &mut out);

        let result = super::handle_ax_workflow_status(&json!({"name": "progress-wf"}), &wf);

        assert!(!result.is_error);
        let v = parse_payload(&result);
        assert_eq!(v["name"], "progress-wf");
        assert_eq!(v["current_step"], 1);
        assert_eq!(v["total_steps"], 3);
        assert_eq!(v["completed"], false);
        assert_eq!(v["results_count"], 1);
        assert_eq!(v["checkpoint_count"], 1);
        assert_eq!(v["last_checkpoint_step"], 0);
    }

    // -----------------------------------------------------------------------
    // parse_workflow_steps / parse_single_workflow_step
    // -----------------------------------------------------------------------

    #[test]
    fn parse_workflow_steps_rejects_null() {
        let err = super::parse_workflow_steps(&json!(null)).unwrap_err();
        assert_eq!(err, "Field 'steps' must be an array");
    }

    #[test]
    fn parse_workflow_steps_rejects_unknown_actions() {
        let err = super::parse_workflow_steps(&json!([
            { "id": "s1", "action": "click", "target": "OK" },
            { "id": "s2", "action": "teleport" }
        ]))
        .unwrap_err();
        assert_eq!(
            err,
            "Invalid workflow step at index 1: unknown action: teleport"
        );
    }

    #[test]
    fn parse_workflow_steps_rejects_missing_target() {
        let err = super::parse_workflow_steps(&json!([
            { "id": "s1", "action": "click" }
        ]))
        .unwrap_err();
        assert_eq!(
            err,
            "Invalid workflow step at index 0: missing required field: target"
        );
    }

    #[test]
    fn parse_workflow_steps_rejects_type_steps_without_text() {
        let err = super::parse_workflow_steps(&json!([
            { "id": "s1", "action": "type", "target": "Field" }
        ]))
        .unwrap_err();
        assert_eq!(
            err,
            "Invalid workflow step at index 0: missing required field: text"
        );
    }

    #[test]
    fn parse_workflow_steps_rejects_negative_max_retries() {
        let err = super::parse_workflow_steps(&json!([
            { "id": "s1", "action": "click", "target": "OK", "max_retries": -1 }
        ]))
        .unwrap_err();
        assert_eq!(
            err,
            "Invalid workflow step at index 0: field 'max_retries' must be a non-negative integer"
        );
    }

    #[test]
    fn parse_workflow_steps_rejects_overflowing_max_retries() {
        let err = super::parse_workflow_steps(&json!([
            { "id": "s1", "action": "click", "target": "OK", "max_retries": 4294967296u64 }
        ]))
        .unwrap_err();
        assert_eq!(
            err,
            "Invalid workflow step at index 0: field 'max_retries' must be between 0 and 4294967295"
        );
    }

    #[test]
    fn parse_workflow_steps_rejects_negative_timeout() {
        let err = super::parse_workflow_steps(&json!([
            { "id": "s1", "action": "click", "target": "OK", "timeout_ms": -1 }
        ]))
        .unwrap_err();
        assert_eq!(
            err,
            "Invalid workflow step at index 0: field 'timeout_ms' must be a non-negative integer"
        );
    }

    #[test]
    fn parse_workflow_steps_rejects_null_max_retries() {
        let err = super::parse_workflow_steps(&json!([
            { "id": "s1", "action": "click", "target": "OK", "max_retries": null }
        ]))
        .unwrap_err();
        assert_eq!(
            err,
            "Invalid workflow step at index 0: field 'max_retries' must be a non-negative integer"
        );
    }

    #[test]
    fn parse_workflow_steps_rejects_null_timeout() {
        let err = super::parse_workflow_steps(&json!([
            { "id": "s1", "action": "click", "target": "OK", "timeout_ms": null }
        ]))
        .unwrap_err();
        assert_eq!(
            err,
            "Invalid workflow step at index 0: field 'timeout_ms' must be a non-negative integer"
        );
    }

    #[test]
    fn parse_workflow_steps_rejects_unknown_fields() {
        let err = super::parse_workflow_steps(&json!([
            { "id": "s1", "action": "click", "target": "OK", "surprise": true }
        ]))
        .unwrap_err();
        assert_eq!(
            err,
            "Invalid workflow step at index 0: unknown field: surprise"
        );
    }

    #[test]
    fn parse_workflow_steps_rejects_non_array_input() {
        let err = super::parse_workflow_steps(&json!({"oops": true})).unwrap_err();
        assert_eq!(err, "Field 'steps' must be an array");
    }

    #[test]
    fn parse_workflow_steps_all_action_variants_parse_correctly() {
        let steps = super::parse_workflow_steps(&json!([
            { "id": "c",  "action": "click",      "target": "Btn"     },
            { "id": "t",  "action": "type",       "target": "Field", "text": "hello" },
            { "id": "w",  "action": "wait",       "target": "Spinner" },
            { "id": "a",  "action": "assert",     "target": "Result"  },
            { "id": "cp", "action": "checkpoint" }
        ]))
        .unwrap();
        assert_eq!(steps.len(), 5);
    }

    #[test]
    fn workflow_create_schema_requires_text_for_type_steps() {
        let tool = super::tool_ax_workflow_create();
        let all_of = tool.input_schema["properties"]["steps"]["items"]["allOf"]
            .as_array()
            .expect("workflow step conditionals");

        let type_rule = all_of
            .iter()
            .find(|rule| rule["if"]["properties"]["action"]["const"] == "type")
            .expect("type conditional requirement");

        let required = type_rule["then"]["required"]
            .as_array()
            .expect("required array");
        assert!(required.iter().any(|value| value == "target"));
        assert!(required.iter().any(|value| value == "text"));
    }

    #[test]
    fn workflow_create_schema_bounds_retry_and_timeout_values() {
        let tool = super::tool_ax_workflow_create();
        let step_schema = &tool.input_schema["properties"]["steps"]["items"];
        let items = &step_schema["properties"];

        assert_eq!(items["max_retries"]["minimum"], 0);
        assert_eq!(items["max_retries"]["maximum"], 4294967295u64);
        assert_eq!(items["timeout_ms"]["minimum"], 0);
        assert_eq!(step_schema["additionalProperties"], false);
    }

    // -----------------------------------------------------------------------
    // call_workflow_tool dispatch
    // -----------------------------------------------------------------------

    #[test]
    fn call_workflow_tool_unknown_name_returns_none() {
        let registry = make_registry();
        let wf = make_workflows();
        let mut out = Vec::<u8>::new();
        let result =
            super::call_workflow_tool("ax_nonexistent", &json!({}), &registry, &wf, &mut out);
        assert!(result.is_none());
    }

    #[test]
    fn call_workflow_tool_recognises_all_three_names() {
        let registry = make_registry();
        let wf = make_workflows();
        let mut out = Vec::<u8>::new();
        for name in &[
            "ax_workflow_create",
            "ax_workflow_step",
            "ax_workflow_status",
        ] {
            let result = super::call_workflow_tool(name, &json!({}), &registry, &wf, &mut out);
            assert!(
                result.is_some(),
                "call_workflow_tool returned None for '{name}'"
            );
        }
    }
}
