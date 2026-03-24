//! `handle_*` method implementations for [`Server`].
//!
//! This module is a private implementation detail of [`server`].  All public
//! types live in [`server`]; this file only contains the `impl Server` block
//! for the individual JSON-RPC method handlers.
//!
//! [`server`]: crate::mcp::server

use std::io::Write;
use std::sync::Arc;

use serde_json::Value;
use tracing::{debug, info};

use crate::mcp::protocol::{
    task_status, ElicitationCapability, InitializeParams, InitializeResult, JsonRpcResponse,
    LoggingCapability, PingResult, PromptGetParams, PromptsCapability, RequestId,
    ResourceReadParams, ResourceSubscribeParams, ResourceSubscribeResult,
    ResourceUnsubscribeParams, ResourcesCapability, RpcError, SamplingCapability,
    ServerCapabilities, ServerInfo, TaskCancelParams, TaskCancelResult, TaskInfo, TaskResultParams,
    TaskResultResponse, TasksCapability, TasksListResult, ToolCallParams, ToolListResult,
    ToolsCapability,
};
use crate::mcp::security::SecurityMode;
use crate::mcp::tools::call_tool;

use super::server::{next_task_id, Phase, Server, TaskEntry};

impl Server {
    // -----------------------------------------------------------------------
    // Core lifecycle
    // -----------------------------------------------------------------------

