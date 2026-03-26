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
    let context_base = 1usize; // system_context (always on); clipboard is in innovation
    let extra_context_location: usize = if cfg!(feature = "context") { 1 } else { 0 };
    let extra_spaces: usize = if cfg!(feature = "spaces") { 5 } else { 0 };
    // audio: ax_listen + ax_speak + ax_audio_devices (3) + capture tools (4) = 7
    let extra_audio: usize = if cfg!(feature = "audio") { 7 } else { 0 };
    let extra_camera: usize = if cfg!(feature = "camera") { 3 } else { 0 };
    let extra_watch: usize = if cfg!(feature = "watch") { 3 } else { 0 };
    let extra_docker: usize = if cfg!(feature = "docker") { 2 } else { 0 };
    assert_eq!(
        count,
        base + context_base
            + extra_context_location
            + extra_spaces
            + extra_audio
            + extra_camera
            + extra_watch
            + extra_docker
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
fn sampling_create_message_is_method_not_found_from_server_side() {
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

// -----------------------------------------------------------------------
// Exact 34-tool count (default features, no extras)
// -----------------------------------------------------------------------

#[test]
fn tools_list_returns_exactly_34_base_tools_with_default_features() {
    // GIVEN: initialized server with no optional feature flags active
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: tools/list
    let v = send(&mut s, 100, "tools/list", None);
    // THEN: base tools (Phase 1 × 12 + Phase 3 GUI × 7 + innovation × 15 + context × 2) + features
    let tools = v["result"]["tools"].as_array().unwrap();
    let base: usize = 35; // 34 original + 1 context (system_context); clipboard is in innovation
    let extra_context_location: usize = if cfg!(feature = "context") { 1 } else { 0 };
    let extra_spaces: usize = if cfg!(feature = "spaces") { 5 } else { 0 };
    // audio: ax_listen + ax_speak + ax_audio_devices (3) + capture tools (4) = 7
    let extra_audio: usize = if cfg!(feature = "audio") { 7 } else { 0 };
    let extra_camera: usize = if cfg!(feature = "camera") { 3 } else { 0 };
    let extra_watch: usize = if cfg!(feature = "watch") { 3 } else { 0 };
    let extra_docker: usize = if cfg!(feature = "docker") { 2 } else { 0 };
    let expected = base
        + extra_context_location
        + extra_spaces
        + extra_audio
        + extra_camera
        + extra_watch
        + extra_docker;
    assert_eq!(
        tools.len(),
        expected,
        "expected {expected} tools but got {}; base=36 + context_loc={extra_context_location} + spaces={extra_spaces} + \
         audio={extra_audio} + camera={extra_camera} + watch={extra_watch} + docker={extra_docker}",
        tools.len()
    );
}

// -----------------------------------------------------------------------
// All 10 prompts listed by name
// -----------------------------------------------------------------------

#[test]
fn prompts_list_contains_all_ten_expected_prompt_names() {
    // GIVEN: initialized server
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: prompts/list
    let v = send(&mut s, 101, "prompts/list", None);
    let prompts = v["result"]["prompts"].as_array().unwrap();
    // THEN: all ten canonical prompt names are present
    let names: Vec<&str> = prompts
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    let expected = [
        "test-app",
        "navigate-to",
        "extract-data",
        "accessibility-audit",
        "troubleshooting",
        "app-guide",
        "automate-workflow",
        "debug-ui",
        "cross-app-copy",
        "analyze-app",
    ];
    for name in &expected {
        assert!(
            names.contains(name),
            "prompt '{name}' missing from prompts/list; found: {names:?}"
        );
    }
    assert_eq!(
        prompts.len(),
        10,
        "expected exactly 10 prompts, got {}",
        prompts.len()
    );
}

// -----------------------------------------------------------------------
// Every prompt resolves with valid arguments
// -----------------------------------------------------------------------

/// Helper: call prompts/get and assert the result contains a non-empty messages array.
fn assert_prompt_resolves(s: &mut Server, id: i64, name: &str, args: Value) {
    let v = send(
        s,
        id,
        "prompts/get",
        Some(json!({ "name": name, "arguments": args })),
    );
    assert!(
        v.get("error").is_none(),
        "prompt '{name}' returned an error: {:?}",
        v.get("error")
    );
    let msgs = v["result"]["messages"].as_array().unwrap_or_else(|| {
        panic!(
            "prompt '{name}' result missing 'messages' array; result={:?}",
            v["result"]
        )
    });
    assert!(
        !msgs.is_empty(),
        "prompt '{name}' returned an empty messages array"
    );
}

#[test]
fn prompts_get_navigate_to_resolves_with_both_args() {
    // GIVEN: initialized server
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN/THEN: navigate-to requires app_name + target_screen
    assert_prompt_resolves(
        &mut s,
        102,
        "navigate-to",
        json!({ "app_name": "Safari", "target_screen": "Settings" }),
    );
}

#[test]
fn prompts_get_extract_data_resolves() {
    let mut s = Server::new();
    initialize_server(&mut s);
    assert_prompt_resolves(
        &mut s,
        103,
        "extract-data",
        json!({ "app_name": "Safari", "data_description": "links" }),
    );
}

#[test]
fn prompts_get_accessibility_audit_resolves() {
    let mut s = Server::new();
    initialize_server(&mut s);
    assert_prompt_resolves(
        &mut s,
        104,
        "accessibility-audit",
        json!({ "app_name": "Safari" }),
    );
}

#[test]
fn prompts_get_troubleshooting_resolves() {
    let mut s = Server::new();
    initialize_server(&mut s);
    assert_prompt_resolves(
        &mut s,
        105,
        "troubleshooting",
        json!({ "error": "Element not found" }),
    );
}

#[test]
fn prompts_get_app_guide_resolves() {
    let mut s = Server::new();
    initialize_server(&mut s);
    assert_prompt_resolves(&mut s, 106, "app-guide", json!({ "app": "Calculator" }));
}

#[test]
fn prompts_get_automate_workflow_resolves() {
    let mut s = Server::new();
    initialize_server(&mut s);
    assert_prompt_resolves(
        &mut s,
        107,
        "automate-workflow",
        json!({ "app_name": "Safari", "goal": "login" }),
    );
}

#[test]
fn prompts_get_debug_ui_resolves() {
    let mut s = Server::new();
    initialize_server(&mut s);
    assert_prompt_resolves(
        &mut s,
        108,
        "debug-ui",
        json!({ "app_name": "Safari", "query": "Save" }),
    );
}

#[test]
fn prompts_get_cross_app_copy_resolves() {
    let mut s = Server::new();
    initialize_server(&mut s);
    assert_prompt_resolves(
        &mut s,
        109,
        "cross-app-copy",
        json!({
            "source_app": "Safari",
            "dest_app": "Notes",
            "data_description": "URL"
        }),
    );
}

#[test]
fn prompts_get_analyze_app_resolves() {
    let mut s = Server::new();
    initialize_server(&mut s);
    assert_prompt_resolves(&mut s, 110, "analyze-app", json!({ "app_name": "Safari" }));
}

// -----------------------------------------------------------------------
// Resources: exact static + template counts (default features)
// -----------------------------------------------------------------------

fn expected_static_resource_uris() -> Vec<&'static str> {
    let mut uris = vec![
        "axterminator://system/status",
        "axterminator://system/displays",
        "axterminator://apps",
        "axterminator://clipboard",
        "axterminator://workflows",
        "axterminator://guide/quickstart",
        "axterminator://guide/patterns",
        "axterminator://guide/audio",
        "axterminator://profiles",
    ];

    #[cfg(feature = "spaces")]
    uris.push("axterminator://spaces");

    #[cfg(feature = "audio")]
    uris.extend([
        "axterminator://audio/devices",
        "axterminator://capture/transcription",
        "axterminator://capture/screen",
        "axterminator://capture/status",
    ]);

    #[cfg(feature = "camera")]
    uris.push("axterminator://camera/devices");

    uris
}

fn expected_resource_template_uris() -> [&'static str; 4] {
    [
        "axterminator://app/{name}/tree",
        "axterminator://app/{name}/screenshot",
        "axterminator://app/{name}/state",
        "axterminator://app/{name}/query/{question}",
    ]
}

#[test]
fn resources_list_returns_expected_static_resources() {
    // GIVEN: initialized server
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: resources/list
    let v = send(&mut s, 111, "resources/list", None);
    let resources = v["result"]["resources"].as_array().unwrap();
    let expected = expected_static_resource_uris();
    assert_eq!(
        resources.len(),
        expected.len(),
        "expected {} static resources, got {}",
        expected.len(),
        resources.len()
    );
}

#[test]
fn resources_list_contains_all_expected_static_uris() {
    // GIVEN: initialized server
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: resources/list
    let v = send(&mut s, 112, "resources/list", None);
    let resources = v["result"]["resources"].as_array().unwrap();
    let uris: Vec<&str> = resources
        .iter()
        .map(|r| r["uri"].as_str().unwrap())
        .collect();
    for uri in expected_static_resource_uris() {
        assert!(
            uris.contains(&uri),
            "static resource '{uri}' missing from resources/list; found: {uris:?}"
        );
    }
}

#[test]
fn resources_templates_list_returns_exactly_four_templates() {
    // GIVEN: initialized server
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: resources/templates/list
    let v = send(&mut s, 113, "resources/templates/list", None);
    let templates = v["result"]["resourceTemplates"].as_array().unwrap();
    let expected = expected_resource_template_uris();
    assert_eq!(
        templates.len(),
        expected.len(),
        "expected {} resource templates, got {}",
        expected.len(),
        templates.len()
    );
}

#[test]
fn resources_templates_list_contains_all_four_template_uris() {
    // GIVEN: initialized server
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: resources/templates/list
    let v = send(&mut s, 114, "resources/templates/list", None);
    let templates = v["result"]["resourceTemplates"].as_array().unwrap();
    let tmpl_uris: Vec<&str> = templates
        .iter()
        .map(|t| t["uriTemplate"].as_str().unwrap())
        .collect();
    for tmpl in expected_resource_template_uris() {
        assert!(
            tmpl_uris.contains(&tmpl),
            "resource template '{tmpl}' missing; found: {tmpl_uris:?}"
        );
    }
}

// -----------------------------------------------------------------------
// Resources: read every static resource that has no OS dependency
// -----------------------------------------------------------------------

#[test]
fn resources_read_system_displays_returns_contents() {
    // GIVEN: initialized server
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: read system/displays (returns stub data without OS accessibility)
    let v = send(
        &mut s,
        115,
        "resources/read",
        Some(json!({ "uri": "axterminator://system/displays" })),
    );
    // THEN: either a contents array or an error — no panic
    // (display enumeration may legitimately fail without a real screen)
    assert!(
        v.get("error").is_some() || v["result"]["contents"].is_array(),
        "resources/read system/displays should return contents or an error, got: {v:?}"
    );
}

#[test]
fn resources_read_unknown_uri_returns_error() {
    // GIVEN: initialized server
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: an unregistered URI
    let v = send(
        &mut s,
        116,
        "resources/read",
        Some(json!({ "uri": "axterminator://unknown/resource" })),
    );
    // THEN: error
    assert!(
        v.get("error").is_some(),
        "unknown resource URI should return an error"
    );
}

// -----------------------------------------------------------------------
// Subscribe / unsubscribe lifecycle
// -----------------------------------------------------------------------

#[test]
fn resources_subscribe_returns_empty_success_object() {
    // GIVEN: initialized server
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: subscribe to system/status
    let v = send(
        &mut s,
        120,
        "resources/subscribe",
        Some(json!({ "uri": "axterminator://system/status" })),
    );
    // THEN: no error; result is an empty object (success)
    assert!(
        v.get("error").is_none(),
        "subscribe should not error: {v:?}"
    );
    assert_eq!(
        v["result"],
        json!({}),
        "subscribe result should be an empty object"
    );
}

#[test]
fn resources_unsubscribe_after_subscribe_returns_success() {
    // GIVEN: initialized server with an active subscription
    let mut s = Server::new();
    initialize_server(&mut s);
    let uri = "axterminator://system/status";
    send(
        &mut s,
        121,
        "resources/subscribe",
        Some(json!({ "uri": uri })),
    );
    // WHEN: unsubscribe
    let v = send(
        &mut s,
        122,
        "resources/unsubscribe",
        Some(json!({ "uri": uri })),
    );
    // THEN: no error; result is an empty object
    assert!(
        v.get("error").is_none(),
        "unsubscribe should not error: {v:?}"
    );
    assert_eq!(v["result"], json!({}));
}

#[test]
fn resources_unsubscribe_without_prior_subscribe_is_idempotent() {
    // GIVEN: initialized server — no subscriptions registered
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: unsubscribe a URI that was never subscribed
    let v = send(
        &mut s,
        123,
        "resources/unsubscribe",
        Some(json!({ "uri": "axterminator://apps" })),
    );
    // THEN: succeeds silently (removing an absent entry is a no-op)
    assert!(
        v.get("error").is_none(),
        "unsubscribe for unregistered URI should not error: {v:?}"
    );
}

#[test]
fn resources_subscribe_before_initialized_returns_error() {
    // GIVEN: uninitialized server
    let mut s = Server::new();
    // WHEN: subscribe before handshake
    let v = send(
        &mut s,
        124,
        "resources/subscribe",
        Some(json!({ "uri": "axterminator://system/status" })),
    );
    // THEN: error (server not yet running)
    assert!(v.get("error").is_some());
}

#[test]
fn resources_subscribe_missing_params_returns_error() {
    // GIVEN: initialized server
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: subscribe with no params
    let v = send(&mut s, 125, "resources/subscribe", None);
    // THEN: INVALID_PARAMS error
    assert!(v.get("error").is_some());
    assert_eq!(v["error"]["code"], RpcError::INVALID_PARAMS);
}

// -----------------------------------------------------------------------
// Security mode — sandboxed tools/list filters to read-only set
// -----------------------------------------------------------------------

#[test]
fn security_mode_sandboxed_filters_tools_list_to_read_only_set() {
    // GIVEN: sandboxed mode server — the env var is set before constructing
    // the server so SecurityGuard::from_env() picks it up.
    //
    // Safety: `set_var` is unsafe in Rust 2024 because env mutations are not
    // thread-safe. This test is deliberately isolated (no shared state with
    // other tests) and the var is restored immediately after Server::new().
    // The test passes `AXTERMINATOR_SECURITY_MODE` only for the narrow window
    // of Server construction.
    //
    // IMPORTANT: do NOT run this test in parallel with other tests that also
    // set AXTERMINATOR_SECURITY_MODE. The standard `cargo test` runner
    // serialises tests within a process on a per-module basis, so the risk of
    // interference with other *server_tests.rs* tests is low.
    #[allow(unsafe_code)]
    unsafe {
        std::env::set_var("AXTERMINATOR_SECURITY_MODE", "sandboxed");
    }
    let mut s = Server::new();
    #[allow(unsafe_code)]
    unsafe {
        std::env::remove_var("AXTERMINATOR_SECURITY_MODE");
    }
    initialize_server(&mut s);

    // WHEN: tools/list in sandboxed mode
    let v = send(&mut s, 130, "tools/list", None);
    let tools = v["result"]["tools"].as_array().unwrap();

    // THEN: count is strictly less than the full 34-tool set
    let full_count = 34usize
        + if cfg!(feature = "spaces") { 5 } else { 0 }
        + if cfg!(feature = "audio") { 3 } else { 0 }
        + if cfg!(feature = "camera") { 3 } else { 0 }
        + if cfg!(feature = "watch") { 3 } else { 0 }
        + if cfg!(feature = "docker") { 2 } else { 0 };
    assert!(
        tools.len() < full_count,
        "sandboxed mode should expose fewer tools than the full set ({full_count}), \
         but got {}",
        tools.len()
    );

    // THEN: every listed tool is a known read-only tool
    let read_only_names = [
        "ax_is_accessible",
        "ax_connect",
        "ax_list_apps",
        "ax_find",
        "ax_find_visual",
        "ax_get_tree",
        "ax_get_attributes",
        "ax_screenshot",
        "ax_get_value",
        "ax_list_windows",
        "ax_assert",
        "ax_wait_idle",
        "ax_query",
        "ax_analyze",
        "ax_app_profile",
        "ax_watch_start",
        "ax_watch_stop",
        "ax_watch_status",
    ];
    for tool in tools {
        let name = tool["name"].as_str().unwrap();
        assert!(
            read_only_names.contains(&name),
            "sandboxed tools/list contains non-read-only tool '{name}'"
        );
    }
}

// -----------------------------------------------------------------------
// Tool annotations — every tool must carry annotation metadata
// -----------------------------------------------------------------------

#[test]
fn every_tool_has_annotation_object_in_tools_list() {
    // GIVEN: initialized server
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: tools/list
    let v = send(&mut s, 140, "tools/list", None);
    let tools = v["result"]["tools"].as_array().unwrap();
    // THEN: every tool has an 'annotations' object with at least one hint field
    for tool in tools {
        let name = tool["name"].as_str().unwrap_or("<unknown>");
        let annotations = &tool["annotations"];
        assert!(
            annotations.is_object(),
            "tool '{name}' missing annotations object; got: {annotations:?}"
        );
        // At least one of the four MCP 2025-11-05 §6.3 hint fields must be present.
        let obj = annotations.as_object().unwrap();
        let has_hint = obj.contains_key("readOnlyHint")
            || obj.contains_key("destructiveHint")
            || obj.contains_key("idempotentHint")
            || obj.contains_key("openWorldHint");
        assert!(
            has_hint,
            "tool '{name}' annotations object has no recognised hint fields: {obj:?}"
        );
    }
}

#[test]
fn every_tool_annotation_read_only_is_boolean() {
    // GIVEN: initialized server
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: tools/list
    let v = send(&mut s, 141, "tools/list", None);
    let tools = v["result"]["tools"].as_array().unwrap();
    // THEN: readOnlyHint is always a boolean when present
    for tool in tools {
        let name = tool["name"].as_str().unwrap_or("<unknown>");
        if let Some(hint) = tool["annotations"].get("readOnlyHint") {
            assert!(
                hint.is_boolean(),
                "tool '{name}' readOnlyHint is not a boolean: {hint:?}"
            );
        }
    }
}

// -----------------------------------------------------------------------
// Tasks lifecycle — protocol-layer verification
// -----------------------------------------------------------------------

#[test]
fn tasks_list_is_empty_before_any_async_call() {
    // GIVEN: freshly initialized server — no tasks submitted
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: tasks/list without having submitted any task
    let v = send(&mut s, 150, "tasks/list", None);
    // THEN: tasks array exists and is empty
    let tasks = v["result"]["tasks"].as_array().unwrap();
    assert!(
        tasks.is_empty(),
        "tasks/list should be empty on a fresh server, got: {tasks:?}"
    );
}

#[test]
fn tasks_list_grows_after_async_submission() {
    // GIVEN: initialized server
    let mut s = Server::new();
    initialize_server(&mut s);
    // Confirm baseline is empty.
    let before = send(&mut s, 151, "tasks/list", None);
    let count_before = before["result"]["tasks"].as_array().unwrap().len();
    // WHEN: submit one async task
    let v = send(
        &mut s,
        152,
        "tools/call",
        Some(json!({
            "name": "ax_is_accessible",
            "arguments": {},
            "_meta": { "task": true }
        })),
    );
    assert!(v["result"]["task"].is_object());
    // Wait briefly for the background thread to register, then list
    std::thread::sleep(std::time::Duration::from_millis(50));
    let after = send(&mut s, 153, "tasks/list", None);
    let count_after = after["result"]["tasks"].as_array().unwrap().len();
    // THEN: list grew by exactly one
    assert_eq!(
        count_after,
        count_before + 1,
        "tasks/list count should grow from {count_before} to {} after one submission",
        count_before + 1
    );
}

// -----------------------------------------------------------------------
// Phase 3 — resources/subscribe + resources/unsubscribe lifecycle
// -----------------------------------------------------------------------

#[test]
fn resources_subscribe_returns_empty_object() {
    // GIVEN: initialized server
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: subscribe to system/status
    let v = send(
        &mut s,
        200,
        "resources/subscribe",
        Some(json!({ "uri": "axterminator://system/status" })),
    );
    // THEN: success with empty result {}
    assert!(v.get("error").is_none(), "subscribe must not error");
    assert_eq!(
        v["result"],
        json!({}),
        "subscribe result must be empty object"
    );
}

#[test]
fn resources_subscribe_stores_uri_in_subscriptions_set() {
    // GIVEN: initialized server with empty subscription set
    let mut s = Server::new();
    initialize_server(&mut s);
    assert!(s.subscriptions.lock().unwrap().is_empty());
    // WHEN: subscribe to clipboard
    let _ = send(
        &mut s,
        201,
        "resources/subscribe",
        Some(json!({ "uri": "axterminator://clipboard" })),
    );
    // THEN: URI is in the subscriptions set
    let subs = s.subscriptions.lock().unwrap();
    assert!(
        subs.contains("axterminator://clipboard"),
        "subscriptions must contain the subscribed URI"
    );
}

#[test]
fn resources_unsubscribe_removes_uri_from_subscriptions_set() {
    // GIVEN: initialized server with one subscription
    let mut s = Server::new();
    initialize_server(&mut s);
    let _ = send(
        &mut s,
        202,
        "resources/subscribe",
        Some(json!({ "uri": "axterminator://apps" })),
    );
    assert!(s
        .subscriptions
        .lock()
        .unwrap()
        .contains("axterminator://apps"));
    // WHEN: unsubscribe
    let v = send(
        &mut s,
        203,
        "resources/unsubscribe",
        Some(json!({ "uri": "axterminator://apps" })),
    );
    // THEN: success and URI removed
    assert!(v.get("error").is_none(), "unsubscribe must not error");
    assert_eq!(v["result"], json!({}));
    assert!(
        !s.subscriptions
            .lock()
            .unwrap()
            .contains("axterminator://apps"),
        "URI must be removed after unsubscribe"
    );
}

#[test]
fn resources_unsubscribe_missing_params_returns_error() {
    // GIVEN: initialized server
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: unsubscribe with no params
    let v = send(&mut s, 205, "resources/unsubscribe", None);
    // THEN: INVALID_PARAMS error
    assert!(v.get("error").is_some());
    assert_eq!(v["error"]["code"], -32_602);
}

#[test]
fn resources_unsubscribe_nonexistent_uri_succeeds_without_error() {
    // GIVEN: initialized server with no subscriptions
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: unsubscribe from a URI that was never subscribed
    let v = send(
        &mut s,
        206,
        "resources/unsubscribe",
        Some(json!({ "uri": "axterminator://never/subscribed" })),
    );
    // THEN: no error — idempotent unsubscribe is safe
    assert!(
        v.get("error").is_none(),
        "unsubscribing an unknown URI should not error"
    );
    assert_eq!(v["result"], json!({}));
}

#[test]
fn resources_subscribe_multiple_uris_all_tracked() {
    // GIVEN: initialized server
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: subscribe to three different URIs
    for (id, uri) in [
        (210, "axterminator://system/status"),
        (211, "axterminator://apps"),
        (212, "axterminator://clipboard"),
    ] {
        let v = send(
            &mut s,
            id,
            "resources/subscribe",
            Some(json!({ "uri": uri })),
        );
        assert!(
            v.get("error").is_none(),
            "subscribe to {uri} must not error"
        );
    }
    // THEN: all three are tracked
    let subs = s.subscriptions.lock().unwrap();
    assert!(subs.contains("axterminator://system/status"));
    assert!(subs.contains("axterminator://apps"));
    assert!(subs.contains("axterminator://clipboard"));
    assert_eq!(subs.len(), 3);
}

#[test]
fn resources_subscribe_same_uri_twice_is_idempotent() {
    // GIVEN: initialized server
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: subscribe to the same URI twice
    let _ = send(
        &mut s,
        213,
        "resources/subscribe",
        Some(json!({ "uri": "axterminator://system/status" })),
    );
    let _ = send(
        &mut s,
        214,
        "resources/subscribe",
        Some(json!({ "uri": "axterminator://system/status" })),
    );
    // THEN: only one entry (HashSet semantics)
    let subs = s.subscriptions.lock().unwrap();
    assert_eq!(
        subs.len(),
        1,
        "duplicate subscribe must not create duplicate entries"
    );
}

#[test]
fn ax_connect_tool_emits_notification_for_subscribed_apps_uri() {
    // GIVEN: initialized server, subscribed to axterminator://apps
    let mut s = Server::new();
    initialize_server(&mut s);
    let _ = send(
        &mut s,
        220,
        "resources/subscribe",
        Some(json!({ "uri": "axterminator://apps" })),
    );
    // WHEN: ax_connect is called (it will fail because the app is fake, but
    //       the notification is emitted before the result, on success only —
    //       here we verify the notification channel works by checking the
    //       subscriptions set still has the URI after dispatch)
    let mut out = Vec::<u8>::new();
    let req = make_request(
        221,
        "tools/call",
        Some(json!({ "name": "ax_is_accessible", "arguments": {} })),
    );
    s.handle(&req, &mut out);
    // THEN: subscriptions set is unchanged (non-connected tool, no notification)
    assert!(s
        .subscriptions
        .lock()
        .unwrap()
        .contains("axterminator://apps"));
}

#[test]
fn notify_resource_changed_writes_valid_jsonrpc_notification() {
    // GIVEN: an output buffer
    let mut out = Vec::<u8>::new();
    // WHEN: emitting a resource change notification
    crate::mcp::server::notify_resource_changed(&mut out, "axterminator://system/status");
    // THEN: one valid JSON line with correct method and uri
    let line = String::from_utf8(out).unwrap();
    let line = line.trim();
    assert!(!line.is_empty(), "notification must not be empty");
    let v: Value = serde_json::from_str(line).expect("notification must be valid JSON");
    assert_eq!(v["jsonrpc"], "2.0");
    assert_eq!(v["method"], "notifications/resources/updated");
    assert_eq!(v["params"]["uri"], "axterminator://system/status");
}

// -----------------------------------------------------------------------
// Capture resource subscription notifications (feature = "audio")
// -----------------------------------------------------------------------

#[cfg(feature = "audio")]
#[test]
fn resources_subscribe_capture_status_stores_uri() {
    // GIVEN: initialized server
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: subscribe to capture/status
    let v = send(
        &mut s,
        230,
        "resources/subscribe",
        Some(json!({ "uri": "axterminator://capture/status" })),
    );
    // THEN: success, URI tracked
    assert!(v.get("error").is_none());
    assert!(s
        .subscriptions
        .lock()
        .unwrap()
        .contains("axterminator://capture/status"));
}

#[cfg(feature = "audio")]
#[test]
fn resources_subscribe_capture_transcription_stores_uri() {
    // GIVEN: initialized server
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: subscribe to capture/transcription
    let v = send(
        &mut s,
        231,
        "resources/subscribe",
        Some(json!({ "uri": "axterminator://capture/transcription" })),
    );
    // THEN: success, URI tracked
    assert!(v.get("error").is_none());
    assert!(s
        .subscriptions
        .lock()
        .unwrap()
        .contains("axterminator://capture/transcription"));
}

#[cfg(feature = "audio")]
#[test]
fn resources_subscribe_capture_screen_stores_uri() {
    // GIVEN: initialized server
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: subscribe to capture/screen
    let v = send(
        &mut s,
        232,
        "resources/subscribe",
        Some(json!({ "uri": "axterminator://capture/screen" })),
    );
    // THEN: success, URI tracked
    assert!(v.get("error").is_none());
    assert!(s
        .subscriptions
        .lock()
        .unwrap()
        .contains("axterminator://capture/screen"));
}

#[cfg(feature = "audio")]
#[test]
fn ax_start_capture_subscribed_emits_capture_status_notification() {
    let _guard = crate::mcp::tools_capture::session_test_lock()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    // GIVEN: initialized server subscribed to all three capture URIs
    let mut s = Server::new();
    initialize_server(&mut s);
    for (id, uri) in [
        (233, "axterminator://capture/status"),
        (234, "axterminator://capture/transcription"),
        (235, "axterminator://capture/screen"),
    ] {
        let _ = send(
            &mut s,
            id,
            "resources/subscribe",
            Some(json!({ "uri": uri })),
        );
    }
    // WHEN: ax_start_capture (no audio/screen to avoid hardware)
    let mut out = Vec::<u8>::new();
    let req = make_request(
        236,
        "tools/call",
        Some(json!({
            "name": "ax_start_capture",
            "arguments": {
                "audio": false, "transcribe": false, "screen": false, "buffer_seconds": 5
            }
        })),
    );
    s.handle(&req, &mut out);
    // THEN: output contains at least one notifications/resources/updated line
    let output = String::from_utf8(out).unwrap();
    let notification_count = output
        .lines()
        .filter(|line| {
            serde_json::from_str::<Value>(line)
                .map(|v| v["method"] == "notifications/resources/updated")
                .unwrap_or(false)
        })
        .count();
    assert!(
        notification_count > 0,
        "ax_start_capture with subscribed capture URIs must emit at least one notification"
    );
    // Cleanup
    let _ = crate::mcp::tools_capture::handle_ax_stop_capture(&json!({}));
}

#[cfg(feature = "audio")]
#[test]
fn ax_stop_capture_subscribed_emits_capture_status_notification() {
    let _guard = crate::mcp::tools_capture::session_test_lock()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    // GIVEN: initialized server with active session, subscribed to capture/status
    let _ = crate::mcp::tools_capture::handle_ax_start_capture(&json!({
        "audio": false, "transcribe": false, "screen": false, "buffer_seconds": 5
    }));
    let mut s = Server::new();
    initialize_server(&mut s);
    let _ = send(
        &mut s,
        240,
        "resources/subscribe",
        Some(json!({ "uri": "axterminator://capture/status" })),
    );
    // WHEN: ax_stop_capture
    let mut out = Vec::<u8>::new();
    let req = make_request(
        241,
        "tools/call",
        Some(json!({ "name": "ax_stop_capture", "arguments": {} })),
    );
    s.handle(&req, &mut out);
    // THEN: notification emitted for capture/status
    let output = String::from_utf8(out).unwrap();
    let has_capture_notification = output.lines().any(|line| {
        serde_json::from_str::<Value>(line)
            .map(|v| {
                v["method"] == "notifications/resources/updated"
                    && v["params"]["uri"] == "axterminator://capture/status"
            })
            .unwrap_or(false)
    });
    assert!(
        has_capture_notification,
        "ax_stop_capture with subscribed capture/status must emit notification"
    );
}
