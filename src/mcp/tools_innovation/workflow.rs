//! Workflow tracking, durable workflow execution, and workflow detection.
//!
//! Extracted from `mod.rs` to keep the tools_innovation module under 800 LOC.

use std::collections::HashMap;
use std::io::Write;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::mcp::server::WorkflowState;

// ToolCallResult is re-exported through mod.rs; access via super.
use super::ToolCallResult;

pub(super) fn handle_ax_track_workflow(args: &Value) -> ToolCallResult {
    let Some(app_name) = args["app"].as_str() else {
        return ToolCallResult::error("Missing required field: app");
    };
    let action = args["action"].as_str().unwrap_or("record");

    match action {
        "record" => handle_workflow_record(app_name, args),
        "detect" => handle_workflow_detect(args),
        "stats" => handle_workflow_stats(),
        other => ToolCallResult::error(format!(
            "Unknown action '{other}'. Expected: record, detect, stats"
        )),
    }
}

/// Record a focus transition into the global tracker.
fn handle_workflow_record(app_name: &str, args: &Value) -> ToolCallResult {
    let trigger = parse_transition_trigger(args["trigger"].as_str().unwrap_or("unknown"));

    let Ok(mut tracker) = super::WORKFLOW_TRACKER.lock() else {
        return ToolCallResult::error("Tracker mutex poisoned");
    };
    tracker.record_focus(app_name, trigger);

    ToolCallResult::ok(
        json!({
            "action":   "record",
            "recorded": true,
            "app":      app_name
        })
        .to_string(),
    )
}

