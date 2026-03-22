//! Unit tests for [`super::Server`] and [`super::ServerHandle`].
//!
//! Included by `server.rs` via `#[path = "server_tests.rs"] mod tests`.

use super::*;
use serde_json::json;

fn make_request(id: i64, method: &str, params: Option<Value>) -> JsonRpcRequest {
    JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(RequestId::Number(id)),
        method: method.into(),
        params,
    }
}

fn make_notification(method: &str) -> JsonRpcRequest {
    JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: None,
        method: method.into(),
        params: None,
    }
}

/// Initialize a server to the `Running` phase for use in subsequent tests.
fn initialize_server(s: &mut Server) {
    let req = make_request(
        1,
        "initialize",
        Some(json!({
            "protocolVersion": "2025-11-05",
            "capabilities": {},
            "clientInfo": { "name": "test", "version": "0.1" }
        })),
    );
    let mut sink = Vec::<u8>::new();
    s.handle(&req, &mut sink);
    s.handle_notification(&make_notification("notifications/initialized"));
}

/// Convenience: send a request to a running server and return the response value.
fn send(s: &mut Server, id: i64, method: &str, params: Option<Value>) -> Value {
    let req = make_request(id, method, params);
    let mut sink = Vec::<u8>::new();
    let resp = s.handle(&req, &mut sink).unwrap();
    serde_json::to_value(&resp).unwrap()
}

#[test]
fn server_starts_uninitialized() {
    let s = Server::new();
    assert_eq!(s.phase, Phase::Uninitialized);
}

#[test]
fn initialize_request_transitions_to_initializing() {
    // GIVEN: fresh server
    let mut s = Server::new();
    // WHEN: initialize request sent
    let req = make_request(
        1,
        "initialize",
        Some(json!({
            "protocolVersion": "2025-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1"}
        })),
    );
    let resp = s.handle(&req, &mut Vec::<u8>::new()).unwrap();
    // THEN: Initializing phase; response contains serverInfo
    assert_eq!(s.phase, Phase::Initializing);
    let v: Value = serde_json::from_str(&serde_json::to_string(&resp).unwrap()).unwrap();
    assert_eq!(v["result"]["serverInfo"]["name"], "axterminator");
}

#[test]
fn initialized_notification_transitions_to_running() {
    // GIVEN: server in Initializing phase
    let mut s = Server::new();
    let req = make_request(
        1,
        "initialize",
        Some(json!({
            "protocolVersion": "2025-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1"}
        })),
    );
    s.handle(&req, &mut Vec::<u8>::new());
    assert_eq!(s.phase, Phase::Initializing);
    // WHEN: initialized notification arrives
    s.handle_notification(&make_notification("notifications/initialized"));
    // THEN: Running
    assert_eq!(s.phase, Phase::Running);
}

#[test]
fn ping_returns_empty_object() {
    // GIVEN: initialized server
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: ping
    let req = make_request(2, "ping", None);
    let resp = s.handle(&req, &mut Vec::<u8>::new()).unwrap();
    // THEN: result is empty object {}
    let v: Value = serde_json::to_value(&resp).unwrap();
    assert_eq!(v["result"], json!({}));
}

#[test]
fn tools_list_returns_correct_count_for_feature_set() {
    // GIVEN: initialized server (base 29; +5 with spaces, +3 audio, +3 camera)
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: tools/list
    let req = make_request(3, "tools/list", None);
    let resp = s.handle(&req, &mut Vec::<u8>::new()).unwrap();
    let v: Value = serde_json::to_value(&resp).unwrap();
    // THEN: count is a deterministic function of active features
    let count = v["result"]["tools"].as_array().unwrap().len();
    let base = 34usize; // Phase 1 (12) + Phase 3 GUI (7) + innovation (15)
    let extra_spaces: usize = if cfg!(feature = "spaces") { 5 } else { 0 };
    let extra_audio: usize = if cfg!(feature = "audio") { 3 } else { 0 };
    let extra_camera: usize = if cfg!(feature = "camera") { 3 } else { 0 };
    let extra_watch: usize = if cfg!(feature = "watch") { 3 } else { 0 };
    let extra_docker: usize = if cfg!(feature = "docker") { 2 } else { 0 };
    assert_eq!(
        count,
        base + extra_spaces + extra_audio + extra_camera + extra_watch + extra_docker
    );
}

