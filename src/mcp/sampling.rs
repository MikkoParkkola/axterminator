//! MCP §14 Sampling — server-initiated LLM inference via the connected client.
//!
//! Sampling inverts the normal MCP data flow: instead of the client calling a
//! tool on the server, the server sends a `sampling/createMessage` request *to*
//! the client. The client (which has access to an LLM) generates a response and
//! returns it. This enables:
//!
//! - **Screenshot interpretation** — take a screenshot, ask the LLM to describe
//!   what it sees or locate a UI element.
//! - **Next-action planning** — send current UI state, receive a suggested step.
//! - **Visual element finding** — when `ax_find` fails, ask the LLM to locate
//!   the element in a screenshot.
//!
//! ## Synchronous stdio constraint
//!
//! The axterminator MCP server is synchronous — it owns stdin/stdout and
//! processes one message at a time. True mid-call sampling would require the
//! tool handler to write a `sampling/createMessage` to stdout and then read the
//! client's response from stdin before returning. This is architecturally sound
//! but requires the handler to hold a reference to the I/O streams.
//!
//! The [`create_message`] function implements exactly this pattern: it takes
//! mutable references to an output writer and input reader, performs the
//! synchronous exchange, and returns the LLM's text reply. Callers that do not
//! have stdin access (e.g. HTTP transport handlers) should check
//! [`SamplingContext::is_available`] before attempting a call.
//!
//! ## Wire format
//!
//! ```json
//! // Server → client (request)
//! {
//!   "jsonrpc": "2.0",
//!   "id": 9001,
//!   "method": "sampling/createMessage",
//!   "params": {
//!     "messages": [
//!       { "role": "user", "content": { "type": "text", "text": "Describe this UI." } }
//!     ],
//!     "maxTokens": 512
//!   }
//! }
//!
//! // Client → server (response)
//! {
//!   "jsonrpc": "2.0",
//!   "id": 9001,
//!   "result": {
//!     "role": "assistant",
//!     "content": { "type": "text", "text": "I see a Save button in the top-right corner." },
//!     "model": "claude-opus-4-5",
//!     "stopReason": "end_turn"
//!   }
//! }
//! ```

use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicI64, Ordering};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::debug;

use crate::mcp::protocol::RequestId;

// ---------------------------------------------------------------------------
// Unique request-ID counter
// ---------------------------------------------------------------------------

/// Monotonically increasing counter for sampling request IDs.
///
/// Starts at 9001 to avoid colliding with the low-numbered IDs that MCP
/// clients typically use for their own requests (1, 2, 3, …).
static SAMPLING_ID: AtomicI64 = AtomicI64::new(9001);

fn next_sampling_id() -> RequestId {
    RequestId::Number(SAMPLING_ID.fetch_add(1, Ordering::Relaxed))
}

// ---------------------------------------------------------------------------
// Wire types — outbound (server → client)
// ---------------------------------------------------------------------------

/// A `sampling/createMessage` request sent from the server to the client.
#[derive(Debug, Serialize)]
pub struct SamplingRequest {
    pub jsonrpc: &'static str,
    pub id: RequestId,
    pub method: &'static str,
    pub params: SamplingParams,
}

/// Parameters for `sampling/createMessage`.
#[derive(Debug, Clone, Serialize)]
pub struct SamplingParams {
    pub messages: Vec<SamplingMessage>,
    #[serde(rename = "maxTokens")]
    pub max_tokens: u32,
    #[serde(rename = "systemPrompt", skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    /// Model preference hints passed to the client. The client may ignore these.
    #[serde(rename = "modelPreferences", skip_serializing_if = "Option::is_none")]
    pub model_preferences: Option<ModelPreferences>,
}

/// A single conversation turn in a sampling request.
#[derive(Debug, Clone, Serialize)]
pub struct SamplingMessage {
    pub role: SamplingRole,
    pub content: SamplingContent,
}

/// Conversation role in a sampling exchange.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SamplingRole {
    User,
    Assistant,
}

/// Content of a sampling message — either plain text or a base64-encoded image.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SamplingContent {
    /// Plain text content.
    Text { text: String },
    /// A base64-encoded image (e.g. a PNG screenshot).
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: &'static str,
    },
}

impl SamplingContent {
    /// Build a plain-text content item.
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    /// Build an image content item from raw PNG bytes (base64-encoded internally).
    #[must_use]
    pub fn png(bytes: &[u8]) -> Self {
        use base64::Engine as _;
        let data = base64::engine::general_purpose::STANDARD.encode(bytes);
        Self::Image {
            data,
            mime_type: "image/png",
        }
    }
}