/// Detect repeated workflow patterns from the accumulated transition log.
fn handle_workflow_detect(args: &Value) -> ToolCallResult {
    let min_frequency = args["min_frequency"].as_u64().unwrap_or(2) as u32;

    let Ok(tracker) = super::WORKFLOW_TRACKER.lock() else {
        return ToolCallResult::error("Tracker mutex poisoned");
    };
    let workflows = tracker.detect_workflows(min_frequency);

    let workflows_json: Vec<Value> = workflows
        .iter()
        .map(|wf| {
            let automation = crate::cross_app::CrossAppTracker::suggest_automation(wf)
                .into_iter()
                .map(|s| {
                    json!({
                        "app":         s.app,
                        "description": s.description,
                        "step_index":  s.step_index
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "name":            wf.name,
                "apps":            wf.apps,
                "frequency":       wf.frequency,
                "avg_duration_ms": wf.avg_duration_ms,
                "automation":      automation
            })
        })
        .collect();

    ToolCallResult::ok(
        json!({
            "action":    "detect",
            "workflows": workflows_json
        })
        .to_string(),
    )
}

/// Return aggregate statistics from the global tracker.
fn handle_workflow_stats() -> ToolCallResult {
    let Ok(tracker) = super::WORKFLOW_TRACKER.lock() else {
        return ToolCallResult::error("Tracker mutex poisoned");
    };
    let stats = tracker.stats();

    let top_transition = stats
        .top_transition
        .map(|(from, to)| json!({ "from": from, "to": to }));

    ToolCallResult::ok(
        json!({
            "action": "stats",
            "stats": {
                "total_transitions": stats.total_transitions,
                "distinct_apps":     stats.distinct_apps,
                "top_app":           stats.top_app,
                "top_transition":    top_transition
            }
        })
        .to_string(),
    )
}

// ---------------------------------------------------------------------------
// Resource endpoint helpers
// ---------------------------------------------------------------------------

/// Snapshot the global workflow tracker for the `axterminator://workflows` resource.
///
/// Returns a JSON object with aggregate stats and all detected workflow patterns
/// (using the default minimum frequency of 2 occurrences).
///
/// # Panics
///
/// Panics when the tracker mutex is poisoned, which only occurs if a previous
/// holder panicked while holding the lock — an unrecoverable state.
pub(crate) fn workflow_tracking_data() -> serde_json::Value {
    let tracker = super::WORKFLOW_TRACKER
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let stats = tracker.stats();
    let workflows = tracker.detect_workflows(2);

    let top_transition = stats
        .top_transition
        .map(|(from, to)| json!({ "from": from, "to": to }));

    let workflows_json: Vec<serde_json::Value> = workflows
        .iter()
        .map(|wf| {
            json!({
                "name":            wf.name,
                "apps":            wf.apps,
                "frequency":       wf.frequency,
                "avg_duration_ms": wf.avg_duration_ms,
            })
        })
        .collect();

    json!({
        "workflows_detected": workflows_json.len(),
        "workflows":          workflows_json,
        "stats": {
            "total_transitions": stats.total_transitions,
            "distinct_apps":     stats.distinct_apps,
            "top_app":           stats.top_app,
            "top_transition":    top_transition,
        },
    })
}

/// Map a trigger string to the [`TransitionTrigger`] enum.
pub(super) fn parse_transition_trigger(s: &str) -> crate::cross_app::TransitionTrigger {
    use crate::cross_app::TransitionTrigger;
    match s {
        "user_switch" => TransitionTrigger::UserSwitch,
        "automation" => TransitionTrigger::Automation,
        "notification" => TransitionTrigger::Notification,
        _ => TransitionTrigger::Unknown,
    }
}

// ---------------------------------------------------------------------------
// Workflow tool handlers
// ---------------------------------------------------------------------------

/// Handle `ax_workflow_create` — parse step definitions and store the workflow plan.
pub(super) fn handle_ax_workflow_create(
    args: &Value,
    workflows: &Arc<Mutex<HashMap<String, WorkflowState>>>,
) -> ToolCallResult {
    if let Err(error) = reject_unknown_fields(args, &["name", "steps"]) {
        return ToolCallResult::error(error);
    }

    let Some(name) = args["name"].as_str().map(str::to_string) else {
        return ToolCallResult::error("Missing required field: name");
    };

    let steps = match args.get("steps") {
        Some(steps_value) => match parse_workflow_steps(steps_value) {
            Ok(steps) => steps,
            Err(error) => return ToolCallResult::error(error),
        },
        None => Vec::new(),
    };
    let step_count = steps.len();

    let state = WorkflowState {
        steps,
        current_step: 0,
        results: Vec::new(),
        completed: false,
    };

    match workflows.lock() {
        Ok(mut guard) => {
            if guard.contains_key(&name) {
                return ToolCallResult::error(format!(
                    "Workflow '{name}' already exists — choose a unique name"
                ));
            }
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

/// Handle `ax_workflow_step` — advance the next pending step in workflow state.
///
/// Emits a progress notification before recording the step so MCP clients can
/// track how far through the workflow plan has advanced.
pub(super) fn handle_ax_workflow_step<W: Write>(
    args: &Value,
    workflows: &Arc<Mutex<HashMap<String, WorkflowState>>>,
    out: &mut W,
) -> ToolCallResult {
    if let Err(error) = reject_unknown_fields(args, &["name"]) {
        return ToolCallResult::error(error);
    }

    let Some(name) = args["name"].as_str() else {
        return ToolCallResult::error("Missing required field: name");
    };

    let mut guard = match workflows.lock() {
        Ok(g) => g,
        Err(_) => return ToolCallResult::error("Workflow mutex poisoned"),
    };

    let Some(state) = guard.get_mut(name) else {
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

    // Emit progress before recording the step. Best-effort: silently ignore
    // I/O failures so they never mask the workflow-state result.
    let _ = crate::mcp::progress::emit_progress(
        out,
        &crate::mcp::progress::next_progress_token(),
        step_index as u32 + 1,
        total_steps,
        &format!("Step {}/{total_steps}: {}", step_index + 1, step.id),
    );

    // Record workflow progress only. This surface does not execute the
    // underlying UI action or call DurableRunner here.
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
            "message":    format!("Recorded workflow step '{}'", step.id)
        })
        .to_string(),
    )
}

/// Handle `ax_workflow_status` — return the current progress of a workflow.
pub(super) fn handle_ax_workflow_status(
    args: &Value,
    workflows: &Arc<Mutex<HashMap<String, WorkflowState>>>,
) -> ToolCallResult {
    if let Err(error) = reject_unknown_fields(args, &["name"]) {
        return ToolCallResult::error(error);
    }

    let Some(name) = args["name"].as_str() else {
        return ToolCallResult::error("Missing required field: name");
    };

    let guard = match workflows.lock() {
        Ok(g) => g,
        Err(_) => return ToolCallResult::error("Workflow mutex poisoned"),
    };

    let Some(state) = guard.get(name) else {
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

/// Parse a JSON array of step objects into [`Vec<DurableStep>`].
///
/// Malformed steps fail the full workflow creation request so the stored plan
/// always matches the caller's actual intent.
pub(super) fn parse_workflow_steps(
    steps_val: &Value,
) -> Result<Vec<crate::durable_steps::DurableStep>, String> {
    let Some(arr) = steps_val.as_array() else {
        return Err("Field 'steps' must be an array".to_string());
    };
    arr.iter().map(parse_single_workflow_step).collect()
}

/// Parse one step JSON object into a [`DurableStep`].
fn parse_single_workflow_step(s: &Value) -> Result<crate::durable_steps::DurableStep, String> {
    use crate::durable_steps::{DurableStep, StepAction};

    reject_unknown_fields(
        s,
        &[
            "id",
            "action",
            "target",
            "text",
            "max_retries",
            "timeout_ms",
        ],
    )?;

    let id = match s.get("id").and_then(Value::as_str) {
        Some(value) => value.to_string(),
        None => return Err("Workflow step missing string field: id".to_string()),
    };
    let action_str = match s.get("action").and_then(Value::as_str) {
        Some(value) => value,
        None => return Err(format!("Workflow step '{id}' missing string field: action")),
    };
    let max_retries = parse_optional_workflow_u32_field(s, "max_retries", 2)?;
    let timeout_ms = parse_optional_workflow_u64_field(s, "timeout_ms", 5_000)?;

    let action = match action_str {
        "checkpoint" => StepAction::Checkpoint,
        "click" => StepAction::Click(required_workflow_step_string(s, &id, "target")?),
        "type" => StepAction::Type(
            required_workflow_step_string(s, &id, "target")?,
            required_workflow_step_string(s, &id, "text")?,
        ),
        "wait" => StepAction::Wait(required_workflow_step_string(s, &id, "target")?),
        "assert" => StepAction::Assert(required_workflow_step_string(s, &id, "target")?),
        other => return Err(format!("Workflow step '{id}' has unknown action: {other}")),
    };

    Ok(DurableStep::with_config(
        id,
        action,
        max_retries,
        timeout_ms,
    ))
}

fn reject_unknown_fields(value: &Value, allowed: &[&str]) -> Result<(), String> {
    let Some(obj) = value.as_object() else {
        return Ok(());
    };

    for key in obj.keys() {
        if !allowed.iter().any(|allowed_key| allowed_key == key) {
            return Err(format!("unknown field: {key}"));
        }
    }

    Ok(())
}

fn required_workflow_step_string(
    value: &Value,
    step_id: &str,
    field: &str,
) -> Result<String, String> {
    match value.get(field) {
        Some(Value::String(s)) => Ok(s.clone()),
        _ => Err(format!(
            "Workflow step '{step_id}' missing string field: {field}"
        )),
    }
}

fn parse_optional_workflow_u32_field(
    value: &Value,
    field: &str,
    default: u32,
) -> Result<u32, String> {
    match value.get(field) {
        None => Ok(default),
        Some(Value::Number(n)) => {
            let raw = n.as_u64().ok_or_else(|| {
                format!("Workflow field '{field}' must be a non-negative integer")
            })?;
            u32::try_from(raw).map_err(|_| format!("Workflow field '{field}' exceeds u32 range"))
        }
        _ => Err(format!(
            "Workflow field '{field}' must be a non-negative integer"
        )),
    }
}

fn parse_optional_workflow_u64_field(
    value: &Value,
    field: &str,
    default: u64,
) -> Result<u64, String> {
    match value.get(field) {
        None => Ok(default),
        Some(Value::Number(n)) => n
            .as_u64()
            .ok_or_else(|| format!("Workflow field '{field}' must be a non-negative integer")),
        _ => Err(format!(
            "Workflow field '{field}' must be a non-negative integer"
        )),
    }
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
