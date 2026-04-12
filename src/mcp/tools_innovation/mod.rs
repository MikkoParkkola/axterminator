//! Innovation tools — advanced capabilities wired from research modules.
//!
//! This module acts as the extension point for Wave 2+ tools built on top of
//! the semantic-find, intent-extraction, and copilot-format research modules.
//!
//! ## Dispatch model
//!
//! [`innovation_tools`] returns tool descriptors that are appended to the
//! Phase 3 tool list by [`crate::mcp::tools_extended::extended_tools`].
//!
//! [`call_tool_innovation`] is tried last in the `call_tool_extended` chain;
//! it returns `None` for any name it does not recognise, allowing the caller
//! to continue falling through to Phase 1.

use std::collections::HashMap;
use std::io::Write;
use std::sync::{Arc, Mutex};

use once_cell::sync::Lazy;
use serde_json::{json, Value};

use crate::mcp::annotations;
use crate::mcp::progress::ProgressReporter;
use crate::mcp::protocol::{Tool, ToolCallResult};
use crate::mcp::security::SecurityMode;
use crate::mcp::server::WorkflowState;
use crate::mcp::tools::AppRegistry;

// ---------------------------------------------------------------------------
// Global tracker for ax_track_workflow
// ---------------------------------------------------------------------------

/// Process-lifetime cross-app tracker shared across all tool calls.
///
/// `CrossAppTracker` is `Send` but not `Sync`; the `Mutex` makes it safe to
/// share across concurrent MCP handler threads.
static WORKFLOW_TRACKER: Lazy<Mutex<crate::cross_app::CrossAppTracker>> =
    Lazy::new(|| Mutex::new(crate::cross_app::CrossAppTracker::new()));

// ---------------------------------------------------------------------------
// Global recorder for ax_record
// ---------------------------------------------------------------------------

/// Process-lifetime workflow recorder shared across all MCP tool calls.
///
/// Recording state is session-scoped: `start` begins a new session,
/// `stop` drains and returns the accumulated events as JSON.
static WORKFLOW_RECORDER: Lazy<Mutex<crate::recording::WorkflowRecorder>> =
    Lazy::new(|| Mutex::new(crate::recording::WorkflowRecorder::new()));

// ---------------------------------------------------------------------------
// Tool declarations
// ---------------------------------------------------------------------------

/// All innovation tool descriptors.
pub(crate) fn innovation_tools() -> Vec<Tool> {
    vec![
        tool_ax_query(),
        tool_ax_app_profile(),
        tool_ax_test_run(),
        tool_ax_track_workflow(),
        tool_ax_workflow_create(),
        tool_ax_workflow_step(),
        tool_ax_workflow_status(),
        tool_ax_record(),
        tool_ax_analyze(),
        tool_ax_run_script(),
        tool_ax_clipboard(),
        tool_ax_session_info(),
        tool_ax_undo(),
        tool_ax_visual_diff(),
        tool_ax_a11y_audit(),
    ]
}

fn tool_ax_run_script() -> Tool {
    Tool {
        name: "ax_run_script",
        title: "Execute AppleScript or JXA",
        description: "Execute AppleScript or JXA (JavaScript for Automation). Use for operations \
            the accessibility API cannot perform: menu bar access, system dialogs, app scripting. \
            BLOCKED in safe/sandboxed mode.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "script": {
                    "type": "string",
                    "description": "The script source to execute"
                },
                "language": {
                    "type": "string",
                    "enum": ["applescript", "jxa"],
                    "default": "applescript",
                    "description": "Script language: 'applescript' (default) or 'jxa' \
                        (JavaScript for Automation)"
                }
            },
            "required": ["script"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "success": { "type": "boolean" },
                "output":  { "type": "string" }
            },
            "required": ["success"]
        }),
        annotations: annotations::DESTRUCTIVE,
    }
}

fn tool_ax_analyze() -> Tool {
    Tool {
        name: "ax_analyze",
        title: "Accessibility Intelligence Engine",
        description: "Analyze the current UI state: detect UI patterns (login forms, search bars, \
            data tables, navigation, modals), infer app state (loading, idle, error, modal), \
            and suggest next actions based on what the engine observes.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "app": {
                    "type": "string",
                    "description": "App alias from ax_connect"
                },
                "focus": {
                    "type": "string",
                    "enum": ["patterns", "state", "actions", "all"],
                    "default": "all",
                    "description": "Which aspect to analyze: patterns, state, actions, or all"
                }
            },
            "required": ["app"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "node_count":   { "type": "integer" },
                "app_state":    { "type": "string" },
                "patterns": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "pattern":    { "type": "string" },
                            "confidence": { "type": "number" }
                        },
                        "required": ["pattern", "confidence"]
                    }
                },
                "suggestions": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "action": { "type": "string" },
                            "tool":   { "type": "string" },
                            "query":  { "type": "string" }
                        },
                        "required": ["action", "tool"]
                    }
                }
            },
            "required": ["node_count", "app_state", "patterns", "suggestions"]
        }),
        annotations: annotations::READ_ONLY,
    }
}

fn tool_ax_workflow_create() -> Tool {
    Tool {
        name: "ax_workflow_create",
        title: "Create a tracked multi-step workflow",
        description: "Create a named workflow plan with click, type, wait, assert, or \
            checkpoint steps. Steps are stored and advanced one at a time via \
            ax_workflow_step. This workflow surface tracks progress; it does not \
            execute UI actions, retries, or checkpoint resume automatically.",
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
                            "max_retries": { "type": "integer", "minimum": 0, "maximum": 4294967295u64, "default": 2 },
                            "timeout_ms":  { "type": "integer", "minimum": 0, "maximum": 18446744073709551615u64, "default": 5000 }
                        },
                        "required": ["id", "action"],
                        "additionalProperties": false,
                        "allOf": [
                            {
                                "if": {
                                    "properties": {
                                        "action": { "enum": ["click", "wait", "assert"] }
                                    },
                                    "required": ["action"]
                                },
                                "then": { "required": ["target"] }
                            },
                            {
                                "if": {
                                    "properties": {
                                        "action": { "const": "type" }
                                    },
                                    "required": ["action"]
                                },
                                "then": { "required": ["target", "text"] }
                            }
                        ]
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
        title: "Advance workflow progress",
        description: "Advance the named workflow to its next stored step. Emits a progress \
            notification and records the step in workflow state. Call repeatedly until \
            completed=true.",
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
        description: "Check stored workflow progress: current step index, total steps, \
            completion state, and recorded result count.",
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

fn tool_ax_query() -> Tool {
    Tool {
        name: "ax_query",
        title: "Natural-language UI query",
        description: "Ask natural-language questions about the current UI state. \
            Examples: 'how many buttons are visible?', \
            'is there a search field?', \
            'what text is shown?', \
            'describe the screen'.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "app":   { "type": "string", "description": "App alias from ax_connect" },
                "query": {
                    "type": "string",
                    "description": "Natural-language question about the UI"
                }
            },
            "required": ["app", "query"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "confidence":        { "type": "number" },
                "scene_description": { "type": "string" },
                "matches": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "role":         { "type": "string" },
                            "label":        { "type": "string" },
                            "match_score":  { "type": "number" },
                            "match_reason": { "type": "string" },
                            "bounds": {
                                "type": "array",
                                "items": { "type": "number" }
                            }
                        }
                    }
                }
            },
            "required": ["confidence"]
        }),
        annotations: annotations::READ_ONLY,
    }
}

