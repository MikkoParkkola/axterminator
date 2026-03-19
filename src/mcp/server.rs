//! JSON-RPC 2.0 stdio transport for the MCP server.
//!
//! The MCP stdio protocol is simple:
//!   - Client sends newline-delimited JSON to stdin.
//!   - Server writes newline-delimited JSON to stdout.
//!   - Stderr is for logging only.
//!
//! The event loop is single-threaded by design — tool calls are synchronous
//! against the macOS accessibility API, which must be called from the same
//! thread as the `AXUIElement` was created on (or at least from the main thread).
//! For CPU-bound or blocking tools the handler itself is responsible for spawning
//! worker threads if needed.
//!
//! ## Phase 2 additions
//!
//! This module now routes all six Phase 2 methods alongside the Phase 1 set:
//!
//! | Method | Phase | Handler |
//! |--------|-------|---------|
//! | `resources/list` | 2 | [`handle_resources_list`] |
//! | `resources/templates/list` | 2 | [`handle_resources_templates_list`] |
//! | `resources/read` | 2 | [`handle_resources_read`] |
//! | `prompts/list` | 2 | [`handle_prompts_list`] |
//! | `prompts/get` | 2 | [`handle_prompts_get`] |

use std::io::{self, BufRead, Write};
use std::sync::Arc;

use serde_json::{json, Value};
use tracing::{debug, error, info, warn};

use crate::mcp::protocol::{
    ElicitationCapability, InitializeParams, InitializeResult, JsonRpcNotification, JsonRpcRequest,
    JsonRpcResponse, LoggingCapability, PingResult, PromptGetParams, PromptsCapability, RequestId,
    ResourceReadParams, ResourcesCapability, RpcError, ServerCapabilities, ServerInfo,
    ToolCallParams, ToolListResult, ToolsCapability,
};
use crate::mcp::tools::{call_tool, AppRegistry};

// ---------------------------------------------------------------------------
// Server state
// ---------------------------------------------------------------------------

/// Lifecycle phase of the server.
#[derive(Debug, PartialEq, Eq)]
enum Phase {
    /// Waiting for the `initialize` request.
    Uninitialized,
    /// `initialize` acknowledged; `initialized` notification expected next.
    Initializing,
    /// Fully operational.
    Running,
}

/// MCP stdio server state.
struct Server {
    registry: Arc<AppRegistry>,
    phase: Phase,
}

impl Server {
    fn new() -> Self {
        Self {
            registry: Arc::new(AppRegistry::default()),
            phase: Phase::Uninitialized,
        }
    }

    // -----------------------------------------------------------------------
    // Message routing
    // -----------------------------------------------------------------------

    /// Route one parsed JSON-RPC message and return an optional response.
    ///
    /// `out` receives any MCP notifications (progress, log) emitted while
    /// handling the request.  For `tools/call`, progress notifications may be
    /// written to `out` before the response is returned.
    ///
    /// Returns `None` for notifications (no id) that require no reply.
    fn handle<W: Write>(&mut self, msg: &JsonRpcRequest, out: &mut W) -> Option<JsonRpcResponse> {
        debug!(method = %msg.method, "incoming message");

        // Notifications have no id — never reply to them.
        if msg.id.is_none() {
            self.handle_notification(msg);
            return None;
        }

        let id = match msg.id.clone() {
            Some(id) => id,
            None => {
                return Some(JsonRpcResponse::err(
                    RequestId::Number(0),
                    RpcError::new(RpcError::INVALID_REQUEST, "Missing request id".to_string()),
                ));
            }
        };

        match msg.method.as_str() {
            "initialize" => Some(self.handle_initialize(id, msg.params.as_ref())),
            "ping" => Some(Self::handle_ping(id)),
            // Phase 1 + Phase 3 — tools
            "tools/list" if self.phase == Phase::Running => Some(Self::handle_tools_list(id)),
            "tools/call" if self.phase == Phase::Running => {
                Some(self.handle_tools_call(id, msg.params.as_ref(), out))
            }
            // Phase 2 — resources
            "resources/list" if self.phase == Phase::Running => {
                Some(Self::handle_resources_list(id))
            }
            "resources/templates/list" if self.phase == Phase::Running => {
                Some(Self::handle_resources_templates_list(id))
            }
            "resources/read" if self.phase == Phase::Running => {
                Some(self.handle_resources_read(id, msg.params.as_ref()))
            }
            // Phase 2 — prompts
            "prompts/list" if self.phase == Phase::Running => Some(Self::handle_prompts_list(id)),
            "prompts/get" if self.phase == Phase::Running => {
                Some(Self::handle_prompts_get(id, msg.params.as_ref()))
            }
            method if self.phase != Phase::Running => {
                warn!(method, "request before initialized");
                Some(JsonRpcResponse::err(
                    id,
                    RpcError::new(RpcError::INVALID_REQUEST, "Server not yet initialized"),
                ))
            }
            method => {
                warn!(method, "method not found");
                Some(JsonRpcResponse::err(
                    id,
                    RpcError::new(
                        RpcError::METHOD_NOT_FOUND,
                        format!("Method not found: {method}"),
                    ),
                ))
            }
        }
    }

