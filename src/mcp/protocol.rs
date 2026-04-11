//! MCP 2025-11-05 protocol types.
//!
//! Covers the wire types for Phase 1, Phase 2, and Phase 5 (Tasks API):
//! - `initialize` handshake with resources + prompts + tasks capabilities
//! - `tools/list` and `tools/call`
//! - `resources/list`, `resources/templates/list`, and `resources/read`
//! - `prompts/list` and `prompts/get`
//! - `tasks/list`, `tasks/result`, `tasks/cancel`
//!
//! All types derive `serde::{Serialize, Deserialize}` so they round-trip
//! cleanly through `serde_json`.
//!
//! ## Adding new capabilities
//!
//! Extend [`ServerCapabilities`] and add the corresponding request/result
//! types following the existing pattern. Wire the method in `server.rs`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// JSON-RPC 2.0 envelope
// ---------------------------------------------------------------------------

/// A JSON-RPC 2.0 request (server receives from client).
#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Option<RequestId>,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

/// A JSON-RPC 2.0 response (server sends to client).
#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: &'static str,
    pub id: RequestId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

/// A JSON-RPC 2.0 notification (server sends to client, no id).
#[derive(Debug, Serialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: &'static str,
    pub method: &'static str,
    pub params: Value,
}

/// JSON-RPC request identifier — either a number or a string.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    Number(i64),
    String(String),
}

/// JSON-RPC error object.
#[derive(Debug, Serialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl RpcError {
    pub const PARSE_ERROR: i32 = -32_700;
    pub const INVALID_REQUEST: i32 = -32_600;
    pub const METHOD_NOT_FOUND: i32 = -32_601;
    pub const INVALID_PARAMS: i32 = -32_602;
    pub const INTERNAL_ERROR: i32 = -32_603;

    /// Convenience constructor.
    #[must_use]
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }
}

impl JsonRpcResponse {
    /// Build a successful response.
    #[must_use]
    pub fn ok(id: RequestId, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    /// Build an error response.
    #[must_use]
    pub fn err(id: RequestId, error: RpcError) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(error),
        }
    }
}

// ---------------------------------------------------------------------------
// MCP initialize
// ---------------------------------------------------------------------------

/// Client sends this in `params` of the `initialize` request.
#[derive(Debug, Deserialize)]
pub struct InitializeParams {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub capabilities: ClientCapabilities,
    #[serde(rename = "clientInfo")]
    pub client_info: ClientInfo,
}

/// Subset of client capabilities we inspect.
#[derive(Debug, Default, Deserialize)]
pub struct ClientCapabilities {
    pub roots: Option<Value>,
    pub sampling: Option<Value>,
    pub elicitation: Option<Value>,
}

impl ClientCapabilities {
    /// Returns `true` when the client advertised elicitation support.
    ///
    /// MCP clients that support `elicitation/create` include the `elicitation`
    /// key in their capabilities object (value may be an empty object `{}`).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use serde_json::json;
    /// use axterminator::mcp::protocol::ClientCapabilities;
    ///
    /// let mut caps = ClientCapabilities::default();
    /// assert!(!caps.supports_elicitation());
    ///
    /// caps.elicitation = Some(json!({}));
    /// assert!(caps.supports_elicitation());
    /// ```
    #[must_use]
    pub fn supports_elicitation(&self) -> bool {
        self.elicitation.is_some()
    }

    /// Returns `true` when the client advertised sampling support.
    #[must_use]
    pub fn supports_sampling(&self) -> bool {
        self.sampling.is_some()
    }
}

/// Client identity (name + version).
#[derive(Debug, Deserialize)]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

/// Server sends this as `result` of the `initialize` response.
#[derive(Debug, Serialize)]
pub struct InitializeResult {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: &'static str,
    pub capabilities: ServerCapabilities,
    #[serde(rename = "serverInfo")]
    pub server_info: ServerInfo,
    pub instructions: &'static str,
}