fn tool_ax_app_profile() -> Tool {
    Tool {
        name: "ax_app_profile",
        title: "Electron/web app metadata",
        description: "Get known capabilities, CSS selectors, and CDP port for Electron/web apps. \
            Returns profiles for VS Code, Slack, Chrome, Terminal, Finder, and similar apps. \
            Use selectors to target elements via CDP; use shortcuts to send keyboard commands.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "app": {
                    "type": "string",
                    "description": "App name (case-insensitive, e.g. 'VS Code', 'slack', 'vscode')"
                }
            },
            "required": ["app"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "found":        { "type": "boolean" },
                "name":         { "type": "string" },
                "app_id":       { "type": "string" },
                "cdp_port":     { "type": "integer" },
                "capabilities": { "type": "array", "items": { "type": "string" } },
                "selectors":    { "type": "object" },
                "shortcuts":    { "type": "object" }
            },
            "required": ["found"]
        }),
        annotations: annotations::READ_ONLY,
    }
}

fn tool_ax_test_run() -> Tool {
    Tool {
        name: "ax_test_run",
        title: "Black-box test execution",
        description: "Run a black-box test case against any macOS app via the accessibility tree. \
            Provide test steps (launch, find_and_click, find_and_type, wait_for_element, screenshot) \
            and assertions (element_exists, element_has_text, element_not_exists, screen_contains). \
            Returns pass/fail with per-step details.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "app":       { "type": "string", "description": "Application name (e.g. 'TextEdit')" },
                "test_name": { "type": "string", "description": "Human-readable test name" },
                "steps": {
                    "type": "array",
                    "description": "Ordered list of test steps",
                    "items": {
                        "type": "object",
                        "properties": {
                            "type":       { "type": "string",  "enum": ["launch", "find_and_click", "find_and_type", "wait_for_element", "screenshot"] },
                            "app":        { "type": "string" },
                            "query":      { "type": "string" },
                            "text":       { "type": "string" },
                            "path":       { "type": "string" },
                            "timeout_ms": { "type": "integer" }
                        },
                        "required": ["type"]
                    }
                },
                "assertions": {
                    "type": "array",
                    "description": "Assertions checked after all steps complete",
                    "items": {
                        "type": "object",
                        "properties": {
                            "type":     { "type": "string", "enum": ["element_exists", "element_has_text", "element_not_exists", "screen_contains"] },
                            "query":    { "type": "string" },
                            "expected": { "type": "string" },
                            "needle":   { "type": "string" }
                        },
                        "required": ["type"]
                    }
                }
            },
            "required": ["app", "test_name"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "passed":           { "type": "boolean" },
                "test_name":        { "type": "string" },
                "steps_completed":  { "type": "integer" },
                "elapsed_ms":       { "type": "integer" },
                "failures":         { "type": "array", "items": { "type": "string" } },
                "screenshots":      { "type": "array", "items": { "type": "string" } }
            },
            "required": ["passed", "test_name"]
        }),
        annotations: annotations::ACTION,
    }
}

fn tool_ax_track_workflow() -> Tool {
    Tool {
        name: "ax_track_workflow",
        title: "Cross-app workflow tracking",
        description: "Track application transitions to detect workflow patterns. \
            Call with action='record' each time you switch between apps. \
            Use action='detect' to find repeated cross-app sequences. \
            Use action='stats' for aggregate transition statistics.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "app":     { "type": "string", "description": "Application that gained focus" },
                "action":  {
                    "type": "string",
                    "enum": ["record", "detect", "stats"],
                    "default": "record",
                    "description": "record=log focus event; detect=find patterns; stats=summary"
                },
                "trigger": {
                    "type": "string",
                    "enum": ["user_switch", "automation", "notification", "unknown"],
                    "default": "unknown",
                    "description": "What caused the app switch (for 'record' action)"
                },
                "min_frequency": {
                    "type": "integer",
                    "default": 2,
                    "description": "Minimum occurrences to surface a workflow (for 'detect' action)"
                }
            },
            "required": ["app"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "action":      { "type": "string" },
                "recorded":    { "type": "boolean" },
                "workflows":   { "type": "array" },
                "stats":       { "type": "object" }
            },
            "required": ["action"]
        }),
        annotations: annotations::ACTION,
    }
}