/// Optional model-preference hints (MCP §14).
#[derive(Debug, Clone, Serialize)]
pub struct ModelPreferences {
    /// Relative weight for intelligence (0.0–1.0). Higher means prefer smarter models.
    #[serde(
        rename = "intelligencePriority",
        skip_serializing_if = "Option::is_none"
    )]
    pub intelligence_priority: Option<f32>,
    /// Relative weight for speed (0.0–1.0). Higher means prefer faster models.
    #[serde(rename = "speedPriority", skip_serializing_if = "Option::is_none")]
    pub speed_priority: Option<f32>,
}

// ---------------------------------------------------------------------------
// Wire types — inbound (client → server)
// ---------------------------------------------------------------------------

/// The `result` object inside a successful `sampling/createMessage` response.
#[derive(Debug, Deserialize)]
pub struct SamplingResult {
    pub role: SamplingRole,
    pub content: SamplingResultContent,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(rename = "stopReason", default)]
    pub stop_reason: Option<String>,
}

/// Content inside a sampling response — may be text or image.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SamplingResultContent {
    Text { text: String },
    Image { data: String },
}

impl SamplingResultContent {
    /// Extract the text, if this is a text response.
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text { text } => Some(text),
            Self::Image { .. } => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur during a sampling exchange.
#[derive(Debug, thiserror::Error)]
pub enum SamplingError {
    #[error("client does not support sampling")]
    NotSupported,
    #[error("I/O error during sampling exchange: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to serialise sampling request: {0}")]
    Serialise(serde_json::Error),
    #[error("failed to parse sampling response: {0}")]
    Parse(serde_json::Error),
    #[error("sampling response contained an error: code={code}, message={message}")]
    RpcError { code: i32, message: String },
    #[error("sampling response had no text content")]
    NoTextContent,
}

// ---------------------------------------------------------------------------
// Core exchange function
// ---------------------------------------------------------------------------

/// Send a `sampling/createMessage` request to the client and return the text reply.
///
/// This implements the synchronous stdio sampling pattern: the function writes
/// one JSON-RPC request line to `out`, then reads one JSON-RPC response line
/// from `input`. The entire exchange is blocking and must complete before the
/// enclosing tool handler returns.
///
/// # Errors
///
/// - [`SamplingError::Io`] — if writing or reading the stream fails.
/// - [`SamplingError::Serialise`] — if the request cannot be JSON-encoded (never
///   happens in practice given static structure).
/// - [`SamplingError::Parse`] — if the response line is not valid JSON.
/// - [`SamplingError::RpcError`] — if the client returns a JSON-RPC error object.
/// - [`SamplingError::NoTextContent`] — if the result exists but has no text.
pub fn create_message<W, R>(
    out: &mut W,
    input: &mut R,
    messages: Vec<SamplingMessage>,
    max_tokens: u32,
    system_prompt: Option<String>,
) -> Result<String, SamplingError>
where
    W: Write,
    R: BufRead,
{
    let id = next_sampling_id();
    let request = SamplingRequest {
        jsonrpc: "2.0",
        id,
        method: "sampling/createMessage",
        params: SamplingParams {
            messages,
            max_tokens,
            system_prompt,
            model_preferences: None,
        },
    };

    let json = serde_json::to_string(&request).map_err(SamplingError::Serialise)?;
    debug!(bytes = json.len(), "sending sampling/createMessage");
    writeln!(out, "{json}")?;
    out.flush()?;

    let mut line = String::new();
    input.read_line(&mut line)?;
    debug!(bytes = line.len(), "received sampling response");

    parse_sampling_response(&line)
}

/// Parse a raw JSON-RPC response line into the LLM's text reply.
fn parse_sampling_response(line: &str) -> Result<String, SamplingError> {
    let value: Value = serde_json::from_str(line.trim()).map_err(SamplingError::Parse)?;

    // Check for JSON-RPC error object first.
    if let Some(err) = value.get("error") {
        let code = err["code"].as_i64().unwrap_or(-1) as i32;
        let message = err["message"].as_str().unwrap_or("unknown").to_string();
        return Err(SamplingError::RpcError { code, message });
    }

    let result: SamplingResult =
        serde_json::from_value(value.get("result").cloned().unwrap_or(Value::Null))
            .map_err(SamplingError::Parse)?;

    result
        .content
        .as_text()
        .map(str::to_string)
        .ok_or(SamplingError::NoTextContent)
}

// ---------------------------------------------------------------------------
// Context — capability guard
// ---------------------------------------------------------------------------

/// Carries whether the current MCP session supports sampling.
///
/// Passed into tool handlers that can optionally use LLM inference. Handlers
/// call [`SamplingContext::is_available`] before attempting a sampling exchange,
/// and fall back gracefully when the client does not advertise the capability.
///
/// # Example
///
/// ```rust
/// use axterminator::mcp::sampling::SamplingContext;
///
/// let ctx = SamplingContext::unavailable();
/// assert!(!ctx.is_available());
///
/// let ctx = SamplingContext::available();
/// assert!(ctx.is_available());
/// ```
#[derive(Debug, Clone, Copy)]
pub struct SamplingContext {
    available: bool,
}

impl SamplingContext {
    /// Build a context where sampling is available.
    #[must_use]
    pub const fn available() -> Self {
        Self { available: true }
    }