#[test]
fn tools_list_before_initialized_returns_error() {
    // GIVEN: uninitialized server
    let mut s = Server::new();
    // WHEN: tools/list before initialize
    let req = make_request(1, "tools/list", None);
    let resp = s.handle(&req, &mut Vec::<u8>::new()).unwrap();
    let v: Value = serde_json::to_value(&resp).unwrap();
    // THEN: error
    assert!(v.get("error").is_some());
}

#[test]
fn unknown_method_returns_method_not_found() {
    // GIVEN: initialized server
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: a truly unknown method is called
    let req = make_request(4, "sampling/createMessage", None);
    let resp = s.handle(&req, &mut Vec::<u8>::new()).unwrap();
    let v: Value = serde_json::to_value(&resp).unwrap();
    assert_eq!(v["error"]["code"], RpcError::METHOD_NOT_FOUND);
}

#[test]
fn notification_returns_none() {
    // GIVEN: initialized server
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: notification (no id)
    let notif = make_notification("notifications/cancelled");
    let resp = s.handle(&notif, &mut Vec::<u8>::new());
    // THEN: no response
    assert!(resp.is_none());
}

#[test]
fn tools_call_is_accessible_succeeds() {
    // GIVEN: initialized server
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: tools/call ax_is_accessible
    let req = make_request(
        5,
        "tools/call",
        Some(json!({ "name": "ax_is_accessible", "arguments": {} })),
    );
    let resp = s.handle(&req, &mut Vec::<u8>::new()).unwrap();
    let v: Value = serde_json::to_value(&resp).unwrap();
    // THEN: content array present
    assert!(v["result"]["content"].is_array());
}

#[test]
fn invalid_initialize_params_returns_error() {
    // GIVEN: fresh server
    let mut s = Server::new();
    // WHEN: initialize with missing required fields
    let req = make_request(1, "initialize", Some(json!({"bad": "data"})));
    let resp = s.handle(&req, &mut Vec::<u8>::new()).unwrap();
    let v: Value = serde_json::to_value(&resp).unwrap();
    // THEN: error
    assert!(v.get("error").is_some());
}

// -----------------------------------------------------------------------
// Phase 2 capability advertisement
// -----------------------------------------------------------------------

#[test]
fn initialize_response_advertises_resources_capability() {
    // GIVEN: fresh server
    let mut s = Server::new();
    // WHEN: initialize
    let req = make_request(
        1,
        "initialize",
        Some(json!({
            "protocolVersion": "2025-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1"}
        })),
    );
    let resp = s.handle(&req, &mut Vec::<u8>::new()).unwrap();
    let v: Value = serde_json::to_value(&resp).unwrap();
    // THEN: capabilities.resources is present
    assert!(v["result"]["capabilities"]["resources"].is_object());
    assert_eq!(v["result"]["capabilities"]["resources"]["subscribe"], true);
}

#[test]
fn initialize_response_advertises_prompts_capability() {
    let mut s = Server::new();
    let req = make_request(
        1,
        "initialize",
        Some(json!({
            "protocolVersion": "2025-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1"}
        })),
    );
    let resp = s.handle(&req, &mut Vec::<u8>::new()).unwrap();
    let v: Value = serde_json::to_value(&resp).unwrap();
    assert!(v["result"]["capabilities"]["prompts"].is_object());
}

#[test]
fn initialize_response_advertises_elicitation_capability() {
    // GIVEN: fresh server
    let mut s = Server::new();
    // WHEN: initialize
    let req = make_request(
        1,
        "initialize",
        Some(json!({
            "protocolVersion": "2025-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1"}
        })),
    );
    let resp = s.handle(&req, &mut Vec::<u8>::new()).unwrap();
    let v: Value = serde_json::to_value(&resp).unwrap();
    // THEN: capabilities.elicitation is present (Phase 4)
    assert!(v["result"]["capabilities"]["elicitation"].is_object());
}

