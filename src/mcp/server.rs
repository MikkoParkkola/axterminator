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
//! ## Phase 2 + 3 additions
//!
//! This module routes all Phase 2 and Phase 3 methods alongside the Phase 1 set:
//!
//! | Method | Phase | Handler |
//! |--------|-------|---------|
//! | `resources/list` | 2 | [`server_handlers`] |
//! | `resources/templates/list` | 2 | [`server_handlers`] |
//! | `resources/read` | 2 | [`server_handlers`] |
//! | `prompts/list` | 2 | [`server_handlers`] |
//! | `prompts/get` | 2 | [`server_handlers`] |
//! | `resources/subscribe` | 3 | [`server_handlers`] |
//! | `resources/unsubscribe` | 3 | [`server_handlers`] |

use std::collections::{HashMap, HashSet};
use std::io::{self, BufRead, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::json;
#[cfg(test)]
use serde_json::Value;
use tracing::{debug, error, info, warn};

use crate::mcp::protocol::{
    JsonRpcNotification, JsonRpcRequest, JsonRpcResponse, RequestId, RpcError, TaskInfo,
    ToolCallResult,
};
use crate::mcp::security::{SecurityGuard, SecurityMode};
use crate::mcp::tools::AppRegistry;

// ---------------------------------------------------------------------------
// Task ID generator
// ---------------------------------------------------------------------------

/// Session-scoped monotonic counter for task IDs.
///
/// IDs are formatted as `"task-{n:016}"` to be URL-safe, sortable, and
/// trivially unique within a single server session without requiring `uuid`.
static TASK_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Allocate the next task ID.
pub(crate) fn next_task_id() -> String {
    let n = TASK_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("task-{n:016}")
}

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

/// Tracks the in-progress state of a single workflow plan across MCP calls.
pub(crate) struct WorkflowState {
    /// The ordered steps that make up this workflow.
    pub steps: Vec<crate::durable_steps::DurableStep>,
    /// Zero-based index of the next step to execute.
    pub current_step: usize,
    /// Results accumulated from already-executed steps.
    pub results: Vec<crate::durable_steps::WorkflowResult>,
    /// Whether all steps have been executed successfully.
    pub completed: bool,
}

/// One entry in the task store.
///
/// Created when a `tools/call` request carries `_meta.task: true`.
/// The `result` field is `None` while the task is executing and `Some` once
/// it has completed (successfully or with an error).
pub(crate) struct TaskEntry {
    /// Current status snapshot.  Mutated in place as the task progresses.
    pub info: TaskInfo,
    /// Final tool result; `None` while `info.status == "working"`.
    pub result: Option<ToolCallResult>,
}

/// MCP stdio server state.
pub(super) struct Server {
    pub(super) registry: Arc<AppRegistry>,
    pub(super) phase: Phase,
    /// Active workflow plans, keyed by workflow name.
    pub(super) workflows: Arc<Mutex<HashMap<String, WorkflowState>>>,
    /// Resource URIs the client has subscribed to via `resources/subscribe`.
    ///
    /// When a state-changing tool completes successfully, the server checks
    /// whether any affected URI is in this set and emits a
    /// `notifications/resources/updated` notification if so.
    pub(crate) subscriptions: Arc<Mutex<HashSet<String>>>,
    /// Task store for the Tasks API (§5).
    ///
    /// Keyed by task ID.  Entries are never evicted within a session so that
    /// clients can always retrieve results even after a long delay. The store
    /// is shared with `server_handlers` via `Arc` so that background threads
    /// can write results back without holding a reference to `Server`.
    pub(crate) tasks: Arc<Mutex<HashMap<String, TaskEntry>>>,
    /// §13 security model — mode, app policy, rate limiter, audit log.
    pub(super) security: SecurityGuard,
    /// Whether the connected client advertised `sampling` in its `initialize` capabilities.
    ///
    /// Set to `true` during `handle_initialize` when the client capabilities object
    /// contains the `sampling` key. Used by tool handlers to decide whether they can
    /// delegate visual inference to the client via `sampling/createMessage`.
    pub(crate) client_supports_sampling: bool,
    #[cfg(feature = "watch")]
    pub(super) watch_state: Arc<crate::mcp::tools_watch::WatchState>,
}

impl Server {
    pub(super) fn new() -> Self {
        Self::with_security(SecurityGuard::new())
    }

    pub(super) fn new_with_security_mode(mode: SecurityMode) -> Self {
        Self::with_security(SecurityGuard::with_mode(mode))
    }

    fn with_security(security: SecurityGuard) -> Self {
        Self {
            registry: Arc::new(AppRegistry::default()),
            phase: Phase::Uninitialized,
            workflows: Arc::new(Mutex::new(HashMap::new())),
            subscriptions: Arc::new(Mutex::new(HashSet::new())),
            tasks: Arc::new(Mutex::new(HashMap::new())),
            security,
            client_supports_sampling: false,
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
            "tools/list" if self.phase == Phase::Running => Some(self.handle_tools_list(id)),
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
            // Phase 3 — resource subscriptions
            "resources/subscribe" if self.phase == Phase::Running => {
                Some(self.handle_resources_subscribe(id, msg.params.as_ref()))
            }
            "resources/unsubscribe" if self.phase == Phase::Running => {
                Some(self.handle_resources_unsubscribe(id, msg.params.as_ref()))
            }
            // Phase 2 — prompts
            "prompts/list" if self.phase == Phase::Running => Some(Self::handle_prompts_list(id)),
            "prompts/get" if self.phase == Phase::Running => {
                Some(Self::handle_prompts_get(id, msg.params.as_ref()))
            }
            // Phase 5 — tasks
            "tasks/list" if self.phase == Phase::Running => Some(self.handle_tasks_list(id)),
            "tasks/result" if self.phase == Phase::Running => {
                Some(self.handle_tasks_result(id, msg.params.as_ref()))
            }
            "tasks/cancel" if self.phase == Phase::Running => {
                Some(self.handle_tasks_cancel(id, msg.params.as_ref()))
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

    /// Create a new, uninitialised server handle with an explicit security mode.
    #[must_use]
    pub fn new_with_security_mode(mode: SecurityMode) -> Self {
        Self(Server::new_with_security_mode(mode))
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

/// Emit a `notifications/resources/updated` notification for `uri`.
///
/// Called after any state-changing tool completes successfully, when `uri` is
/// present in the server's subscription set. Best-effort — I/O errors are
/// silently swallowed so a broken notification never aborts a tool result.
///
/// The notification body follows the MCP 2025-11-05 §6.3 wire format:
///
/// ```json
/// {"jsonrpc":"2.0","method":"notifications/resources/updated","params":{"uri":"..."}}
/// ```
///
/// # Panics
///
/// Panics if serialisation of the notification fails, which cannot happen in
/// practice because the structure is statically defined.
pub fn notify_resource_changed(out: &mut impl Write, uri: &str) {
    let notif = JsonRpcNotification {
        jsonrpc: "2.0",
        method: "notifications/resources/updated",
        params: json!({ "uri": uri }),
    };
    let json = serde_json::to_string(&notif).expect("notification serialization cannot fail");
    // Best-effort: ignore I/O errors on notifications.
    let _ = writeln!(out, "{json}");
    let _ = out.flush();
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
