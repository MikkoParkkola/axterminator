//! MCP Phase 2 prompt handlers.
//!
//! Prompts are pre-built conversation starters that guide an AI agent through
//! common axterminator workflows. Each prompt accepts typed arguments and returns
//! a sequence of [`PromptMessage`] objects that establish the initial context
//! for a multi-turn interaction.
//!
//! ## Available prompts
//!
//! | Name | Purpose |
//! |------|---------|
//! | `test-app` | Guided testing workflow (connect → explore → assert) |
//! | `navigate-to` | Navigate to a specific screen or dialog |
//! | `extract-data` | Extract structured data from the app's UI |
//! | `accessibility-audit` | WCAG compliance audit |
//!
//! ## Adding a new prompt
//!
//! 1. Add a descriptor function returning a [`Prompt`] constant.
//! 2. Add it to the [`all_prompts`] list.
//! 3. Add a match arm in [`get_prompt`] that calls a message-builder function.
//! 4. Add tests for argument handling and message content.

use crate::mcp::protocol::{
    Prompt, PromptArgument, PromptContent, PromptGetParams, PromptGetResult, PromptListResult,
    PromptMessage, PromptRole,
};

// ---------------------------------------------------------------------------
// Prompt registry
// ---------------------------------------------------------------------------

/// All Phase 2 prompts in registration order.
///
/// # Examples
///
/// ```
/// let list = axterminator::mcp::prompts::all_prompts();
/// assert_eq!(list.prompts.len(), 4);
/// ```
#[must_use]
pub fn all_prompts() -> PromptListResult {
    PromptListResult {
        prompts: vec![
            prompt_test_app(),
            prompt_navigate_to(),
            prompt_extract_data(),
            prompt_accessibility_audit(),
        ],
    }
}

/// Resolve a prompt by name and fill in the provided arguments.
///
/// Returns `Ok(PromptGetResult)` on success or `Err(String)` when the name
/// is unknown or a required argument is missing.
///
/// # Errors
///
/// - `"Unknown prompt: {name}"` when `params.name` is not registered.
/// - `"Missing required argument: {arg}"` when a required argument is absent.
///
/// # Examples
///
/// ```
/// use serde_json::json;
/// use axterminator::mcp::prompts::get_prompt;
/// use axterminator::mcp::protocol::PromptGetParams;
///
/// let mut args = serde_json::Map::new();
/// args.insert("app_name".into(), json!("Safari"));
/// let params = PromptGetParams { name: "test-app".into(), arguments: Some(args) };
/// let result = get_prompt(&params).unwrap();
/// assert!(!result.messages.is_empty());
/// ```
pub fn get_prompt(params: &PromptGetParams) -> Result<PromptGetResult, String> {
    match params.name.as_str() {
        "test-app" => build_test_app(params),
        "navigate-to" => build_navigate_to(params),
        "extract-data" => build_extract_data(params),
        "accessibility-audit" => build_accessibility_audit(params),
        other => Err(format!("Unknown prompt: {other}")),
    }
}

// ---------------------------------------------------------------------------
// Prompt descriptors
// ---------------------------------------------------------------------------

fn prompt_test_app() -> Prompt {
    Prompt {
        name: "test-app",
        title: "Test a macOS Application",
        description: "Step-by-step guide to test a macOS application. \
            Connects, explores the UI, runs interactions, and reports findings.",
        arguments: vec![
            PromptArgument {
                name: "app_name",
                description: "Name of the app to test (e.g. Safari, Finder)",
                required: true,
            },
            PromptArgument {
                name: "focus_area",
                description: "Specific area to test (e.g. toolbar, sidebar). Omit to test all.",
                required: false,
            },
        ],
    }
}

fn prompt_navigate_to() -> Prompt {
    Prompt {
        name: "navigate-to",
        title: "Navigate to a Screen",
        description: "Navigate to a specific screen, dialog, or state within a macOS application.",
        arguments: vec![
            PromptArgument {
                name: "app_name",
                description: "Name of the app",
                required: true,
            },
            PromptArgument {
                name: "target_screen",
                description: "Where to navigate (e.g. Settings > General, File > New)",
                required: true,
            },
        ],
    }
}