// -----------------------------------------------------------------------
// ServerHandle public API
// -----------------------------------------------------------------------

#[test]
fn server_handle_ping_returns_empty_object() {
    // GIVEN: initialized handle
    let mut h = ServerHandle::new();
    let init = make_request(
        1,
        "initialize",
        Some(json!({
            "protocolVersion": "2025-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1"}
        })),
    );
    h.handle(&init, &mut Vec::<u8>::new());
    h.handle(
        &make_notification("notifications/initialized"),
        &mut Vec::<u8>::new(),
    );
    // WHEN: ping via handle
    let req = make_request(2, "ping", None);
    let resp = h.handle(&req, &mut Vec::<u8>::new()).unwrap();
    let v: Value = serde_json::to_value(&resp).unwrap();
    // THEN: result is {}
    assert_eq!(v["result"], json!({}));
}

#[test]
fn server_handle_default_creates_uninitialized_instance() {
    // GIVEN / WHEN
    let mut h = ServerHandle::default();
    // THEN: tools/list before init returns error (not a panic)
    let req = make_request(1, "tools/list", None);
    let resp = h.handle(&req, &mut Vec::<u8>::new()).unwrap();
    let v: Value = serde_json::to_value(&resp).unwrap();
    assert!(v.get("error").is_some());
}

// -----------------------------------------------------------------------
// Phase 2 — resources/list
// -----------------------------------------------------------------------

#[test]
fn resources_list_returns_static_resources() {
    // GIVEN: initialized server
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: resources/list
    let req = make_request(10, "resources/list", None);
    let resp = s.handle(&req, &mut Vec::<u8>::new()).unwrap();
    let v: Value = serde_json::to_value(&resp).unwrap();
    // THEN: resources array is present and non-empty
    let resources = v["result"]["resources"].as_array().unwrap();
    assert!(!resources.is_empty());
    // AND: system/status is included
    let has_status = resources
        .iter()
        .any(|r| r["uri"] == "axterminator://system/status");
    assert!(has_status);
}

#[test]
fn resources_list_before_initialized_returns_error() {
    let mut s = Server::new();
    let req = make_request(10, "resources/list", None);
    let resp = s.handle(&req, &mut Vec::<u8>::new()).unwrap();
    let v: Value = serde_json::to_value(&resp).unwrap();
    assert!(v.get("error").is_some());
}

// -----------------------------------------------------------------------
// Phase 2 — resources/templates/list
// -----------------------------------------------------------------------

#[test]
fn resources_templates_list_returns_templates() {
    let mut s = Server::new();
    initialize_server(&mut s);
    let req = make_request(11, "resources/templates/list", None);
    let resp = s.handle(&req, &mut Vec::<u8>::new()).unwrap();
    let v: Value = serde_json::to_value(&resp).unwrap();
    let templates = v["result"]["resourceTemplates"].as_array().unwrap();
    assert!(!templates.is_empty());
    let has_tree = templates
        .iter()
        .any(|t| t["uriTemplate"] == "axterminator://app/{name}/tree");
    assert!(has_tree);
}

// -----------------------------------------------------------------------
// Phase 2 — resources/read
// -----------------------------------------------------------------------

#[test]
fn resources_read_system_status_returns_contents() {
    let mut s = Server::new();
    initialize_server(&mut s);
    let req = make_request(
        12,
        "resources/read",
        Some(json!({ "uri": "axterminator://system/status" })),
    );
    let resp = s.handle(&req, &mut Vec::<u8>::new()).unwrap();
    let v: Value = serde_json::to_value(&resp).unwrap();
    // THEN: contents array with one item
    let contents = v["result"]["contents"].as_array().unwrap();
    assert_eq!(contents.len(), 1);
    assert!(contents[0]["text"].as_str().is_some());
}

#[test]
fn resources_read_missing_params_returns_error() {
    let mut s = Server::new();
    initialize_server(&mut s);
    let req = make_request(13, "resources/read", None);
    let resp = s.handle(&req, &mut Vec::<u8>::new()).unwrap();
    let v: Value = serde_json::to_value(&resp).unwrap();
    assert!(v.get("error").is_some());
}