/// Capabilities the server advertises in the `initialize` response.
///
/// Phase 2 adds `resources` and `prompts` alongside the existing `tools`
/// and `logging` capabilities.
///
/// Phase 4 adds `elicitation` — the server can ask the user questions
/// mid-operation via `elicitation/create`.
///
/// Phase 5 adds `tasks` — long-running operations return immediately and
/// results are polled via `tasks/result`.
///
/// Phase 6 (§14) adds `sampling` — the server can delegate LLM inference back
/// to the connected client via `sampling/createMessage`.  Presence of this
/// capability signals that the server *may* send sampling requests; whether it
/// actually does depends on whether the client also advertises `sampling` in
/// its own capabilities.
///
/// The `experimental` field carries non-standard capabilities such as
/// `claude/channel` for push notifications to Claude Code sessions.
#[derive(Debug, Serialize)]
pub struct ServerCapabilities {
    pub tools: ToolsCapability,
    pub logging: LoggingCapability,
    pub resources: ResourcesCapability,
    pub prompts: PromptsCapability,
    pub elicitation: ElicitationCapability,
    pub tasks: TasksCapability,
    /// §14 Sampling — server can send `sampling/createMessage` to the client.
    pub sampling: SamplingCapability,
    /// Experimental capabilities map.  Present only when the `watch` feature
    /// is enabled; omitted entirely otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experimental: Option<Value>,
}

/// Sampling capability advertised in `initialize` (§14).
///
/// Presence signals that the server may send `sampling/createMessage` requests
/// to the client mid-tool-call.  The client is free to reject or ignore these
/// requests; the server must handle the case gracefully.
///
/// The value is an empty object per the MCP 2025-11-05 spec — capability
/// presence alone is the signal.
#[derive(Debug, Serialize)]
pub struct SamplingCapability {}

/// Elicitation capability advertised in `initialize`.
///
/// Presence signals that the server may send `elicitation/create` requests.
/// The value is an empty object per the MCP 2025-11-05 spec.
#[derive(Debug, Serialize)]
pub struct ElicitationCapability {}

/// Tasks capability advertised in `initialize`.
///
/// Presence signals that the server supports the Tasks API (§5):
/// - `tools/call` with `_meta.task: true` returns immediately with a task ID
/// - Clients poll `tasks/result` for the final result
/// - `tasks/list` enumerates all in-flight and completed tasks
/// - `tasks/cancel` requests cancellation of a pending task
#[derive(Debug, Serialize)]
pub struct TasksCapability {}

/// Tool list capability.
#[derive(Debug, Serialize)]
pub struct ToolsCapability {
    #[serde(rename = "listChanged")]
    pub list_changed: bool,
}

/// Logging capability (empty — presence signals support).
#[derive(Debug, Serialize)]
pub struct LoggingCapability {}

/// Resource capability advertised in `initialize`.
///
/// `subscribe: true` — Phase 3 reactive subscriptions are enabled. Clients
/// may send `resources/subscribe` and will receive
/// `notifications/resources/updated` when subscribed URIs change.
#[derive(Debug, Serialize)]
pub struct ResourcesCapability {
    pub subscribe: bool,
    #[serde(rename = "listChanged")]
    pub list_changed: bool,
}

/// Prompt capability advertised in `initialize`.
#[derive(Debug, Serialize)]
pub struct PromptsCapability {
    #[serde(rename = "listChanged")]
    pub list_changed: bool,
}

/// Server identity.
#[derive(Debug, Serialize)]
pub struct ServerInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub title: &'static str,
}

// ---------------------------------------------------------------------------
// MCP tools
// ---------------------------------------------------------------------------

/// A single tool descriptor returned by `tools/list`.
#[derive(Debug, Clone, Serialize)]
pub struct Tool {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
    #[serde(rename = "outputSchema")]
    pub output_schema: Value,
    pub annotations: ToolAnnotations,
}

/// Semantic hints for MCP clients (MCP 2025-11-05 §6.3).
///
/// These four boolean fields are a direct serialisation of the MCP wire format —
/// each maps to a distinct JSON property. Refactoring into an enum would break
/// the protocol contract.
#[derive(Debug, Clone, Serialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct ToolAnnotations {
    /// True if the tool never mutates state.
    #[serde(rename = "readOnlyHint")]
    pub read_only: bool,
    /// True if the action cannot be undone.
    #[serde(rename = "destructiveHint")]
    pub destructive: bool,
    /// True if calling multiple times has the same effect as once.
    #[serde(rename = "idempotentHint")]
    pub idempotent: bool,
    /// True if the tool may interact with external services.
    #[serde(rename = "openWorldHint")]
    pub open_world: bool,
}