fn prompt_extract_data() -> Prompt {
    Prompt {
        name: "extract-data",
        title: "Extract Data from Application",
        description: "Extract structured data from a running macOS application. \
            Reads element values, table contents, or form fields.",
        arguments: vec![
            PromptArgument {
                name: "app_name",
                description: "Name of the connected app",
                required: true,
            },
            PromptArgument {
                name: "data_description",
                description: "What data to extract (e.g. list of contacts, form fields)",
                required: true,
            },
        ],
    }
}

fn prompt_accessibility_audit() -> Prompt {
    Prompt {
        name: "accessibility-audit",
        title: "Accessibility Audit",
        description: "Audit a macOS application for accessibility issues: \
            missing labels, incorrect roles, keyboard navigation, and WCAG compliance.",
        arguments: vec![PromptArgument {
            name: "app_name",
            description: "Name of the app to audit",
            required: true,
        }],
    }
}

// ---------------------------------------------------------------------------
// Message builders
// ---------------------------------------------------------------------------

fn build_test_app(params: &PromptGetParams) -> Result<PromptGetResult, String> {
    let app = require_arg(params, "app_name")?;
    let focus_hint = optional_arg(params, "focus_area")
        .map(|f| format!(" Focus your testing on the {f} area."))
        .unwrap_or_default();

    let user_msg = format!(
        "Test the macOS application \"{app}\".{focus_hint}\n\
        Follow these steps:\n\
        1. Call ax_is_accessible to verify accessibility permissions are enabled.\n\
        2. Call ax_connect with app=\"{app}\" to connect.\n\
        3. Call ax_list_windows with app=\"{app}\" to see open windows.\n\
        4. Call ax_screenshot with app=\"{app}\" for visual context.\n\
        5. Find key interactive elements using ax_find and document what you discover.\n\
        6. Test each interactive element: click buttons, fill text fields, \
           verify expected state changes.\n\
        7. Report your findings: what works, what looks broken, what is confusing."
    );

    let assistant_msg = format!(
        "I will test {app} systematically. \
        Starting with accessibility verification, \
        then connecting and exploring the UI visually and through the element tree."
    );

    Ok(PromptGetResult {
        description: format!("Guided testing workflow for {app}"),
        messages: vec![user_message(user_msg), assistant_message(assistant_msg)],
    })
}

fn build_navigate_to(params: &PromptGetParams) -> Result<PromptGetResult, String> {
    let app = require_arg(params, "app_name")?;
    let target = require_arg(params, "target_screen")?;

    let user_msg = format!(
        "Navigate to \"{target}\" in {app}.\n\
        Steps:\n\
        1. Call ax_connect with app=\"{app}\" to connect (if not already connected).\n\
        2. Call ax_screenshot to see the current state.\n\
        3. Use ax_find to locate navigation elements (menu items, buttons, tabs).\n\
        4. Click the required sequence of elements to reach \"{target}\".\n\
        5. Take a final ax_screenshot to confirm you have arrived at the right screen."
    );

    let assistant_msg = format!(
        "I will navigate to \"{target}\" in {app} step by step, \
        verifying each step with a screenshot."
    );

    Ok(PromptGetResult {
        description: format!("Navigate to {target} in {app}"),
        messages: vec![user_message(user_msg), assistant_message(assistant_msg)],
    })
}

fn build_extract_data(params: &PromptGetParams) -> Result<PromptGetResult, String> {
    let app = require_arg(params, "app_name")?;
    let description = require_arg(params, "data_description")?;

    let user_msg = format!(
        "Extract \"{description}\" from {app}.\n\
        Steps:\n\
        1. Call ax_connect with app=\"{app}\" to connect (if not already connected).\n\
        2. Call ax_screenshot to see the current screen.\n\
        3. Use ax_find to locate the elements that contain the data.\n\
        4. Call ax_get_value for each relevant element to read its content.\n\
        5. Structure the extracted data as JSON and present it clearly."
    );

    let assistant_msg = format!(
        "I will extract \"{description}\" from {app} by reading the accessibility \
        element values and structuring the output as JSON."
    );

    Ok(PromptGetResult {
        description: format!("Extract {description} from {app}"),
        messages: vec![user_message(user_msg), assistant_message(assistant_msg)],
    })
}