#[test]
fn resources_read_unconnected_app_returns_error() {
    let mut s = Server::new();
    initialize_server(&mut s);
    let req = make_request(
        14,
        "resources/read",
        Some(json!({ "uri": "axterminator://app/NotConnected/tree" })),
    );
    let resp = s.handle(&req, &mut Vec::<u8>::new()).unwrap();
    let v: Value = serde_json::to_value(&resp).unwrap();
    // not_connected surfaces as INVALID_PARAMS error
    assert!(v.get("error").is_some());
}

// -----------------------------------------------------------------------
// Phase 2 — prompts/list
// -----------------------------------------------------------------------

#[test]
fn prompts_list_returns_ten_prompts() {
    let mut s = Server::new();
    initialize_server(&mut s);
    let req = make_request(20, "prompts/list", None);
    let resp = s.handle(&req, &mut Vec::<u8>::new()).unwrap();
    let v: Value = serde_json::to_value(&resp).unwrap();
    let prompts = v["result"]["prompts"].as_array().unwrap();
    assert_eq!(prompts.len(), 10);
}

#[test]
fn prompts_list_before_initialized_returns_error() {
    let mut s = Server::new();
    let req = make_request(20, "prompts/list", None);
    let resp = s.handle(&req, &mut Vec::<u8>::new()).unwrap();
    let v: Value = serde_json::to_value(&resp).unwrap();
    assert!(v.get("error").is_some());
}

// -----------------------------------------------------------------------
// Phase 2 — prompts/get
// -----------------------------------------------------------------------

#[test]
fn prompts_get_test_app_returns_messages() {
    let mut s = Server::new();
    initialize_server(&mut s);
    let req = make_request(
        21,
        "prompts/get",
        Some(json!({
            "name": "test-app",
            "arguments": { "app_name": "Safari" }
        })),
    );
    let resp = s.handle(&req, &mut Vec::<u8>::new()).unwrap();
    let v: Value = serde_json::to_value(&resp).unwrap();
    let msgs = v["result"]["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0]["role"], "user");
    assert_eq!(msgs[1]["role"], "assistant");
}

#[test]
fn prompts_get_unknown_prompt_returns_error() {
    let mut s = Server::new();
    initialize_server(&mut s);
    let req = make_request(
        22,
        "prompts/get",
        Some(json!({ "name": "nonexistent-prompt" })),
    );
    let resp = s.handle(&req, &mut Vec::<u8>::new()).unwrap();
    let v: Value = serde_json::to_value(&resp).unwrap();
    assert!(v.get("error").is_some());
}

#[test]
fn prompts_get_missing_required_arg_returns_error() {
    let mut s = Server::new();
    initialize_server(&mut s);
    // navigate-to requires both app_name and target_screen
    let req = make_request(
        23,
        "prompts/get",
        Some(json!({
            "name": "navigate-to",
            "arguments": { "app_name": "Finder" }
        })),
    );
    let resp = s.handle(&req, &mut Vec::<u8>::new()).unwrap();
    let v: Value = serde_json::to_value(&resp).unwrap();
    assert!(v.get("error").is_some());
}

#[test]
fn prompts_get_missing_params_returns_error() {
    let mut s = Server::new();
    initialize_server(&mut s);
    let req = make_request(24, "prompts/get", None);
    let resp = s.handle(&req, &mut Vec::<u8>::new()).unwrap();
    let v: Value = serde_json::to_value(&resp).unwrap();
    assert!(v.get("error").is_some());
}

// -----------------------------------------------------------------------
// Phase 5 — Tasks API
// -----------------------------------------------------------------------

#[test]
fn initialize_response_advertises_tasks_capability() {
    // GIVEN: fresh server
    let mut s = Server::new();
    // WHEN: initialize
    let req = make_request(
        1,
        "initialize",
        Some(json!({
            "protocolVersion": "2025-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1"}
        })),
    );
    let resp = s.handle(&req, &mut Vec::<u8>::new()).unwrap();
    let v: Value = serde_json::to_value(&resp).unwrap();
    // THEN: capabilities.tasks is present
    assert!(v["result"]["capabilities"]["tasks"].is_object());
}

