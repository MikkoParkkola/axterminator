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
//! | `resources/list` | 2 | [`server_handlers`] |
//! | `resources/templates/list` | 2 | [`server_handlers`] |
//! | `resources/read` | 2 | [`server_handlers`] |
//! | `prompts/list` | 2 | [`server_handlers`] |
//! | `prompts/get` | 2 | [`server_handlers`] |

use std::io::{self, BufRead, Write};
use std::sync::Arc;

use serde_json::json;
#[cfg(test)]
use serde_json::Value;
use tracing::{debug, error, info, warn};

use crate::mcp::protocol::{
    JsonRpcNotification, JsonRpcRequest, JsonRpcResponse, RequestId, RpcError,
};
use crate::mcp::tools::AppRegistry;

// ---------------------------------------------------------------------------
// Server state
// ---------------------------------------------------------------------------

/// Lifecycle phase of the server.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Phase {
    /// Waiting for the `initialize` request.
    Uninitialized,
    /// `initialize` acknowledged; `initialized` notification expected next.
    Initializing,
    /// Fully operational.
    Running,
}

/// MCP stdio server state.
pub(super) struct Server {
    pub(super) registry: Arc<AppRegistry>,
    pub(super) phase: Phase,
    #[cfg(feature = "watch")]
    pub(super) watch_state: Arc<crate::mcp::tools_watch::WatchState>,
}

impl Server {
    pub(super) fn new() -> Self {
        Self {
            registry: Arc::new(AppRegistry::default()),
            phase: Phase::Uninitialized,
            #[cfg(feature = "watch")]
            watch_state: Arc::new(crate::mcp::tools_watch::WatchState::new()),
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
    pub(super) fn handle<W: Write>(
        &mut self,
        msg: &JsonRpcRequest,
        out: &mut W,
    ) -> Option<JsonRpcResponse> {
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

    pub(super) fn handle_notification(&mut self, msg: &JsonRpcRequest) {
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
/// When the `watch` feature is active, the server also drains any pending
/// watch events from the active watcher channel and emits them as
/// `notifications/claude/channel` notifications after each request.
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
    #[cfg(feature = "watch")]
    let mut watch_event_rx: Option<tokio::sync::mpsc::Receiver<crate::watch::WatchEvent>> = None;

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

        // Drain any buffered watch events before processing the next request.
        #[cfg(feature = "watch")]
        drain_watch_events(&mut watch_event_rx, &mut stdout_lock);

        if let Some(resp) = server.handle(&msg, &mut stdout_lock) {
            // After ax_watch_start, capture the new event receiver.
            #[cfg(feature = "watch")]
            maybe_capture_watch_receiver(&server, &mut watch_event_rx, &msg.method);

            write_response(&mut stdout_lock, &resp)?;
        }

        // Drain again after responding to minimise notification latency.
        #[cfg(feature = "watch")]
        drain_watch_events(&mut watch_event_rx, &mut stdout_lock);
    }

    info!("stdin closed, shutting down");
    Ok(())
}

/// Drain all pending watch events and emit them as channel notifications.
#[cfg(feature = "watch")]
fn drain_watch_events(
    rx: &mut Option<tokio::sync::mpsc::Receiver<crate::watch::WatchEvent>>,
    out: &mut impl io::Write,
) {
    use crate::mcp::watch_channel::{emit_channel_notification, event_to_channel_notification};

    let Some(receiver) = rx else { return };
    while let Ok(event) = receiver.try_recv() {
        if let Some(params) = event_to_channel_notification(&event) {
            // Best-effort — I/O errors on notifications do not terminate the server.
            let _ = emit_channel_notification(out, params);
        }
    }
}

/// After any `tools/call`, check whether a new watch event receiver is
/// pending (set by `ax_watch_start`) and wire it into the drain loop.
#[cfg(feature = "watch")]
fn maybe_capture_watch_receiver(
    server: &Server,
    rx: &mut Option<tokio::sync::mpsc::Receiver<crate::watch::WatchEvent>>,
    method: &str,
) {
    if method != "tools/call" {
        return;
    }
    if let Some(new_rx) = server.watch_state.take_pending_receiver() {
        *rx = Some(new_rx);
    }
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
#[path = "server_tests.rs"]
mod tests;