fn build_accessibility_audit(params: &PromptGetParams) -> Result<PromptGetResult, String> {
    let app = require_arg(params, "app_name")?;

    let user_msg = format!(
        "Audit {app} for accessibility compliance.\n\
        Steps:\n\
        1. Call ax_connect with app=\"{app}\" to connect.\n\
        2. Call ax_screenshot for visual context.\n\
        3. Read axterminator://app/{app}/tree to inspect the full element hierarchy.\n\
        4. Check every interactive element for:\n\
           - A descriptive title or label (missing label = WCAG 1.3.1 failure)\n\
           - A correct semantic role (e.g. AXButton, AXTextField, not AXUnknown)\n\
           - Keyboard focus support (AXFocused attribute present)\n\
        5. Report issues by element with severity: critical / warning / info."
    );

    let assistant_msg = format!(
        "I will audit {app} for accessibility issues, checking labels, \
        roles, and keyboard navigation, and report findings by severity."
    );

    Ok(PromptGetResult {
        description: format!("Accessibility audit for {app}"),
        messages: vec![user_message(user_msg), assistant_message(assistant_msg)],
    })
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

/// Extract a required argument from prompt params.
///
/// # Errors
///
/// Returns `Err("Missing required argument: {name}")` when the argument is
/// absent or not a string.
fn require_arg<'a>(params: &'a PromptGetParams, name: &str) -> Result<&'a str, String> {
    params
        .arguments
        .as_ref()
        .and_then(|args| args.get(name))
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("Missing required argument: {name}"))
}

/// Extract an optional string argument, returning `None` when absent.
fn optional_arg<'a>(params: &'a PromptGetParams, name: &str) -> Option<&'a str> {
    params
        .arguments
        .as_ref()
        .and_then(|args| args.get(name))
        .and_then(|v| v.as_str())
}

// ---------------------------------------------------------------------------
// Message constructors
// ---------------------------------------------------------------------------

fn user_message(text: impl Into<String>) -> PromptMessage {
    PromptMessage {
        role: PromptRole::User,
        content: PromptContent::text(text),
    }
}