    pub(super) fn handle_initialize(
        &mut self,
        id: RequestId,
        params: Option<&Value>,
    ) -> JsonRpcResponse {
        let Some(params_val) = params else {
            return JsonRpcResponse::err(
                id,
                RpcError::new(RpcError::INVALID_PARAMS, "Missing params"),
            );
        };

        match serde_json::from_value::<InitializeParams>(params_val.clone()) {
            Ok(p) => {
                let supports_sampling = p.capabilities.supports_sampling();
                info!(
                    client = %p.client_info.name,
                    version = %p.client_info.version,
                    protocol = %p.protocol_version,
                    sampling = supports_sampling,
                    "client connected"
                );
                self.phase = Phase::Initializing;
                self.client_supports_sampling = supports_sampling;
                let result = build_initialize_result();
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

    pub(super) fn handle_ping(id: RequestId) -> JsonRpcResponse {
        JsonRpcResponse::ok(id, serde_json::to_value(PingResult {}).unwrap())
    }

    // -----------------------------------------------------------------------
    // Tools
    // -----------------------------------------------------------------------

    pub(super) fn handle_tools_list(&self, id: RequestId) -> JsonRpcResponse {
        let all = crate::mcp::tools::all_tools();
        let tools = if self.security.mode() == SecurityMode::Sandboxed {
            all.into_iter()
                .filter(|t| self.security.mode().is_tool_allowed(t.name))
                .collect()
        } else {
            all
        };
        let result = ToolListResult { tools };
        JsonRpcResponse::ok(id, serde_json::to_value(result).unwrap())
    }

    pub(super) fn handle_tools_call<W: Write>(
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

        match serde_json::from_value::<ToolCallParams>(params_val.clone()) {
            Ok(p) => {
                let args = p
                    .arguments
                    .unwrap_or(Value::Object(serde_json::Map::default()));
                if is_task_request(params_val) {
                    self.dispatch_as_task(id, &p.name, args)
                } else {
                    let tool_result = self.dispatch_tool(&p.name, &args, out);
                    JsonRpcResponse::ok(id, serde_json::to_value(tool_result).unwrap())
                }
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

    /// Execute a tool as an async task.
    ///
    /// For the synchronous stdio transport, the tool runs on a background
    /// thread while this method returns immediately with a `"working"` status.
    /// The thread stores the result in the shared task store; the client then
    /// polls via `tasks/result` to retrieve it.
    ///
    /// The response body wraps the initial [`TaskInfo`] in a `task` field,
    /// matching the MCP Tasks §5 wire format:
    ///
    /// ```json
    /// {"jsonrpc":"2.0","id":1,"result":{"task":{"taskId":"task-0000000000000001","status":"working"}}}
    /// ```
    fn dispatch_as_task(&self, id: RequestId, tool_name: &str, args: Value) -> JsonRpcResponse {
        let task_id = next_task_id();
        let info = TaskInfo {
            task_id: task_id.clone(),
            status: task_status::WORKING,
            status_message: Some(format!("Running {tool_name}…")),
        };

        // Register the task before spawning so the entry always exists when
        // the client polls (even if the thread completes before the poll).
        let tasks = Arc::clone(&self.tasks);
        {
            let mut store = tasks.lock().unwrap_or_else(|e| e.into_inner());
            store.insert(
                task_id.clone(),
                TaskEntry {
                    info: info.clone(),
                    result: None,
                },
            );
        }

        // Capture everything the background thread needs.
        let registry = Arc::clone(&self.registry);
        let workflows = Arc::clone(&self.workflows);
        let subscriptions = Arc::clone(&self.subscriptions);
        let name = tool_name.to_owned();

        std::thread::spawn(move || {
            // Background tool execution — no stdout access; notifications are
            // suppressed (dev/null sink) because the stdio loop is single-
            // threaded. Progress notifications require Phase 6 (SSE transport).
            let mut sink = std::io::sink();
            let tool_result = execute_tool_background(
                &name,
                &args,
                &registry,
                &workflows,
                &subscriptions,
                &mut sink,
            );

            let (final_status, message) = if tool_result.is_error {
                (task_status::FAILED, "Tool returned an error".to_owned())
            } else {
                (task_status::DONE, format!("{name} completed"))
            };

            let mut store = tasks.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(entry) = store.get_mut(&task_id) {
                // Only advance if the task was not cancelled before we finished.
                if entry.info.status == task_status::WORKING {
                    entry.info.status = final_status;
                    entry.info.status_message = Some(message);
                    entry.result = Some(tool_result);
                }
            }
        });

        let response_value = serde_json::json!({ "task": info });
        JsonRpcResponse::ok(id, response_value)
    }

    // -----------------------------------------------------------------------
    // Phase 2 — resources
    // -----------------------------------------------------------------------

    /// Return the list of static (concrete URI) resources.
    pub(super) fn handle_resources_list(id: RequestId) -> JsonRpcResponse {
        let result = crate::mcp::resources::static_resources();
        JsonRpcResponse::ok(id, serde_json::to_value(result).unwrap())
    }

    /// Return the list of dynamic URI template resources.
    pub(super) fn handle_resources_templates_list(id: RequestId) -> JsonRpcResponse {
        let result = crate::mcp::resources::resource_templates();
        JsonRpcResponse::ok(id, serde_json::to_value(result).unwrap())
    }

    /// Read a resource by URI, dispatching to the appropriate handler.
    pub(super) fn handle_resources_read(
        &self,
        id: RequestId,
        params: Option<&Value>,
    ) -> JsonRpcResponse {
        let Some(params_val) = params else {
            return JsonRpcResponse::err(
                id,
                RpcError::new(RpcError::INVALID_PARAMS, "Missing params"),
            );
        };

        match serde_json::from_value::<ResourceReadParams>(params_val.clone()) {
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
    // Phase 3 — resource subscriptions
    // -----------------------------------------------------------------------

    /// Register a client subscription for `notifications/resources/updated`.
    ///
    /// The URI is stored in the server's subscription set. After any
    /// state-changing tool completes, the server checks the set and emits a
    /// notification for every affected URI. The client then calls
    /// `resources/read` to fetch the updated content.
    pub(super) fn handle_resources_subscribe(
        &self,
        id: RequestId,
        params: Option<&Value>,
    ) -> JsonRpcResponse {
        let Some(params_val) = params else {
            return JsonRpcResponse::err(
                id,
                RpcError::new(RpcError::INVALID_PARAMS, "Missing params"),
            );
        };

        match serde_json::from_value::<ResourceSubscribeParams>(params_val.clone()) {
            Ok(p) => {
                let uri = p.uri.clone();
                if let Ok(mut subs) = self.subscriptions.lock() {
                    subs.insert(uri.clone());
                }
                debug!(uri, "resource subscribed");
                JsonRpcResponse::ok(
                    id,
                    serde_json::to_value(ResourceSubscribeResult {}).unwrap(),
                )
            }
            Err(e) => JsonRpcResponse::err(
                id,
                RpcError::new(
                    RpcError::INVALID_PARAMS,
                    format!("Invalid resources/subscribe params: {e}"),
                ),
            ),
        }
    }

    /// Remove a client subscription, stopping update notifications for that URI.
    pub(super) fn handle_resources_unsubscribe(
        &self,
        id: RequestId,
        params: Option<&Value>,
    ) -> JsonRpcResponse {
        let Some(params_val) = params else {
            return JsonRpcResponse::err(
                id,
                RpcError::new(RpcError::INVALID_PARAMS, "Missing params"),
            );
        };

        match serde_json::from_value::<ResourceUnsubscribeParams>(params_val.clone()) {
            Ok(p) => {
                let uri = p.uri.clone();
                if let Ok(mut subs) = self.subscriptions.lock() {
                    subs.remove(&uri);
                }
                debug!(uri, "resource unsubscribed");
                JsonRpcResponse::ok(
                    id,
                    serde_json::to_value(ResourceSubscribeResult {}).unwrap(),
                )
            }
            Err(e) => JsonRpcResponse::err(
                id,
                RpcError::new(
                    RpcError::INVALID_PARAMS,
                    format!("Invalid resources/unsubscribe params: {e}"),
                ),
            ),
        }
    }

    // -----------------------------------------------------------------------
    // Phase 2 — prompts
    // -----------------------------------------------------------------------

    /// Return all registered prompt descriptors.
    pub(super) fn handle_prompts_list(id: RequestId) -> JsonRpcResponse {
        let result = crate::mcp::prompts::all_prompts();
        JsonRpcResponse::ok(id, serde_json::to_value(result).unwrap())
    }

    /// Resolve a prompt by name, filling in caller-supplied arguments.
    pub(super) fn handle_prompts_get(id: RequestId, params: Option<&Value>) -> JsonRpcResponse {
        let Some(params_val) = params else {
            return JsonRpcResponse::err(
                id,
                RpcError::new(RpcError::INVALID_PARAMS, "Missing params"),
            );
        };

        match serde_json::from_value::<PromptGetParams>(params_val.clone()) {
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

    // -----------------------------------------------------------------------
    // Phase 5 — Tasks API
    // -----------------------------------------------------------------------

    /// `tasks/list` — return the status of every known task in the session.
    ///
    /// Results are returned in task-ID order (which is insertion order for
    /// the monotonic counter). Terminal tasks (`done`, `failed`, `cancelled`)
    /// are included so clients can audit completed work.
    pub(super) fn handle_tasks_list(&self, id: RequestId) -> JsonRpcResponse {
        let store = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
        let mut tasks: Vec<TaskInfo> = store.values().map(|e| e.info.clone()).collect();
        // Sort by task_id so the list is deterministic (IDs are zero-padded).
        tasks.sort_by(|a, b| a.task_id.cmp(&b.task_id));
        let result = TasksListResult { tasks };
        JsonRpcResponse::ok(id, serde_json::to_value(result).unwrap())
    }

    /// `tasks/result` — retrieve the result of a completed task, or its
    /// current status if still in progress.
    ///
    /// Returns an `INVALID_PARAMS` error when the `taskId` is not found.
    pub(super) fn handle_tasks_result(
        &self,
        id: RequestId,
        params: Option<&Value>,
    ) -> JsonRpcResponse {
        let Some(params_val) = params else {
            return JsonRpcResponse::err(
                id,
                RpcError::new(RpcError::INVALID_PARAMS, "Missing params"),
            );
        };

        let parsed = serde_json::from_value::<TaskResultParams>(params_val.clone());
        let task_id = match parsed {
            Ok(p) => p.task_id,
            Err(e) => {
                return JsonRpcResponse::err(
                    id,
                    RpcError::new(
                        RpcError::INVALID_PARAMS,
                        format!("Invalid tasks/result params: {e}"),
                    ),
                )
            }
        };

        let store = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
        match store.get(&task_id) {
            None => JsonRpcResponse::err(
                id,
                RpcError::new(
                    RpcError::INVALID_PARAMS,
                    format!("Unknown taskId: {task_id}"),
                ),
            ),
            Some(entry) => {
                let response = match &entry.result {
                    Some(result) => TaskResultResponse::Complete(result.clone()),
                    None => TaskResultResponse::Pending {
                        task: entry.info.clone(),
                    },
                };
                JsonRpcResponse::ok(id, serde_json::to_value(response).unwrap())
            }
        }
    }

    /// `tasks/cancel` — request cancellation of an in-progress task.
    ///
    /// For the synchronous stdio transport, tasks run to completion on a
    /// background thread before the next poll arrives, so cancellation is
    /// a best-effort hint: if the task is still `"working"` when this method
    /// runs, its status is set to `"cancelled"` and any eventual result is
    /// discarded by the background thread. If the task has already reached a
    /// terminal state, the cancel is a no-op and returns success.
    ///
    /// Returns `INVALID_PARAMS` when the `taskId` is not found.
    pub(super) fn handle_tasks_cancel(
        &self,
        id: RequestId,
        params: Option<&Value>,
    ) -> JsonRpcResponse {
        let Some(params_val) = params else {
            return JsonRpcResponse::err(
                id,
                RpcError::new(RpcError::INVALID_PARAMS, "Missing params"),
            );
        };

        let parsed = serde_json::from_value::<TaskCancelParams>(params_val.clone());
        let task_id = match parsed {
            Ok(p) => p.task_id,
            Err(e) => {
                return JsonRpcResponse::err(
                    id,
                    RpcError::new(
                        RpcError::INVALID_PARAMS,
                        format!("Invalid tasks/cancel params: {e}"),
                    ),
                )
            }
        };

        let mut store = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
        match store.get_mut(&task_id) {
            None => JsonRpcResponse::err(
                id,
                RpcError::new(
                    RpcError::INVALID_PARAMS,
                    format!("Unknown taskId: {task_id}"),
                ),
            ),
            Some(entry) => {
                if entry.info.status == task_status::WORKING {
                    entry.info.status = task_status::CANCELLED;
                    entry.info.status_message = Some("Cancelled by client".to_owned());
                }
                debug!(task_id, "task cancelled");
                JsonRpcResponse::ok(id, serde_json::to_value(TaskCancelResult {}).unwrap())
            }
        }
    }

    // -----------------------------------------------------------------------
    // Tool dispatch helper
    // -----------------------------------------------------------------------

    /// Dispatch a tool call with §13 security gates applied.
    ///
    /// Gate order:
    /// 1. Rate limit — returns error code –32000 when exceeded.
    /// 2. Security mode — blocks tools not permitted in the active mode.
    /// 3. Execute the tool.
    /// 4. Audit log — appends a record for every mutating tool call.
    ///
    /// After any state-changing tool succeeds, the server also checks its
    /// subscription set and emits `notifications/resources/updated` for every
    /// subscribed URI that the tool may have affected.
    fn dispatch_tool<W: Write>(
        &self,
        name: &str,
        args: &Value,
        out: &mut W,
    ) -> crate::mcp::protocol::ToolCallResult {
        // Gate 1 — rate limit.
        if let Err(msg) = self.security.check_rate_limit() {
            return crate::mcp::protocol::ToolCallResult::error(msg);
        }

        // Gate 2 — security mode.
        if let Err(msg) = self.security.check_tool_allowed(name) {
            return crate::mcp::protocol::ToolCallResult::error(msg);
        }

        // Gate 3 — app policy (applies to ax_connect only; the app_id is
        // checked before we even attempt the AX connection).
        if name == "ax_connect" {
            if let Some(app_id) = args.get("app").and_then(Value::as_str) {
                if let Err(msg) = self.security.check_app_allowed(app_id) {
                    return crate::mcp::protocol::ToolCallResult::error(msg);
                }
            }
        }

        // Execute.
        #[cfg(feature = "watch")]
        {
            let result = self.call_watch_tool(name, args);
            if let Some(r) = result {
                let outcome = if r.is_error { "error" } else { "ok" };
                self.security.audit_tool_call(name, args, outcome);
                return r;
            }
        }

        // §14 Sampling — ax_find_visual gets a sampling context so it can
        // include screenshot data and sampling availability in its response,
        // enabling clients to perform VLM inference on the caller's behalf.
        if name == "ax_find_visual" {
            let sampling_ctx =
                crate::mcp::sampling::SamplingContext::from(self.client_supports_sampling);
            let result = crate::mcp::tools_handlers::handle_find_visual_with_sampling(
                args,
                &self.registry,
                sampling_ctx,
            );
            let outcome = if result.is_error { "error" } else { "ok" };
            self.security.audit_tool_call(name, args, outcome);
            return result;
        }

        let result = if let Some(r) =
            crate::mcp::tools_innovation::call_workflow_tool(name, args, &self.workflows, out)
        {
            if !r.is_error {
                self.notify_subscribed(name, args, out);
            }
            r
        } else {
            let r = call_tool(name, args, &self.registry, out);
            if !r.is_error {
                self.notify_subscribed(name, args, out);
            }
            r
        };

        // Gate 4 — audit log.
        let outcome = if result.is_error { "error" } else { "ok" };
        self.security.audit_tool_call(name, args, outcome);

        result
    }

    /// Emit `notifications/resources/updated` for every subscribed URI that
    /// `tool_name` is known to affect.
    ///
    /// This implements the §6.3 "notify after state change" pattern without
    /// the full AX observer backend (which is Phase 5). Only URIs that the
    /// client has actively subscribed to receive a notification.
    fn notify_subscribed<W: Write>(&self, tool_name: &str, args: &Value, out: &mut W) {
        let Ok(subs) = self.subscriptions.lock() else {
            return;
        };
        if subs.is_empty() {
            return;
        }
        for uri in affected_uris(tool_name, args) {
            if subs.contains(*uri) {
                crate::mcp::server::notify_resource_changed(out, uri);
            }
        }
    }

    /// Try to dispatch a watch tool by name; returns `None` for non-watch tools.
    #[cfg(feature = "watch")]
    fn call_watch_tool(
        &self,
        name: &str,
        args: &Value,
    ) -> Option<crate::mcp::protocol::ToolCallResult> {
        use crate::mcp::tools_watch::{
            handle_ax_watch_start, handle_ax_watch_status, handle_ax_watch_stop,
        };
        match name {
            "ax_watch_start" => Some(handle_ax_watch_start(args, &self.watch_state)),
            "ax_watch_stop" => Some(handle_ax_watch_stop(&self.watch_state)),
            "ax_watch_status" => Some(handle_ax_watch_status(&self.watch_state)),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Map a tool call to the set of resource URIs it may have modified.
///
/// Returns a static slice of URI strings so the subscription notifier can
/// cheaply check which subscriptions to trigger without heap allocation.
/// Only the most directly affected URIs are listed — callers iterate and check
/// membership against the live subscription set.
fn affected_uris(tool_name: &str, _args: &Value) -> &'static [&'static str] {
    match tool_name {
        // Connection tools change the apps list and system status.
        "ax_connect" | "ax_disconnect" => &["axterminator://apps", "axterminator://system/status"],
        // Clipboard write changes the clipboard resource.
        "ax_clipboard" => &["axterminator://clipboard"],
        // Starting a capture session affects all three capture resources.
        "ax_start_capture" => &[
            "axterminator://capture/status",
            "axterminator://capture/transcription",
            "axterminator://capture/screen",
        ],
        // Stopping a session changes status and clears transcription/screen.
        "ax_stop_capture" => &[
            "axterminator://capture/status",
            "axterminator://capture/transcription",
            "axterminator://capture/screen",
        ],
        // All other tools may change per-app state/tree — the app name would
        // be needed for precise URI matching, but without it we skip per-app
        // URIs here to avoid false positives. Future phases will use the AX
        // observer backend for precise per-app notifications.
        _ => &[],
    }
}

/// Return `true` when the `tools/call` params request asynchronous execution.
///
/// The client signals this by including `"_meta": {"task": true}` anywhere in
/// the top-level params object, following the MCP Tasks §5 convention.
fn is_task_request(params: &Value) -> bool {
    params
        .get("_meta")
        .and_then(|m| m.get("task"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// Execute a tool without holding a reference to `Server`.
///
/// This is the function called from background threads spawned by
/// [`Server::dispatch_as_task`]. It mirrors the logic in
/// [`Server::dispatch_tool`] but accepts the individual shared state pieces
/// rather than `&self`, avoiding a lifetime dependency on the server.
///
/// Watch tools are intentionally excluded: they manage `WatchState` which is
/// not part of the task-dispatch path (they return immediately and are never
/// long-running).
fn execute_tool_background<W: Write>(
    name: &str,
    args: &Value,
    registry: &std::sync::Arc<crate::mcp::tools::AppRegistry>,
    workflows: &std::sync::Arc<
        std::sync::Mutex<std::collections::HashMap<String, super::server::WorkflowState>>,
    >,
    subscriptions: &std::sync::Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
    out: &mut W,
) -> crate::mcp::protocol::ToolCallResult {
    if let Some(result) =
        crate::mcp::tools_innovation::call_workflow_tool(name, args, workflows, out)
    {
        notify_subscribed_bg(name, args, subscriptions, out);
        return result;
    }
    let result = call_tool(name, args, registry, out);
    if !result.is_error {
        notify_subscribed_bg(name, args, subscriptions, out);
    }
    result
}

/// Background-thread variant of `Server::notify_subscribed`.
///
/// Identical logic but takes the subscription set directly rather than
/// borrowing from `Server`, which is not `Send`.
fn notify_subscribed_bg<W: Write>(
    tool_name: &str,
    args: &Value,
    subscriptions: &std::sync::Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
    out: &mut W,
) {
    let Ok(subs) = subscriptions.lock() else {
        return;
    };
    if subs.is_empty() {
        return;
    }
    for uri in affected_uris(tool_name, args) {
        if subs.contains(*uri) {
            crate::mcp::server::notify_resource_changed(out, uri);
        }
    }
}

fn build_initialize_result() -> InitializeResult {
    #[cfg(feature = "watch")]
    let experimental = Some(serde_json::json!({ "claude/channel": {} }));
    #[cfg(not(feature = "watch"))]
    let experimental = None;

    InitializeResult {
        protocol_version: "2025-11-05",
        capabilities: ServerCapabilities {
            tools: ToolsCapability {
                list_changed: false,
            },
            logging: LoggingCapability {},
            resources: ResourcesCapability {
                subscribe: true,
                list_changed: false,
            },
            prompts: PromptsCapability {
                list_changed: false,
            },
            elicitation: ElicitationCapability {},
            tasks: TasksCapability {},
            sampling: SamplingCapability {},
            experimental,
        },
        server_info: ServerInfo {
            name: "axterminator",
            version: env!("CARGO_PKG_VERSION"),
            title: "AXTerminator - macOS GUI Automation",
        },
        instructions: "\
AXTerminator: background-first macOS GUI automation via Accessibility API.\n\
\n\
Workflow: ax_connect → ax_get_tree (ALWAYS look first) → ax_find → interact → verify.\n\
\n\
Query syntax:\n\
- Simple text: query=\"Save\" — matches title/description/value/label/id (OR)\n\
- Prefixed: query=\"role:AXButton\" or \"description:7\" or \"value:42\" (AND when combined)\n\
\n\
Key rules:\n\
- ALWAYS ax_get_tree before ax_find — never guess element names\n\
- ax_click auto-falls-back to coordinate clicks when AXPress unsupported\n\
- ax_set_value for instant text, ax_type for keystroke simulation (needs focus)\n\
- ax_find includes semantic fallback — fuzzy matches when exact match fails\n\
\n\
Advanced tools:\n\
- ax_query: natural language UI questions\n\
- ax_analyze: detect UI patterns, infer app state, suggest actions\n\
- ax_workflow_create/step/status: durable multi-step automation\n\
- ax_test_run: black-box testing\n\
- ax_app_profile: Electron app metadata\n\
- ax_track_workflow: cross-app pattern detection\n\
- ax_record: interaction recording for test generation\n\
- ax_visual_diff: visual regression testing\n\
- ax_a11y_audit: WCAG accessibility compliance\n\
- ax_clipboard: read/write system clipboard\n\
- ax_run_script: AppleScript/JXA execution\n\
- ax_session_info: server state\n\
- ax_undo: undo last actions\n\
\n\
Use prompts/get for detailed guidance:\n\
- 'troubleshooting' — when something fails\n\
- 'app-guide' with app arg — per-app playbook\n\
- 'debug-ui' — debug element-not-found issues\n\
- 'automate-workflow' — durable workflow guidance\n\
- 'analyze-app' — comprehensive UI analysis\n\
- 'test-app' / 'navigate-to' / 'extract-data' / 'accessibility-audit'",
    }
}
