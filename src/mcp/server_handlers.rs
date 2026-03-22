//! `handle_*` method implementations for [`Server`].
//!
//! This module is a private implementation detail of [`server`].  All public
//! types live in [`server`]; this file only contains the `impl Server` block
//! for the individual JSON-RPC method handlers.
//!
//! [`server`]: crate::mcp::server

use std::io::Write;

use serde_json::Value;
use tracing::{debug, info};

use crate::mcp::protocol::{
    ElicitationCapability, InitializeParams, InitializeResult, JsonRpcResponse, LoggingCapability,
    PingResult, PromptGetParams, PromptsCapability, RequestId, ResourceReadParams,
    ResourceSubscribeParams, ResourceSubscribeResult, ResourceUnsubscribeParams,
    ResourcesCapability, RpcError, ServerCapabilities, ServerInfo, ToolCallParams, ToolListResult,
    ToolsCapability,
};
use crate::mcp::tools::call_tool;

use super::server::{Phase, Server};

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
                info!(
                    client = %p.client_info.name,
                    version = %p.client_info.version,
                    protocol = %p.protocol_version,
                    "client connected"
                );
                self.phase = Phase::Initializing;
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

    pub(super) fn handle_tools_list(id: RequestId) -> JsonRpcResponse {
        let result = ToolListResult {
            tools: crate::mcp::tools::all_tools(),
        };
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
                let tool_result = self.dispatch_tool(&p.name, &args, out);
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
    // Tool dispatch helper
    // -----------------------------------------------------------------------

    /// Dispatch a tool call, routing watch tools to the watch state first.
    ///
    /// After any state-changing tool succeeds, the server checks its
    /// subscription set and emits `notifications/resources/updated` for every
    /// subscribed URI that the tool may have affected.
    fn dispatch_tool<W: Write>(
        &self,
        name: &str,
        args: &Value,
        out: &mut W,
    ) -> crate::mcp::protocol::ToolCallResult {
        #[cfg(feature = "watch")]
        {
            let result = self.call_watch_tool(name, args);
            if let Some(r) = result {
                return r;
            }
        }
        if let Some(result) =
            crate::mcp::tools_innovation::call_workflow_tool(name, args, &self.workflows, out)
        {
            self.notify_subscribed(name, args, out);
            return result;
        }
        let result = call_tool(name, args, &self.registry, out);
        if !result.is_error {
            self.notify_subscribed(name, args, out);
        }
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
        // All other tools may change per-app state/tree — the app name would
        // be needed for precise URI matching, but without it we skip per-app
        // URIs here to avoid false positives. Future phases will use the AX
        // observer backend for precise per-app notifications.
        _ => &[],
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
