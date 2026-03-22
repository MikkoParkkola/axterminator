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
    // GIVEN: initialized server (base 19; +5 with spaces, +3 audio, +3 camera)
    let mut s = Server::new();
    initialize_server(&mut s);
    // WHEN: tools/list
    let req = make_request(3, "tools/list", None);
    let resp = s.handle(&req, &mut Vec::<u8>::new()).unwrap();
    let v: Value = serde_json::to_value(&resp).unwrap();
    // THEN: count is a deterministic function of active features
    let count = v["result"]["tools"].as_array().unwrap().len();
    let base = 19usize;
    let extra_spaces: usize = if cfg!(feature = "spaces") { 5 } else { 0 };
    let extra_audio: usize = if cfg!(feature = "audio") { 3 } else { 0 };
    let extra_camera: usize = if cfg!(feature = "camera") { 3 } else { 0 };
    let extra_watch: usize = if cfg!(feature = "watch") { 3 } else { 0 };
    assert_eq!(
        count,
        base + extra_spaces + extra_audio + extra_camera + extra_watch
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
    assert_eq!(v["result"]["capabilities"]["resources"]["subscribe"], false);
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
fn prompts_list_returns_six_prompts() {
    let mut s = Server::new();
    initialize_server(&mut s);
    let req = make_request(20, "prompts/list", None);
    let resp = s.handle(&req, &mut Vec::<u8>::new()).unwrap();
    let v: Value = serde_json::to_value(&resp).unwrap();
    let prompts = v["result"]["prompts"].as_array().unwrap();
    assert_eq!(prompts.len(), 6);
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
