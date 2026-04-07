//! MCP elicitation — server-initiated user questions.
//!
//! Elicitation lets the server ask the user a question mid-operation. The
//! server sends an `elicitation/create` JSON-RPC request to the client; the
//! client renders a form (or URL prompt) and returns the user's answer.
//!
//! This module implements the four scenarios that have the highest user-facing
//! impact, matching the MCP 2025-11-25 `elicitation/create` wire format:
//!
//! | # | Scenario | Trigger |
//! |---|----------|---------|
//! | 1 | Ambiguous app name | Multiple running apps match the given name |
//! | 2 | Element not found | Element missing, show closest matches |
//! | 3 | Destructive action | Element text contains destructive keyword |
//! | 4 | Permissions missing | `ax_is_accessible` returned `false` |
//!
//! # Design
//!
//! All four helpers build a [`ElicitRequest`] and return it. The caller (the
//! MCP server dispatch loop) is responsible for sending it to the client and
//! awaiting the response. This separation keeps the module pure and trivially
//! testable without a live MCP session.
//!
//! # Graceful fallback
//!
//! When the client does not advertise `elicitation` in its capabilities, the
//! server must fall back gracefully (return an error or proceed with a warning
//! log). The session layer owns that decision; this module only constructs
//! requests and parses responses.
//!
//! # Wire format
//!
//! ```json
//! {
//!   "jsonrpc": "2.0",
//!   "id": 42,
//!   "method": "elicitation/create",
//!   "params": {
//!     "message": "Multiple apps match 'Mail'. Which one?",
//!     "requestedSchema": { "type": "object", ... }
//!   }
//! }
//! ```
//!
//! The client responds with:
//!
//! ```json
//! { "action": "accept", "content": { "choice": "Mail (Apple)" } }
//! ```

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can arise when building or interpreting an elicitation.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ElicitError {
    /// The client does not support elicitation.
    ///
    /// Callers should fall back to a plain error message.
    #[error("client does not support elicitation")]
    NotSupported,

    /// The user explicitly declined the request.
    #[error("user declined: {0}")]
    Declined(String),

    /// The user cancelled the operation (closed the dialog without answering).
    #[error("user cancelled the operation")]
    Cancelled,

    /// The response arrived but the expected field was absent or had the wrong type.
    #[error("elicitation response missing field '{0}'")]
    MissingField(String),
}

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

/// Parameters for `elicitation/create` — sent from server to client.
///
/// Matches the MCP 2025-11-25 wire format. The `requested_schema` field is a
/// JSON Schema object describing the expected response shape.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ElicitParams {
    /// Human-readable description shown to the user.
    pub message: String,
    /// JSON Schema for the expected response `content` object.
    #[serde(rename = "requestedSchema")]
    pub requested_schema: Value,
}

/// Full `elicitation/create` request, ready to serialize into a JSON-RPC
/// request body.
///
/// This type is returned by every `elicit_*` helper in this module.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ElicitRequest {
    /// The `params` block for `elicitation/create`.
    pub params: ElicitParams,
}

impl ElicitRequest {
    /// Construct a new [`ElicitRequest`] from a message and a JSON Schema.
    #[must_use]
    pub fn new(message: impl Into<String>, schema: Value) -> Self {
        Self {
            params: ElicitParams {
                message: message.into(),
                requested_schema: schema,
            },
        }
    }
}

/// The action the user took in response to an elicitation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ElicitAction {
    /// User confirmed and submitted the form.
    Accept,
    /// User explicitly declined.
    Decline,
    /// User dismissed the dialog without answering.
    Cancel,
}

/// Full `elicitation/create` response, received from the client.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ElicitResponse {
    /// What the user did.
    pub action: ElicitAction,
    /// The submitted form data, present only when `action` is `Accept`.
    #[serde(default)]
    pub content: Option<Value>,
}

impl ElicitResponse {
    /// Return `Ok(content)` if the user accepted, or the appropriate error.
    ///
    /// # Errors
    ///
    /// - [`ElicitError::Declined`] if `action` is `Decline`.
    /// - [`ElicitError::Cancelled`] if `action` is `Cancel`.
    pub fn into_accepted(self) -> Result<Value, ElicitError> {
        match self.action {
            ElicitAction::Accept => Ok(self
                .content
                .unwrap_or(Value::Object(serde_json::Map::default()))),
            ElicitAction::Decline => Err(ElicitError::Declined("user declined".into())),
            ElicitAction::Cancel => Err(ElicitError::Cancelled),
        }
    }
}