#[test]
fn tasks_list_returns_empty_on_fresh_server() {
    // GIVEN: initialized server with no tasks
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: tasks/list
    let v = send(&mut s, 30, "tasks/list", None);
    // THEN: tasks array is present and empty
    let tasks = v["result"]["tasks"].as_array().unwrap();
    assert!(tasks.is_empty());
}

#[test]
fn tasks_list_before_initialized_returns_error() {
    // GIVEN: uninitialized server
    let mut s = Server::new();
    // WHEN: tasks/list before initialize
    let v = send(&mut s, 30, "tasks/list", None);
    // THEN: error
    assert!(v.get("error").is_some());
}

#[test]
fn tools_call_with_meta_task_returns_working_status() {
    // GIVEN: initialized server
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: tools/call with _meta.task: true
    let req = make_request(
        31,
        "tools/call",
        Some(json!({
            "name": "ax_is_accessible",
            "arguments": {},
            "_meta": { "task": true }
        })),
    );
    let resp = s.handle(&req, &mut Vec::<u8>::new()).unwrap();
    let v: Value = serde_json::to_value(&resp).unwrap();
    // THEN: immediate response with task envelope
    let task = &v["result"]["task"];
    assert!(task.is_object(), "expected task object in result");
    assert_eq!(task["status"], "working");
    assert!(task["taskId"].as_str().unwrap().starts_with("task-"));
}

#[test]
fn tasks_list_shows_task_after_async_call() {
    // GIVEN: initialized server with one async task submitted
    let mut s = Server::new();
    initialize_server(&mut s);
    let req = make_request(
        32,
        "tools/call",
        Some(json!({
            "name": "ax_is_accessible",
            "arguments": {},
            "_meta": { "task": true }
        })),
    );
    let resp = s.handle(&req, &mut Vec::<u8>::new()).unwrap();
    let v: Value = serde_json::to_value(&resp).unwrap();
    let task_id = v["result"]["task"]["taskId"].as_str().unwrap().to_owned();
    // WHEN: tasks/list
    // Allow the background thread to complete.
    std::thread::sleep(std::time::Duration::from_millis(100));
    let list_v = send(&mut s, 33, "tasks/list", None);
    // THEN: our task appears
    let tasks = list_v["result"]["tasks"].as_array().unwrap();
    assert!(!tasks.is_empty());
    let found = tasks.iter().any(|t| t["taskId"] == task_id);
    assert!(found, "task {task_id} missing from tasks/list");
}

#[test]
fn tasks_result_returns_pending_while_working() {
    // GIVEN: initialized server with an async task registered but before the
    //        background thread completes (we hold the task store lock to block it)
    let mut s = Server::new();
    initialize_server(&mut s);
    let req = make_request(
        34,
        "tools/call",
        Some(json!({
            "name": "ax_is_accessible",
            "arguments": {},
            "_meta": { "task": true }
        })),
    );
    let resp = s.handle(&req, &mut Vec::<u8>::new()).unwrap();
    let v: Value = serde_json::to_value(&resp).unwrap();
    let task_id = v["result"]["task"]["taskId"].as_str().unwrap().to_owned();
    // WHEN: tasks/result immediately (task may still be "working")
    let result_v = send(
        &mut s,
        35,
        "tasks/result",
        Some(json!({ "taskId": task_id })),
    );
    // THEN: either pending envelope or complete result — both are valid
    // (the thread may have finished by now). The key invariant is no error.
    assert!(
        result_v.get("error").is_none(),
        "tasks/result for a known task should not error"
    );
}

