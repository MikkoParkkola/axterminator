//! MCP elicitation — server-initiated user questions.
//!
//! Elicitation lets the server ask the user a question mid-operation. The
//! server sends an `elicitation/create` JSON-RPC request to the client; the
//! client renders a form (or URL prompt) and returns the user's answer.
//!
//! This module implements the four scenarios that have the highest user-facing
//! impact, matching the MCP 2025-11-05 `elicitation/create` wire format:
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
/// Matches the MCP 2025-11-05 wire format. The `requested_schema` field is a
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
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // ElicitResponse helpers
    // -----------------------------------------------------------------------

    #[test]
    fn into_accepted_returns_content_on_accept() {
        // GIVEN: accepted response with content
        let resp = ElicitResponse {
            action: ElicitAction::Accept,
            content: Some(json!({"answer": 42})),
        };
        // WHEN: into_accepted
        let v = resp.into_accepted().unwrap();
        // THEN: content returned
        assert_eq!(v["answer"], 42);
    }

    #[test]
    fn into_accepted_returns_empty_object_when_no_content() {
        let resp = ElicitResponse {
            action: ElicitAction::Accept,
            content: None,
        };
        assert_eq!(resp.into_accepted().unwrap(), json!({}));
    }

    #[test]
    fn into_accepted_returns_declined_error() {
        let resp = ElicitResponse {
            action: ElicitAction::Decline,
            content: None,
        };
        assert!(matches!(
            resp.into_accepted(),
            Err(ElicitError::Declined(_))
        ));
    }

    #[test]
    fn into_accepted_returns_cancelled_error() {
        let resp = ElicitResponse {
            action: ElicitAction::Cancel,
            content: None,
        };
        assert_eq!(resp.into_accepted(), Err(ElicitError::Cancelled));
    }

    // -----------------------------------------------------------------------
    // Scenario 1 — ambiguous app
    // -----------------------------------------------------------------------

    #[test]
    fn elicit_ambiguous_app_message_contains_query() {
        // GIVEN: two matching apps
        let req = elicit_ambiguous_app(
            "Chrome",
            &[
                ("Google Chrome".into(), "com.google.Chrome".into()),
                ("Chrome Canary".into(), "com.google.Chrome.canary".into()),
            ],
        );
        // THEN: message includes the query
        assert!(req.params.message.contains("Chrome"));
    }

    #[test]
    fn elicit_ambiguous_app_schema_has_correct_choices() {
        // GIVEN: two apps
        let req = elicit_ambiguous_app(
            "Mail",
            &[
                ("Mail (Apple)".into(), "com.apple.mail".into()),
                ("Mailspring".into(), "com.mailspring.Mailspring".into()),
            ],
        );
        // THEN: two oneOf entries
        let choices = req.params.requested_schema["properties"]["app"]["oneOf"]
            .as_array()
            .unwrap();
        assert_eq!(choices.len(), 2);
        assert_eq!(choices[0]["const"], "com.apple.mail");
        assert_eq!(choices[1]["title"], "Mailspring");
    }

    #[test]
    fn parse_ambiguous_app_extracts_bundle_id_on_accept() {
        // GIVEN: accepted response with app selection
        let resp = ElicitResponse {
            action: ElicitAction::Accept,
            content: Some(json!({"app": "com.google.Chrome"})),
        };
        // THEN: bundle ID returned
        assert_eq!(parse_ambiguous_app(resp).unwrap(), "com.google.Chrome");
    }

    #[test]
    fn parse_ambiguous_app_returns_missing_field_when_no_app_key() {
        let resp = ElicitResponse {
            action: ElicitAction::Accept,
            content: Some(json!({})),
        };
        assert_eq!(
            parse_ambiguous_app(resp),
            Err(ElicitError::MissingField("app".into()))
        );
    }

    #[test]
    fn parse_ambiguous_app_returns_cancelled_on_cancel() {
        let resp = ElicitResponse {
            action: ElicitAction::Cancel,
            content: None,
        };
        assert_eq!(parse_ambiguous_app(resp), Err(ElicitError::Cancelled));
    }

    // -----------------------------------------------------------------------
    // Scenario 2 — element not found
    // -----------------------------------------------------------------------

    #[test]
    fn elicit_element_not_found_message_mentions_query_and_app() {
        let req = elicit_element_not_found("Submit", "Safari", &["Submit Form", "Cancel"]);
        assert!(req.params.message.contains("Submit"));
        assert!(req.params.message.contains("Safari"));
    }

    #[test]
    fn elicit_element_not_found_schema_caps_candidates_at_three() {
        // GIVEN: five candidates
        let candidates = vec!["A", "B", "C", "D", "E"];
        let req = elicit_element_not_found("query", "App", &candidates);
        // THEN: schema has 3 + 1 ("custom") = 4 entries
        let choices = req.params.requested_schema["properties"]["choice"]["oneOf"]
            .as_array()
            .unwrap();
        assert_eq!(choices.len(), 4); // 3 candidates + custom
    }

    #[test]
    fn elicit_element_not_found_no_candidates_uses_description_schema() {
        let req = elicit_element_not_found("query", "App", &[] as &[&str]);
        assert!(req.params.requested_schema["properties"]["description"].is_object());
    }

    #[test]
    fn parse_element_not_found_returns_candidate() {
        let resp = ElicitResponse {
            action: ElicitAction::Accept,
            content: Some(json!({"choice": "Submit Form", "use_visual": false})),
        };
        assert_eq!(
            parse_element_not_found(resp).unwrap(),
            ElementChoice::Candidate("Submit Form".into())
        );
    }

    #[test]
    fn parse_element_not_found_returns_custom_when_custom_selected() {
        let resp = ElicitResponse {
            action: ElicitAction::Accept,
            content: Some(json!({"choice": "__custom__", "custom_query": "My Button"})),
        };
        assert_eq!(
            parse_element_not_found(resp).unwrap(),
            ElementChoice::Custom("My Button".into())
        );
    }

    #[test]
    fn parse_element_not_found_returns_visual_when_use_visual_true() {
        let resp = ElicitResponse {
            action: ElicitAction::Accept,
            content: Some(json!({"choice": "btn", "use_visual": true})),
        };
        assert_eq!(
            parse_element_not_found(resp).unwrap(),
            ElementChoice::UseVisual
        );
    }

    #[test]
    fn parse_element_not_found_no_candidates_custom_description() {
        let resp = ElicitResponse {
            action: ElicitAction::Accept,
            content: Some(json!({"description": "the red save button", "use_visual": false})),
        };
        assert_eq!(
            parse_element_not_found(resp).unwrap(),
            ElementChoice::Custom("the red save button".into())
        );
    }

    #[test]
    fn parse_element_not_found_no_candidates_use_visual() {
        let resp = ElicitResponse {
            action: ElicitAction::Accept,
            content: Some(json!({"use_visual": true})),
        };
        assert_eq!(
            parse_element_not_found(resp).unwrap(),
            ElementChoice::UseVisual
        );
    }

    // -----------------------------------------------------------------------
    // Scenario 3 — destructive action
    // -----------------------------------------------------------------------

    #[test]
    fn is_destructive_element_detects_delete() {
        assert!(is_destructive_element("Delete All Data"));
    }

    #[test]
    fn is_destructive_element_detects_format() {
        assert!(is_destructive_element("Format Drive"));
    }

    #[test]
    fn is_destructive_element_is_case_insensitive() {
        assert!(is_destructive_element("ERASE EVERYTHING"));
        assert!(is_destructive_element("Quit Application"));
    }

    #[test]
    fn is_destructive_element_false_for_safe_text() {
        assert!(!is_destructive_element("Save Document"));
        assert!(!is_destructive_element("Submit Form"));
        assert!(!is_destructive_element("Next"));
    }

    #[test]
    fn elicit_destructive_action_message_contains_element_and_app() {
        let req = elicit_destructive_action("Delete All", "Finder");
        assert!(req.params.message.contains("Delete All"));
        assert!(req.params.message.contains("Finder"));
    }

    #[test]
    fn elicit_destructive_action_schema_requires_confirm() {
        let req = elicit_destructive_action("Delete", "App");
        assert_eq!(req.params.requested_schema["required"], json!(["confirm"]));
    }

    #[test]
    fn parse_destructive_action_returns_ok_when_confirmed() {
        let resp = ElicitResponse {
            action: ElicitAction::Accept,
            content: Some(json!({"confirm": true})),
        };
        assert!(parse_destructive_action(resp).is_ok());
    }

    #[test]
    fn parse_destructive_action_returns_declined_when_false() {
        let resp = ElicitResponse {
            action: ElicitAction::Accept,
            content: Some(json!({"confirm": false})),
        };
        assert!(matches!(
            parse_destructive_action(resp),
            Err(ElicitError::Declined(_))
        ));
    }

    #[test]
    fn parse_destructive_action_returns_missing_field_when_no_confirm() {
        let resp = ElicitResponse {
            action: ElicitAction::Accept,
            content: Some(json!({})),
        };
        assert_eq!(
            parse_destructive_action(resp),
            Err(ElicitError::MissingField("confirm".into()))
        );
    }

    #[test]
    fn parse_destructive_action_returns_cancelled_on_cancel() {
        let resp = ElicitResponse {
            action: ElicitAction::Cancel,
            content: None,
        };
        assert_eq!(parse_destructive_action(resp), Err(ElicitError::Cancelled));
    }

    // -----------------------------------------------------------------------
    // Scenario 4 — permissions missing
    // -----------------------------------------------------------------------

    #[test]
    fn elicit_permissions_missing_message_mentions_accessibility() {
        let req = elicit_permissions_missing();
        // Message begins with "Accessibility permissions are not enabled…"
        assert!(
            req.params
                .message
                .to_ascii_lowercase()
                .contains("accessibility"),
            "message should mention accessibility: {}",
            req.params.message
        );
    }

    #[test]
    fn elicit_permissions_missing_schema_has_three_actions() {
        let req = elicit_permissions_missing();
        let choices = req.params.requested_schema["properties"]["action"]["oneOf"]
            .as_array()
            .unwrap();
        assert_eq!(choices.len(), 3);
    }

    #[test]
    fn parse_permissions_missing_open_settings() {
        let resp = ElicitResponse {
            action: ElicitAction::Accept,
            content: Some(json!({"action": "open_settings"})),
        };
        assert_eq!(
            parse_permissions_missing(resp).unwrap(),
            PermissionAction::OpenSettings
        );
    }

    #[test]
    fn parse_permissions_missing_show_instructions() {
        let resp = ElicitResponse {
            action: ElicitAction::Accept,
            content: Some(json!({"action": "show_instructions"})),
        };
        assert_eq!(
            parse_permissions_missing(resp).unwrap(),
            PermissionAction::ShowInstructions
        );
    }

    #[test]
    fn parse_permissions_missing_cancel_action() {
        let resp = ElicitResponse {
            action: ElicitAction::Accept,
            content: Some(json!({"action": "cancel"})),
        };
        assert_eq!(parse_permissions_missing(resp), Err(ElicitError::Cancelled));
    }

    #[test]
    fn parse_permissions_missing_dialog_cancel() {
        let resp = ElicitResponse {
            action: ElicitAction::Cancel,
            content: None,
        };
        assert_eq!(parse_permissions_missing(resp), Err(ElicitError::Cancelled));
    }

    #[test]
    fn parse_permissions_missing_unknown_action_returns_missing_field() {
        let resp = ElicitResponse {
            action: ElicitAction::Accept,
            content: Some(json!({"action": "fly_to_moon"})),
        };
        assert!(matches!(
            parse_permissions_missing(resp),
            Err(ElicitError::MissingField(_))
        ));
    }

    // -----------------------------------------------------------------------
    // ElicitRequest serialization round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn elicit_request_round_trips_via_json() {
        // GIVEN: a request
        let req = elicit_destructive_action("Delete", "App");
        // WHEN: serialized and deserialized
        let json = serde_json::to_string(&req).unwrap();
        let back: ElicitRequest = serde_json::from_str(&json).unwrap();
        // THEN: structurally identical
        assert_eq!(req, back);
    }

    #[test]
    fn elicit_response_deserializes_from_wire_format() {
        // GIVEN: wire JSON as a client would send
        let wire = r#"{"action":"accept","content":{"confirm":true}}"#;
        let resp: ElicitResponse = serde_json::from_str(wire).unwrap();
        assert_eq!(resp.action, ElicitAction::Accept);
        assert_eq!(resp.content.unwrap()["confirm"], true);
    }

    // -----------------------------------------------------------------------
    // Constants
    // -----------------------------------------------------------------------

    #[test]
    fn accessibility_settings_url_is_valid_apple_url() {
        assert!(ACCESSIBILITY_SETTINGS_URL.starts_with("x-apple.systempreferences:"));
    }
}