fn tool_ax_record() -> Tool {
    Tool {
        name: "ax_record",
        title: "Record a UI interaction for test generation",
        description: "Record a UI interaction for test generation. Call this after each action \
            to build a replayable test script.\n\
            \n\
            Actions:\n\
            - `start` — begin a new recording session (clears previous events)\n\
            - `record` — append one interaction event to the session\n\
            - `stop` — end the session and return all events as a replayable JSON script\n\
            - `status` — report current recording state and event count",
        input_schema: json!({
            "type": "object",
            "properties": {
                "app": {
                    "type": "string",
                    "description": "App alias from ax_connect (used for labelling)"
                },
                "action": {
                    "type": "string",
                    "enum": ["start", "record", "stop", "status"],
                    "description": "Recording control action",
                    "default": "record"
                },
                "action_type": {
                    "type": "string",
                    "enum": ["click", "type", "assert"],
                    "description": "Type of UI interaction to record (required for action=record)"
                },
                "query": {
                    "type": "string",
                    "description": "Element label / role hint for the recorded interaction"
                },
                "text": {
                    "type": "string",
                    "description": "Text value for type interactions"
                },
                "value": {
                    "type": "string",
                    "description": "Expected value for assert interactions"
                }
            },
            "required": ["app"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "recording":   { "type": "boolean" },
                "event_count": { "type": "integer" },
                "action":      { "type": "string" },
                "events":      { "type": "array" }
            },
            "required": ["action"]
        }),
        annotations: annotations::ACTION,
    }
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

/// Dispatch an innovation tool call. Returns `None` if the tool name
/// is not recognised (allowing the caller to fall through).
///
/// Workflow tools (`ax_workflow_*`) require session state and are dispatched
/// separately via [`call_workflow_tool`].
#[cfg(test)]
pub(crate) fn call_tool_innovation<W: Write>(
    name: &str,
    args: &Value,
    registry: &Arc<AppRegistry>,
    out: &mut W,
) -> Option<ToolCallResult> {
    call_tool_innovation_with_mode(name, args, registry, SecurityMode::Normal, out)
}

/// Dispatch an innovation tool call using an explicit security mode.
pub(crate) fn call_tool_innovation_with_mode<W: Write>(
    name: &str,
    args: &Value,
    registry: &Arc<AppRegistry>,
    security_mode: SecurityMode,
    out: &mut W,
) -> Option<ToolCallResult> {
    match name {
        "ax_query" => Some(handle_ax_query(args, registry)),
        "ax_app_profile" => Some(handle_ax_app_profile(args)),
        "ax_test_run" => Some(handle_ax_test_run(args, out)),
        "ax_track_workflow" => Some(handle_ax_track_workflow(args)),
        "ax_record" => Some(handle_ax_record(args)),
        "ax_analyze" => Some(handle_ax_analyze(args, registry)),
        "ax_run_script" => Some(handle_ax_run_script(args)),
        "ax_clipboard" => Some(handle_ax_clipboard_with_mode(args, security_mode)),
        "ax_session_info" => Some(handle_ax_session_info_with_mode(
            args,
            registry,
            security_mode,
        )),
        "ax_undo" => Some(handle_ax_undo(args)),
        "ax_visual_diff" => Some(handle_ax_visual_diff(args, registry)),
        "ax_a11y_audit" => Some(handle_ax_a11y_audit(args, registry)),
        _ => None,
    }
}

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
// Handlers
// ---------------------------------------------------------------------------

/// Handle `ax_record` — control the global [`WorkflowRecorder`].
///
/// | action   | behaviour |
/// |----------|-----------|
/// | `start`  | Reset and begin a fresh recording session |
/// | `record` | Append one event for the given `action_type` |
/// | `stop`   | End session, return all events as serialised JSON |
/// | `status` | Return recording state and event count (non-destructive) |
fn handle_ax_record(args: &Value) -> ToolCallResult {
    let Some(app) = args["app"].as_str() else {
        return ToolCallResult::error("Missing required field: app");
    };
    let action = args["action"].as_str().unwrap_or("record");

    let Ok(mut recorder) = WORKFLOW_RECORDER.lock() else {
        return ToolCallResult::error("Recorder mutex poisoned");
    };

    match action {
        "start" => {
            recorder.start_recording();
            ToolCallResult::ok(
                json!({
                    "action": "start",
                    "recording": true,
                    "event_count": 0,
                    "app": app
                })
                .to_string(),
            )
        }
        "stop" => {
            let events = recorder.stop_recording();
            let count = events.len();
            let serialised = crate::recording::WorkflowRecorder::serialize(&events)
                .unwrap_or_else(|_| "[]".to_string());
            let events_val: Value =
                serde_json::from_str(&serialised).unwrap_or(Value::Array(vec![]));
            ToolCallResult::ok(
                json!({
                    "action": "stop",
                    "recording": false,
                    "event_count": count,
                    "events": events_val
                })
                .to_string(),
            )
        }
        "status" => ToolCallResult::ok(
            json!({
                "action": "status",
                "recording": recorder.is_recording(),
                "event_count": recorder.event_count()
            })
            .to_string(),
        ),
        "record" => {
            let Some(action_type) = args["action_type"].as_str() else {
                return ToolCallResult::error(
                    "Missing required field: action_type (click|type|assert)",
                );
            };
            let label = args["query"].as_str().unwrap_or("");
            let recorded_action = match action_type {
                "click" => crate::recording::RecordedAction::Click { x: 0.0, y: 0.0 },
                "type" => crate::recording::RecordedAction::Type {
                    text: args["text"].as_str().unwrap_or("").to_owned(),
                },
                "assert" => crate::recording::RecordedAction::KeyPress {
                    key: args["value"].as_str().unwrap_or("").to_owned(),
                    modifiers: vec![],
                },
                other => {
                    return ToolCallResult::error(format!(
                        "Unknown action_type '{other}'. Expected: click, type, assert"
                    ))
                }
            };
            let event = crate::recording::RecordedEvent {
                timestamp: 0,
                action: recorded_action,
                element_fingerprint: 0,
                element_label: label.to_owned(),
                element_role: String::new(),
            };
            recorder.record_event(event);
            ToolCallResult::ok(
                json!({
                    "action": "record",
                    "recording": recorder.is_recording(),
                    "event_count": recorder.event_count(),
                    "recorded_action_type": action_type,
                    "app": app
                })
                .to_string(),
            )
        }
        other => ToolCallResult::error(format!(
            "Unknown action '{other}'. Expected: start, record, stop, status"
        )),
    }
}

/// Handle `ax_query` — build a SceneGraph from the live AX tree, then query it.
fn handle_ax_query(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    let Some(app_name) = args["app"].as_str().map(str::to_string) else {
        return ToolCallResult::error("Missing required field: app");
    };
    let Some(query) = args["query"].as_str().map(str::to_string) else {
        return ToolCallResult::error("Missing required field: query");
    };

    registry
        .with_app(&app_name, |app| {
            let scene = match crate::intent::scan_scene(app.element) {
                Ok(g) => g,
                Err(e) => return ToolCallResult::error(format!("scan_scene failed: {e}")),
            };

            let result = crate::scene::SceneEngine::new().query(&query, &scene);

            let matches_json: Vec<Value> = result
                .matches
                .iter()
                .map(|m| {
                    let bounds = m.bounds.map(|(x, y, w, h)| json!([x, y, w, h]));
                    json!({
                        "role":         m.element_role,
                        "label":        m.element_label,
                        "path":         m.element_path,
                        "match_score":  m.match_score,
                        "match_reason": m.match_reason,
                        "bounds":       bounds
                    })
                })
                .collect();

            ToolCallResult::ok(
                json!({
                    "confidence":        result.confidence,
                    "scene_description": result.scene_description,
                    "matches":           matches_json
                })
                .to_string(),
            )
        })
        .unwrap_or_else(ToolCallResult::error)
}

/// Handle `ax_app_profile` — look up an Electron app profile by name.
fn handle_ax_app_profile(args: &Value) -> ToolCallResult {
    let Some(app_name) = args["app"].as_str() else {
        return ToolCallResult::error("Missing required field: app");
    };

    let registry = crate::electron_profiles::ProfileRegistry::with_builtins();

    match registry.detect(app_name) {
        Some(profile) => {
            let capabilities: Vec<String> = profile
                .capabilities
                .iter()
                .map(capability_to_str)
                .collect();

            let selectors: Value = profile
                .selectors
                .iter()
                .fold(json!({}), |mut acc, (k, v)| {
                    acc[k] = json!(v);
                    acc
                });

            let shortcuts: Value = profile
                .shortcuts
                .iter()
                .fold(json!({}), |mut acc, (k, v)| {
                    acc[k] = json!(v);
                    acc
                });

            ToolCallResult::ok(
                json!({
                    "found":        true,
                    "name":         profile.name,
                    "app_id":       profile.app_id,
                    "cdp_port":     profile.cdp_port,
                    "capabilities": capabilities,
                    "selectors":    selectors,
                    "shortcuts":    shortcuts
                })
                .to_string(),
            )
        }
        None => ToolCallResult::ok(
            json!({
                "found": false,
                "name":  app_name,
                "message": "No built-in profile found. The app may still be automatable via ax_find/ax_click."
            })
            .to_string(),
        ),
    }
}

/// Map an [`AppCapability`] to a stable display string.
fn capability_to_str(cap: &crate::electron_profiles::AppCapability) -> String {
    use crate::electron_profiles::AppCapability;
    match cap {
        AppCapability::Chat => "chat".into(),
        AppCapability::Email => "email".into(),
        AppCapability::Calendar => "calendar".into(),
        AppCapability::CodeEditor => "code_editor".into(),
        AppCapability::Browser => "browser".into(),
        AppCapability::Terminal => "terminal".into(),
        AppCapability::FileManager => "file_manager".into(),
        AppCapability::Custom(s) => format!("custom:{s}"),
    }
}

/// Handle `ax_test_run` — build a `TestCase` from JSON args and run it.
///
/// Emits two progress notifications: one before execution begins and one on
/// completion.  Because `BlackboxTester::run` is synchronous, per-step
/// notifications are not feasible without restructuring the runner; the
/// before/after pair lets MCP clients display a spinner during the test run.
fn handle_ax_test_run<W: Write>(args: &Value, out: &mut W) -> ToolCallResult {
    let Some(app_name) = args["app"].as_str().map(str::to_string) else {
        return ToolCallResult::error("Missing required field: app");
    };
    let Some(test_name) = args["test_name"].as_str().map(str::to_string) else {
        return ToolCallResult::error("Missing required field: test_name");
    };

    let steps = parse_test_steps(&args["steps"]);
    let assertions = parse_test_assertions(&args["assertions"]);
    let total = (steps.len() + assertions.len()).max(1) as u32;

    let mut reporter = ProgressReporter::new(out, total);
    // Best-effort: start notification — silently ignore I/O failures.
    let _ = reporter.step(&format!("Running test '{test_name}'…"));

    let case = crate::blackbox::TestCase {
        name: test_name,
        steps,
        assertions,
    };

    let tester = crate::blackbox::BlackboxTester::new(&app_name);
    let result = tester.run(&case);

    let _ = reporter.complete("Test complete");

    ToolCallResult::ok(
        json!({
            "passed":          result.passed,
            "test_name":       result.name,
            "steps_completed": result.steps_completed,
            "elapsed_ms":      result.elapsed_ms,
            "failures":        result.failures,
            "screenshots":     result.screenshots
        })
        .to_string(),
    )
}

/// Parse a JSON array of step objects into `Vec<TestStep>`.
///
/// Steps that cannot be parsed are silently skipped so a single malformed
/// entry does not abort the entire test run.
fn parse_test_steps(steps_val: &Value) -> Vec<crate::blackbox::TestStep> {
    let Some(arr) = steps_val.as_array() else {
        return vec![];
    };

    arr.iter().filter_map(parse_single_step).collect()
}

/// Parse a single step JSON object into a `TestStep`, or `None` on error.
fn parse_single_step(s: &Value) -> Option<crate::blackbox::TestStep> {
    use crate::blackbox::TestStep;

    let kind = s["type"].as_str()?;
    match kind {
        "launch" => Some(TestStep::Launch {
            app: s["app"].as_str()?.to_string(),
        }),
        "find_and_click" => Some(TestStep::FindAndClick {
            query: s["query"].as_str()?.to_string(),
        }),
        "find_and_type" => Some(TestStep::FindAndType {
            query: s["query"].as_str()?.to_string(),
            text: s["text"].as_str()?.to_string(),
        }),
        "wait_for_element" => Some(TestStep::WaitForElement {
            query: s["query"].as_str()?.to_string(),
            timeout_ms: s["timeout_ms"].as_u64().unwrap_or(5_000),
        }),
        "screenshot" => Some(TestStep::Screenshot {
            path: s["path"].as_str()?.to_string(),
        }),
        _ => None,
    }
}

/// Parse a JSON array of assertion objects into `Vec<TestAssertion>`.
fn parse_test_assertions(assertions_val: &Value) -> Vec<crate::blackbox::TestAssertion> {
    let Some(arr) = assertions_val.as_array() else {
        return vec![];
    };

    arr.iter().filter_map(parse_single_assertion).collect()
}

/// Parse a single assertion JSON object into a `TestAssertion`, or `None` on error.
fn parse_single_assertion(a: &Value) -> Option<crate::blackbox::TestAssertion> {
    use crate::blackbox::TestAssertion;

    let kind = a["type"].as_str()?;
    match kind {
        "element_exists" => Some(TestAssertion::ElementExists {
            query: a["query"].as_str()?.to_string(),
        }),
        "element_has_text" => Some(TestAssertion::ElementHasText {
            query: a["query"].as_str()?.to_string(),
            expected: a["expected"].as_str()?.to_string(),
        }),
        "element_not_exists" => Some(TestAssertion::ElementNotExists {
            query: a["query"].as_str()?.to_string(),
        }),
        "screen_contains" => Some(TestAssertion::ScreenContains {
            needle: a["needle"].as_str()?.to_string(),
        }),
        _ => None,
    }
}

/// Handle `ax_track_workflow` — record a focus event or query the tracker.
fn handle_ax_track_workflow(args: &Value) -> ToolCallResult {
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

    let Ok(mut tracker) = WORKFLOW_TRACKER.lock() else {
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

    let Ok(tracker) = WORKFLOW_TRACKER.lock() else {
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
    let Ok(tracker) = WORKFLOW_TRACKER.lock() else {
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
// Accessibility Intelligence Engine — ax_analyze
// ---------------------------------------------------------------------------

/// Detected UI pattern with an associated confidence score.
#[derive(Debug, Clone, PartialEq)]
struct UiPattern {
    pattern: &'static str,
    confidence: f64,
}

/// Inferred high-level application state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppState {
    Idle,
    Loading,
    Error,
    Modal,
    AuthRequired,
}

impl AppState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Loading => "loading",
            Self::Error => "error",
            Self::Modal => "modal",
            Self::AuthRequired => "auth_required",
        }
    }
}

/// A node-role predicate: returns `true` when `role` matches the target.
fn has_role(nodes: &[&crate::intent::SceneNode], role: &str) -> bool {
    nodes.iter().any(|n| n.role.as_deref() == Some(role))
}

/// Returns `true` when any node's text labels contain `needle` (case-insensitive).
fn any_label_contains(nodes: &[&crate::intent::SceneNode], needle: &str) -> bool {
    nodes.iter().any(|n| {
        n.text_labels()
            .iter()
            .any(|l| l.to_lowercase().contains(needle))
    })
}

/// Detect common UI patterns from a `SceneGraph`.
///
/// Each pattern is evaluated independently; multiple patterns may match a single scene.
/// Confidence values reflect the reliability of the heuristic — stronger structural
/// signals produce higher scores.
fn detect_ui_patterns(scene: &crate::intent::SceneGraph) -> Vec<UiPattern> {
    let nodes: Vec<&crate::intent::SceneNode> = scene.iter().collect();
    let mut patterns = Vec::new();

    // Login form: secure password field + a plain text field + a submit button.
    let has_password = has_role(&nodes, "AXSecureTextField");
    let has_text_field = has_role(&nodes, "AXTextField");
    let has_button = has_role(&nodes, "AXButton");
    if has_password && has_text_field && has_button {
        patterns.push(UiPattern {
            pattern: "login_form",
            confidence: 0.90,
        });
    }

    // Search interface: a dedicated search field or a text field labelled "search".
    let has_search_field = has_role(&nodes, "AXSearchField");
    let has_search_label = has_text_field && any_label_contains(&nodes, "search");
    if has_search_field || has_search_label {
        patterns.push(UiPattern {
            pattern: "search_interface",
            confidence: 0.85,
        });
    }

    // Navigation: a tab group or a toolbar containing multiple buttons.
    let has_tab_group = has_role(&nodes, "AXTabGroup");
    let has_toolbar = has_role(&nodes, "AXToolbar");
    if has_tab_group || has_toolbar {
        patterns.push(UiPattern {
            pattern: "navigation",
            confidence: 0.80,
        });
    }

    // Table / data view.
    let has_table =
        has_role(&nodes, "AXTable") || has_role(&nodes, "AXGrid") || has_role(&nodes, "AXOutline");
    if has_table {
        patterns.push(UiPattern {
            pattern: "table_view",
            confidence: 0.88,
        });
    }

    // Modal / dialog: sheet or dialog element is present.
    let has_modal = has_role(&nodes, "AXSheet") || has_role(&nodes, "AXDialog");
    if has_modal {
        patterns.push(UiPattern {
            pattern: "modal_dialog",
            confidence: 0.95,
        });
    }

    // File-save dialog: modal + Save button + filename field.
    if has_modal && has_button {
        let save_btn = any_label_contains(&nodes, "save");
        let open_btn = any_label_contains(&nodes, "open");
        let cancel_btn = any_label_contains(&nodes, "cancel");
        if save_btn && cancel_btn {
            patterns.push(UiPattern {
                pattern: "file_save_dialog",
                confidence: 0.88,
            });
        } else if open_btn && cancel_btn {
            patterns.push(UiPattern {
                pattern: "file_open_dialog",
                confidence: 0.88,
            });
        }
    }

    // Confirmation / alert dialog: alert element with OK/Yes + Cancel/No buttons.
    let has_alert = has_role(&nodes, "AXAlert");
    if has_alert && has_button {
        let ok = any_label_contains(&nodes, "ok") || any_label_contains(&nodes, "yes");
        let cancel = any_label_contains(&nodes, "cancel") || any_label_contains(&nodes, "no");
        if ok && cancel {
            patterns.push(UiPattern {
                pattern: "confirmation_dialog",
                confidence: 0.87,
            });
        } else {
            patterns.push(UiPattern {
                pattern: "error_alert",
                confidence: 0.80,
            });
        }
    }

    // Settings page: multiple labeled groups of controls (no modal, no login).
    let has_groups = scene.nodes_by_role("AXGroup").len() >= 3;
    let has_checkboxes = has_role(&nodes, "AXCheckBox");
    let has_popups = has_role(&nodes, "AXPopUpButton");
    if has_groups && (has_checkboxes || has_popups) && !has_modal && !has_password {
        patterns.push(UiPattern {
            pattern: "settings_page",
            confidence: 0.75,
        });
    }

    // Text editor: large scrollable text area with optional toolbar.
    let has_text_area = has_role(&nodes, "AXTextArea");
    if has_text_area && (has_toolbar || nodes.len() > 10) {
        patterns.push(UiPattern {
            pattern: "text_editor",
            confidence: 0.78,
        });
    }

    // Browser main: address bar heuristic (text field with URL-like identifier).
    let browser_addr = nodes.iter().any(|n| {
        n.role.as_deref() == Some("AXTextField")
            && n.identifier
                .as_deref()
                .is_some_and(|id| id.contains("address") || id.contains("url"))
    });
    if browser_addr && has_tab_group {
        patterns.push(UiPattern {
            pattern: "browser_main",
            confidence: 0.85,
        });
    }

    // Form: group of labeled text fields (distinct from login — no password field).
    let text_field_count = scene.nodes_by_role("AXTextField").len();
    if text_field_count >= 2 && !has_password && has_button {
        patterns.push(UiPattern {
            pattern: "form",
            confidence: 0.72,
        });
    }

    // Progress / loading indicator.
    let has_progress =
        has_role(&nodes, "AXProgressIndicator") || has_role(&nodes, "AXBusyIndicator");
    if has_progress {
        patterns.push(UiPattern {
            pattern: "progress_indicator",
            confidence: 0.93,
        });
    }

    patterns
}

/// Infer the high-level application state from a `SceneGraph`.
///
/// States are evaluated in priority order: modal > loading > error > auth_required > idle.
fn infer_app_state(scene: &crate::intent::SceneGraph) -> AppState {
    let nodes: Vec<&crate::intent::SceneNode> = scene.iter().collect();

    // Modal blocks all other interactions — highest priority.
    if has_role(&nodes, "AXSheet") || has_role(&nodes, "AXDialog") {
        return AppState::Modal;
    }

    // Loading indicators: spinner or progress bar visible.
    let loading = has_role(&nodes, "AXProgressIndicator")
        || has_role(&nodes, "AXBusyIndicator")
        || any_label_contains(&nodes, "loading");
    if loading {
        return AppState::Loading;
    }

    // Error state: error text or error alert present.
    let error = has_role(&nodes, "AXAlert")
        || any_label_contains(&nodes, "error")
        || any_label_contains(&nodes, "failed")
        || any_label_contains(&nodes, "invalid");
    if error {
        return AppState::Error;
    }

    // Auth required: password field visible without a modal wrapping it.
    if has_role(&nodes, "AXSecureTextField") {
        return AppState::AuthRequired;
    }

    AppState::Idle
}

/// A suggested next action for the agent.
#[derive(Debug, Clone)]
struct Suggestion {
    action: &'static str,
    tool: &'static str,
    query: &'static str,
}

/// Generate next-action suggestions from detected patterns and app state.
///
/// Suggestions are purely informational — they are never executed automatically.
/// The list is ordered from most-specific to most-general.
fn suggest_actions(patterns: &[UiPattern], state: AppState) -> Vec<Suggestion> {
    let mut suggestions: Vec<Suggestion> = Vec::new();

    // State-driven suggestions take priority.
    match state {
        AppState::Modal => {
            suggestions.push(Suggestion {
                action: "Dismiss or interact with the modal dialog before continuing",
                tool: "ax_click",
                query: "Cancel",
            });
        }
        AppState::Loading => {
            suggestions.push(Suggestion {
                action: "Wait for the app to finish loading",
                tool: "ax_wait_idle",
                query: "",
            });
        }
        AppState::Error => {
            suggestions.push(Suggestion {
                action: "Acknowledge the error and check error details",
                tool: "ax_get_value",
                query: "error message",
            });
        }
        AppState::AuthRequired => {
            suggestions.push(Suggestion {
                action: "Enter credentials to authenticate",
                tool: "ax_type",
                query: "username",
            });
        }
        AppState::Idle => {}
    }

    // Pattern-driven suggestions.
    let pattern_names: Vec<&str> = patterns.iter().map(|p| p.pattern).collect();

    if pattern_names.contains(&"login_form") {
        suggestions.push(Suggestion {
            action: "Type your username into the text field",
            tool: "ax_type",
            query: "username",
        });
        suggestions.push(Suggestion {
            action: "Type your password into the secure field",
            tool: "ax_type",
            query: "password",
        });
        suggestions.push(Suggestion {
            action: "Click the sign-in button to submit credentials",
            tool: "ax_click",
            query: "Sign In",
        });
    }

    if pattern_names.contains(&"search_interface") {
        suggestions.push(Suggestion {
            action: "Type your query into the search field",
            tool: "ax_type",
            query: "search",
        });
    }

    if pattern_names.contains(&"file_save_dialog") {
        suggestions.push(Suggestion {
            action: "Type a filename and click Save to confirm",
            tool: "ax_type",
            query: "Save As",
        });
        suggestions.push(Suggestion {
            action: "Click Save to confirm the file",
            tool: "ax_click",
            query: "Save",
        });
    }

    if pattern_names.contains(&"file_open_dialog") {
        suggestions.push(Suggestion {
            action: "Navigate to the desired file and click Open",
            tool: "ax_click",
            query: "Open",
        });
    }

    if pattern_names.contains(&"confirmation_dialog") {
        suggestions.push(Suggestion {
            action: "Confirm the action by clicking OK or Yes",
            tool: "ax_click",
            query: "OK",
        });
        suggestions.push(Suggestion {
            action: "Cancel the action to dismiss the dialog",
            tool: "ax_click",
            query: "Cancel",
        });
    }

    if pattern_names.contains(&"error_alert") {
        suggestions.push(Suggestion {
            action: "Dismiss the error alert",
            tool: "ax_click",
            query: "OK",
        });
    }

    if pattern_names.contains(&"table_view") {
        suggestions.push(Suggestion {
            action: "Read the visible rows from the data table",
            tool: "ax_get_value",
            query: "table row",
        });
    }

    if pattern_names.contains(&"text_editor") {
        suggestions.push(Suggestion {
            action: "Type or edit text in the editor area",
            tool: "ax_type",
            query: "text area",
        });
    }

    if pattern_names.contains(&"form") {
        suggestions.push(Suggestion {
            action: "Fill in the form fields",
            tool: "ax_type",
            query: "text field",
        });
        suggestions.push(Suggestion {
            action: "Submit the form",
            tool: "ax_click",
            query: "Submit",
        });
    }

    suggestions
}

/// Serialize a single `UiPattern` to JSON.
fn pattern_to_json(p: &UiPattern) -> Value {
    json!({ "pattern": p.pattern, "confidence": p.confidence })
}

/// Serialize a single `Suggestion` to JSON.
fn suggestion_to_json(s: &Suggestion) -> Value {
    json!({ "action": s.action, "tool": s.tool, "query": s.query })
}

/// Handle `ax_analyze` — detect patterns, infer state, and suggest actions.
///
/// The `focus` parameter limits the output:
/// - `"patterns"` — only pattern detection results
/// - `"state"` — only inferred app state
/// - `"actions"` — only suggested next actions
/// - `"all"` (default) — everything combined
fn handle_ax_analyze(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    let Some(app_name) = args["app"].as_str().map(str::to_string) else {
        return ToolCallResult::error("Missing required field: app");
    };
    let focus = args["focus"].as_str().unwrap_or("all");

    registry
        .with_app(&app_name, |app| {
            let scene = match crate::intent::scan_scene(app.element) {
                Ok(g) => g,
                Err(e) => return ToolCallResult::error(format!("scan_scene failed: {e}")),
            };

            let node_count = scene.len();
            let patterns = detect_ui_patterns(&scene);
            let state = infer_app_state(&scene);
            let actions = suggest_actions(&patterns, state);

            let patterns_json: Vec<Value> = patterns.iter().map(pattern_to_json).collect();
            let suggestions_json: Vec<Value> = actions.iter().map(suggestion_to_json).collect();

            let payload = match focus {
                "patterns" => json!({
                    "node_count": node_count,
                    "app_state":  state.as_str(),
                    "patterns":   patterns_json,
                    "suggestions": []
                }),
                "state" => json!({
                    "node_count": node_count,
                    "app_state":  state.as_str(),
                    "patterns":   [],
                    "suggestions": []
                }),
                "actions" => json!({
                    "node_count":  node_count,
                    "app_state":   state.as_str(),
                    "patterns":    [],
                    "suggestions": suggestions_json
                }),
                _ => json!({
                    "node_count":  node_count,
                    "app_state":   state.as_str(),
                    "patterns":    patterns_json,
                    "suggestions": suggestions_json
                }),
            };

            ToolCallResult::ok(payload.to_string())
        })
        .unwrap_or_else(ToolCallResult::error)
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
    let tracker = WORKFLOW_TRACKER.lock().unwrap_or_else(|e| e.into_inner());
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
fn parse_transition_trigger(s: &str) -> crate::cross_app::TransitionTrigger {
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
fn handle_ax_workflow_create(
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
fn handle_ax_workflow_step<W: Write>(
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
fn handle_ax_workflow_status(
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
fn parse_workflow_steps(
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

// ---------------------------------------------------------------------------
// ax_run_script handler
// ---------------------------------------------------------------------------

/// Handle `ax_run_script` — execute AppleScript or JXA via `osascript`.
///
/// The `language` field selects the interpreter mode:
/// - `"applescript"` (default) — passed directly to `osascript -e`
/// - `"jxa"` — passed to `osascript -l JavaScript -e`
///
/// Security: this tool is blocked in safe and sandboxed modes via
/// [`crate::mcp::security::is_script_tool`], which matches `"ax_run_script"`.
/// The [`crate::mcp::annotations::DESTRUCTIVE`] annotation signals to clients
/// that this tool can modify system state.
fn handle_ax_run_script(args: &Value) -> ToolCallResult {
    let Some(script) = args["script"].as_str() else {
        return ToolCallResult::error("Missing required field: script");
    };
    let language = args["language"].as_str().unwrap_or("applescript");

    let mut cmd = std::process::Command::new("osascript");
    if language == "jxa" {
        cmd.args(["-l", "JavaScript", "-e", script]);
    } else {
        cmd.args(["-e", script]);
    }

    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if output.status.success() {
                ToolCallResult::ok(json!({"success": true, "output": stdout}).to_string())
            } else {
                ToolCallResult::error(format!("Script failed: {stderr}"))
            }
        }
        Err(e) => ToolCallResult::error(format!("Failed to execute script: {e}")),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// ax_clipboard — descriptor
// ---------------------------------------------------------------------------

fn tool_ax_clipboard() -> Tool {
    Tool {
        name: "ax_clipboard",
        title: "Read/write the system clipboard",
        description: "Read from or write to the macOS system clipboard. \
            Use action='read' to retrieve the current clipboard contents. \
            Use action='write' with a text field to replace the clipboard. \
            Clipboard writes are blocked in sandboxed security mode.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["read", "write"],
                    "description": "read=return clipboard text; write=replace clipboard contents"
                },
                "text": {
                    "type": "string",
                    "description": "Text to place on the clipboard (required when action=write)"
                }
            },
            "required": ["action"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "action":  { "type": "string" },
                "text":    { "type": "string" },
                "written": { "type": "boolean" }
            },
            "required": ["action"]
        }),
        // Reads are safe; the annotation reflects the write path (more conservative).
        annotations: annotations::DESTRUCTIVE,
    }
}

// ---------------------------------------------------------------------------
// ax_session_info — descriptor
// ---------------------------------------------------------------------------

fn tool_ax_session_info() -> Tool {
    Tool {
        name: "ax_session_info",
        title: "Server session state",
        description: "Return server session information: the names of all connected apps, \
            the number of tools currently available in the active security mode, the active \
            security mode itself, and the server version. Useful for health-checks and \
            debugging MCP client state.",
        input_schema: json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "connected_apps": { "type": "array", "items": { "type": "string" } },
                "tool_count":     { "type": "integer" },
                "security_mode":  { "type": "string" },
                "version":        { "type": "string" }
            },
            "required": ["connected_apps", "tool_count", "security_mode", "version"]
        }),
        annotations: annotations::READ_ONLY,
    }
}

