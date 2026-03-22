//! `handle_*` method implementations for [`Server`].
//!
//! This module is a private implementation detail of [`server`].  All public
//! types live in [`server`]; this file only contains the `impl Server` block
//! for the individual JSON-RPC method handlers.
//!
//! [`server`]: crate::mcp::server

use std::io::Write;

use serde_json::Value;
use tracing::info;

use crate::mcp::protocol::{
    ElicitationCapability, InitializeParams, InitializeResult, JsonRpcResponse, LoggingCapability,
    PingResult, PromptGetParams, PromptsCapability, RequestId, ResourceReadParams,
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
        call_tool(name, args, &self.registry, out)
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
                subscribe: false,
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
- If ax_click fails with 'AXPress unsupported' → use ax_click_at with coordinates\n\
- Use ax_set_value for instant text, ax_type for keystroke simulation (needs focus)\n\
\n\
Use prompts/get for detailed guidance:\n\
- 'troubleshooting' — when something fails (element not found, click not working)\n\
- 'app-guide' with app arg — app-specific playbook (Calculator, TextEdit, Safari, etc.)\n\
- 'test-app' — step-by-step testing guide\n\
- 'navigate-to' — reach a specific UI state\n\
- 'extract-data' — read structured data from an app\n\
- 'accessibility-audit' — audit app for a11y issues",
    }
}
