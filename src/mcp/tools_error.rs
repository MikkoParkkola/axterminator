//! Shared error-response helpers for the MCP handler / UI layer.
//!
//! This module centralises the **exact** user-visible error strings that appear
//! verbatim in more than one handler so they cannot silently diverge.  Only
//! proven-repeated, stable contracts live here; one-off errors stay inline.

use crate::mcp::protocol::ToolCallResult;

// ---------------------------------------------------------------------------
// Element-lookup errors
// ---------------------------------------------------------------------------

/// `"Element not found: '{query}'"` — returned whenever an AX element lookup
/// yields no match.  Used by every handler that calls `app.find_native`.
pub(crate) fn element_not_found(query: &str) -> ToolCallResult {
    ToolCallResult::error(format!("Element not found: '{query}'"))
}

/// `"Element not found: '{query}' (semantic fallback also failed)"` — returned
/// by the semantic-find path when both the direct lookup *and* the bigram-based
/// fuzzy scan come up empty.
pub(crate) fn element_not_found_semantic_fallback(query: &str) -> ToolCallResult {
    ToolCallResult::error(format!(
        "Element not found: '{query}' (semantic fallback also failed)"
    ))
}

// ---------------------------------------------------------------------------
// Tests — lock the exact strings so a rename is a compile + test failure
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{element_not_found, element_not_found_semantic_fallback};

    fn error_text(r: crate::mcp::protocol::ToolCallResult) -> String {
        assert!(r.is_error, "expected an error result");
        assert_eq!(r.content.len(), 1);
        r.content[0].text.clone()
    }

    #[test]
    fn element_not_found_exact_string() {
        assert_eq!(
            error_text(element_not_found("Submit button")),
            "Element not found: 'Submit button'"
        );
    }

    #[test]
    fn element_not_found_semantic_fallback_exact_string() {
        assert_eq!(
            error_text(element_not_found_semantic_fallback("Close")),
            "Element not found: 'Close' (semantic fallback also failed)"
        );
    }

    #[test]
    fn element_not_found_empty_query() {
        assert_eq!(error_text(element_not_found("")), "Element not found: ''");
    }
}