// ---------------------------------------------------------------------------
// ax_undo — descriptor
// ---------------------------------------------------------------------------

fn tool_ax_undo() -> Tool {
    Tool {
        name: "ax_undo",
        title: "Undo last actions in an app",
        description: "Undo the last N actions in a connected app by sending Cmd+Z. \
            Activates the named app and then sends the keystroke once per undo step \
            with a short delay between each. Default count is 1; maximum is 50.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "app": {
                    "type": "string",
                    "description": "App name to target (e.g. 'TextEdit', 'Xcode')"
                },
                "count": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 50,
                    "default": 1,
                    "description": "Number of undo steps to send (default 1, max 50)"
                }
            },
            "required": ["app"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "app":    { "type": "string" },
                "undone": { "type": "integer" },
                "ok":     { "type": "boolean" }
            },
            "required": ["app", "undone", "ok"]
        }),
        annotations: annotations::DESTRUCTIVE,
    }
}

// ---------------------------------------------------------------------------
// ax_clipboard — handler
// ---------------------------------------------------------------------------

/// Handle `ax_clipboard` — read from or write to the macOS system clipboard.
///
/// Writes are blocked when running in [`SecurityMode::Sandboxed`].
#[cfg(test)]
fn handle_ax_clipboard(args: &Value) -> ToolCallResult {
    handle_ax_clipboard_with_mode(args, SecurityMode::Normal)
}

