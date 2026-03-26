//! Shared helpers for destructive-action safety gates in MCP tool handlers.

use crate::mcp::elicitation::is_destructive_element;
use crate::mcp::protocol::ToolCallResult;

/// Return `true` when the element title or description contains a destructive
/// keyword.
#[must_use]
pub(crate) fn is_destructive_target(title: Option<&str>, description: Option<&str>) -> bool {
    title.is_some_and(is_destructive_element) || description.is_some_and(is_destructive_element)
}

/// Return `true` when the element title or description contains a destructive
/// keyword.
#[must_use]
pub(crate) fn is_element_destructive(element: &crate::AXElement) -> bool {
    let title = element.title();
    let description = element.description();
    is_destructive_target(title.as_deref(), description.as_deref())
}

/// Block an unconfirmed destructive action while preserving the current
/// tool-facing error text semantics.
pub(crate) fn require_destructive_confirmation(
    query: &str,
    destructive: bool,
    confirmed: bool,
    tool_name: &str,
    action_label: &str,
) -> Result<(), ToolCallResult> {
    if destructive && !confirmed {
        return Err(ToolCallResult::error(format!(
            "Destructive action detected: {action_label} '{query}'. \
             Re-call {tool_name} with confirm=true to proceed."
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destructive_click_gate_preserves_exact_error_text() {
        let result = require_destructive_confirmation(
            "Delete All Files",
            true,
            false,
            "ax_click",
            "clicking",
        )
        .unwrap_err();

        assert!(result.is_error);
        assert_eq!(
            result.content[0].text,
            "Destructive action detected: clicking 'Delete All Files'. \
Re-call ax_click with confirm=true to proceed."
        );
    }

    #[test]
    fn destructive_click_gate_allows_non_destructive_without_confirmation() {
        let result =
            require_destructive_confirmation("Save Document", false, false, "ax_click", "clicking");

        assert!(result.is_ok());
    }

    #[test]
    fn destructive_click_gate_allows_confirmed_destructive_action() {
        let result = require_destructive_confirmation(
            "Delete All Files",
            true,
            true,
            "ax_click",
            "clicking",
        );

        assert!(result.is_ok());
    }

    #[test]
    fn is_destructive_target_checks_title_and_description() {
        assert!(is_destructive_target(Some("Delete All Files"), None));
        assert!(is_destructive_target(None, Some("Close Window")));
        assert!(!is_destructive_target(
            Some("Save"),
            Some("Open preferences")
        ));
    }
}