/// `tools/list` result.
#[derive(Debug, Serialize)]
pub struct ToolListResult {
    pub tools: Vec<Tool>,
}

/// `tools/call` params.
#[derive(Debug, Deserialize)]
pub struct ToolCallParams {
    pub name: String,
    #[serde(default)]
    pub arguments: Option<Value>,
}

/// A single content item returned by `tools/call`.
#[derive(Debug, Clone, Serialize)]
pub struct ContentItem {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub text: String,
}

impl ContentItem {
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            kind: "text",
            text: text.into(),
        }
    }
}

/// `tools/call` result.
#[derive(Debug, Clone, Serialize)]
pub struct ToolCallResult {
    pub content: Vec<ContentItem>,
    #[serde(rename = "isError")]
    pub is_error: bool,
}

impl ToolCallResult {
    #[must_use]
    pub fn ok(text: impl Into<String>) -> Self {
        Self {
            content: vec![ContentItem::text(text)],
            is_error: false,
        }
    }

    #[must_use]
    pub fn error(text: impl Into<String>) -> Self {
        Self {
            content: vec![ContentItem::text(text)],
            is_error: true,
        }
    }
}

// ---------------------------------------------------------------------------
// MCP ping
// ---------------------------------------------------------------------------

/// `ping` result — empty object per spec.
#[derive(Debug, Serialize)]
pub struct PingResult {}

// ---------------------------------------------------------------------------
// MCP resources/subscribe — Phase 3
// ---------------------------------------------------------------------------

/// `resources/subscribe` params — client sends to start receiving
/// `notifications/resources/updated` for a specific resource URI.
///
/// Per MCP 2025-11-05 §6.3, the server must declare `resources.subscribe: true`
/// in its `initialize` capabilities before clients will send this method.
#[derive(Debug, Deserialize)]
pub struct ResourceSubscribeParams {
    pub uri: String,
}

/// `resources/unsubscribe` params — client sends to stop receiving updates.
///
/// When the session ends, all subscriptions are implicitly removed because
/// the `Server` (and its subscription store) is dropped.
#[derive(Debug, Deserialize)]
pub struct ResourceUnsubscribeParams {
    pub uri: String,
}

/// `resources/subscribe` and `resources/unsubscribe` both return an empty
/// object per the MCP 2025-11-05 wire spec.
#[derive(Debug, Serialize)]
pub struct ResourceSubscribeResult {}

// ---------------------------------------------------------------------------
// MCP resources — Phase 2
// ---------------------------------------------------------------------------

/// A single resource descriptor returned by `resources/list`.
///
/// Static resources have a concrete `uri`; dynamic resources use
/// `uri_template` (RFC 6570) and appear in `resources/templates/list`.
#[derive(Debug, Clone, Serialize)]
pub struct Resource {
    pub uri: &'static str,
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    #[serde(rename = "mimeType")]
    pub mime_type: &'static str,
}

/// A URI template descriptor returned by `resources/templates/list`.
#[derive(Debug, Clone, Serialize)]
pub struct ResourceTemplate {
    #[serde(rename = "uriTemplate")]
    pub uri_template: &'static str,
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    #[serde(rename = "mimeType")]
    pub mime_type: &'static str,
}

/// `resources/list` result.
#[derive(Debug, Serialize)]
pub struct ResourceListResult {
    pub resources: Vec<Resource>,
}

/// `resources/templates/list` result.
#[derive(Debug, Serialize)]
pub struct ResourceTemplateListResult {
    #[serde(rename = "resourceTemplates")]
    pub resource_templates: Vec<ResourceTemplate>,
}

/// `resources/read` params.
#[derive(Debug, Deserialize)]
pub struct ResourceReadParams {
    pub uri: String,
}

/// A single resource content item (text or blob).
///
/// Text resources carry UTF-8 JSON in `text`.
/// Binary resources (e.g., PNG screenshots) carry base64 in `blob`.
#[derive(Debug, Serialize)]
pub struct ResourceContents {
    pub uri: String,
    #[serde(rename = "mimeType")]
    pub mime_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
}