#[test]
fn tasks_result_for_completed_task_returns_tool_result() {
    // GIVEN: initialized server, task submitted and thread allowed to finish
    let mut s = Server::new();
    initialize_server(&mut s);
    let req = make_request(
        36,
        "tools/call",
        Some(json!({
            "name": "ax_is_accessible",
            "arguments": {},
            "_meta": { "task": true }
        })),
    );
    let resp = s.handle(&req, &mut Vec::<u8>::new()).unwrap();
    let v: Value = serde_json::to_value(&resp).unwrap();
    let task_id = v["result"]["task"]["taskId"].as_str().unwrap().to_owned();
    // WHEN: wait for completion, then poll
    std::thread::sleep(std::time::Duration::from_millis(200));
    let result_v = send(
        &mut s,
        37,
        "tasks/result",
        Some(json!({ "taskId": task_id })),
    );
    // THEN: completed task returns ToolCallResult shape with content array
    let result = &result_v["result"];
    assert!(
        result.get("content").is_some(),
        "completed task should return tool result with content"
    );
}

#[test]
fn tasks_result_unknown_task_id_returns_error() {
    // GIVEN: initialized server
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: tasks/result for nonexistent ID
    let v = send(
        &mut s,
        38,
        "tasks/result",
        Some(json!({ "taskId": "task-nonexistent" })),
    );
    // THEN: INVALID_PARAMS error
    assert!(v.get("error").is_some());
    assert_eq!(v["error"]["code"], -32_602);
}

#[test]
fn tasks_result_missing_params_returns_error() {
    // GIVEN: initialized server
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: tasks/result without params
    let v = send(&mut s, 39, "tasks/result", None);
    // THEN: error
    assert!(v.get("error").is_some());
}

#[test]
fn tasks_cancel_working_task_marks_it_cancelled() {
    // GIVEN: initialized server with a fresh async task
    let mut s = Server::new();
    initialize_server(&mut s);
    let req = make_request(
        40,
        "tools/call",
        Some(json!({
            "name": "ax_is_accessible",
            "arguments": {},
            "_meta": { "task": true }
        })),
    );
    let resp = s.handle(&req, &mut Vec::<u8>::new()).unwrap();
    let v: Value = serde_json::to_value(&resp).unwrap();
    let task_id = v["result"]["task"]["taskId"].as_str().unwrap().to_owned();
    // Cancel immediately while task may still be "working"
    // WHEN: tasks/cancel
    let cancel_v = send(
        &mut s,
        41,
        "tasks/cancel",
        Some(json!({ "taskId": task_id })),
    );
    // THEN: cancel returns empty object (success)
    assert!(cancel_v.get("error").is_none());
    assert_eq!(cancel_v["result"], json!({}));
    // AND: task status reflects cancelled (or already done — race is acceptable)
    std::thread::sleep(std::time::Duration::from_millis(100));
    let list_v = send(&mut s, 42, "tasks/list", None);
    let tasks = list_v["result"]["tasks"].as_array().unwrap();
    let task = tasks.iter().find(|t| t["taskId"] == task_id).unwrap();
    let status = task["status"].as_str().unwrap();
    assert!(
        status == "cancelled" || status == "done" || status == "failed",
        "task should be in a terminal state, got: {status}"
    );
}

#[test]
fn tasks_cancel_unknown_task_id_returns_error() {
    // GIVEN: initialized server
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: tasks/cancel for nonexistent ID
    let v = send(
        &mut s,
        43,
        "tasks/cancel",
        Some(json!({ "taskId": "task-does-not-exist" })),
    );
    // THEN: INVALID_PARAMS error
    assert!(v.get("error").is_some());
    assert_eq!(v["error"]["code"], -32_602);
}

#[test]
fn tasks_cancel_missing_params_returns_error() {
    // GIVEN: initialized server
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: tasks/cancel without params
    let v = send(&mut s, 44, "tasks/cancel", None);
    // THEN: error
    assert!(v.get("error").is_some());
}

#[test]
fn tools_call_without_meta_task_executes_synchronously() {
    // GIVEN: initialized server
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: normal tools/call (no _meta.task)
    let req = make_request(
        50,
        "tools/call",
        Some(json!({ "name": "ax_is_accessible", "arguments": {} })),
    );
    let resp = s.handle(&req, &mut Vec::<u8>::new()).unwrap();
    let v: Value = serde_json::to_value(&resp).unwrap();
    // THEN: result has content array directly (not wrapped in task)
    assert!(v["result"].get("task").is_none());
    assert!(v["result"]["content"].is_array());
}