    /// Build a context where sampling is not available (client did not advertise it).
    #[must_use]
    pub const fn unavailable() -> Self {
        Self { available: false }
    }

    /// Returns `true` when the connected client supports `sampling/createMessage`.
    #[must_use]
    pub const fn is_available(self) -> bool {
        self.available
    }
}

impl From<bool> for SamplingContext {
    fn from(available: bool) -> Self {
        Self { available }
    }
}

// ---------------------------------------------------------------------------
// Builder helpers
// ---------------------------------------------------------------------------

/// Build a sampling request for visual element location.
///
/// Constructs a `messages` vec that asks the LLM to find a described UI element
/// in a screenshot and return its approximate screen coordinates.
///
/// # Parameters
///
/// - `description` — natural language description of the element to find.
/// - `screenshot_png` — raw PNG bytes of the screenshot.
///
/// # Returns
///
/// A `(messages, system_prompt)` pair ready to pass to [`create_message`].
#[must_use]
pub fn locate_element_messages(
    description: &str,
    screenshot_png: &[u8],
) -> (Vec<SamplingMessage>, Option<String>) {
    let system = "You are a macOS UI automation assistant. When shown a screenshot, \
                  identify the requested UI element and respond with a JSON object containing \
                  the element's approximate center coordinates: \
                  {\"found\": true, \"x\": <int>, \"y\": <int>, \"description\": \"<what you see>\"}. \
                  If the element is not found, respond with: \
                  {\"found\": false, \"description\": \"<what you see instead>\"}.";

    let messages = vec![
        SamplingMessage {
            role: SamplingRole::User,
            content: SamplingContent::png(screenshot_png),
        },
        SamplingMessage {
            role: SamplingRole::User,
            content: SamplingContent::text(format!(
                "Find this UI element in the screenshot: {description}"
            )),
        },
    ];

    (messages, Some(system.to_string()))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    // -----------------------------------------------------------------------
    // SamplingContent
    // -----------------------------------------------------------------------

    #[test]
    fn sampling_content_text_serialises_with_type_tag() {
        // GIVEN: a text content item
        let content = SamplingContent::text("hello");
        // WHEN: serialised
        let v: serde_json::Value = serde_json::to_value(&content).unwrap();
        // THEN: type tag present, text field present
        assert_eq!(v["type"], "text");
        assert_eq!(v["text"], "hello");
    }

    #[test]
    fn sampling_content_image_serialises_with_mime_type() {
        // GIVEN: a PNG content item (1-byte payload for simplicity)
        let content = SamplingContent::png(&[0u8]);
        // WHEN: serialised
        let v: serde_json::Value = serde_json::to_value(&content).unwrap();
        // THEN: type=image, mimeType=image/png, data is base64
        assert_eq!(v["type"], "image");
        assert_eq!(v["mimeType"], "image/png");
        assert!(v["data"].as_str().is_some());
    }

    // -----------------------------------------------------------------------
    // SamplingRequest wire format
    // -----------------------------------------------------------------------

    #[test]
    fn sampling_request_serialises_required_fields() {
        // GIVEN: a sampling request
        let req = SamplingRequest {
            jsonrpc: "2.0",
            id: RequestId::Number(9001),
            method: "sampling/createMessage",
            params: SamplingParams {
                messages: vec![SamplingMessage {
                    role: SamplingRole::User,
                    content: SamplingContent::text("describe"),
                }],
                max_tokens: 256,
                system_prompt: None,
                model_preferences: None,
            },
        };
        // WHEN: serialised
        let v: serde_json::Value = serde_json::to_value(&req).unwrap();
        // THEN: all required JSON-RPC fields present
        assert_eq!(v["jsonrpc"], "2.0");
        assert_eq!(v["id"], 9001);
        assert_eq!(v["method"], "sampling/createMessage");
        assert_eq!(v["params"]["maxTokens"], 256);
        assert!(v["params"]["messages"].as_array().is_some());
        // AND: optional fields absent
        assert!(v["params"].get("systemPrompt").is_none());
        assert!(v["params"].get("modelPreferences").is_none());
    }

    #[test]
    fn sampling_request_includes_system_prompt_when_set() {
        // GIVEN: request with system prompt
        let req = SamplingRequest {
            jsonrpc: "2.0",
            id: RequestId::Number(9002),
            method: "sampling/createMessage",
            params: SamplingParams {
                messages: vec![],
                max_tokens: 128,
                system_prompt: Some("be concise".into()),
                model_preferences: None,
            },
        };
        // WHEN: serialised
        let v: serde_json::Value = serde_json::to_value(&req).unwrap();
        // THEN: systemPrompt present
        assert_eq!(v["params"]["systemPrompt"], "be concise");
    }

    // -----------------------------------------------------------------------
    // parse_sampling_response
    // -----------------------------------------------------------------------

    #[test]
    fn parse_response_extracts_text_from_result() {
        // GIVEN: a well-formed client response
        let line = r#"{"jsonrpc":"2.0","id":9001,"result":{"role":"assistant","content":{"type":"text","text":"I see a Save button."}}}"#;
        // WHEN: parsed
        let text = parse_sampling_response(line).unwrap();
        // THEN: text extracted
        assert_eq!(text, "I see a Save button.");
    }

    #[test]
    fn parse_response_returns_rpc_error_on_error_object() {
        // GIVEN: response with JSON-RPC error
        let line =
            r#"{"jsonrpc":"2.0","id":9001,"error":{"code":-32600,"message":"Invalid Request"}}"#;
        // WHEN: parsed
        let err = parse_sampling_response(line).unwrap_err();
        // THEN: RpcError variant with correct fields
        assert!(matches!(err, SamplingError::RpcError { code: -32600, .. }));
    }

    #[test]
    fn parse_response_returns_parse_error_on_bad_json() {
        // GIVEN: malformed JSON
        let err = parse_sampling_response("not json").unwrap_err();
        // THEN: Parse error
        assert!(matches!(err, SamplingError::Parse(_)));
    }

    #[test]
    fn parse_response_returns_no_text_on_image_content() {
        // GIVEN: response with image content (not text)
        let line = r#"{"jsonrpc":"2.0","id":9001,"result":{"role":"assistant","content":{"type":"image","data":"abc123"}}}"#;
        // WHEN: parsed
        let err = parse_sampling_response(line).unwrap_err();
        // THEN: NoTextContent error
        assert!(matches!(err, SamplingError::NoTextContent));
    }

    // -----------------------------------------------------------------------
    // create_message (full I/O round-trip)
    // -----------------------------------------------------------------------

    #[test]
    fn create_message_writes_request_and_reads_response() {
        // GIVEN: a mock response pre-loaded in a Cursor
        let response_line = "{\"jsonrpc\":\"2.0\",\"id\":9999,\"result\":{\"role\":\"assistant\",\"content\":{\"type\":\"text\",\"text\":\"found it\"}}}\n";
        let mut input = Cursor::new(response_line.as_bytes().to_vec());
        let mut output = Vec::<u8>::new();

        // WHEN: create_message is called
        let text = create_message(
            &mut output,
            &mut input,
            vec![SamplingMessage {
                role: SamplingRole::User,
                content: SamplingContent::text("find the Save button"),
            }],
            256,
            None,
        )
        .unwrap();

        // THEN: text extracted from response
        assert_eq!(text, "found it");
        // AND: a valid JSON-RPC request was written to output
        let written = std::str::from_utf8(&output).unwrap();
        let v: serde_json::Value = serde_json::from_str(written.trim()).unwrap();
        assert_eq!(v["method"], "sampling/createMessage");
        assert_eq!(v["params"]["maxTokens"], 256);
    }

    // -----------------------------------------------------------------------
    // SamplingContext
    // -----------------------------------------------------------------------

    #[test]
    fn sampling_context_available_reports_true() {
        assert!(SamplingContext::available().is_available());
    }

    #[test]
    fn sampling_context_unavailable_reports_false() {
        assert!(!SamplingContext::unavailable().is_available());
    }

    // -----------------------------------------------------------------------
    // locate_element_messages
    // -----------------------------------------------------------------------

    #[test]
    fn locate_element_messages_produces_two_user_turns() {
        // GIVEN: a description and trivial screenshot bytes
        let (messages, system_prompt) = locate_element_messages("Save button", &[0u8; 4]);
        // THEN: two user messages
        assert_eq!(messages.len(), 2);
        assert!(matches!(messages[0].role, SamplingRole::User));
        assert!(matches!(messages[0].content, SamplingContent::Image { .. }));
        assert!(matches!(messages[1].content, SamplingContent::Text { .. }));
        // AND: system prompt present
        assert!(system_prompt.is_some());
    }

    // -----------------------------------------------------------------------
    // next_sampling_id
    // -----------------------------------------------------------------------

    #[test]
    fn next_sampling_id_is_monotonically_increasing() {
        // GIVEN: two consecutive calls (may start at any value >= 9001)
        let a = match next_sampling_id() {
            RequestId::Number(n) => n,
            RequestId::String(_) => panic!("expected number"),
        };
        let b = match next_sampling_id() {
            RequestId::Number(n) => n,
            RequestId::String(_) => panic!("expected number"),
        };
        // THEN: strictly increasing
        assert!(b > a);
    }
}