impl ResourceContents {
    /// Build a text/JSON resource content item.
    #[must_use]
    pub fn text(uri: impl Into<String>, mime_type: &'static str, text: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            mime_type,
            text: Some(text.into()),
            blob: None,
        }
    }

    /// Build a binary (base64) resource content item.
    #[must_use]
    pub fn blob(uri: impl Into<String>, mime_type: &'static str, blob: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            mime_type,
            text: None,
            blob: Some(blob.into()),
        }
    }
}

/// `resources/read` result.
#[derive(Debug, Serialize)]
pub struct ResourceReadResult {
    pub contents: Vec<ResourceContents>,
}

// ---------------------------------------------------------------------------
// MCP prompts — Phase 2
// ---------------------------------------------------------------------------

/// A single prompt descriptor returned by `prompts/list`.
#[derive(Debug, Clone, Serialize)]
pub struct Prompt {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub arguments: Vec<PromptArgument>,
}

/// One argument declared by a prompt.
#[derive(Debug, Clone, Serialize)]
pub struct PromptArgument {
    pub name: &'static str,
    pub description: &'static str,
    pub required: bool,
}

/// `prompts/list` result.
#[derive(Debug, Serialize)]
pub struct PromptListResult {
    pub prompts: Vec<Prompt>,
}

/// `prompts/get` params.
#[derive(Debug, Deserialize)]
pub struct PromptGetParams {
    pub name: String,
    #[serde(default)]
    pub arguments: Option<serde_json::Map<String, serde_json::Value>>,
}

/// Role of a prompt message (MCP spec: "user" | "assistant").
#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PromptRole {
    User,
    Assistant,
}

/// A single message in a prompt result.
#[derive(Debug, Serialize)]
pub struct PromptMessage {
    pub role: PromptRole,
    pub content: PromptContent,
}

/// Content of a prompt message (text only for Phase 2).
#[derive(Debug, Serialize)]
pub struct PromptContent {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub text: String,
}

impl PromptContent {
    /// Build a plain-text prompt content item.
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            kind: "text",
            text: text.into(),
        }
    }
}

/// `prompts/get` result.
#[derive(Debug, Serialize)]
pub struct PromptGetResult {
    pub description: String,
    pub messages: Vec<PromptMessage>,
}

// ---------------------------------------------------------------------------
// MCP Tasks API — Phase 5
// ---------------------------------------------------------------------------

/// Task status values used in `TaskInfo.status`.
///
/// These are the four states defined by the MCP Tasks spec:
/// - `"working"` — the task is still executing
/// - `"done"` — the task completed successfully; call `tasks/result` to fetch
/// - `"failed"` — the task failed; `tasks/result` will return an error result
/// - `"cancelled"` — the task was cancelled via `tasks/cancel`
pub mod task_status {
    pub const WORKING: &str = "working";
    pub const DONE: &str = "done";
    pub const FAILED: &str = "failed";
    pub const CANCELLED: &str = "cancelled";
}

/// Status snapshot of a single task, returned by `tasks/list` and embedded
/// in the immediate `tools/call` response when `_meta.task: true`.
///
/// # Wire format
///
/// ```json
/// {
///   "taskId": "task-0000000000000001",
///   "status": "working",
///   "statusMessage": "Running ax_screenshot…"
/// }
/// ```
#[derive(Debug, Clone, Serialize)]
pub struct TaskInfo {
    /// Stable identifier for this task. Unique within the server session.
    #[serde(rename = "taskId")]
    pub task_id: String,
    /// One of the constants in [`task_status`].
    pub status: &'static str,
    /// Optional human-readable message describing the current status.
    #[serde(rename = "statusMessage", skip_serializing_if = "Option::is_none")]
    pub status_message: Option<String>,
}

/// `tasks/result` request params.
///
/// The client sends the `taskId` received from the initial `tools/call`
/// response and receives either the completed `ToolCallResult` or the current
/// `TaskInfo` if still in progress.
#[derive(Debug, Deserialize)]
pub struct TaskResultParams {
    #[serde(rename = "taskId")]
    pub task_id: String,
}