    fn handle_notification(&mut self, msg: &JsonRpcRequest) {
        match msg.method.as_str() {
            "notifications/initialized" => {
                if self.phase == Phase::Initializing {
                    self.phase = Phase::Running;
                    info!("MCP server ready");
                }
            }
            method => debug!(method, "unhandled notification"),
        }
    }

    // -----------------------------------------------------------------------
    // Method handlers
    // -----------------------------------------------------------------------

    fn handle_initialize(&mut self, id: RequestId, params: Option<&Value>) -> JsonRpcResponse {
        let Some(params_val) = params else {
            return JsonRpcResponse::err(
                id,
                RpcError::new(RpcError::INVALID_PARAMS, "Missing params"),
            );
        };

        let parsed: Result<InitializeParams, _> = serde_json::from_value(params_val.clone());
        match parsed {
            Ok(p) => {
                info!(
                    client = %p.client_info.name,
                    version = %p.client_info.version,
                    protocol = %p.protocol_version,
                    "client connected"
                );
                self.phase = Phase::Initializing;
                let result = InitializeResult {
                    protocol_version: "2025-11-05",
                    capabilities: ServerCapabilities {
                        tools: ToolsCapability {
                            list_changed: false,
                        },
                        logging: LoggingCapability {},
                        resources: ResourcesCapability {
                            subscribe: false,
                            list_changed: false,
                        },
                        prompts: PromptsCapability {
                            list_changed: false,
                        },
                        elicitation: ElicitationCapability {},
                    },
                    server_info: ServerInfo {
                        name: "axterminator",
                        version: env!("CARGO_PKG_VERSION"),
                        title: "AXTerminator - macOS GUI Automation",
                    },
                    instructions: "\
AXTerminator: background-first macOS GUI automation.\n\
\n\
Workflow:\n\
1. ax_is_accessible — verify permissions\n\
2. ax_connect — connect to an app by name, bundle ID, or PID\n\
3. ax_find — locate elements (self-healing, 7 strategies)\n\
4. ax_click / ax_type / ax_set_value — interact\n\
5. ax_screenshot — visual context\n\
6. ax_wait_idle — wait for UI to settle before asserting state\n\
\n\
All actions run in background mode by default (no focus stealing).",
                };
                JsonRpcResponse::ok(id, serde_json::to_value(result).unwrap())
            }
            Err(e) => JsonRpcResponse::err(
                id,
                RpcError::new(
                    RpcError::INVALID_PARAMS,
                    format!("Invalid initialize params: {e}"),
                ),
            ),
        }
    }

    fn handle_ping(id: RequestId) -> JsonRpcResponse {
        JsonRpcResponse::ok(id, serde_json::to_value(PingResult {}).unwrap())
    }

    fn handle_tools_list(id: RequestId) -> JsonRpcResponse {
        let result = ToolListResult {
            tools: crate::mcp::tools::all_tools(),
        };
        JsonRpcResponse::ok(id, serde_json::to_value(result).unwrap())
    }