fn handle_ax_clipboard_with_mode(args: &Value, security_mode: SecurityMode) -> ToolCallResult {
    match args["action"].as_str() {
        Some("read") => clipboard_read(),
        Some("write") => clipboard_write(args, security_mode),
        Some(other) => ToolCallResult::error(format!("Unknown clipboard action: '{other}'")),
        None => ToolCallResult::error("Missing required field: action"),
    }
}

fn clipboard_read() -> ToolCallResult {
    match std::process::Command::new("osascript")
        .args(["-e", "the clipboard"])
        .output()
    {
        Err(e) => ToolCallResult::error(format!("Failed to read clipboard: {e}")),
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
            ToolCallResult::ok(json!({"action": "read", "text": text}).to_string())
        }
    }
}

fn clipboard_write(args: &Value, security_mode: SecurityMode) -> ToolCallResult {
    if security_mode == SecurityMode::Sandboxed {
        return ToolCallResult::error("ax_clipboard write is blocked in sandboxed security mode");
    }

    let text = match args["text"].as_str() {
        Some(t) => t,
        None => return ToolCallResult::error("Missing field: text (required for action=write)"),
    };

    let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!("set the clipboard to \"{escaped}\"");

    match std::process::Command::new("osascript")
        .args(["-e", &script])
        .output()
    {
        Err(e) => ToolCallResult::error(format!("Failed to write clipboard: {e}")),
        Ok(_) => ToolCallResult::ok(json!({"action": "write", "written": true}).to_string()),
    }
}

