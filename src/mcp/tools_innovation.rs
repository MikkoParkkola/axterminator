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

use std::sync::{Arc, Mutex};

use once_cell::sync::Lazy;
use serde_json::{json, Value};

use crate::mcp::annotations;
use crate::mcp::protocol::{Tool, ToolCallResult};
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
// Tool declarations
// ---------------------------------------------------------------------------

/// All innovation tool descriptors.
pub(crate) fn innovation_tools() -> Vec<Tool> {
    vec![
        tool_ax_query(),
        tool_ax_app_profile(),
        tool_ax_test_run(),
        tool_ax_track_workflow(),
    ]
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

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

/// Dispatch an innovation tool call. Returns `None` if the tool name
/// is not recognised (allowing the caller to fall through).
pub(crate) fn call_tool_innovation(
    name: &str,
    args: &Value,
    registry: &Arc<AppRegistry>,
) -> Option<ToolCallResult> {
    match name {
        "ax_query" => Some(handle_ax_query(args, registry)),
        "ax_app_profile" => Some(handle_ax_app_profile(args)),
        "ax_test_run" => Some(handle_ax_test_run(args)),
        "ax_track_workflow" => Some(handle_ax_track_workflow(args)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

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
fn handle_ax_test_run(args: &Value) -> ToolCallResult {
    let Some(app_name) = args["app"].as_str().map(str::to_string) else {
        return ToolCallResult::error("Missing required field: app");
    };
    let Some(test_name) = args["test_name"].as_str().map(str::to_string) else {
        return ToolCallResult::error("Missing required field: test_name");
    };

    let steps = parse_test_steps(&args["steps"]);
    let assertions = parse_test_assertions(&args["assertions"]);

    let case = crate::blackbox::TestCase {
        name: test_name,
        steps,
        assertions,
    };

    let tester = crate::blackbox::BlackboxTester::new(&app_name);
    let result = tester.run(&case);

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

    arr.iter()
        .filter_map(parse_single_assertion)
        .collect()
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
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use crate::mcp::tools::AppRegistry;

    // -----------------------------------------------------------------------
    // innovation_tools descriptor invariants
    // -----------------------------------------------------------------------

    #[test]
    fn innovation_tools_registers_four_tools() {
        // GIVEN: Wave 2 implementation
        // WHEN: requesting descriptors
        let tools = super::innovation_tools();
        // THEN: exactly four tools registered
        assert_eq!(
            tools.len(),
            4,
            "expected 4 innovation tools, got {}",
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
        // THEN: expected names are all present
        for expected in &[
            "ax_query",
            "ax_app_profile",
            "ax_test_run",
            "ax_track_workflow",
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
        // WHEN: dispatching unknown name
        let result =
            super::call_tool_innovation("ax_nonexistent_innovation", &json!({}), &registry);
        // THEN: falls through cleanly
        assert!(result.is_none());
    }

    #[test]
    fn call_tool_innovation_empty_name_returns_none() {
        // GIVEN: empty name (malformed request)
        let registry = Arc::new(AppRegistry::default());
        // WHEN: dispatching empty name
        let result = super::call_tool_innovation("", &json!({}), &registry);
        // THEN: falls through cleanly
        assert!(result.is_none());
    }

    #[test]
    fn call_tool_innovation_recognises_all_four_names() {
        // GIVEN: all four innovation tool names
        let registry = Arc::new(AppRegistry::default());
        for name in &[
            "ax_query",
            "ax_app_profile",
            "ax_test_run",
            "ax_track_workflow",
        ] {
            // WHEN: dispatching with minimal args
            let result = super::call_tool_innovation(name, &json!({"app": "Ghost"}), &registry);
            // THEN: result is Some (handler ran, even if it returned an error payload)
            assert!(
                result.is_some(),
                "call_tool_innovation returned None for '{name}'"
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
        let result = super::handle_ax_test_run(&json!({"test_name": "t"}));
        assert!(result.is_error);
        assert!(result.content[0].text.contains("Missing"));
    }

    #[test]
    fn ax_test_run_missing_test_name_returns_error() {
        // GIVEN: no test_name field
        let result = super::handle_ax_test_run(&json!({"app": "TextEdit"}));
        assert!(result.is_error);
        assert!(result.content[0].text.contains("Missing"));
    }

    #[test]
    fn ax_test_run_empty_case_passes_with_no_steps() {
        // GIVEN: minimal args — no steps, no assertions
        // WHEN: running against a ghost app (no live process needed for empty case)
        let result =
            super::handle_ax_test_run(&json!({"app": "__ghost__", "test_name": "empty_test"}));
        // THEN: not an error; result payload has passed=true
        assert!(!result.is_error);
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["passed"], true);
        assert_eq!(v["test_name"], "empty_test");
        assert_eq!(v["steps_completed"], 0);
    }

    #[test]
    fn ax_test_run_with_wait_step_times_out_for_ghost_app() {
        // GIVEN: one WaitForElement step against an app that doesn't exist
        let result = super::handle_ax_test_run(&json!({
            "app": "__ghost__",
            "test_name": "wait_timeout",
            "steps": [
                { "type": "wait_for_element", "query": "Button", "timeout_ms": 1 }
            ]
        }));
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
}