fn assistant_message(text: impl Into<String>) -> PromptMessage {
    PromptMessage {
        role: PromptRole::Assistant,
        content: PromptContent::text(text),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn args(pairs: &[(&str, &str)]) -> Option<serde_json::Map<String, serde_json::Value>> {
        let mut map = serde_json::Map::new();
        for (k, v) in pairs {
            map.insert((*k).into(), json!(*v));
        }
        Some(map)
    }

    fn params(name: &str, pairs: &[(&str, &str)]) -> PromptGetParams {
        PromptGetParams {
            name: name.into(),
            arguments: args(pairs),
        }
    }

    // -----------------------------------------------------------------------
    // all_prompts
    // -----------------------------------------------------------------------

    #[test]
    fn all_prompts_returns_four_prompts() {
        let list = all_prompts();
        assert_eq!(list.prompts.len(), 4);
    }

    #[test]
    fn all_prompts_names_are_unique() {
        let list = all_prompts();
        let names: std::collections::HashSet<&str> = list.prompts.iter().map(|p| p.name).collect();
        assert_eq!(names.len(), list.prompts.len());
    }

    #[test]
    fn all_prompts_serialise_without_panic() {
        let list = all_prompts();
        let json = serde_json::to_string(&list).unwrap();
        assert!(json.contains("test-app"));
        assert!(json.contains("accessibility-audit"));
    }

    // -----------------------------------------------------------------------
    // get_prompt dispatch
    // -----------------------------------------------------------------------

    #[test]
    fn unknown_prompt_returns_error() {
        let p = PromptGetParams {
            name: "does-not-exist".into(),
            arguments: None,
        };
        assert!(get_prompt(&p).is_err());
    }

    // -----------------------------------------------------------------------
    // test-app prompt
    // -----------------------------------------------------------------------

    #[test]
    fn test_app_with_valid_args_returns_two_messages() {
        // GIVEN: valid arguments for test-app
        let p = params("test-app", &[("app_name", "Safari")]);
        // WHEN: prompt resolved
        let result = get_prompt(&p).unwrap();
        // THEN: two messages (user + assistant)
        assert_eq!(result.messages.len(), 2);
    }

    #[test]
    fn test_app_user_message_contains_app_name() {
        let p = params("test-app", &[("app_name", "Finder")]);
        let result = get_prompt(&p).unwrap();
        assert!(result.messages[0].content.text.contains("Finder"));
    }

    #[test]
    fn test_app_with_focus_area_includes_it_in_message() {
        let p = params(
            "test-app",
            &[("app_name", "Safari"), ("focus_area", "toolbar")],
        );
        let result = get_prompt(&p).unwrap();
        assert!(result.messages[0].content.text.contains("toolbar"));
    }

    #[test]
    fn test_app_missing_app_name_returns_error() {
        let p = PromptGetParams {
            name: "test-app".into(),
            arguments: None,
        };
        let err = get_prompt(&p).unwrap_err();
        assert!(err.contains("app_name"));
    }

    // -----------------------------------------------------------------------
    // navigate-to prompt
    // -----------------------------------------------------------------------

    #[test]
    fn navigate_to_with_valid_args_returns_two_messages() {
        let p = params(
            "navigate-to",
            &[("app_name", "Safari"), ("target_screen", "Settings")],
        );
        let result = get_prompt(&p).unwrap();
        assert_eq!(result.messages.len(), 2);
    }

    #[test]
    fn navigate_to_user_message_contains_target() {
        let p = params(
            "navigate-to",
            &[
                ("app_name", "Finder"),
                ("target_screen", "File > New Folder"),
            ],
        );
        let result = get_prompt(&p).unwrap();
        assert!(result.messages[0]
            .content
            .text
            .contains("File > New Folder"));
    }

    #[test]
    fn navigate_to_missing_target_screen_returns_error() {
        let p = params("navigate-to", &[("app_name", "Safari")]);
        let err = get_prompt(&p).unwrap_err();
        assert!(err.contains("target_screen"));
    }

    // -----------------------------------------------------------------------
    // extract-data prompt
    // -----------------------------------------------------------------------

    #[test]
    fn extract_data_with_valid_args_returns_two_messages() {
        let p = params(
            "extract-data",
            &[
                ("app_name", "Contacts"),
                ("data_description", "all contact names"),
            ],
        );
        let result = get_prompt(&p).unwrap();
        assert_eq!(result.messages.len(), 2);
    }

    #[test]
    fn extract_data_description_contains_data_hint() {
        let p = params(
            "extract-data",
            &[("app_name", "Notes"), ("data_description", "note titles")],
        );
        let result = get_prompt(&p).unwrap();
        assert!(result.description.contains("note titles"));
    }

    #[test]
    fn extract_data_missing_data_description_returns_error() {
        let p = params("extract-data", &[("app_name", "Notes")]);
        assert!(get_prompt(&p).is_err());
    }

    // -----------------------------------------------------------------------
    // accessibility-audit prompt
    // -----------------------------------------------------------------------

    #[test]
    fn accessibility_audit_with_valid_app_returns_two_messages() {
        let p = params("accessibility-audit", &[("app_name", "Mail")]);
        let result = get_prompt(&p).unwrap();
        assert_eq!(result.messages.len(), 2);
    }

    #[test]
    fn accessibility_audit_user_message_mentions_wcag() {
        let p = params("accessibility-audit", &[("app_name", "Mail")]);
        let result = get_prompt(&p).unwrap();
        // The audit message should mention the WCAG criterion
        assert!(result.messages[0].content.text.contains("WCAG"));
    }

    #[test]
    fn accessibility_audit_mentions_resource_uri() {
        let p = params("accessibility-audit", &[("app_name", "Mail")]);
        let result = get_prompt(&p).unwrap();
        // Should reference the tree resource for the named app
        assert!(result.messages[0]
            .content
            .text
            .contains("axterminator://app/Mail/tree"));
    }

    #[test]
    fn accessibility_audit_missing_app_name_returns_error() {
        let p = PromptGetParams {
            name: "accessibility-audit".into(),
            arguments: None,
        };
        assert!(get_prompt(&p).is_err());
    }

    // -----------------------------------------------------------------------
    // PromptContent
    // -----------------------------------------------------------------------

    #[test]
    fn prompt_content_text_kind_is_text() {
        let c = PromptContent::text("hello");
        assert_eq!(c.kind, "text");
        assert_eq!(c.text, "hello");
    }

    #[test]
    fn prompt_message_serialises_role_lowercase() {
        let msg = user_message("hi");
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""role":"user""#));
    }
}