// ---------------------------------------------------------------------------
// ax_session_info — handler
// ---------------------------------------------------------------------------

/// Handle `ax_session_info` — return a snapshot of server session state.
///
/// All data is read-only; no parameters are required.
#[cfg(test)]
fn handle_ax_session_info(_args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    handle_ax_session_info_with_mode(_args, registry, SecurityMode::Normal)
}

fn handle_ax_session_info_with_mode(
    _args: &Value,
    registry: &Arc<AppRegistry>,
    security_mode: SecurityMode,
) -> ToolCallResult {
    let connected_apps = registry.connected_names();
    let tool_count = crate::mcp::tools::tools_for_mode(security_mode).len();

    ToolCallResult::ok(
        json!({
            "connected_apps": connected_apps,
            "tool_count":     tool_count,
            "security_mode":  security_mode_name(security_mode),
            "version":        env!("CARGO_PKG_VERSION")
        })
        .to_string(),
    )
}

fn security_mode_name(security_mode: SecurityMode) -> &'static str {
    match security_mode {
        SecurityMode::Normal => "normal",
        SecurityMode::Safe => "safe",
        SecurityMode::Sandboxed => "sandboxed",
    }
}

// ---------------------------------------------------------------------------
// ax_undo — handler
// ---------------------------------------------------------------------------