// ---------------------------------------------------------------------------
// Scenario 1 — Ambiguous app name
// ---------------------------------------------------------------------------

/// Build an elicitation request for an ambiguous app name.
///
/// When multiple running apps match the name supplied to `ax_connect`, the
/// server asks the user to select exactly one.
///
/// # Parameters
///
/// - `query`: the original search string the user provided.
/// - `candidates`: slice of `(display_name, bundle_id)` pairs. Must be
///   non-empty; the first entry is pre-selected.
///
/// # Examples
///
/// ```rust
/// use axterminator::mcp::elicitation::elicit_ambiguous_app;
///
/// let req = elicit_ambiguous_app(
///     "Chrome",
///     &[
///         ("Google Chrome".into(), "com.google.Chrome".into()),
///         ("Chrome Canary".into(), "com.google.Chrome.canary".into()),
///     ],
/// );
/// assert!(req.params.message.contains("Chrome"));
/// assert_eq!(req.params.requested_schema["properties"]["app"]["oneOf"]
///     .as_array().unwrap().len(), 2);
/// ```
#[must_use]
pub fn elicit_ambiguous_app(query: &str, candidates: &[(String, String)]) -> ElicitRequest {
    let choices: Vec<Value> = candidates
        .iter()
        .map(|(name, bundle)| json!({ "const": bundle, "title": name }))
        .collect();

    let schema = json!({
        "type": "object",
        "properties": {
            "app": {
                "type": "string",
                "title": "Select application",
                "oneOf": choices
            }
        },
        "required": ["app"]
    });

    ElicitRequest::new(
        format!("Multiple apps match '{query}'. Which one do you want to connect to?"),
        schema,
    )
}

/// Extract the selected bundle ID from an ambiguous-app elicitation response.
///
/// # Errors
///
/// - [`ElicitError::Declined`] / [`ElicitError::Cancelled`] if the user did
///   not accept.
/// - [`ElicitError::MissingField`] if the `app` field is absent or not a
///   string.
pub fn parse_ambiguous_app(resp: ElicitResponse) -> Result<String, ElicitError> {
    let content = resp.into_accepted()?;
    content["app"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| ElicitError::MissingField("app".into()))
}

// ---------------------------------------------------------------------------
// Scenario 2 — Element not found / clarification
// ---------------------------------------------------------------------------

/// Build an elicitation request when an element cannot be found.
///
/// Shows up to three closest matches and lets the user pick one, supply a
/// different query, or trigger AI vision search.
///
/// # Examples
///
/// ```rust
/// use axterminator::mcp::elicitation::elicit_element_not_found;
///
/// let req = elicit_element_not_found("Submit", "Safari", &["Submit Form", "Cancel"]);
/// assert!(req.params.message.contains("Submit"));
/// ```
#[must_use]
pub fn elicit_element_not_found(
    query: &str,
    app: &str,
    closest: &[impl AsRef<str>],
) -> ElicitRequest {
    let top: Vec<&str> = closest.iter().take(3).map(AsRef::as_ref).collect();

    let message = build_not_found_message(query, app, &top);
    let schema = build_not_found_schema(&top);

    ElicitRequest::new(message, schema)
}

fn build_not_found_message(query: &str, app: &str, top: &[&str]) -> String {
    if top.is_empty() {
        return format!("Could not find '{query}' in {app}. Please describe it differently.");
    }
    let list = top
        .iter()
        .enumerate()
        .map(|(i, s)| format!("  {}. {}", i + 1, s))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Could not find '{query}' in {app}. Closest matches:\n{list}\n\nPick one or supply a new query."
    )
}

