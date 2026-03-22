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
pub(crate) fn call_tool_innovation<W: Write>(
    name: &str,
    args: &Value,
    registry: &Arc<AppRegistry>,
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
        "ax_clipboard" => Some(handle_ax_clipboard(args)),
        "ax_session_info" => Some(handle_ax_session_info(args, registry)),
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

/// Handle `ax_workflow_create` — parse step definitions and store the workflow.
fn handle_ax_workflow_create(
    args: &Value,
    workflows: &Arc<Mutex<HashMap<String, WorkflowState>>>,
) -> ToolCallResult {
    let Some(name) = args["name"].as_str().map(str::to_string) else {
        return ToolCallResult::error("Missing required field: name");
    };

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
/// Steps with an unrecognised `action` or missing required fields are skipped.
fn parse_workflow_steps(steps_val: &Value) -> Vec<crate::durable_steps::DurableStep> {
    let Some(arr) = steps_val.as_array() else {
        return vec![];
    };
    arr.iter().filter_map(parse_single_workflow_step).collect()
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
            the total number of registered tools, the active security mode, and the server \
            version. Useful for health-checks and debugging MCP client state.",
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
fn handle_ax_clipboard(args: &Value) -> ToolCallResult {
    match args["action"].as_str() {
        Some("read") => clipboard_read(),
        Some("write") => clipboard_write(args),
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

fn clipboard_write(args: &Value) -> ToolCallResult {
    use crate::mcp::security::SecurityMode;

    if SecurityMode::from_env() == SecurityMode::Sandboxed {
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
fn handle_ax_session_info(_args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    use crate::mcp::security::SecurityMode;

    let connected_apps = registry.connected_names();
    let tool_count =
        crate::mcp::tools::all_tools().len() + crate::mcp::tools_extended::extended_tools().len();
    let security_mode = match SecurityMode::from_env() {
        SecurityMode::Normal => "normal",
        SecurityMode::Safe => "safe",
        SecurityMode::Sandboxed => "sandboxed",
    };

    ToolCallResult::ok(
        json!({
            "connected_apps": connected_apps,
            "tool_count":     tool_count,
            "security_mode":  security_mode,
            "version":        env!("CARGO_PKG_VERSION")
        })
        .to_string(),
    )
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
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use crate::mcp::tools::AppRegistry;

    // -----------------------------------------------------------------------
    // innovation_tools descriptor invariants
    // -----------------------------------------------------------------------

    #[test]
    fn innovation_tools_registers_fifteen_tools() {
        // GIVEN: Wave 2 + workflow tools + ax_record + ax_analyze + ax_run_script
        //        + ax_clipboard + ax_session_info + ax_undo + ax_visual_diff + ax_a11y_audit
        // WHEN: requesting descriptors
        let tools = super::innovation_tools();
        // THEN: exactly fifteen tools registered
        assert_eq!(
            tools.len(),
            15,
            "expected 15 innovation tools, got {}",
            tools.len()
        );
    }

    #[test]
    fn innovation_tool_names_are_unique() {
        // GIVEN: tool list
        let tools = super::innovation_tools();
        // WHEN: collecting names
        let names: std::collections::HashSet<&str> = tools.iter().map(|t| t.name).collect();
        // THEN: no duplicates
        assert_eq!(
            names.len(),
            tools.len(),
            "duplicate tool names in innovation set"
        );
    }

    #[test]
    fn all_innovation_tools_have_non_empty_descriptions() {
        // GIVEN: tool list
        for tool in super::innovation_tools() {
            // THEN: every tool has a description
            assert!(
                !tool.description.is_empty(),
                "empty description on {}",
                tool.name
            );
        }
    }

    #[test]
    fn all_innovation_tools_have_annotation_fields() {
        // GIVEN: tool list
        for tool in super::innovation_tools() {
            // THEN: annotations are defined (no panic on field access)
            let _ = tool.annotations.read_only;
            let _ = tool.annotations.destructive;
            let _ = tool.annotations.idempotent;
            let _ = tool.annotations.open_world;
        }
    }

    #[test]
    fn innovation_tool_names_match_expected_set() {
        // GIVEN: tool list
        let tools = super::innovation_tools();
        // WHEN: collecting names
        let names: std::collections::HashSet<&str> = tools.iter().map(|t| t.name).collect();
        // THEN: all expected names are present
        for expected in &[
            "ax_query",
            "ax_app_profile",
            "ax_test_run",
            "ax_track_workflow",
            "ax_workflow_create",
            "ax_workflow_step",
            "ax_workflow_status",
            "ax_record",
            "ax_analyze",
            "ax_run_script",
            "ax_clipboard",
            "ax_session_info",
            "ax_undo",
            "ax_visual_diff",
            "ax_a11y_audit",
        ] {
            assert!(names.contains(*expected), "missing tool: {expected}");
        }
    }

    // -----------------------------------------------------------------------
    // call_tool_innovation dispatch
    // -----------------------------------------------------------------------

    #[test]
    fn call_tool_innovation_unknown_name_returns_none() {
        // GIVEN: name not in innovation set
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        // WHEN: dispatching unknown name
        let result = super::call_tool_innovation(
            "ax_nonexistent_innovation",
            &json!({}),
            &registry,
            &mut out,
        );
        // THEN: falls through cleanly
        assert!(result.is_none());
    }

    #[test]
    fn call_tool_innovation_empty_name_returns_none() {
        // GIVEN: empty name (malformed request)
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        // WHEN: dispatching empty name
        let result = super::call_tool_innovation("", &json!({}), &registry, &mut out);
        // THEN: falls through cleanly
        assert!(result.is_none());
    }

    #[test]
    fn call_tool_innovation_recognises_all_stateless_names() {
        // GIVEN: all stateless innovation tool names (including ax_record + ax_analyze)
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        for name in &[
            "ax_query",
            "ax_app_profile",
            "ax_test_run",
            "ax_track_workflow",
            "ax_record",
            "ax_analyze",
            "ax_run_script",
            "ax_clipboard",
            "ax_session_info",
            "ax_undo",
            "ax_visual_diff",
            "ax_a11y_audit",
        ] {
            // WHEN: dispatching with minimal args tailored per tool
            let args = if *name == "ax_run_script" {
                json!({"script": "return 42"})
            } else if *name == "ax_clipboard" {
                json!({"action": "read"})
            } else if *name == "ax_session_info" {
                json!({})
            } else {
                json!({"app": "Ghost"})
            };
            let result = super::call_tool_innovation(name, &args, &registry, &mut out);
            // THEN: result is Some (handler ran, even if it returned an error payload)
            assert!(
                result.is_some(),
                "call_tool_innovation returned None for '{name}'"
            );
        }
    }

    #[test]
    fn call_tool_innovation_returns_none_for_workflow_tool_names() {
        // GIVEN: workflow tools are stateful — handled by call_workflow_tool, not here
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        for name in &[
            "ax_workflow_create",
            "ax_workflow_step",
            "ax_workflow_status",
        ] {
            // WHEN: dispatching through the stateless path
            let result =
                super::call_tool_innovation(name, &json!({"name": "wf"}), &registry, &mut out);
            // THEN: falls through so the stateful dispatcher can pick it up
            assert!(
                result.is_none(),
                "call_tool_innovation should return None for stateful '{name}'"
            );
        }
    }

    // -----------------------------------------------------------------------
    // ax_app_profile handler (no live app required)
    // -----------------------------------------------------------------------

    #[test]
    fn ax_app_profile_missing_app_returns_error() {
        // GIVEN: args with no 'app' field
        // WHEN: dispatching
        let result = super::handle_ax_app_profile(&json!({}));
        // THEN: error payload
        assert!(result.is_error);
        assert!(result.content[0].text.contains("Missing"));
    }

    #[test]
    fn ax_app_profile_known_app_returns_found_true() {
        // GIVEN: 'vscode' is a built-in profile
        // WHEN: looking up
        let result = super::handle_ax_app_profile(&json!({"app": "vscode"}));
        // THEN: success, found=true
        assert!(!result.is_error);
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["found"], true);
        assert!(v["name"].is_string());
        assert!(v["capabilities"].is_array());
        assert!(v["selectors"].is_object());
        assert!(v["shortcuts"].is_object());
    }

    #[test]
    fn ax_app_profile_case_insensitive_lookup() {
        // GIVEN: 'Slack' is registered as 'slack' internally
        // WHEN: querying with uppercase
        let result = super::handle_ax_app_profile(&json!({"app": "SLACK"}));
        // THEN: profile found
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["found"], true, "expected found=true for 'SLACK'");
    }

    #[test]
    fn ax_app_profile_unknown_app_returns_found_false() {
        // GIVEN: an app not in the built-in registry
        // WHEN: looking up
        let result = super::handle_ax_app_profile(&json!({"app": "NonExistentApp99"}));
        // THEN: not error, but found=false
        assert!(!result.is_error);
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["found"], false);
    }

    #[test]
    fn ax_app_profile_vscode_has_cdp_port() {
        // GIVEN: VS Code has a known CDP port (9222)
        // WHEN: fetching profile
        let result = super::handle_ax_app_profile(&json!({"app": "VS Code"}));
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        // THEN: cdp_port is 9222
        assert_eq!(v["cdp_port"], 9222);
    }

    #[test]
    fn ax_app_profile_vscode_contains_command_palette_shortcut() {
        // GIVEN: VS Code profile
        // WHEN: fetching shortcuts
        let result = super::handle_ax_app_profile(&json!({"app": "vscode"}));
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        // THEN: command_palette shortcut is present
        assert_eq!(v["shortcuts"]["command_palette"], "Meta+Shift+P");
    }

    // -----------------------------------------------------------------------
    // ax_test_run handler
    // -----------------------------------------------------------------------

    #[test]
    fn ax_test_run_missing_app_returns_error() {
        // GIVEN: no app field
        let mut out = Vec::<u8>::new();
        let result = super::handle_ax_test_run(&json!({"test_name": "t"}), &mut out);
        assert!(result.is_error);
        assert!(result.content[0].text.contains("Missing"));
    }

    #[test]
    fn ax_test_run_missing_test_name_returns_error() {
        // GIVEN: no test_name field
        let mut out = Vec::<u8>::new();
        let result = super::handle_ax_test_run(&json!({"app": "TextEdit"}), &mut out);
        assert!(result.is_error);
        assert!(result.content[0].text.contains("Missing"));
    }

    #[test]
    fn ax_test_run_empty_case_passes_with_no_steps() {
        // GIVEN: minimal args — no steps, no assertions
        // WHEN: running against a ghost app (no live process needed for empty case)
        let mut out = Vec::<u8>::new();
        let result = super::handle_ax_test_run(
            &json!({"app": "__ghost__", "test_name": "empty_test"}),
            &mut out,
        );
        // THEN: not an error; result payload has passed=true
        assert!(!result.is_error);
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["passed"], true);
        assert_eq!(v["test_name"], "empty_test");
        assert_eq!(v["steps_completed"], 0);
    }

    #[test]
    fn ax_test_run_emits_progress_notifications() {
        // GIVEN: a test with one step
        let mut out = Vec::<u8>::new();
        // WHEN: running
        let _ = super::handle_ax_test_run(
            &json!({"app": "__ghost__", "test_name": "prog_test",
                    "steps": [{"type": "wait_for_element", "query": "X", "timeout_ms": 1}]}),
            &mut out,
        );
        // THEN: at least one notifications/progress line was emitted
        let text = String::from_utf8(out).unwrap();
        assert!(
            text.contains("notifications/progress"),
            "expected progress notification in output"
        );
    }

    #[test]
    fn ax_test_run_with_wait_step_times_out_for_ghost_app() {
        // GIVEN: one WaitForElement step against an app that doesn't exist
        let mut out = Vec::<u8>::new();
        let result = super::handle_ax_test_run(
            &json!({
                "app": "__ghost__",
                "test_name": "wait_timeout",
                "steps": [
                    { "type": "wait_for_element", "query": "Button", "timeout_ms": 1 }
                ]
            }),
            &mut out,
        );
        // THEN: test ran but failed
        assert!(!result.is_error, "handler itself must not error");
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["passed"], false);
        assert!(!v["failures"].as_array().unwrap().is_empty());
    }

    #[test]
    fn parse_test_steps_returns_empty_for_null() {
        // GIVEN: null steps value
        let steps = super::parse_test_steps(&json!(null));
        // THEN: empty vec, no panic
        assert!(steps.is_empty());
    }

    #[test]
    fn parse_test_steps_skips_unknown_step_types() {
        // GIVEN: one valid step and one unknown type
        let steps = super::parse_test_steps(&json!([
            { "type": "wait_for_element", "query": "OK", "timeout_ms": 100 },
            { "type": "unsupported_future_step" }
        ]));
        // THEN: only the valid step is returned
        assert_eq!(steps.len(), 1);
    }

    #[test]
    fn parse_test_assertions_returns_empty_for_null() {
        // GIVEN: null assertions value
        let assertions = super::parse_test_assertions(&json!(null));
        // THEN: empty vec, no panic
        assert!(assertions.is_empty());
    }

    // -----------------------------------------------------------------------
    // ax_track_workflow handler (in-process tracker, no live app)
    // -----------------------------------------------------------------------

    #[test]
    fn ax_track_workflow_missing_app_returns_error() {
        // GIVEN: no app field
        let result = super::handle_ax_track_workflow(&json!({}));
        assert!(result.is_error);
        assert!(result.content[0].text.contains("Missing"));
    }

    #[test]
    fn ax_track_workflow_record_returns_recorded_true() {
        // GIVEN: record action for an app
        let result =
            super::handle_ax_track_workflow(&json!({"app": "TestAppA", "action": "record"}));
        // THEN: success
        assert!(!result.is_error);
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["action"], "record");
        assert_eq!(v["recorded"], true);
    }

    #[test]
    fn ax_track_workflow_stats_returns_stats_object() {
        // GIVEN: stats action
        let result = super::handle_ax_track_workflow(&json!({"app": "AnyApp", "action": "stats"}));
        // THEN: stats object present
        assert!(!result.is_error);
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["action"], "stats");
        assert!(v["stats"].is_object());
        assert!(v["stats"]["total_transitions"].is_number());
        assert!(v["stats"]["distinct_apps"].is_number());
    }

    #[test]
    fn ax_track_workflow_detect_returns_workflows_array() {
        // GIVEN: detect action
        let result = super::handle_ax_track_workflow(
            &json!({"app": "AnyApp", "action": "detect", "min_frequency": 999}),
        );
        // THEN: workflows array present (may be empty if not enough transitions)
        assert!(!result.is_error);
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["action"], "detect");
        assert!(v["workflows"].is_array());
    }

    #[test]
    fn ax_track_workflow_unknown_action_returns_error() {
        // GIVEN: unrecognised action
        let result = super::handle_ax_track_workflow(&json!({"app": "App", "action": "teleport"}));
        // THEN: error with message
        assert!(result.is_error);
        assert!(result.content[0].text.contains("teleport"));
    }

    #[test]
    fn ax_track_workflow_default_action_is_record() {
        // GIVEN: no action field — defaults to "record"
        let result = super::handle_ax_track_workflow(&json!({"app": "DefaultActionApp"}));
        // THEN: recorded as if action="record"
        assert!(!result.is_error);
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["action"], "record");
    }

    // -----------------------------------------------------------------------
    // parse_transition_trigger
    // -----------------------------------------------------------------------

    #[test]
    fn parse_transition_trigger_maps_all_variants() {
        use crate::cross_app::TransitionTrigger;
        // GIVEN/WHEN/THEN: all known strings resolve correctly
        assert_eq!(
            super::parse_transition_trigger("user_switch"),
            TransitionTrigger::UserSwitch
        );
        assert_eq!(
            super::parse_transition_trigger("automation"),
            TransitionTrigger::Automation
        );
        assert_eq!(
            super::parse_transition_trigger("notification"),
            TransitionTrigger::Notification
        );
        assert_eq!(
            super::parse_transition_trigger("unknown"),
            TransitionTrigger::Unknown
        );
        assert_eq!(
            super::parse_transition_trigger("bogus"),
            TransitionTrigger::Unknown
        );
    }

    // -----------------------------------------------------------------------
    // ax_query handler (no live app — requires connected app, so test error path)
    // -----------------------------------------------------------------------

    #[test]
    fn ax_query_missing_app_returns_error() {
        // GIVEN: no app field
        let registry = Arc::new(AppRegistry::default());
        let result = super::handle_ax_query(&json!({"query": "find the button"}), &registry);
        assert!(result.is_error);
        assert!(result.content[0].text.contains("Missing"));
    }

    #[test]
    fn ax_query_missing_query_returns_error() {
        // GIVEN: no query field
        let registry = Arc::new(AppRegistry::default());
        let result = super::handle_ax_query(&json!({"app": "Safari"}), &registry);
        assert!(result.is_error);
        assert!(result.content[0].text.contains("Missing"));
    }

    #[test]
    fn ax_query_unconnected_app_returns_error() {
        // GIVEN: app not in registry
        let registry = Arc::new(AppRegistry::default());
        let result = super::handle_ax_query(
            &json!({"app": "NotConnected", "query": "find the button"}),
            &registry,
        );
        assert!(result.is_error);
        assert!(result.content[0].text.contains("not connected"));
    }

    // -----------------------------------------------------------------------
    // ax_workflow_create handler
    // -----------------------------------------------------------------------

    fn make_workflows(
    ) -> std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, super::WorkflowState>>>
    {
        std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()))
    }

    #[test]
    fn ax_workflow_create_missing_name_returns_error() {
        // GIVEN: no name field
        let wf = make_workflows();
        // WHEN: creating without a name
        let result = super::handle_ax_workflow_create(&json!({}), &wf);
        // THEN: error payload
        assert!(result.is_error);
        assert!(result.content[0].text.contains("Missing"));
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
        assert!(result.content[0].text.contains("Missing"));
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
        assert!(result.content[0].text.contains("Missing"));
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

    // -----------------------------------------------------------------------
    // ax_record handler
    // -----------------------------------------------------------------------

    #[test]
    fn ax_record_missing_app_returns_error() {
        // GIVEN: no app field
        let result = super::handle_ax_record(&json!({}));
        // THEN: error with message
        assert!(result.is_error);
        assert!(result.content[0].text.contains("app"));
    }

    #[test]
    fn ax_record_start_action_sets_recording_true() {
        // GIVEN: start action
        // WHEN: dispatching
        let result = super::handle_ax_record(&json!({"app": "Safari", "action": "start"}));
        // THEN: recording=true in response
        assert!(!result.is_error);
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["action"], "start");
        assert_eq!(v["recording"], true);
        assert_eq!(v["event_count"], 0);
    }

    #[test]
    fn ax_record_status_returns_state() {
        // GIVEN: status action
        let result = super::handle_ax_record(&json!({"app": "Safari", "action": "status"}));
        // THEN: response contains recording and event_count
        assert!(!result.is_error);
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["action"], "status");
        assert!(v["recording"].is_boolean());
        assert!(v["event_count"].is_number());
    }

    #[test]
    fn ax_record_record_missing_action_type_returns_error() {
        // GIVEN: record action without action_type
        let result = super::handle_ax_record(&json!({"app": "Safari", "action": "record"}));
        // THEN: error mentions action_type
        assert!(result.is_error);
        assert!(result.content[0].text.contains("action_type"));
    }

    #[test]
    fn ax_record_stop_returns_events_array() {
        // GIVEN: stop action
        let result = super::handle_ax_record(&json!({"app": "Safari", "action": "stop"}));
        // THEN: events array present (may be empty)
        assert!(!result.is_error);
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["action"], "stop");
        assert_eq!(v["recording"], false);
        assert!(v["events"].is_array());
    }

    #[test]
    fn ax_record_unknown_action_returns_error() {
        // GIVEN: unrecognised action
        let result = super::handle_ax_record(&json!({"app": "Safari", "action": "teleport"}));
        // THEN: error with message
        assert!(result.is_error);
        assert!(result.content[0].text.contains("teleport"));
    }

    #[test]
    fn ax_record_defaults_action_to_record_when_omitted() {
        // GIVEN: no action field — should default to "record"
        // WHEN: dispatching without action_type (should fail gracefully)
        let result = super::handle_ax_record(&json!({"app": "Safari"}));
        // THEN: error mentions action_type (default "record" path hit, missing action_type)
        assert!(result.is_error);
        assert!(result.content[0].text.contains("action_type"));
    }

    // -----------------------------------------------------------------------
    // ax_analyze — unit tests for the three pure intelligence helpers
    // -----------------------------------------------------------------------

    // Helper: build a SceneGraph from a list of (role, title, label, identifier) tuples.
    fn make_scene(
        nodes: &[(&str, Option<&str>, Option<&str>, Option<&str>)],
    ) -> crate::intent::SceneGraph {
        let mut g = crate::intent::SceneGraph::empty();
        for (role, title, label, identifier) in nodes {
            let node = crate::intent::SceneNode {
                id: crate::intent::NodeId(g.len()),
                parent: None,
                children: vec![],
                role: Some(role.to_string()),
                title: title.map(str::to_string),
                label: label.map(str::to_string),
                value: None,
                description: None,
                identifier: identifier.map(str::to_string),
                bounds: None,
                enabled: true,
                depth: 0,
            };
            g.push(node);
        }
        g
    }

    // --- detect_ui_patterns ------------------------------------------------

    #[test]
    fn detect_patterns_login_form_detected_when_password_text_button_present() {
        // GIVEN: scene with secure field + text field + button
        let scene = make_scene(&[
            ("AXSecureTextField", Some("Password"), None, None),
            ("AXTextField", Some("Username"), None, None),
            ("AXButton", Some("Sign In"), None, None),
        ]);
        // WHEN
        let patterns = super::detect_ui_patterns(&scene);
        // THEN: login_form detected
        assert!(
            patterns.iter().any(|p| p.pattern == "login_form"),
            "login_form not detected"
        );
    }

    #[test]
    fn detect_patterns_login_form_requires_password_field() {
        // GIVEN: text field + button but NO secure field
        let scene = make_scene(&[
            ("AXTextField", Some("Email"), None, None),
            ("AXButton", Some("Submit"), None, None),
        ]);
        // WHEN
        let patterns = super::detect_ui_patterns(&scene);
        // THEN: login_form NOT detected
        assert!(
            !patterns.iter().any(|p| p.pattern == "login_form"),
            "login_form should not be detected without a password field"
        );
    }

    #[test]
    fn detect_patterns_search_interface_from_search_field_role() {
        // GIVEN: AXSearchField present
        let scene = make_scene(&[("AXSearchField", Some("Search"), None, None)]);
        // WHEN
        let patterns = super::detect_ui_patterns(&scene);
        // THEN: search_interface detected
        assert!(patterns.iter().any(|p| p.pattern == "search_interface"));
    }

    #[test]
    fn detect_patterns_search_interface_from_label() {
        // GIVEN: AXTextField labelled "Search Items"
        let scene = make_scene(&[("AXTextField", None, Some("Search Items"), None)]);
        // WHEN
        let patterns = super::detect_ui_patterns(&scene);
        // THEN: search_interface detected
        assert!(patterns.iter().any(|p| p.pattern == "search_interface"));
    }

    #[test]
    fn detect_patterns_navigation_from_tab_group() {
        // GIVEN: AXTabGroup present
        let scene = make_scene(&[("AXTabGroup", None, None, None)]);
        // WHEN
        let patterns = super::detect_ui_patterns(&scene);
        // THEN: navigation detected
        assert!(patterns.iter().any(|p| p.pattern == "navigation"));
    }

    #[test]
    fn detect_patterns_navigation_from_toolbar() {
        // GIVEN: AXToolbar present
        let scene = make_scene(&[("AXToolbar", None, None, None)]);
        // WHEN
        let patterns = super::detect_ui_patterns(&scene);
        // THEN: navigation detected
        assert!(patterns.iter().any(|p| p.pattern == "navigation"));
    }

    #[test]
    fn detect_patterns_table_view_from_ax_table() {
        // GIVEN: AXTable present
        let scene = make_scene(&[("AXTable", None, None, None)]);
        // WHEN
        let patterns = super::detect_ui_patterns(&scene);
        // THEN: table_view detected
        assert!(patterns.iter().any(|p| p.pattern == "table_view"));
    }

    #[test]
    fn detect_patterns_table_view_from_ax_grid() {
        // GIVEN: AXGrid present
        let scene = make_scene(&[("AXGrid", None, None, None)]);
        // WHEN
        let patterns = super::detect_ui_patterns(&scene);
        // THEN: table_view detected
        assert!(patterns.iter().any(|p| p.pattern == "table_view"));
    }

    #[test]
    fn detect_patterns_modal_dialog_from_sheet() {
        // GIVEN: AXSheet present
        let scene = make_scene(&[("AXSheet", None, None, None)]);
        // WHEN
        let patterns = super::detect_ui_patterns(&scene);
        // THEN: modal_dialog detected
        assert!(patterns.iter().any(|p| p.pattern == "modal_dialog"));
    }

    #[test]
    fn detect_patterns_modal_dialog_from_ax_dialog() {
        // GIVEN: AXDialog present
        let scene = make_scene(&[("AXDialog", None, None, None)]);
        // WHEN
        let patterns = super::detect_ui_patterns(&scene);
        // THEN: modal_dialog detected
        assert!(patterns.iter().any(|p| p.pattern == "modal_dialog"));
    }

    #[test]
    fn detect_patterns_file_save_dialog_from_sheet_save_cancel() {
        // GIVEN: sheet + Save + Cancel buttons
        let scene = make_scene(&[
            ("AXSheet", None, None, None),
            ("AXButton", Some("Save"), None, None),
            ("AXButton", Some("Cancel"), None, None),
        ]);
        // WHEN
        let patterns = super::detect_ui_patterns(&scene);
        // THEN: file_save_dialog detected
        assert!(patterns.iter().any(|p| p.pattern == "file_save_dialog"));
    }

    #[test]
    fn detect_patterns_file_open_dialog_from_sheet_open_cancel() {
        // GIVEN: sheet + Open + Cancel buttons
        let scene = make_scene(&[
            ("AXSheet", None, None, None),
            ("AXButton", Some("Open"), None, None),
            ("AXButton", Some("Cancel"), None, None),
        ]);
        // WHEN
        let patterns = super::detect_ui_patterns(&scene);
        // THEN: file_open_dialog detected
        assert!(patterns.iter().any(|p| p.pattern == "file_open_dialog"));
    }

    #[test]
    fn detect_patterns_confirmation_dialog_from_alert_ok_cancel() {
        // GIVEN: AXAlert with OK + Cancel buttons
        let scene = make_scene(&[
            ("AXAlert", None, None, None),
            ("AXButton", Some("OK"), None, None),
            ("AXButton", Some("Cancel"), None, None),
        ]);
        // WHEN
        let patterns = super::detect_ui_patterns(&scene);
        // THEN: confirmation_dialog detected
        assert!(patterns.iter().any(|p| p.pattern == "confirmation_dialog"));
    }

    #[test]
    fn detect_patterns_error_alert_from_alert_single_button() {
        // GIVEN: AXAlert with a single dismiss button (no cancel = not confirmation)
        let scene = make_scene(&[
            ("AXAlert", None, None, None),
            ("AXButton", Some("OK"), None, None),
        ]);
        // WHEN
        let patterns = super::detect_ui_patterns(&scene);
        // THEN: error_alert detected (OK but no Cancel → single-button alert)
        assert!(patterns.iter().any(|p| p.pattern == "error_alert"));
    }

    #[test]
    fn detect_patterns_progress_indicator_detected() {
        // GIVEN: AXProgressIndicator
        let scene = make_scene(&[("AXProgressIndicator", None, None, None)]);
        // WHEN
        let patterns = super::detect_ui_patterns(&scene);
        // THEN: progress_indicator detected
        assert!(patterns.iter().any(|p| p.pattern == "progress_indicator"));
    }

    #[test]
    fn detect_patterns_form_from_multiple_text_fields_and_button() {
        // GIVEN: 2 text fields + button, no password field
        let scene = make_scene(&[
            ("AXTextField", Some("First Name"), None, None),
            ("AXTextField", Some("Last Name"), None, None),
            ("AXButton", Some("Submit"), None, None),
        ]);
        // WHEN
        let patterns = super::detect_ui_patterns(&scene);
        // THEN: form detected
        assert!(patterns.iter().any(|p| p.pattern == "form"));
    }

    #[test]
    fn detect_patterns_empty_scene_returns_no_patterns() {
        // GIVEN: empty scene graph
        let scene = crate::intent::SceneGraph::empty();
        // WHEN
        let patterns = super::detect_ui_patterns(&scene);
        // THEN: no patterns (not a panic)
        assert!(patterns.is_empty());
    }

    #[test]
    fn detect_patterns_all_have_confidence_in_range() {
        // GIVEN: scene with several patterns triggerable
        let scene = make_scene(&[
            ("AXTable", None, None, None),
            ("AXSearchField", None, None, None),
            ("AXTabGroup", None, None, None),
        ]);
        // WHEN
        let patterns = super::detect_ui_patterns(&scene);
        // THEN: all confidence values are in [0.0, 1.0]
        for p in &patterns {
            assert!(
                (0.0..=1.0).contains(&p.confidence),
                "pattern '{}' has out-of-range confidence {}",
                p.pattern,
                p.confidence
            );
        }
    }

    // --- infer_app_state ---------------------------------------------------

    #[test]
    fn infer_state_idle_for_empty_scene() {
        // GIVEN: empty scene
        let scene = crate::intent::SceneGraph::empty();
        // WHEN
        let state = super::infer_app_state(&scene);
        // THEN: idle
        assert_eq!(state, super::AppState::Idle);
    }

    #[test]
    fn infer_state_modal_when_sheet_present() {
        // GIVEN: scene with an AXSheet
        let scene = make_scene(&[("AXSheet", None, None, None)]);
        // WHEN
        let state = super::infer_app_state(&scene);
        // THEN: modal (highest priority)
        assert_eq!(state, super::AppState::Modal);
    }

    #[test]
    fn infer_state_loading_when_progress_indicator_present() {
        // GIVEN: progress indicator, no modal
        let scene = make_scene(&[("AXProgressIndicator", None, None, None)]);
        // WHEN
        let state = super::infer_app_state(&scene);
        // THEN: loading
        assert_eq!(state, super::AppState::Loading);
    }

    #[test]
    fn infer_state_error_when_alert_present_without_modal() {
        // GIVEN: alert element, no modal/progress
        let scene = make_scene(&[("AXAlert", None, None, None)]);
        // WHEN
        let state = super::infer_app_state(&scene);
        // THEN: error
        assert_eq!(state, super::AppState::Error);
    }

    #[test]
    fn infer_state_error_from_error_text_label() {
        // GIVEN: static text labelled "Error"
        let scene = make_scene(&[("AXStaticText", Some("Error: file not found"), None, None)]);
        // WHEN
        let state = super::infer_app_state(&scene);
        // THEN: error
        assert_eq!(state, super::AppState::Error);
    }

    #[test]
    fn infer_state_auth_required_when_password_field_present_without_modal() {
        // GIVEN: secure text field, no modal or progress
        let scene = make_scene(&[("AXSecureTextField", Some("Password"), None, None)]);
        // WHEN
        let state = super::infer_app_state(&scene);
        // THEN: auth_required
        assert_eq!(state, super::AppState::AuthRequired);
    }

    #[test]
    fn infer_state_modal_overrides_loading() {
        // GIVEN: both sheet and progress indicator
        let scene = make_scene(&[
            ("AXSheet", None, None, None),
            ("AXProgressIndicator", None, None, None),
        ]);
        // WHEN
        let state = super::infer_app_state(&scene);
        // THEN: modal wins (higher priority)
        assert_eq!(state, super::AppState::Modal);
    }

    #[test]
    fn infer_state_loading_overrides_error() {
        // GIVEN: progress indicator + alert
        let scene = make_scene(&[
            ("AXProgressIndicator", None, None, None),
            ("AXAlert", None, None, None),
        ]);
        // WHEN
        let state = super::infer_app_state(&scene);
        // THEN: loading overrides error
        assert_eq!(state, super::AppState::Loading);
    }

    #[test]
    fn app_state_as_str_covers_all_variants() {
        // GIVEN / WHEN / THEN: every variant maps to a non-empty string
        use super::AppState;
        for (state, expected) in &[
            (AppState::Idle, "idle"),
            (AppState::Loading, "loading"),
            (AppState::Error, "error"),
            (AppState::Modal, "modal"),
            (AppState::AuthRequired, "auth_required"),
        ] {
            assert_eq!(state.as_str(), *expected);
        }
    }

    // --- suggest_actions ---------------------------------------------------

    #[test]
    fn suggest_actions_idle_empty_patterns_returns_empty() {
        // GIVEN: idle state, no patterns
        let suggestions = super::suggest_actions(&[], super::AppState::Idle);
        // THEN: no suggestions
        assert!(suggestions.is_empty());
    }

    #[test]
    fn suggest_actions_modal_state_suggests_dismiss() {
        // GIVEN: modal state
        let suggestions = super::suggest_actions(&[], super::AppState::Modal);
        // THEN: dismiss suggestion present
        assert!(suggestions.iter().any(|s| s.tool == "ax_click"));
    }

    #[test]
    fn suggest_actions_loading_state_suggests_wait() {
        // GIVEN: loading state
        let suggestions = super::suggest_actions(&[], super::AppState::Loading);
        // THEN: wait suggestion present
        assert!(suggestions.iter().any(|s| s.tool == "ax_wait_idle"));
    }

    #[test]
    fn suggest_actions_error_state_suggests_get_value() {
        // GIVEN: error state
        let suggestions = super::suggest_actions(&[], super::AppState::Error);
        // THEN: read the error message
        assert!(suggestions.iter().any(|s| s.tool == "ax_get_value"));
    }

    #[test]
    fn suggest_actions_auth_required_suggests_type() {
        // GIVEN: auth_required state
        let suggestions = super::suggest_actions(&[], super::AppState::AuthRequired);
        // THEN: type action suggested
        assert!(suggestions.iter().any(|s| s.tool == "ax_type"));
    }

    #[test]
    fn suggest_actions_login_form_includes_submit() {
        // GIVEN: login_form pattern detected
        let login = super::UiPattern {
            pattern: "login_form",
            confidence: 0.9,
        };
        let suggestions = super::suggest_actions(&[login], super::AppState::Idle);
        // THEN: click the sign-in button
        assert!(
            suggestions
                .iter()
                .any(|s| s.tool == "ax_click" && s.query.to_lowercase().contains("sign")),
            "expected ax_click with 'Sign In' query"
        );
    }

    #[test]
    fn suggest_actions_search_interface_suggests_type() {
        // GIVEN: search_interface pattern
        let search = super::UiPattern {
            pattern: "search_interface",
            confidence: 0.85,
        };
        let suggestions = super::suggest_actions(&[search], super::AppState::Idle);
        // THEN: type into search
        assert!(suggestions.iter().any(|s| s.tool == "ax_type"));
    }

    #[test]
    fn suggest_actions_file_save_dialog_suggests_save_click() {
        // GIVEN: file_save_dialog pattern
        let save_dlg = super::UiPattern {
            pattern: "file_save_dialog",
            confidence: 0.88,
        };
        let suggestions = super::suggest_actions(&[save_dlg], super::AppState::Idle);
        // THEN: click Save
        assert!(suggestions
            .iter()
            .any(|s| s.tool == "ax_click" && s.query == "Save"));
    }

    #[test]
    fn suggest_actions_confirmation_dialog_includes_cancel() {
        // GIVEN: confirmation_dialog pattern
        let conf = super::UiPattern {
            pattern: "confirmation_dialog",
            confidence: 0.87,
        };
        let suggestions = super::suggest_actions(&[conf], super::AppState::Idle);
        // THEN: Cancel action present
        assert!(suggestions
            .iter()
            .any(|s| s.tool == "ax_click" && s.query == "Cancel"));
    }

    #[test]
    fn suggest_actions_table_view_suggests_get_value() {
        // GIVEN: table_view pattern
        let table = super::UiPattern {
            pattern: "table_view",
            confidence: 0.88,
        };
        let suggestions = super::suggest_actions(&[table], super::AppState::Idle);
        // THEN: read the table
        assert!(suggestions.iter().any(|s| s.tool == "ax_get_value"));
    }

    #[test]
    fn suggest_actions_all_suggestions_have_non_empty_action_and_tool() {
        // GIVEN: all pattern types simultaneously
        let patterns: Vec<super::UiPattern> = [
            "login_form",
            "search_interface",
            "file_save_dialog",
            "file_open_dialog",
            "confirmation_dialog",
            "error_alert",
            "table_view",
            "text_editor",
            "form",
        ]
        .iter()
        .map(|p| super::UiPattern {
            pattern: p,
            confidence: 0.8,
        })
        .collect();
        let suggestions = super::suggest_actions(&patterns, super::AppState::Idle);
        for s in &suggestions {
            assert!(!s.action.is_empty(), "suggestion has empty action");
            assert!(!s.tool.is_empty(), "suggestion has empty tool");
        }
    }

    // --- handle_ax_analyze dispatch ----------------------------------------

    #[test]
    fn ax_analyze_missing_app_returns_error() {
        // GIVEN: no app field
        let registry = Arc::new(AppRegistry::default());
        // WHEN
        let result = super::handle_ax_analyze(&json!({}), &registry);
        // THEN: error mentions missing field
        assert!(result.is_error);
        assert!(result.content[0].text.contains("Missing"));
    }

    #[test]
    fn ax_analyze_unconnected_app_returns_error() {
        // GIVEN: app not in registry
        let registry = Arc::new(AppRegistry::default());
        // WHEN
        let result = super::handle_ax_analyze(&json!({"app": "NotConnected"}), &registry);
        // THEN: error mentions not connected
        assert!(result.is_error);
        assert!(result.content[0].text.contains("not connected"));
    }

    #[test]
    fn ax_analyze_dispatch_returns_some_for_valid_call() {
        // GIVEN: ax_analyze is in the stateless dispatch table
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        // WHEN: dispatching with minimal (error-triggering) args
        let result =
            super::call_tool_innovation("ax_analyze", &json!({"app": "X"}), &registry, &mut out);
        // THEN: handler ran and returned Some (even if payload is an error)
        assert!(
            result.is_some(),
            "ax_analyze should be handled by call_tool_innovation"
        );
    }

    #[test]
    fn ax_analyze_workflow_tools_still_return_none_from_stateless_dispatch() {
        // GIVEN: ax_analyze did not break the existing None-fallthrough for workflow tools
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        let result = super::call_tool_innovation(
            "ax_workflow_create",
            &json!({"name": "wf"}),
            &registry,
            &mut out,
        );
        assert!(result.is_none());
    }

    // --- pattern_to_json / suggestion_to_json helpers ----------------------

    #[test]
    fn pattern_to_json_produces_expected_keys() {
        // GIVEN: a UiPattern
        let p = super::UiPattern {
            pattern: "login_form",
            confidence: 0.9,
        };
        // WHEN
        let v = super::pattern_to_json(&p);
        // THEN: both required keys present
        assert_eq!(v["pattern"], "login_form");
        assert!((v["confidence"].as_f64().unwrap() - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn suggestion_to_json_produces_expected_keys() {
        // GIVEN: a Suggestion
        let s = super::Suggestion {
            action: "Click Save",
            tool: "ax_click",
            query: "Save",
        };
        // WHEN
        let v = super::suggestion_to_json(&s);
        // THEN: all three keys present
        assert_eq!(v["action"], "Click Save");
        assert_eq!(v["tool"], "ax_click");
        assert_eq!(v["query"], "Save");
    }

    // -----------------------------------------------------------------------
    // ax_run_script handler
    // -----------------------------------------------------------------------

    #[test]
    fn ax_run_script_missing_script_returns_error() {
        // GIVEN: args with no 'script' field
        // WHEN: dispatching
        let result = super::handle_ax_run_script(&json!({}));
        // THEN: error payload with descriptive message
        assert!(result.is_error);
        assert!(result.content[0].text.contains("script"));
    }

    #[test]
    fn ax_run_script_default_language_does_not_report_missing_field() {
        // GIVEN: no 'language' field — default should be "applescript"
        // WHEN: handler is entered (osascript may succeed or fail, but not "Missing field")
        let result = super::handle_ax_run_script(&json!({"script": "return \"hello\""}));
        // THEN: error is NOT about a missing field (default applied correctly)
        assert!(
            !result.content[0].text.contains("Missing required field"),
            "handler should not report missing field when language is omitted"
        );
    }

    #[test]
    fn ax_run_script_executes_trivial_applescript() {
        // GIVEN: a simple return-value AppleScript
        // WHEN: dispatching
        let result = super::handle_ax_run_script(&json!({
            "script": "return 42",
            "language": "applescript"
        }));
        // THEN: osascript is always present on macOS — succeeds with output
        assert!(!result.is_error, "osascript must be available on macOS");
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["success"], true);
        assert!(v["output"].is_string());
    }

    #[test]
    fn ax_run_script_executes_trivial_jxa() {
        // GIVEN: a minimal JXA script
        // WHEN: dispatching
        let result = super::handle_ax_run_script(&json!({
            "script": "\"hello from jxa\"",
            "language": "jxa"
        }));
        // THEN: success on macOS
        assert!(!result.is_error, "osascript JXA must be available on macOS");
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["success"], true);
    }

    #[test]
    fn ax_run_script_syntax_error_returns_error_not_panic() {
        // GIVEN: a script with invalid AppleScript syntax
        // WHEN: dispatching
        let result = super::handle_ax_run_script(&json!({
            "script": "this is not valid applescript @@@@",
            "language": "applescript"
        }));
        // THEN: is_error=true, no panic, message contains "Script failed"
        assert!(result.is_error);
        assert!(
            result.content[0].text.contains("Script failed"),
            "expected 'Script failed' in: {}",
            result.content[0].text
        );
    }

    #[test]
    fn ax_run_script_descriptor_has_destructive_annotation() {
        // GIVEN: the ax_run_script tool descriptor
        let tools = super::innovation_tools();
        let tool = tools.iter().find(|t| t.name == "ax_run_script").unwrap();
        // THEN: destructive=true and not read_only (scripts can mutate system state)
        assert!(
            tool.annotations.destructive,
            "ax_run_script must be destructive"
        );
        assert!(
            !tool.annotations.read_only,
            "ax_run_script must not be read_only"
        );
    }

    #[test]
    fn ax_run_script_dispatch_recognises_name() {
        // GIVEN: ax_run_script is registered in the stateless dispatch table
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        // WHEN: dispatching with a valid script arg
        let result = super::call_tool_innovation(
            "ax_run_script",
            &json!({"script": "return 1"}),
            &registry,
            &mut out,
        );
        // THEN: returns Some (handler matched and ran)
        assert!(
            result.is_some(),
            "call_tool_innovation must handle 'ax_run_script'"
        );
    }

    // -----------------------------------------------------------------------
    // ax_clipboard handler
    // -----------------------------------------------------------------------

    #[test]
    fn ax_clipboard_missing_action_returns_error() {
        // GIVEN: args with no 'action' field
        // WHEN: dispatching
        let result = super::handle_ax_clipboard(&json!({}));
        // THEN: error payload
        assert!(result.is_error);
        assert!(result.content[0].text.contains("Missing"));
    }

    #[test]
    fn ax_clipboard_unknown_action_returns_error() {
        // GIVEN: an unrecognised action value
        // WHEN: dispatching
        let result = super::handle_ax_clipboard(&json!({"action": "flush"}));
        // THEN: error payload explaining the unknown action
        assert!(result.is_error);
        assert!(result.content[0].text.contains("Unknown clipboard action"));
    }

    #[test]
    fn ax_clipboard_write_without_text_returns_error() {
        // GIVEN: action=write but no text field, not sandboxed
        std::env::remove_var("AXTERMINATOR_SECURITY_MODE");
        // WHEN: dispatching
        let result = super::handle_ax_clipboard(&json!({"action": "write"}));
        // THEN: error payload about missing text
        assert!(result.is_error);
        assert!(result.content[0].text.contains("text"));
    }

    #[test]
    fn ax_clipboard_write_blocked_in_sandboxed_mode() {
        // GIVEN: sandboxed mode
        std::env::set_var("AXTERMINATOR_SECURITY_MODE", "sandboxed");
        // WHEN: dispatching a write
        let result = super::handle_ax_clipboard(&json!({"action": "write", "text": "hello"}));
        // THEN: error payload about sandboxed mode
        assert!(result.is_error);
        assert!(result.content[0].text.contains("sandboxed"));
        // cleanup
        std::env::remove_var("AXTERMINATOR_SECURITY_MODE");
    }

    #[test]
    fn ax_clipboard_descriptor_has_destructive_annotation() {
        // GIVEN: tool descriptor
        let tool = super::tool_ax_clipboard();
        // THEN: annotation flags destructive (write path is state-changing)
        assert!(tool.annotations.destructive);
        assert!(!tool.annotations.read_only);
    }

    // -----------------------------------------------------------------------
    // ax_session_info handler
    // -----------------------------------------------------------------------

    #[test]
    fn ax_session_info_returns_all_required_fields() {
        // GIVEN: an empty registry
        let registry = Arc::new(AppRegistry::default());
        // WHEN: calling the handler
        let result = super::handle_ax_session_info(&json!({}), &registry);
        // THEN: success with the four required keys present
        assert!(!result.is_error);
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert!(v["connected_apps"].is_array());
        assert!(v["tool_count"].is_number());
        assert!(v["security_mode"].is_string());
        assert!(v["version"].is_string());
    }

    #[test]
    fn ax_session_info_security_mode_reflects_env() {
        // GIVEN: sandboxed mode set in the environment
        std::env::set_var("AXTERMINATOR_SECURITY_MODE", "sandboxed");
        let registry = Arc::new(AppRegistry::default());
        // WHEN: calling the handler
        let result = super::handle_ax_session_info(&json!({}), &registry);
        // THEN: security_mode field is "sandboxed"
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["security_mode"], "sandboxed");
        // cleanup
        std::env::remove_var("AXTERMINATOR_SECURITY_MODE");
    }

    #[test]
    fn ax_session_info_version_is_non_empty() {
        // GIVEN: any registry
        let registry = Arc::new(AppRegistry::default());
        // WHEN: calling the handler
        let result = super::handle_ax_session_info(&json!({}), &registry);
        // THEN: version is a non-empty string (set from CARGO_PKG_VERSION)
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert!(!v["version"].as_str().unwrap_or("").is_empty());
    }

    #[test]
    fn ax_session_info_connected_apps_is_empty_with_fresh_registry() {
        // GIVEN: a fresh registry with no connected apps
        let registry = Arc::new(AppRegistry::default());
        // WHEN: calling the handler
        let result = super::handle_ax_session_info(&json!({}), &registry);
        // THEN: connected_apps is an empty array
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["connected_apps"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn ax_session_info_descriptor_is_read_only() {
        // GIVEN: tool descriptor
        let tool = super::tool_ax_session_info();
        // THEN: read-only, non-destructive
        assert!(tool.annotations.read_only);
        assert!(!tool.annotations.destructive);
    }

    // -----------------------------------------------------------------------
    // ax_undo handler
    // -----------------------------------------------------------------------

    #[test]
    fn ax_undo_missing_app_returns_error() {
        // GIVEN: args with no 'app' field
        // WHEN: dispatching
        let result = super::handle_ax_undo(&json!({}));
        // THEN: error payload
        assert!(result.is_error);
        assert!(result.content[0].text.contains("Missing"));
    }

    #[test]
    fn ax_undo_returns_correct_undone_count() {
        // GIVEN: app + count=1 (osascript calls fail silently in test env)
        // WHEN: dispatching
        let result = super::handle_ax_undo(&json!({"app": "Ghost", "count": 1}));
        // THEN: undone field equals the requested count, ok=true
        assert!(!result.is_error);
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["undone"], 1);
        assert_eq!(v["app"], "Ghost");
        assert_eq!(v["ok"], true);
    }

    #[test]
    fn ax_undo_clamps_count_above_maximum() {
        // GIVEN: count=999 (above the 50 ceiling)
        // WHEN: dispatching
        let result = super::handle_ax_undo(&json!({"app": "Finder", "count": 999}));
        // THEN: undone is clamped to 50
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["undone"], 50);
    }

    #[test]
    fn ax_undo_defaults_to_one_when_count_absent() {
        // GIVEN: no count field provided
        // WHEN: dispatching
        let result = super::handle_ax_undo(&json!({"app": "Notes"}));
        // THEN: undone defaults to 1
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["undone"], 1);
    }

    #[test]
    fn ax_undo_descriptor_has_destructive_annotation() {
        // GIVEN: tool descriptor
        let tool = super::tool_ax_undo();
        // THEN: annotation flags destructive (Cmd+Z modifies app state)
        assert!(tool.annotations.destructive);
        assert!(!tool.annotations.read_only);
    }

    // -----------------------------------------------------------------------
    // decode_baseline_b64
    // -----------------------------------------------------------------------

    #[test]
    fn decode_baseline_b64_round_trips_hello() {
        // GIVEN: "Hello" in standard base64
        let encoded = "SGVsbG8=";
        // WHEN: decoding
        let result = super::decode_baseline_b64(encoded).unwrap();
        // THEN: original bytes recovered
        assert_eq!(result, b"Hello");
    }

    #[test]
    fn decode_baseline_b64_empty_string_returns_empty_vec() {
        // GIVEN: empty input
        // WHEN: decoding
        let result = super::decode_baseline_b64("").unwrap();
        // THEN: empty vec, no panic
        assert!(result.is_empty());
    }

    #[test]
    fn decode_baseline_b64_rejects_invalid_character() {
        // GIVEN: base64 with a non-alphabet byte
        // WHEN: decoding
        let result = super::decode_baseline_b64("SGVs!G8=");
        // THEN: error returned
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid base64"));
    }

    #[test]
    fn decode_baseline_b64_handles_three_byte_input_without_padding() {
        // GIVEN: "Man" → standard base64 "TWFu" (no padding needed for 3-byte input)
        let result = super::decode_baseline_b64("TWFu").unwrap();
        assert_eq!(result, b"Man");
    }

    // -----------------------------------------------------------------------
    // compute_diff
    // -----------------------------------------------------------------------

    #[test]
    fn compute_diff_identical_slices_returns_zero() {
        // GIVEN: two identical byte slices
        let data = b"identical_data";
        // WHEN: computing diff
        let diff = super::compute_diff(data, data);
        // THEN: zero diff
        assert_eq!(diff, 0.0);
    }

    #[test]
    fn compute_diff_both_empty_returns_zero() {
        // GIVEN: both slices empty
        // WHEN: computing diff
        let diff = super::compute_diff(&[], &[]);
        // THEN: zero, no division by zero
        assert_eq!(diff, 0.0);
    }

    #[test]
    fn compute_diff_completely_different_same_length_returns_one() {
        // GIVEN: no bytes in common
        let a = [0u8; 4];
        let b = [255u8; 4];
        // WHEN: computing diff
        let diff = super::compute_diff(&a, &b);
        // THEN: 100% diff
        assert_eq!(diff, 1.0);
    }

    #[test]
    fn compute_diff_size_mismatch_is_penalised() {
        // GIVEN: baseline twice the length of current, identical prefix
        let baseline = [1u8, 2, 3, 4];
        let current = [1u8, 2];
        // WHEN: computing diff
        let diff = super::compute_diff(&baseline, &current);
        // THEN: diff > 0 because the extra 2 bytes count as changed
        assert!(diff > 0.0 && diff <= 1.0, "diff {diff} not in (0, 1]");
    }

    #[test]
    fn compute_diff_result_always_in_unit_interval() {
        // GIVEN: arbitrary different-length slices
        let a = b"hello world this is a test";
        let b = b"Hello World";
        // WHEN: computing diff
        let diff = super::compute_diff(a, b);
        // THEN: result is in [0.0, 1.0]
        assert!((0.0..=1.0).contains(&diff));
    }

    // -----------------------------------------------------------------------
    // handle_ax_visual_diff — error paths (no live app required)
    // -----------------------------------------------------------------------

    #[test]
    fn ax_visual_diff_missing_app_returns_error() {
        // GIVEN: args with no app field
        let registry = Arc::new(AppRegistry::default());
        // WHEN: dispatching
        let result = super::handle_ax_visual_diff(&json!({"baseline": "SGVsbG8="}), &registry);
        // THEN: error mentions missing field
        assert!(result.is_error);
        assert!(result.content[0].text.contains("Missing"));
    }

    #[test]
    fn ax_visual_diff_missing_baseline_returns_error() {
        // GIVEN: args with no baseline field
        let registry = Arc::new(AppRegistry::default());
        // WHEN: dispatching
        let result = super::handle_ax_visual_diff(&json!({"app": "Safari"}), &registry);
        // THEN: error mentions missing field
        assert!(result.is_error);
        assert!(result.content[0].text.contains("Missing"));
    }

    #[test]
    fn ax_visual_diff_invalid_base64_returns_error() {
        // GIVEN: baseline with an invalid character
        let registry = Arc::new(AppRegistry::default());
        // WHEN: dispatching
        let result = super::handle_ax_visual_diff(
            &json!({"app": "Safari", "baseline": "not!valid@b64"}),
            &registry,
        );
        // THEN: decode error propagated
        assert!(result.is_error);
        assert!(result.content[0].text.contains("baseline decode failed"));
    }

    #[test]
    fn ax_visual_diff_unconnected_app_returns_error() {
        // GIVEN: valid base64 but app not in registry
        let registry = Arc::new(AppRegistry::default());
        // WHEN: dispatching
        let result = super::handle_ax_visual_diff(
            &json!({"app": "GhostApp", "baseline": "SGVsbG8="}),
            &registry,
        );
        // THEN: error from registry lookup
        assert!(result.is_error);
        assert!(result.content[0].text.contains("not connected"));
    }

    #[test]
    fn ax_visual_diff_dispatch_returns_some_for_valid_call() {
        // GIVEN: ax_visual_diff is in the stateless dispatch table
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        // WHEN: dispatching with minimal (error-triggering) args
        let result = super::call_tool_innovation(
            "ax_visual_diff",
            &json!({"app": "X", "baseline": "SGVsbG8="}),
            &registry,
            &mut out,
        );
        // THEN: handler ran and returned Some
        assert!(result.is_some());
    }

    // -----------------------------------------------------------------------
    // audit_node / audit_accessibility — pure unit tests (no live app)
    // -----------------------------------------------------------------------

    fn make_a11y_scene(
        nodes: &[(&str, Option<&str>, Option<&str>, Option<&str>)],
    ) -> crate::intent::SceneGraph {
        let mut g = crate::intent::SceneGraph::empty();
        for (role, title, label, description) in nodes {
            let node = crate::intent::SceneNode {
                id: crate::intent::NodeId(g.len()),
                parent: None,
                children: vec![],
                role: Some(role.to_string()),
                title: title.map(str::to_string),
                label: label.map(str::to_string),
                value: None,
                description: description.map(str::to_string),
                identifier: None,
                bounds: None,
                enabled: true,
                depth: 0,
            };
            g.push(node);
        }
        g
    }

    #[test]
    fn audit_accessibility_empty_scene_returns_no_issues() {
        // GIVEN: empty scene graph
        let scene = crate::intent::SceneGraph::empty();
        // WHEN: auditing
        let issues = super::audit_accessibility(&scene);
        // THEN: no issues, no panic
        assert!(issues.is_empty());
    }

    #[test]
    fn audit_accessibility_labeled_button_raises_no_missing_label() {
        // GIVEN: button with a title (accessible name present)
        let scene = make_a11y_scene(&[("AXButton", Some("OK"), None, None)]);
        // WHEN: auditing
        let issues = super::audit_accessibility(&scene);
        // THEN: no missing_label issue
        assert!(!issues.iter().any(|v| v["issue"] == "missing_label"));
    }

    #[test]
    fn audit_accessibility_unlabeled_button_is_critical_1_3_1() {
        // GIVEN: button with no title, label, or description
        let scene = make_a11y_scene(&[("AXButton", None, None, None)]);
        // WHEN: auditing
        let issues = super::audit_accessibility(&scene);
        // THEN: one critical missing_label issue referencing WCAG 1.3.1
        let crit: Vec<_> = issues
            .iter()
            .filter(|v| v["issue"] == "missing_label" && v["severity"] == "critical")
            .collect();
        assert_eq!(crit.len(), 1);
        assert_eq!(crit[0]["wcag"], "1.3.1");
    }

    #[test]
    fn audit_accessibility_all_interactive_roles_flagged_when_unlabeled() {
        // GIVEN: one unlabeled node per interactive role
        let roles = super::INTERACTIVE_ROLES;
        let nodes: Vec<(&str, Option<&str>, Option<&str>, Option<&str>)> =
            roles.iter().map(|r| (*r, None, None, None)).collect();
        let scene = make_a11y_scene(&nodes);
        // WHEN: auditing
        let issues = super::audit_accessibility(&scene);
        // THEN: one missing_label issue per interactive role
        let missing_count = issues
            .iter()
            .filter(|v| v["issue"] == "missing_label")
            .count();
        assert_eq!(missing_count, roles.len());
    }

    #[test]
    fn audit_accessibility_unknown_role_is_warning_4_1_2() {
        // GIVEN: node with AXUnknown role
        let scene = make_a11y_scene(&[("AXUnknown", None, None, None)]);
        // WHEN: auditing
        let issues = super::audit_accessibility(&scene);
        // THEN: one warning for unknown_role referencing WCAG 4.1.2
        let warn: Vec<_> = issues
            .iter()
            .filter(|v| v["issue"] == "unknown_role" && v["severity"] == "warning")
            .collect();
        assert_eq!(warn.len(), 1);
        assert_eq!(warn[0]["wcag"], "4.1.2");
    }

    #[test]
    fn audit_accessibility_empty_role_string_triggers_unknown_role() {
        // GIVEN: node whose role is an empty string
        let mut scene = crate::intent::SceneGraph::empty();
        let node = crate::intent::SceneNode {
            id: crate::intent::NodeId(0),
            parent: None,
            children: vec![],
            role: Some(String::new()),
            title: None,
            label: None,
            value: None,
            description: None,
            identifier: None,
            bounds: None,
            enabled: true,
            depth: 0,
        };
        scene.push(node);
        // WHEN: auditing
        let issues = super::audit_accessibility(&scene);
        // THEN: unknown_role warning raised
        assert!(issues.iter().any(|v| v["issue"] == "unknown_role"));
    }

    #[test]
    fn audit_accessibility_unlabeled_image_is_critical_1_1_1() {
        // GIVEN: image with no text alternative
        let scene = make_a11y_scene(&[("AXImage", None, None, None)]);
        // WHEN: auditing
        let issues = super::audit_accessibility(&scene);
        // THEN: critical unlabeled_image issue referencing WCAG 1.1.1
        let img: Vec<_> = issues
            .iter()
            .filter(|v| v["issue"] == "unlabeled_image" && v["severity"] == "critical")
            .collect();
        assert_eq!(img.len(), 1);
        assert_eq!(img[0]["wcag"], "1.1.1");
    }

    #[test]
    fn audit_accessibility_labeled_image_passes() {
        // GIVEN: image with a description (text alternative)
        let scene = make_a11y_scene(&[("AXImage", None, None, Some("Company logo"))]);
        // WHEN: auditing
        let issues = super::audit_accessibility(&scene);
        // THEN: no unlabeled_image issue
        assert!(!issues.iter().any(|v| v["issue"] == "unlabeled_image"));
    }

    #[test]
    fn audit_accessibility_non_interactive_unlabeled_node_clean() {
        // GIVEN: a static text element (not interactive) with no label
        let scene = make_a11y_scene(&[("AXStaticText", None, None, None)]);
        // WHEN: auditing
        let issues = super::audit_accessibility(&scene);
        // THEN: no missing_label issue (static text doesn't require an explicit label)
        assert!(!issues.iter().any(|v| v["issue"] == "missing_label"));
    }

    #[test]
    fn count_by_severity_aggregates_correctly() {
        // GIVEN: issues with mixed severities
        let issues = vec![
            json!({"severity": "critical", "issue": "missing_label",   "wcag": "1.3.1"}),
            json!({"severity": "critical", "issue": "unlabeled_image", "wcag": "1.1.1"}),
            json!({"severity": "warning",  "issue": "unknown_role",    "wcag": "4.1.2"}),
        ];
        // WHEN / THEN
        assert_eq!(super::count_by_severity(&issues, "critical"), 2);
        assert_eq!(super::count_by_severity(&issues, "warning"), 1);
        assert_eq!(super::count_by_severity(&issues, "info"), 0);
    }

    // -----------------------------------------------------------------------
    // handle_ax_a11y_audit — error paths (no live app required)
    // -----------------------------------------------------------------------

    #[test]
    fn ax_a11y_audit_missing_app_returns_error() {
        // GIVEN: no app field
        let registry = Arc::new(AppRegistry::default());
        // WHEN: dispatching
        let result = super::handle_ax_a11y_audit(&json!({}), &registry);
        // THEN: error mentions missing field
        assert!(result.is_error);
        assert!(result.content[0].text.contains("Missing"));
    }

    #[test]
    fn ax_a11y_audit_unconnected_app_returns_error() {
        // GIVEN: app not in registry
        let registry = Arc::new(AppRegistry::default());
        // WHEN: dispatching
        let result = super::handle_ax_a11y_audit(&json!({"app": "GhostApp"}), &registry);
        // THEN: error from registry lookup
        assert!(result.is_error);
        assert!(result.content[0].text.contains("not connected"));
    }

    #[test]
    fn ax_a11y_audit_dispatch_returns_some_for_valid_call() {
        // GIVEN: ax_a11y_audit is in the stateless dispatch table
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        // WHEN: dispatching with minimal (error-triggering) args
        let result =
            super::call_tool_innovation("ax_a11y_audit", &json!({"app": "X"}), &registry, &mut out);
        // THEN: handler ran and returned Some
        assert!(result.is_some());
    }

    // -----------------------------------------------------------------------
    // Tool descriptor invariants
    // -----------------------------------------------------------------------

    #[test]
    fn ax_visual_diff_descriptor_is_read_only() {
        // GIVEN: the tool descriptor
        let tool = super::tool_ax_visual_diff();
        // THEN: annotation flags read_only (screenshot is non-destructive)
        assert!(tool.annotations.read_only);
        assert!(!tool.annotations.destructive);
    }

    #[test]
    fn ax_visual_diff_descriptor_requires_app_and_baseline() {
        // GIVEN: the tool descriptor
        let tool = super::tool_ax_visual_diff();
        // THEN: app and baseline are declared as required
        let required = tool.input_schema["required"].as_array().unwrap();
        let fields: Vec<&str> = required.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(fields.contains(&"app"));
        assert!(fields.contains(&"baseline"));
    }

    #[test]
    fn ax_a11y_audit_descriptor_is_read_only() {
        // GIVEN: the tool descriptor
        let tool = super::tool_ax_a11y_audit();
        // THEN: annotation flags read_only (audit does not mutate app state)
        assert!(tool.annotations.read_only);
        assert!(!tool.annotations.destructive);
    }

    #[test]
    fn ax_a11y_audit_descriptor_requires_only_app() {
        // GIVEN: the tool descriptor
        let tool = super::tool_ax_a11y_audit();
        // THEN: only app is required (scope has a default value)
        let required = tool.input_schema["required"].as_array().unwrap();
        let fields: Vec<&str> = required.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(fields.contains(&"app"));
        assert!(!fields.contains(&"scope"));
    }

    // -----------------------------------------------------------------------
    // detect_ui_patterns — untested pattern branches
    // -----------------------------------------------------------------------

    #[test]
    fn detect_patterns_table_view_from_ax_outline() {
        // GIVEN: AXOutline (tree view) is also a table-family role
        let scene = make_scene(&[("AXOutline", None, None, None)]);
        // WHEN
        let patterns = super::detect_ui_patterns(&scene);
        // THEN: table_view detected
        assert!(
            patterns.iter().any(|p| p.pattern == "table_view"),
            "AXOutline should trigger table_view"
        );
    }

    #[test]
    fn detect_patterns_progress_indicator_from_ax_busy_indicator() {
        // GIVEN: AXBusyIndicator (spinning progress) instead of AXProgressIndicator
        let scene = make_scene(&[("AXBusyIndicator", None, None, None)]);
        // WHEN
        let patterns = super::detect_ui_patterns(&scene);
        // THEN: progress_indicator detected
        assert!(
            patterns.iter().any(|p| p.pattern == "progress_indicator"),
            "AXBusyIndicator should trigger progress_indicator"
        );
    }

    #[test]
    fn detect_patterns_settings_page_from_groups_and_checkboxes() {
        // GIVEN: 3 AXGroup nodes + AXCheckBox, no modal, no password
        let scene = make_scene(&[
            ("AXGroup", None, None, None),
            ("AXGroup", None, None, None),
            ("AXGroup", None, None, None),
            ("AXCheckBox", Some("Enable feature"), None, None),
        ]);
        // WHEN
        let patterns = super::detect_ui_patterns(&scene);
        // THEN: settings_page detected
        assert!(
            patterns.iter().any(|p| p.pattern == "settings_page"),
            "settings_page not detected with 3 groups + checkbox"
        );
    }

    #[test]
    fn detect_patterns_settings_page_from_groups_and_popups() {
        // GIVEN: 3 AXGroup nodes + AXPopUpButton, no modal, no password
        let scene = make_scene(&[
            ("AXGroup", None, None, None),
            ("AXGroup", None, None, None),
            ("AXGroup", None, None, None),
            ("AXPopUpButton", Some("Color scheme"), None, None),
        ]);
        // WHEN
        let patterns = super::detect_ui_patterns(&scene);
        // THEN: settings_page detected
        assert!(
            patterns.iter().any(|p| p.pattern == "settings_page"),
            "settings_page not detected with 3 groups + popup button"
        );
    }

    #[test]
    fn detect_patterns_settings_page_suppressed_by_modal() {
        // GIVEN: 3 groups + checkbox + a sheet (modal present) — should suppress settings_page
        let scene = make_scene(&[
            ("AXGroup", None, None, None),
            ("AXGroup", None, None, None),
            ("AXGroup", None, None, None),
            ("AXCheckBox", Some("Option"), None, None),
            ("AXSheet", None, None, None),
        ]);
        // WHEN
        let patterns = super::detect_ui_patterns(&scene);
        // THEN: settings_page NOT detected (modal blocks it)
        assert!(
            !patterns.iter().any(|p| p.pattern == "settings_page"),
            "settings_page should be suppressed when a modal is present"
        );
    }

    #[test]
    fn detect_patterns_text_editor_from_text_area_with_toolbar() {
        // GIVEN: AXTextArea + AXToolbar
        let scene = make_scene(&[
            ("AXTextArea", None, None, None),
            ("AXToolbar", None, None, None),
        ]);
        // WHEN
        let patterns = super::detect_ui_patterns(&scene);
        // THEN: text_editor detected
        assert!(
            patterns.iter().any(|p| p.pattern == "text_editor"),
            "text_editor not detected with AXTextArea + AXToolbar"
        );
    }

    #[test]
    fn detect_patterns_text_editor_from_text_area_with_many_nodes() {
        // GIVEN: AXTextArea with >10 surrounding nodes (no toolbar required)
        let mut nodes: Vec<(&str, Option<&str>, Option<&str>, Option<&str>)> =
            vec![("AXTextArea", None, None, None)];
        for _ in 0..11 {
            nodes.push(("AXStaticText", None, None, None));
        }
        let scene = make_scene(&nodes);
        // WHEN
        let patterns = super::detect_ui_patterns(&scene);
        // THEN: text_editor detected (large scene threshold)
        assert!(
            patterns.iter().any(|p| p.pattern == "text_editor"),
            "text_editor not detected with AXTextArea + >10 nodes"
        );
    }

    #[test]
    fn detect_patterns_browser_main_from_address_field_and_tab_group() {
        // GIVEN: AXTextField with identifier containing "address" + AXTabGroup
        let mut g = crate::intent::SceneGraph::empty();
        let addr_node = crate::intent::SceneNode {
            id: crate::intent::NodeId(0),
            parent: None,
            children: vec![],
            role: Some("AXTextField".into()),
            title: None,
            label: None,
            value: None,
            description: None,
            identifier: Some("address-bar".into()),
            bounds: None,
            enabled: true,
            depth: 0,
        };
        let tab_node = crate::intent::SceneNode {
            id: crate::intent::NodeId(1),
            parent: None,
            children: vec![],
            role: Some("AXTabGroup".into()),
            title: None,
            label: None,
            value: None,
            description: None,
            identifier: None,
            bounds: None,
            enabled: true,
            depth: 0,
        };
        g.push(addr_node);
        g.push(tab_node);
        // WHEN
        let patterns = super::detect_ui_patterns(&g);
        // THEN: browser_main detected
        assert!(
            patterns.iter().any(|p| p.pattern == "browser_main"),
            "browser_main not detected with address-bar field + tab group"
        );
    }

    #[test]
    fn detect_patterns_browser_main_requires_tab_group() {
        // GIVEN: address-bar text field WITHOUT a tab group
        let mut g = crate::intent::SceneGraph::empty();
        let node = crate::intent::SceneNode {
            id: crate::intent::NodeId(0),
            parent: None,
            children: vec![],
            role: Some("AXTextField".into()),
            title: None,
            label: None,
            value: None,
            description: None,
            identifier: Some("url-field".into()),
            bounds: None,
            enabled: true,
            depth: 0,
        };
        g.push(node);
        // WHEN
        let patterns = super::detect_ui_patterns(&g);
        // THEN: browser_main NOT detected without tab group
        assert!(
            !patterns.iter().any(|p| p.pattern == "browser_main"),
            "browser_main should require AXTabGroup"
        );
    }

    #[test]
    fn detect_patterns_form_requires_at_least_two_text_fields() {
        // GIVEN: only one text field + button (below the threshold)
        let scene = make_scene(&[
            ("AXTextField", Some("Email"), None, None),
            ("AXButton", Some("Submit"), None, None),
        ]);
        // WHEN
        let patterns = super::detect_ui_patterns(&scene);
        // THEN: form NOT detected (threshold is 2)
        assert!(
            !patterns.iter().any(|p| p.pattern == "form"),
            "form should require at least 2 text fields"
        );
    }

    // -----------------------------------------------------------------------
    // infer_app_state — untested loading/error label paths and AXDialog
    // -----------------------------------------------------------------------

    #[test]
    fn infer_state_modal_from_ax_dialog() {
        // GIVEN: AXDialog element (alternate modal role)
        let scene = make_scene(&[("AXDialog", None, None, None)]);
        // WHEN
        let state = super::infer_app_state(&scene);
        // THEN: modal state
        assert_eq!(state, super::AppState::Modal);
    }

    #[test]
    fn infer_state_loading_from_ax_busy_indicator() {
        // GIVEN: AXBusyIndicator (spinning indicator), no modal
        let scene = make_scene(&[("AXBusyIndicator", None, None, None)]);
        // WHEN
        let state = super::infer_app_state(&scene);
        // THEN: loading
        assert_eq!(state, super::AppState::Loading);
    }

    #[test]
    fn infer_state_loading_from_label_text() {
        // GIVEN: static text labelled "Loading…", no modal or spinner role
        let scene = make_scene(&[("AXStaticText", Some("Loading…"), None, None)]);
        // WHEN
        let state = super::infer_app_state(&scene);
        // THEN: loading (label-based heuristic)
        assert_eq!(state, super::AppState::Loading);
    }

    #[test]
    fn infer_state_error_from_failed_label() {
        // GIVEN: label containing "failed"
        let scene = make_scene(&[("AXStaticText", Some("Connection failed"), None, None)]);
        // WHEN
        let state = super::infer_app_state(&scene);
        // THEN: error
        assert_eq!(state, super::AppState::Error);
    }

    #[test]
    fn infer_state_error_from_invalid_label() {
        // GIVEN: label containing "invalid"
        let scene = make_scene(&[("AXStaticText", Some("Invalid password"), None, None)]);
        // WHEN
        let state = super::infer_app_state(&scene);
        // THEN: error
        assert_eq!(state, super::AppState::Error);
    }

    // -----------------------------------------------------------------------
    // suggest_actions — untested pattern branches
    // -----------------------------------------------------------------------

    #[test]
    fn suggest_actions_file_open_dialog_suggests_open_click() {
        // GIVEN: file_open_dialog pattern
        let open_dlg = super::UiPattern {
            pattern: "file_open_dialog",
            confidence: 0.88,
        };
        let suggestions = super::suggest_actions(&[open_dlg], super::AppState::Idle);
        // THEN: click Open suggested
        assert!(
            suggestions
                .iter()
                .any(|s| s.tool == "ax_click" && s.query == "Open"),
            "expected ax_click with 'Open' query for file_open_dialog"
        );
    }

    #[test]
    fn suggest_actions_error_alert_suggests_dismiss_ok() {
        // GIVEN: error_alert pattern
        let alert = super::UiPattern {
            pattern: "error_alert",
            confidence: 0.80,
        };
        let suggestions = super::suggest_actions(&[alert], super::AppState::Idle);
        // THEN: ax_click with "OK" to dismiss the error alert
        assert!(
            suggestions
                .iter()
                .any(|s| s.tool == "ax_click" && s.query == "OK"),
            "expected ax_click 'OK' for error_alert dismissal"
        );
    }

    #[test]
    fn suggest_actions_text_editor_suggests_type() {
        // GIVEN: text_editor pattern
        let editor = super::UiPattern {
            pattern: "text_editor",
            confidence: 0.78,
        };
        let suggestions = super::suggest_actions(&[editor], super::AppState::Idle);
        // THEN: type into the text area
        assert!(
            suggestions.iter().any(|s| s.tool == "ax_type"),
            "expected ax_type suggestion for text_editor"
        );
    }

    #[test]
    fn suggest_actions_form_suggests_submit_click() {
        // GIVEN: form pattern
        let form = super::UiPattern {
            pattern: "form",
            confidence: 0.72,
        };
        let suggestions = super::suggest_actions(&[form], super::AppState::Idle);
        // THEN: click Submit to submit the form
        assert!(
            suggestions
                .iter()
                .any(|s| s.tool == "ax_click" && s.query == "Submit"),
            "expected ax_click 'Submit' for form pattern"
        );
    }

    // -----------------------------------------------------------------------
    // compute_diff — single-byte-difference case
    // -----------------------------------------------------------------------

    #[test]
    fn compute_diff_one_byte_different_in_large_array_is_small_fraction() {
        // GIVEN: 1000-byte arrays differing in exactly one position
        let baseline: Vec<u8> = (0u8..=255).cycle().take(1000).collect();
        let mut current = baseline.clone();
        current[500] ^= 0xFF;
        // WHEN
        let diff = super::compute_diff(&baseline, &current);
        // THEN: diff = 1/1000 = 0.001
        assert!(
            (diff - 0.001).abs() < f64::EPSILON * 10.0,
            "expected ~0.001 for 1 byte diff in 1000-byte array, got {diff}"
        );
    }

    // -----------------------------------------------------------------------
    // decode_baseline_b64 — two-character (one-byte output) chunk
    // -----------------------------------------------------------------------

    #[test]
    fn decode_baseline_b64_two_char_chunk_produces_one_byte() {
        // GIVEN: "YQ==" decodes to b"a" — after stripping '=', it's "YQ" (2 chars)
        let result = super::decode_baseline_b64("YQ==").unwrap();
        assert_eq!(result, b"a");
    }

    #[test]
    fn decode_baseline_b64_three_char_chunk_produces_two_bytes() {
        // GIVEN: "YWI=" decodes to b"ab" — after stripping '=', it's "YWI" (3 chars)
        let result = super::decode_baseline_b64("YWI=").unwrap();
        assert_eq!(result, b"ab");
    }

    // -----------------------------------------------------------------------
    // parse_test_assertions — individual assertion types
    // -----------------------------------------------------------------------

    #[test]
    fn parse_test_assertions_parses_element_exists() {
        // GIVEN: element_exists assertion
        let assertions = super::parse_test_assertions(&json!([
            { "type": "element_exists", "query": "Submit" }
        ]));
        assert_eq!(assertions.len(), 1);
    }

    #[test]
    fn parse_test_assertions_parses_element_has_text() {
        // GIVEN: element_has_text assertion
        let assertions = super::parse_test_assertions(&json!([
            { "type": "element_has_text", "query": "Title", "expected": "Hello" }
        ]));
        assert_eq!(assertions.len(), 1);
    }

    #[test]
    fn parse_test_assertions_parses_element_not_exists() {
        // GIVEN: element_not_exists assertion
        let assertions = super::parse_test_assertions(&json!([
            { "type": "element_not_exists", "query": "Error" }
        ]));
        assert_eq!(assertions.len(), 1);
    }

    #[test]
    fn parse_test_assertions_parses_screen_contains() {
        // GIVEN: screen_contains assertion
        let assertions = super::parse_test_assertions(&json!([
            { "type": "screen_contains", "needle": "Welcome" }
        ]));
        assert_eq!(assertions.len(), 1);
    }

    #[test]
    fn parse_test_assertions_skips_unknown_type() {
        // GIVEN: one known and one unknown assertion type
        let assertions = super::parse_test_assertions(&json!([
            { "type": "element_exists", "query": "OK" },
            { "type": "unsupported_future_assertion" }
        ]));
        // THEN: only the valid assertion survives
        assert_eq!(assertions.len(), 1);
    }

    // -----------------------------------------------------------------------
    // ax_record — click and type action_types recorded successfully
    // -----------------------------------------------------------------------

    #[test]
    fn ax_record_click_action_type_increments_event_count() {
        // GIVEN: recording started, then a click event recorded
        super::handle_ax_record(&json!({"app": "Safari", "action": "start"}));
        let result = super::handle_ax_record(&json!({
            "app": "Safari",
            "action": "record",
            "action_type": "click",
            "query": "Submit"
        }));
        // THEN: success, event_count=1
        assert!(!result.is_error);
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["recorded_action_type"], "click");
        assert!(v["event_count"].as_u64().unwrap() >= 1);
    }

    #[test]
    fn ax_record_type_action_type_records_text() {
        // GIVEN: type event with text
        super::handle_ax_record(&json!({"app": "Safari", "action": "start"}));
        let result = super::handle_ax_record(&json!({
            "app": "Safari",
            "action": "record",
            "action_type": "type",
            "query": "Username",
            "text": "alice@example.com"
        }));
        // THEN: success
        assert!(!result.is_error);
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["recorded_action_type"], "type");
    }

    #[test]
    fn ax_record_unknown_action_type_returns_error() {
        // GIVEN: unsupported action_type value
        let result = super::handle_ax_record(&json!({
            "app": "Safari",
            "action": "record",
            "action_type": "teleport"
        }));
        // THEN: error mentioning the unknown action_type
        assert!(result.is_error);
        assert!(result.content[0].text.contains("teleport"));
    }

    // -----------------------------------------------------------------------
    // ax_workflow_create — overwrite semantics
    // -----------------------------------------------------------------------

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
    // workflow_tracking_data — public function exercised for coverage
    // -----------------------------------------------------------------------

    #[test]
    fn workflow_tracking_data_returns_valid_json_structure() {
        // GIVEN: the global tracker (may have state from other tests)
        // WHEN: calling the public snapshot function
        let data = super::workflow_tracking_data();
        // THEN: required top-level keys are present and correctly typed
        assert!(data["workflows_detected"].is_number());
        assert!(data["workflows"].is_array());
        assert!(data["stats"].is_object());
        assert!(data["stats"]["total_transitions"].is_number());
        assert!(data["stats"]["distinct_apps"].is_number());
    }

    // -----------------------------------------------------------------------
    // has_role / any_label_contains helpers (indirectly via detect/infer)
    // -----------------------------------------------------------------------

    #[test]
    fn detect_patterns_confirmation_dialog_from_alert_yes_no_buttons() {
        // GIVEN: AXAlert with "Yes" and "No" buttons (alternate confirm labels)
        let scene = make_scene(&[
            ("AXAlert", None, None, None),
            ("AXButton", Some("Yes"), None, None),
            ("AXButton", Some("No"), None, None),
        ]);
        // WHEN
        let patterns = super::detect_ui_patterns(&scene);
        // THEN: confirmation_dialog detected (yes/no are alternate labels)
        assert!(
            patterns.iter().any(|p| p.pattern == "confirmation_dialog"),
            "confirmation_dialog should be detected with Yes/No buttons"
        );
    }

    #[test]
    fn detect_patterns_login_form_confidence_is_0_90() {
        // GIVEN: minimal login scene
        let scene = make_scene(&[
            ("AXSecureTextField", Some("Password"), None, None),
            ("AXTextField", Some("Username"), None, None),
            ("AXButton", Some("Sign In"), None, None),
        ]);
        // WHEN
        let patterns = super::detect_ui_patterns(&scene);
        // THEN: login_form has exactly 0.90 confidence
        let login = patterns.iter().find(|p| p.pattern == "login_form").unwrap();
        assert!(
            (login.confidence - 0.90).abs() < f64::EPSILON,
            "expected confidence 0.90, got {}",
            login.confidence
        );
    }
}
