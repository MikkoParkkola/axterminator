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

use std::io::{self, BufRead, Write};
use std::sync::Arc;

use serde_json::{json, Value};
use tracing::{debug, error, info, warn};

use crate::mcp::protocol::{
    InitializeParams, InitializeResult, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse,
    LoggingCapability, PingResult, RequestId, RpcError, ServerCapabilities, ServerInfo,
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
    /// Returns `None` for notifications (no id) that require no reply.
    fn handle(&mut self, msg: &JsonRpcRequest) -> Option<JsonRpcResponse> {
        debug!(method = %msg.method, "incoming message");

        // Notifications have no id — never reply to them.
        if msg.id.is_none() {
            self.handle_notification(msg);
            return None;
        }

        let id = msg.id.clone().unwrap();

        match msg.method.as_str() {
            "initialize" => Some(self.handle_initialize(id, msg.params.as_ref())),
            "ping" => Some(Self::handle_ping(id)),
            "tools/list" if self.phase == Phase::Running => Some(Self::handle_tools_list(id)),
            "tools/call" if self.phase == Phase::Running => {
                Some(self.handle_tools_call(id, msg.params.as_ref()))
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
                    RpcError::new(RpcError::METHOD_NOT_FOUND, format!("Method not found: {method}")),
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
            return JsonRpcResponse::err(id, RpcError::new(RpcError::INVALID_PARAMS, "Missing params"));
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
                        tools: ToolsCapability { list_changed: false },
                        logging: LoggingCapability {},
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
                RpcError::new(RpcError::INVALID_PARAMS, format!("Invalid initialize params: {e}")),
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

    fn handle_tools_call(&self, id: RequestId, params: Option<&Value>) -> JsonRpcResponse {
        let Some(params_val) = params else {
            return JsonRpcResponse::err(id, RpcError::new(RpcError::INVALID_PARAMS, "Missing params"));
        };

        let parsed: Result<ToolCallParams, _> = serde_json::from_value(params_val.clone());
        match parsed {
            Ok(p) => {
                let args = p.arguments.unwrap_or(Value::Object(serde_json::Map::default()));
                let tool_result = call_tool(&p.name, &args, &self.registry);
                JsonRpcResponse::ok(id, serde_json::to_value(tool_result).unwrap())
            }
            Err(e) => JsonRpcResponse::err(
                id,
                RpcError::new(RpcError::INVALID_PARAMS, format!("Invalid tools/call params: {e}")),
            ),
        }
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

        if let Some(resp) = server.handle(&msg) {
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
        s.handle(&req);
        s.handle_notification(&make_notification("notifications/initialized"));
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
        let resp = s.handle(&req).unwrap();
        // THEN: Initializing phase; response contains serverInfo
        assert_eq!(s.phase, Phase::Initializing);
        let v: Value = serde_json::from_str(
            &serde_json::to_string(&resp).unwrap()
        ).unwrap();
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
        s.handle(&req);
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
        let resp = s.handle(&req).unwrap();
        // THEN: result is empty object {}
        let v: Value = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["result"], json!({}));
    }

    #[test]
    fn tools_list_returns_twelve_tools() {
        // GIVEN: initialized server
        let mut s = Server::new();
        initialize_server(&mut s);
        // WHEN: tools/list
        let req = make_request(3, "tools/list", None);
        let resp = s.handle(&req).unwrap();
        let v: Value = serde_json::to_value(&resp).unwrap();
        // THEN: 12 tools
        let count = v["result"]["tools"].as_array().unwrap().len();
        assert_eq!(count, 12);
    }

    #[test]
    fn tools_list_before_initialized_returns_error() {
        // GIVEN: uninitialized server
        let mut s = Server::new();
        // WHEN: tools/list before initialize
        let req = make_request(1, "tools/list", None);
        let resp = s.handle(&req).unwrap();
        let v: Value = serde_json::to_value(&resp).unwrap();
        // THEN: error
        assert!(v.get("error").is_some());
    }

    #[test]
    fn unknown_method_returns_method_not_found() {
        // GIVEN: initialized server
        let mut s = Server::new();
        initialize_server(&mut s);
        // WHEN: unknown method
        let req = make_request(4, "resources/list", None);
        let resp = s.handle(&req).unwrap();
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
        let resp = s.handle(&notif);
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
        let resp = s.handle(&req).unwrap();
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
        let resp = s.handle(&req).unwrap();
        let v: Value = serde_json::to_value(&resp).unwrap();
        // THEN: error
        assert!(v.get("error").is_some());
    }
}