fn build_not_found_schema(top: &[&str]) -> Value {
    if top.is_empty() {
        return json!({
            "type": "object",
            "properties": {
                "description": { "type": "string", "title": "Alternative description" },
                "use_visual": { "type": "boolean", "title": "Try AI vision search?", "default": true }
            },
            "required": ["description"]
        });
    }

    let mut choices: Vec<Value> = top
        .iter()
        .map(|s| json!({ "const": s, "title": s }))
        .collect();
    choices.push(json!({ "const": "__custom__", "title": "Enter a different query" }));

    json!({
        "type": "object",
        "properties": {
            "choice": {
                "type": "string",
                "title": "Select element",
                "oneOf": choices
            },
            "custom_query": {
                "type": "string",
                "title": "Custom query (if you selected 'Enter a different query')"
            },
            "use_visual": {
                "type": "boolean",
                "title": "Try AI vision search as fallback?",
                "default": false
            }
        },
        "required": ["choice"]
    })
}

/// Response shape from the element-not-found elicitation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElementChoice {
    /// User selected one of the suggested candidates.
    Candidate(String),
    /// User supplied a custom query string.
    Custom(String),
    /// Fall back to AI visual search.
    UseVisual,
}

/// Parse the user's response from an element-not-found elicitation.
///
/// # Errors
///
/// See [`ElicitResponse::into_accepted`] and [`ElicitError::MissingField`].
pub fn parse_element_not_found(resp: ElicitResponse) -> Result<ElementChoice, ElicitError> {
    let content = resp.into_accepted()?;

    // Schema with closest matches: look at `choice` field.
    if let Some(choice) = content.get("choice").and_then(Value::as_str) {
        if choice == "__custom__" {
            let custom = content["custom_query"]
                .as_str()
                .ok_or_else(|| ElicitError::MissingField("custom_query".into()))?
                .to_owned();
            return Ok(ElementChoice::Custom(custom));
        }
        // Check if they also want visual search.
        let use_visual = content
            .get("use_visual")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if use_visual {
            return Ok(ElementChoice::UseVisual);
        }
        return Ok(ElementChoice::Candidate(choice.to_owned()));
    }

    // Schema without candidates: look at `description` / `use_visual`.
    if content
        .get("use_visual")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Ok(ElementChoice::UseVisual);
    }
    let desc = content["description"]
        .as_str()
        .ok_or_else(|| ElicitError::MissingField("description".into()))?
        .to_owned();
    Ok(ElementChoice::Custom(desc))
}

// ---------------------------------------------------------------------------
// Scenario 3 — Destructive action confirmation
// ---------------------------------------------------------------------------

/// Keywords that indicate a potentially destructive UI element.
///
/// These mirror the set used by the design document.
const DESTRUCTIVE_KEYWORDS: &[&str] = &[
    "delete",
    "remove",
    "erase",
    "quit",
    "close",
    "format",
    "reset",
    "clear",
    "wipe",
    "destroy",
    "terminate",
    "uninstall",
    "revoke",
];

/// Return `true` when `element_text` contains any destructive keyword.
///
/// The check is case-insensitive and works on the element title, value, or
/// any textual representation.
///
/// # Examples
///
/// ```rust
/// use axterminator::mcp::elicitation::is_destructive_element;
///
/// assert!(is_destructive_element("Delete All Data"));
/// assert!(is_destructive_element("Format Drive"));
/// assert!(!is_destructive_element("Submit Form"));
/// ```
#[must_use]
pub fn is_destructive_element(element_text: &str) -> bool {
    let lower = element_text.to_lowercase();
    DESTRUCTIVE_KEYWORDS.iter().any(|kw| lower.contains(kw))
}

/// Build a destructive-action confirmation elicitation request.
///
/// The user must explicitly confirm before the server proceeds.
///
/// # Examples
///
/// ```rust
/// use axterminator::mcp::elicitation::elicit_destructive_action;
///
/// let req = elicit_destructive_action("Delete All", "Finder");
/// assert!(req.params.message.contains("Delete All"));
/// ```
#[must_use]
pub fn elicit_destructive_action(element_text: &str, app: &str) -> ElicitRequest {
    let schema = json!({
        "type": "object",
        "properties": {
            "confirm": {
                "type": "boolean",
                "title": "Confirm destructive action",
                "description": "This action may be irreversible.",
                "default": false
            }
        },
        "required": ["confirm"]
    });

    ElicitRequest::new(
        format!(
            "About to click '{element_text}' in {app}. \
             This action may be irreversible. Proceed?"
        ),
        schema,
    )
}