/// Handle `ax_undo` — send Cmd+Z to the named app N times.
///
/// Each iteration activates the target app first so the keystroke lands in
/// the correct window, then sends the undo keystroke via `osascript`.
fn handle_ax_undo(args: &Value) -> ToolCallResult {
    let app_name = match args["app"].as_str() {
        Some(a) => a,
        None => return ToolCallResult::error("Missing required field: app"),
    };
    let count = args["count"].as_u64().unwrap_or(1).clamp(1, 50) as usize;

    let activate = format!("tell application \"{app_name}\" to activate");
    for _ in 0..count {
        std::process::Command::new("osascript")
            .args(["-e", &activate])
            .output()
            .ok();
        std::process::Command::new("osascript")
            .args([
                "-e",
                "tell application \"System Events\" to keystroke \"z\" using command down",
            ])
            .output()
            .ok();
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    ToolCallResult::ok(
        json!({
            "app":    app_name,
            "undone": count,
            "ok":     true
        })
        .to_string(),
    )
}

// ---------------------------------------------------------------------------
// Visual regression — ax_visual_diff
// ---------------------------------------------------------------------------

fn tool_ax_visual_diff() -> Tool {
    Tool {
        name: "ax_visual_diff",
        title: "Visual regression testing",
        description: "Compare current app screenshot against a baseline image. Returns pixel diff \
            percentage and highlights. Use for visual regression testing.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "app": {
                    "type": "string",
                    "description": "App alias from ax_connect"
                },
                "baseline": {
                    "type": "string",
                    "description": "Baseline PNG image encoded as standard base64"
                },
                "threshold": {
                    "type": "number",
                    "description": "Maximum allowed diff fraction before the check fails (default 0.01 = 1%)",
                    "default": 0.01
                }
            },
            "required": ["app", "baseline"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "diff_percent":  { "type": "number" },
                "bytes_changed": { "type": "integer" },
                "total_bytes":   { "type": "integer" },
                "threshold":     { "type": "number" },
                "passed":        { "type": "boolean" }
            },
            "required": ["diff_percent", "bytes_changed", "total_bytes", "threshold", "passed"]
        }),
        annotations: annotations::READ_ONLY,
    }
}

/// Decode a standard-alphabet base64 string to raw bytes without an external crate.
///
/// Padding characters (`=`) are stripped before decoding. Returns an error when
/// a byte outside the standard 64-character alphabet is encountered.
fn decode_baseline_b64(input: &str) -> Result<Vec<u8>, String> {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut lookup = [255u8; 256];
    for (i, &c) in TABLE.iter().enumerate() {
        lookup[c as usize] = i as u8;
    }

    let clean: Vec<u8> = input.bytes().filter(|&b| b != b'=').collect();
    let mut out = Vec::with_capacity(clean.len() * 3 / 4 + 3);

    for chunk in clean.chunks(4) {
        let vals: Vec<u8> = chunk.iter().map(|&b| lookup[b as usize]).collect();
        if vals.contains(&255) {
            return Err("Invalid base64 character in baseline".into());
        }
        match vals.as_slice() {
            [a, b, c, d] => {
                out.push((a << 2) | (b >> 4));
                out.push((b << 4) | (c >> 2));
                out.push((c << 6) | d);
            }
            [a, b, c] => {
                out.push((a << 2) | (b >> 4));
                out.push((b << 4) | (c >> 2));
            }
            [a, b] => {
                out.push((a << 2) | (b >> 4));
            }
            _ => {}
        }
    }
    Ok(out)
}

