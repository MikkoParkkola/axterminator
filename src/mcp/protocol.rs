//! MCP 2025-11-05 protocol types.
//!
//! Covers the subset of MCP used for Phase 1: initialize handshake,
//! `tools/list`, and `tools/call`. All types derive `serde::{Serialize, Deserialize}`
//! so they round-trip cleanly through `serde_json`.

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

/// Capabilities the server advertises.
#[derive(Debug, Serialize)]
pub struct ServerCapabilities {
    pub tools: ToolsCapability,
    pub logging: LoggingCapability,
}

#[derive(Debug, Serialize)]
pub struct ToolsCapability {
    #[serde(rename = "listChanged")]
    pub list_changed: bool,
}

#[derive(Debug, Serialize)]
pub struct LoggingCapability {}

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
#[derive(Debug, Serialize)]
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
#[derive(Debug, Serialize)]
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
}