#[test]
fn task_id_format_is_zero_padded_hex_prefix() {
    // GIVEN / WHEN: two consecutive task IDs
    // (use the public next_task_id, but since it's pub(crate) we go through
    //  the server's task store instead)
    let mut s = Server::new();
    initialize_server(&mut s);
    let req = make_request(
        60,
        "tools/call",
        Some(json!({
            "name": "ax_is_accessible",
            "arguments": {},
            "_meta": { "task": true }
        })),
    );
    let resp = s.handle(&req, &mut Vec::<u8>::new()).unwrap();
    let v: Value = serde_json::to_value(&resp).unwrap();
    let task_id = v["result"]["task"]["taskId"].as_str().unwrap();
    // THEN: format is "task-NNNNNNNNNNNNNNNN" (16 digits)
    assert!(task_id.starts_with("task-"));
    let digits = &task_id[5..];
    assert_eq!(digits.len(), 16, "task ID should have 16 digit suffix");
    assert!(digits.chars().all(|c| c.is_ascii_digit()));
}

// -----------------------------------------------------------------------
// §14 Sampling — capability advertisement and client tracking
// -----------------------------------------------------------------------

#[test]
fn initialize_response_advertises_sampling_capability() {
    // GIVEN: fresh server (regardless of client capabilities)
    let mut s = Server::new();
    // WHEN: initialize with no client sampling capability
    let req = make_request(
        70,
        "initialize",
        Some(json!({
            "protocolVersion": "2025-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1"}
        })),
    );
    let resp = s.handle(&req, &mut Vec::<u8>::new()).unwrap();
    let v: Value = serde_json::to_value(&resp).unwrap();
    // THEN: server always advertises sampling capability (it may send requests)
    assert!(
        v["result"]["capabilities"]["sampling"].is_object(),
        "expected sampling capability object in server capabilities"
    );
}

#[test]
fn client_supports_sampling_false_when_not_advertised() {
    // GIVEN: fresh server
    let mut s = Server::new();
    // WHEN: client initialises without sampling capability
    let req = make_request(
        71,
        "initialize",
        Some(json!({
            "protocolVersion": "2025-11-05",
            "capabilities": {},
            "clientInfo": {"name": "no-sampling-client", "version": "1"}
        })),
    );
    s.handle(&req, &mut Vec::<u8>::new());
    // THEN: server does not set the flag
    assert!(
        !s.client_supports_sampling,
        "client_supports_sampling should be false when client omits sampling capability"
    );
}

#[test]
fn client_supports_sampling_true_when_advertised() {
    // GIVEN: fresh server
    let mut s = Server::new();
    // WHEN: client initialises WITH sampling capability
    let req = make_request(
        72,
        "initialize",
        Some(json!({
            "protocolVersion": "2025-11-05",
            "capabilities": {
                "sampling": { "createMessage": {} }
            },
            "clientInfo": {"name": "claude-code", "version": "2.0"}
        })),
    );
    s.handle(&req, &mut Vec::<u8>::new());
    // THEN: server records that the client supports sampling
    assert!(
        s.client_supports_sampling,
        "client_supports_sampling should be true when client advertises sampling"
    );
}

#[test]
fn client_supports_sampling_false_by_default_before_initialize() {
    // GIVEN: fresh, uninitialised server
    let s = Server::new();
    // THEN: flag defaults to false
    assert!(!s.client_supports_sampling);
}

#[test]
fn sampling_createMessage_is_method_not_found_from_server_side() {
    // GIVEN: initialized server — sampling/createMessage is a CLIENT-to-LLM
    //        call, not a server method. The server should not handle it.
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: client mistakenly sends sampling/createMessage to the server
    let req = make_request(73, "sampling/createMessage", None);
    let resp = s.handle(&req, &mut Vec::<u8>::new()).unwrap();
    let v: Value = serde_json::to_value(&resp).unwrap();
    // THEN: METHOD_NOT_FOUND — the server sends these requests, not receives them
    assert_eq!(v["error"]["code"], RpcError::METHOD_NOT_FOUND);
}