    fn handle_tools_call<W: Write>(
        &self,
        id: RequestId,
        params: Option<&Value>,
        out: &mut W,
    ) -> JsonRpcResponse {
        let Some(params_val) = params else {
            return JsonRpcResponse::err(
                id,
                RpcError::new(RpcError::INVALID_PARAMS, "Missing params"),
            );
        };

        let parsed: Result<ToolCallParams, _> = serde_json::from_value(params_val.clone());
        match parsed {
            Ok(p) => {
                let args = p
                    .arguments
                    .unwrap_or(Value::Object(serde_json::Map::default()));
                let tool_result = call_tool(&p.name, &args, &self.registry, out);
                JsonRpcResponse::ok(id, serde_json::to_value(tool_result).unwrap())
            }
            Err(e) => JsonRpcResponse::err(
                id,
                RpcError::new(
                    RpcError::INVALID_PARAMS,
                    format!("Invalid tools/call params: {e}"),
                ),
            ),
        }
    }

    // -----------------------------------------------------------------------
    // Phase 2 — resources
    // -----------------------------------------------------------------------

    /// Return the list of static (concrete URI) resources.
    fn handle_resources_list(id: RequestId) -> JsonRpcResponse {
        let result = crate::mcp::resources::static_resources();
        JsonRpcResponse::ok(id, serde_json::to_value(result).unwrap())
    }

    /// Return the list of dynamic URI template resources.
    fn handle_resources_templates_list(id: RequestId) -> JsonRpcResponse {
        let result = crate::mcp::resources::resource_templates();
        JsonRpcResponse::ok(id, serde_json::to_value(result).unwrap())
    }

    /// Read a resource by URI, dispatching to the appropriate handler.
    fn handle_resources_read(&self, id: RequestId, params: Option<&Value>) -> JsonRpcResponse {
        let Some(params_val) = params else {
            return JsonRpcResponse::err(
                id,
                RpcError::new(RpcError::INVALID_PARAMS, "Missing params"),
            );
        };

        let parsed: Result<ResourceReadParams, _> = serde_json::from_value(params_val.clone());
        match parsed {
            Ok(p) => match crate::mcp::resources::read_resource(&p.uri, &self.registry) {
                Ok(result) => JsonRpcResponse::ok(id, serde_json::to_value(result).unwrap()),
                Err(e) => JsonRpcResponse::err(
                    id,
                    RpcError::new(
                        RpcError::INVALID_PARAMS,
                        format!("Resource read failed: {e}"),
                    ),
                ),
            },
            Err(e) => JsonRpcResponse::err(
                id,
                RpcError::new(
                    RpcError::INVALID_PARAMS,
                    format!("Invalid resources/read params: {e}"),
                ),
            ),
        }
    }

    // -----------------------------------------------------------------------
    // Phase 2 — prompts
    // -----------------------------------------------------------------------

    /// Return all registered prompt descriptors.
    fn handle_prompts_list(id: RequestId) -> JsonRpcResponse {
        let result = crate::mcp::prompts::all_prompts();
        JsonRpcResponse::ok(id, serde_json::to_value(result).unwrap())
    }