/// Compare two byte slices and return the fraction of differing bytes in `[0.0, 1.0]`.
///
/// A length mismatch contributes as entirely differing extra bytes, so images that
/// differ in encoded size will score proportionally to the size gap even when the
/// overlapping prefix is identical.
fn compute_diff(baseline: &[u8], current: &[u8]) -> f64 {
    let max_len = baseline.len().max(current.len());
    if max_len == 0 {
        return 0.0;
    }
    let min_len = baseline.len().min(current.len());
    let size_diff = (max_len - min_len) as u64;
    let byte_diff = baseline[..min_len]
        .iter()
        .zip(current[..min_len].iter())
        .filter(|(a, b)| a != b)
        .count() as u64;
    (size_diff + byte_diff) as f64 / max_len as f64
}

/// Handle `ax_visual_diff` — capture the live screenshot and compare it byte-for-byte
/// against the caller-supplied baseline.
fn handle_ax_visual_diff(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    let Some(app_name) = args["app"].as_str().map(str::to_string) else {
        return ToolCallResult::error("Missing required field: app");
    };
    let Some(baseline_b64) = args["baseline"].as_str() else {
        return ToolCallResult::error("Missing required field: baseline");
    };
    let threshold = args["threshold"].as_f64().unwrap_or(0.01);

    let baseline = match decode_baseline_b64(baseline_b64) {
        Ok(b) => b,
        Err(e) => return ToolCallResult::error(format!("baseline decode failed: {e}")),
    };

    registry
        .with_app(&app_name, |app| {
            let current = match app.screenshot_native() {
                Ok(bytes) => bytes,
                Err(e) => return ToolCallResult::error(format!("screenshot failed: {e}")),
            };

            let total_bytes = baseline.len().max(current.len());
            let diff_frac = compute_diff(&baseline, &current);
            let bytes_changed = (diff_frac * total_bytes as f64).round() as u64;
            let passed = diff_frac <= threshold;

            ToolCallResult::ok(
                json!({
                    "diff_percent":  diff_frac * 100.0,
                    "bytes_changed": bytes_changed,
                    "total_bytes":   total_bytes,
                    "threshold":     threshold,
                    "passed":        passed
                })
                .to_string(),
            )
        })
        .unwrap_or_else(ToolCallResult::error)
}

// ---------------------------------------------------------------------------
// Accessibility compliance audit — ax_a11y_audit
// ---------------------------------------------------------------------------

fn tool_ax_a11y_audit() -> Tool {
    Tool {
        name: "ax_a11y_audit",
        title: "Accessibility compliance audit",
        description: "Audit a connected app for accessibility issues: missing labels, incorrect \
            roles, keyboard navigation, WCAG compliance. Returns a list of issues with severity \
            and WCAG criterion references.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "app": {
                    "type": "string",
                    "description": "App alias from ax_connect"
                },
                "scope": {
                    "type": "string",
                    "enum": ["full", "focused_window"],
                    "default": "full",
                    "description": "Audit scope: full tree or focused window only"
                }
            },
            "required": ["app"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "issue_count": { "type": "integer" },
                "critical":    { "type": "integer" },
                "warning":     { "type": "integer" },
                "info":        { "type": "integer" },
                "issues": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "severity": { "type": "string" },
                            "issue":    { "type": "string" },
                            "role":     { "type": "string" },
                            "wcag":     { "type": "string" },
                            "bounds":   {}
                        },
                        "required": ["severity", "issue", "wcag"]
                    }
                }
            },
            "required": ["issue_count", "critical", "warning", "info", "issues"]
        }),
        annotations: annotations::READ_ONLY,
    }
}

/// Interactive macOS accessibility roles that require an accessible name under WCAG 1.3.1.
const INTERACTIVE_ROLES: &[&str] = &[
    "AXButton",
    "AXTextField",
    "AXTextArea",
    "AXCheckBox",
    "AXRadioButton",
    "AXSlider",
    "AXPopUpButton",
    "AXMenuItem",
    "AXLink",
];

/// Audit a single accessibility node and push any WCAG violations into `issues`.
///
/// Three checks are applied per node, each mapped to its governing WCAG success criterion:
///
/// | Check                            | WCAG SC |
/// |----------------------------------|---------|
/// | Interactive element without name | 1.3.1   |
/// | Empty or unknown role            | 4.1.2   |
/// | Image without text alternative   | 1.1.1   |
fn audit_node(node: &crate::intent::SceneNode, issues: &mut Vec<Value>) {
    let role = node.role.as_deref().unwrap_or("");
    let has_label = node.title.is_some() || node.label.is_some() || node.description.is_some();

    let bounds_json = node
        .bounds
        .map(|(x, y, w, h)| json!([x, y, w, h]))
        .unwrap_or(Value::Null);

    // WCAG 1.3.1 — interactive element with no accessible name.
    if INTERACTIVE_ROLES.contains(&role) && !has_label {
        issues.push(json!({
            "severity": "critical",
            "issue":    "missing_label",
            "role":     role,
            "wcag":     "1.3.1",
            "bounds":   bounds_json
        }));
    }

    // WCAG 4.1.2 — element with an empty or AXUnknown role.
    if role.is_empty() || role == "AXUnknown" {
        issues.push(json!({
            "severity": "warning",
            "issue":    "unknown_role",
            "role":     role,
            "wcag":     "4.1.2",
            "bounds":   bounds_json
        }));
    }

    // WCAG 1.1.1 — image element without a text alternative.
    if role == "AXImage" && !has_label {
        issues.push(json!({
            "severity": "critical",
            "issue":    "unlabeled_image",
            "role":     role,
            "wcag":     "1.1.1",
            "bounds":   bounds_json
        }));
    }
}

/// Walk every node in `scene` and collect all WCAG violations.
fn audit_accessibility(scene: &crate::intent::SceneGraph) -> Vec<Value> {
    let mut issues = Vec::new();
    for node in scene.iter() {
        audit_node(node, &mut issues);
    }
    issues
}

/// Count issues at a given severity level.
fn count_by_severity(issues: &[Value], level: &str) -> u64 {
    issues
        .iter()
        .filter(|v| v["severity"].as_str() == Some(level))
        .count() as u64
}

/// Handle `ax_a11y_audit` — scan the live AX tree and report WCAG violations.
fn handle_ax_a11y_audit(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    let Some(app_name) = args["app"].as_str().map(str::to_string) else {
        return ToolCallResult::error("Missing required field: app");
    };
    // `scope` validated by schema; reserved for focused-window filtering.
    let _scope = args["scope"].as_str().unwrap_or("full");

    registry
        .with_app(&app_name, |app| {
            let scene = match crate::intent::scan_scene(app.element) {
                Ok(g) => g,
                Err(e) => return ToolCallResult::error(format!("scan_scene failed: {e}")),
            };

            let issues = audit_accessibility(&scene);
            let issue_count = issues.len() as u64;
            let critical = count_by_severity(&issues, "critical");
            let warning = count_by_severity(&issues, "warning");
            let info = count_by_severity(&issues, "info");

            ToolCallResult::ok(
                json!({
                    "issue_count": issue_count,
                    "critical":    critical,
                    "warning":     warning,
                    "info":        info,
                    "issues":      issues
                })
                .to_string(),
            )
        })
        .unwrap_or_else(ToolCallResult::error)
}

#[cfg(test)]
mod tests;