/// Parse the user's response from a destructive-action confirmation.
///
/// Returns `Ok(())` when confirmed, or an error otherwise.
///
/// # Errors
///
/// - [`ElicitError::Declined`] when `confirm` is `false` or the user
///   declined.
/// - [`ElicitError::Cancelled`] when the user dismissed the dialog.
/// - [`ElicitError::MissingField`] when the `confirm` field is absent.
pub fn parse_destructive_action(resp: ElicitResponse) -> Result<(), ElicitError> {
    let content = resp.into_accepted()?;
    match content.get("confirm").and_then(Value::as_bool) {
        Some(true) => Ok(()),
        Some(false) => Err(ElicitError::Declined("user did not confirm".into())),
        None => Err(ElicitError::MissingField("confirm".into())),
    }
}

// ---------------------------------------------------------------------------
// Scenario 4 — Accessibility permissions missing
// ---------------------------------------------------------------------------

/// Build an elicitation request when accessibility permissions are not granted.
///
/// Presents three choices: open System Settings directly, receive manual
/// instructions, or cancel the operation entirely.
///
/// # Examples
///
/// ```rust
/// use axterminator::mcp::elicitation::{elicit_permissions_missing, PermissionAction};
///
/// let req = elicit_permissions_missing();
/// // Message mentions accessibility (case-insensitive check).
/// assert!(req.params.message.to_ascii_lowercase().contains("accessibility"));
/// ```
#[must_use]
pub fn elicit_permissions_missing() -> ElicitRequest {
    let schema = json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "title": "Action",
                "oneOf": [
                    {
                        "const": "open_settings",
                        "title": "Open System Settings for me (requires brief focus)"
                    },
                    {
                        "const": "show_instructions",
                        "title": "Show me how to enable it manually"
                    },
                    {
                        "const": "cancel",
                        "title": "Cancel — I will handle this myself"
                    }
                ]
            }
        },
        "required": ["action"]
    });

    ElicitRequest::new(
        "Accessibility permissions are not enabled for this process. \
         Without them, no tools can interact with macOS applications. \
         How would you like to proceed?",
        schema,
    )
}

/// The action chosen by the user in response to a permissions-missing prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionAction {
    /// Open `x-apple.systempreferences:…Accessibility` URL.
    OpenSettings,
    /// Return the manual step-by-step instructions as a text response.
    ShowInstructions,
    /// Abort the current operation.
    Cancel,
}

/// The System Settings deep link for macOS Accessibility privacy pane.
pub const ACCESSIBILITY_SETTINGS_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";

/// Parse the user's response from a permissions-missing elicitation.
///
/// # Errors
///
/// - [`ElicitError::Cancelled`] when the user chose "Cancel" or dismissed.
/// - [`ElicitError::MissingField`] when the `action` field is absent.
pub fn parse_permissions_missing(resp: ElicitResponse) -> Result<PermissionAction, ElicitError> {
    let content = match resp.action {
        ElicitAction::Accept => resp
            .content
            .unwrap_or(Value::Object(serde_json::Map::default())),
        ElicitAction::Decline | ElicitAction::Cancel => return Err(ElicitError::Cancelled),
    };

    match content["action"].as_str() {
        Some("open_settings") => Ok(PermissionAction::OpenSettings),
        Some("show_instructions") => Ok(PermissionAction::ShowInstructions),
        Some("cancel") | None => Err(ElicitError::Cancelled),
        Some(other) => Err(ElicitError::MissingField(format!(
            "unknown action '{other}'"
        ))),
    }
}

/// Human-readable manual instructions for enabling macOS accessibility.
pub const MANUAL_ACCESSIBILITY_INSTRUCTIONS: &str = "\
To enable accessibility permissions:\n\
\n\
1. Open System Settings (Apple menu > System Settings)\n\
2. Navigate to Privacy & Security > Accessibility\n\
3. Click the '+' button\n\
4. Add your terminal application (Terminal, iTerm2, Alacritty, etc.)\n\
5. Enable the toggle next to your terminal\n\
6. Restart the terminal and retry\n\
\n\
Alternatively, run: open '";

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "elicitation_tests.rs"]
mod tests;
