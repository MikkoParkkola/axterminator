use std::sync::{Arc, Mutex};

use serde_json::json;

use crate::mcp::tools::AppRegistry;

/// Mutex to serialize tests that mutate `AXTERMINATOR_SECURITY_MODE`.
/// `std::env::set_var` / `remove_var` is not thread-safe — concurrent
/// tests that read/write the same env var produce non-deterministic
/// failures depending on execution order.
static ENV_MUTEX: Mutex<()> = Mutex::new(());

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
    let result =
        super::call_tool_innovation("ax_nonexistent_innovation", &json!({}), &registry, &mut out);
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
        let result = super::call_tool_innovation(name, &json!({"name": "wf"}), &registry, &mut out);
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
    let result = super::handle_ax_track_workflow(&json!({"app": "TestAppA", "action": "record"}));
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
) -> std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, super::WorkflowState>>> {
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
fn ax_workflow_create_rejects_unknown_fields() {
    // GIVEN: extra top-level fields outside the published schema
    let wf = make_workflows();
    let result =
        super::handle_ax_workflow_create(&json!({"name": "wf", "steps": [], "extra": true}), &wf);

    // THEN: runtime validation matches additionalProperties=false
    assert!(result.is_error);
    assert_eq!(result.content[0].text, "unknown field: extra");
}

#[test]
fn ax_workflow_create_rejects_null_steps() {
    // GIVEN: explicit null instead of an array
    let wf = make_workflows();
    let result = super::handle_ax_workflow_create(&json!({"name": "wf", "steps": null}), &wf);

    // THEN: explicit null does not silently degrade into an empty workflow
    assert!(result.is_error);
    assert_eq!(result.content[0].text, "Field 'steps' must be an array");
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
fn ax_workflow_create_rejects_duplicate_workflow_names() {
    // GIVEN: a workflow already stored under the requested name
    let wf = make_workflows();
    super::handle_ax_workflow_create(
        &json!({
            "name": "duplicate-wf",
            "steps": [{ "id": "s1", "action": "checkpoint" }]
        }),
        &wf,
    );

    // WHEN: creating another workflow with the same name
    let result = super::handle_ax_workflow_create(
        &json!({
            "name": "duplicate-wf",
            "steps": [{ "id": "s2", "action": "checkpoint" }]
        }),
        &wf,
    );

    // THEN: the existing workflow is preserved and the duplicate is rejected
    assert!(result.is_error);
    assert_eq!(
        result.content[0].text,
        "Workflow 'duplicate-wf' already exists — choose a unique name"
    );
    let guard = wf.lock().unwrap();
    assert_eq!(guard["duplicate-wf"].steps.len(), 1);
    assert_eq!(guard["duplicate-wf"].steps[0].id, "s1");
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
fn ax_workflow_step_rejects_unknown_fields() {
    // GIVEN: a valid workflow plus an extra field
    let wf = make_workflows();
    super::handle_ax_workflow_create(
        &json!({"name": "step-wf", "steps": [{ "id": "s1", "action": "checkpoint" }]}),
        &wf,
    );
    let mut out = Vec::<u8>::new();

    // WHEN: stepping with schema-invalid extra input
    let result =
        super::handle_ax_workflow_step(&json!({"name": "step-wf", "extra": true}), &wf, &mut out);

    // THEN: runtime rejects the unsupported field
    assert!(result.is_error);
    assert_eq!(result.content[0].text, "unknown field: extra");
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
    assert_eq!(v1["message"], "Recorded workflow step 'step-1'");
    assert_eq!(v2["completed"], true);
    assert_eq!(v2["step_id"], "step-2");
    assert_eq!(v2["message"], "Recorded workflow step 'step-2'");
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
fn ax_workflow_status_rejects_unknown_fields() {
    // GIVEN: a valid workflow plus an extra field
    let wf = make_workflows();
    super::handle_ax_workflow_create(
        &json!({"name": "status-wf", "steps": [{ "id": "s1", "action": "checkpoint" }]}),
        &wf,
    );

    // WHEN: checking status with schema-invalid extra input
    let result =
        super::handle_ax_workflow_status(&json!({"name": "status-wf", "extra": true}), &wf);

    // THEN: runtime rejects the unsupported field
    assert!(result.is_error);
    assert_eq!(result.content[0].text, "unknown field: extra");
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
fn parse_workflow_steps_rejects_null_steps() {
    // GIVEN: null value
    let result = super::parse_workflow_steps(&json!(null));
    // THEN: explicit null is rejected instead of degrading into an empty list
    assert_eq!(result.unwrap_err(), "Field 'steps' must be an array");
}

#[test]
fn parse_workflow_steps_rejects_unknown_actions() {
    // GIVEN: a step with an unsupported action
    let result = super::parse_workflow_steps(&json!([
        { "id": "s1", "action": "teleport" }
    ]));
    // THEN: workflow creation would fail instead of silently dropping the step
    assert_eq!(
        result.unwrap_err(),
        "Workflow step 's1' has unknown action: teleport"
    );
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
    ]))
    .unwrap();
    // THEN: all five steps parsed
    assert_eq!(steps.len(), 5);
}

#[test]
fn parse_workflow_steps_rejects_type_without_text() {
    // GIVEN: a type step missing its required text payload
    let result = super::parse_workflow_steps(&json!([
        { "id": "type-1", "action": "type", "target": "Field" }
    ]));

    // THEN: the parser rejects the malformed step
    assert_eq!(
        result.unwrap_err(),
        "Workflow step 'type-1' missing string field: text"
    );
}

#[test]
fn parse_workflow_steps_rejects_unknown_step_fields() {
    // GIVEN: a step carrying an unsupported key
    let result = super::parse_workflow_steps(&json!([
        { "id": "s1", "action": "checkpoint", "extra": true }
    ]));

    // THEN: the parser rejects the unknown field
    assert_eq!(result.unwrap_err(), "unknown field: extra");
}

#[test]
fn parse_workflow_steps_rejects_invalid_retry_config() {
    // GIVEN: a step with null retry config
    let result = super::parse_workflow_steps(&json!([
        { "id": "s1", "action": "checkpoint", "max_retries": null }
    ]));

    // THEN: retry config must be a non-negative integer
    assert_eq!(
        result.unwrap_err(),
        "Workflow field 'max_retries' must be a non-negative integer"
    );
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

/// (role, title, label, identifier) — describes one mock UI node.
type NodeTuple<'a> = (&'a str, Option<&'a str>, Option<&'a str>, Option<&'a str>);

// Helper: build a SceneGraph from a list of (role, title, label, identifier) tuples.
fn make_scene(nodes: &[NodeTuple<'_>]) -> crate::intent::SceneGraph {
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
    // Hold the env mutex — set_var/remove_var is not thread-safe.
    let _lock = ENV_MUTEX.lock().unwrap();
    // GIVEN: action=write but no text field, not sandboxed
    unsafe { std::env::remove_var("AXTERMINATOR_SECURITY_MODE") };
    // WHEN: dispatching
    let result = super::handle_ax_clipboard(&json!({"action": "write"}));
    // THEN: error payload about missing text
    assert!(result.is_error);
    assert!(result.content[0].text.contains("text"));
}

#[test]
fn ax_clipboard_write_blocked_in_sandboxed_mode() {
    // Hold the env mutex — set_var/remove_var is not thread-safe.
    let _lock = ENV_MUTEX.lock().unwrap();
    // GIVEN: sandboxed mode
    unsafe { std::env::set_var("AXTERMINATOR_SECURITY_MODE", "sandboxed") };
    // WHEN: dispatching a write
    let result = super::handle_ax_clipboard(&json!({"action": "write", "text": "hello"}));
    // THEN: error payload about sandboxed mode
    assert!(result.is_error);
    assert!(result.content[0].text.contains("sandboxed"));
    // cleanup
    unsafe { std::env::remove_var("AXTERMINATOR_SECURITY_MODE") };
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
    // Hold the env mutex — set_var/remove_var is not thread-safe.
    let _lock = ENV_MUTEX.lock().unwrap();
    // GIVEN: sandboxed mode set in the environment
    unsafe { std::env::set_var("AXTERMINATOR_SECURITY_MODE", "sandboxed") };
    let registry = Arc::new(AppRegistry::default());
    // WHEN: calling the handler
    let result = super::handle_ax_session_info(&json!({}), &registry);
    // THEN: security_mode field is "sandboxed"
    let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
    assert_eq!(v["security_mode"], "sandboxed");
    // cleanup
    unsafe { std::env::remove_var("AXTERMINATOR_SECURITY_MODE") };
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

fn make_a11y_scene(nodes: &[NodeTuple<'_>]) -> crate::intent::SceneGraph {
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
    let nodes: Vec<NodeTuple<'_>> = roles.iter().map(|r| (*r, None, None, None)).collect();
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
    let mut nodes: Vec<NodeTuple<'_>> = vec![("AXTextArea", None, None, None)];
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