/// `tasks/cancel` request params.
///
/// Requesting cancellation sets the task status to `"cancelled"`.
/// For the synchronous stdio transport, tasks complete before the next
/// request is processed, so cancellation applies only to tasks that are
/// already in a terminal state or tasks whose result has not yet been fetched.
#[derive(Debug, Deserialize)]
pub struct TaskCancelParams {
    #[serde(rename = "taskId")]
    pub task_id: String,
}

/// `tasks/list` result — envelope for the task status list.
#[derive(Debug, Serialize)]
pub struct TasksListResult {
    pub tasks: Vec<TaskInfo>,
}

/// `tasks/result` response — either the completed result or the current status.
///
/// When the task is still working the client receives `{"task": {...}}`.
/// When the task is done the client receives `{"content": [...], "isError": ...}`.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum TaskResultResponse {
    /// Task is complete — return the full tool result.
    Complete(ToolCallResult),
    /// Task is still in progress (or was cancelled) — return status envelope.
    Pending { task: TaskInfo },
}

/// `tasks/cancel` result — empty object per spec.
#[derive(Debug, Serialize)]
pub struct TaskCancelResult {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn request_id_round_trips_number() {
        // GIVEN: numeric request id
        let id = RequestId::Number(42);
        // WHEN: serialised
        let json = serde_json::to_string(&id).unwrap();
        // THEN: bare number
        assert_eq!(json, "42");
    }

    #[test]
    fn request_id_round_trips_string() {
        // GIVEN: string request id
        let id = RequestId::String("abc".into());
        // WHEN: serialised
        let json = serde_json::to_string(&id).unwrap();
        // THEN: quoted string
        assert_eq!(json, r#""abc""#);
    }

    #[test]
    fn rpc_response_ok_omits_error_field() {
        // GIVEN: success response
        let resp = JsonRpcResponse::ok(RequestId::Number(1), json!({"status": "ok"}));
        // WHEN: serialised
        let v: Value = serde_json::to_value(&resp).unwrap();
        // THEN: no error key
        assert!(v.get("error").is_none());
        assert_eq!(v["result"]["status"], "ok");
    }

    #[test]
    fn rpc_response_err_omits_result_field() {
        // GIVEN: error response
        let resp = JsonRpcResponse::err(
            RequestId::Number(1),
            RpcError::new(RpcError::METHOD_NOT_FOUND, "not found"),
        );
        // WHEN: serialised
        let v: Value = serde_json::to_value(&resp).unwrap();
        // THEN: no result key
        assert!(v.get("result").is_none());
        assert_eq!(v["error"]["code"], RpcError::METHOD_NOT_FOUND);
    }

    #[test]
    fn tool_call_result_ok_is_not_error() {
        let r = ToolCallResult::ok("done");
        assert!(!r.is_error);
        assert_eq!(r.content[0].text, "done");
    }

    #[test]
    fn tool_call_result_error_is_error() {
        let r = ToolCallResult::error("boom");
        assert!(r.is_error);
        assert_eq!(r.content[0].text, "boom");
    }

    #[test]
    fn rpc_error_codes_are_correct_jsonrpc_values() {
        assert_eq!(RpcError::PARSE_ERROR, -32_700);
        assert_eq!(RpcError::METHOD_NOT_FOUND, -32_601);
    }

    // -----------------------------------------------------------------------
    // ClientCapabilities helpers
    // -----------------------------------------------------------------------

    #[test]
    fn supports_elicitation_false_by_default() {
        // GIVEN: empty capabilities
        let caps = ClientCapabilities::default();
        // THEN: elicitation not supported
        assert!(!caps.supports_elicitation());
    }

    #[test]
    fn supports_elicitation_true_when_set() {
        // GIVEN: capabilities with elicitation key
        let caps = ClientCapabilities {
            elicitation: Some(json!({})),
            ..Default::default()
        };
        // THEN: elicitation supported
        assert!(caps.supports_elicitation());
    }

    #[test]
    fn supports_sampling_false_by_default() {
        let caps = ClientCapabilities::default();
        assert!(!caps.supports_sampling());
    }

    #[test]
    fn supports_sampling_true_when_set() {
        let caps = ClientCapabilities {
            sampling: Some(json!({"createMessage": {}})),
            ..Default::default()
        };
        assert!(caps.supports_sampling());
    }
}