    /// Resolve a prompt by name, filling in caller-supplied arguments.
    fn handle_prompts_get(id: RequestId, params: Option<&Value>) -> JsonRpcResponse {
        let Some(params_val) = params else {
            return JsonRpcResponse::err(
                id,
                RpcError::new(RpcError::INVALID_PARAMS, "Missing params"),
            );
        };

        let parsed: Result<PromptGetParams, _> = serde_json::from_value(params_val.clone());
        match parsed {
            Ok(p) => match crate::mcp::prompts::get_prompt(&p) {
                Ok(result) => JsonRpcResponse::ok(id, serde_json::to_value(result).unwrap()),
                Err(e) => JsonRpcResponse::err(
                    id,
                    RpcError::new(RpcError::INVALID_PARAMS, format!("Prompt error: {e}")),
                ),
            },
            Err(e) => JsonRpcResponse::err(
                id,
                RpcError::new(
                    RpcError::INVALID_PARAMS,
                    format!("Invalid prompts/get params: {e}"),
                ),
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// Public handle — used by the HTTP transport
// ---------------------------------------------------------------------------

/// A public wrapper around [`Server`] for use by the HTTP transport layer.
///
/// Each HTTP request creates its own `ServerHandle` (stateless per-request
/// in Phase 4). Stateful HTTP sessions — where connected apps persist across
/// requests — are deferred to Phase 5.
///
/// # Examples
///
/// ```rust
/// use axterminator::mcp::server::ServerHandle;
/// use axterminator::mcp::protocol::{JsonRpcRequest, RequestId};
///
/// let mut handle = ServerHandle::new();
/// let req = JsonRpcRequest {
///     jsonrpc: "2.0".into(),
///     id: Some(RequestId::Number(1)),
///     method: "ping".into(),
///     params: None,
/// };
/// let mut sink = Vec::<u8>::new();
/// // Not yet initialized — will return an error, not a panic.
/// let _ = handle.handle(&req, &mut sink);
/// ```
pub struct ServerHandle(Server);

impl ServerHandle {
    /// Create a new, uninitialised server handle.
    #[must_use]
    pub fn new() -> Self {
        Self(Server::new())
    }

    /// Route one JSON-RPC message through the server.
    ///
    /// Identical contract to the private `Server::handle` — see that method
    /// for full documentation.
    pub fn handle<W: Write>(
        &mut self,
        msg: &JsonRpcRequest,
        out: &mut W,
    ) -> Option<JsonRpcResponse> {
        self.0.handle(msg, out)
    }
}

impl Default for ServerHandle {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// stdio event loop
// ---------------------------------------------------------------------------

/// Run the MCP server until stdin closes or an unrecoverable error occurs.
///
/// This is the entry point called by `axterminator mcp serve --stdio`.
///
/// # Errors
///
/// Returns an error if stdin or stdout I/O fails, or if JSON serialisation fails
/// in a way that cannot be recovered (which should never happen in practice).
pub fn run_stdio() -> anyhow::Result<()> {
    info!("axterminator MCP server starting (stdio)");

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdout_lock = stdout.lock();
    let mut server = Server::new();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        debug!(bytes = line.len(), "received line");

        let msg: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(m) => m,
            Err(e) => {
                error!(error = %e, "parse error");
                let resp = JsonRpcResponse::err(
                    RequestId::Number(0),
                    RpcError::new(RpcError::PARSE_ERROR, format!("Parse error: {e}")),
                );
                write_response(&mut stdout_lock, &resp)?;
                continue;
            }
        };

        if let Some(resp) = server.handle(&msg, &mut stdout_lock) {
            write_response(&mut stdout_lock, &resp)?;
        }
    }

    info!("stdin closed, shutting down");
    Ok(())
}

/// Serialize a response and write it as a single newline-terminated JSON line.
fn write_response(out: &mut impl Write, resp: &JsonRpcResponse) -> io::Result<()> {
    let json = serde_json::to_string(resp).expect("response serialization cannot fail");
    debug!(bytes = json.len(), id = ?resp.id, "sending response");
    writeln!(out, "{json}")?;
    out.flush()
}

/// Emit a `notifications/message` log notification to stdout.
///
/// MCP clients display these in their log panels. This is intentionally a free
/// function so the server loop can call it without borrowing `Server`.
///
/// # Errors
///
/// Returns an I/O error if writing to `out` fails.
///
/// # Panics
///
/// Panics if the notification cannot be serialised to JSON, which cannot happen
/// in practice because the structure is statically defined.
pub fn emit_log(out: &mut impl Write, level: &str, message: &str) -> io::Result<()> {
    let notif = JsonRpcNotification {
        jsonrpc: "2.0",
        method: "notifications/message",
        params: json!({ "level": level, "data": message }),
    };
    let json = serde_json::to_string(&notif).expect("notification serialization cannot fail");
    writeln!(out, "{json}")?;
    out.flush()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
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
        assert_eq!(count, base + extra_spaces + extra_audio + extra_camera);
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
    fn prompts_list_returns_four_prompts() {
        let mut s = Server::new();
        initialize_server(&mut s);
        let req = make_request(20, "prompts/list", None);
        let resp = s.handle(&req, &mut Vec::<u8>::new()).unwrap();
        let v: Value = serde_json::to_value(&resp).unwrap();
        let prompts = v["result"]["prompts"].as_array().unwrap();
        assert_eq!(prompts.len(), 4);
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
}
