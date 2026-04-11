//! Canonical tool annotations for every Phase 1 and Phase 2 tool.
//!
//! Each constant captures the semantic hints defined in MCP 2025-11-05 §6.3.
//! Centralising them here ensures the CLI help text and MCP `tools/list` response
//! stay consistent with the design document.
//!
//! ## Annotation semantics
//!
//! | Constant | `readOnly` | `destructive` | `idempotent` | `openWorld` |
//! |----------|-----------|--------------|-------------|------------|
//! | [`READ_ONLY`] | ✓ | — | ✓ | — |
//! | [`CONNECT`] | — | — | ✓ | — |
//! | [`ACTION`] | — | — | — | — |
//! | [`DESTRUCTIVE`] | — | ✓ | — | — |
//! | [`OPEN_WORLD`] | — | ✓ | — | ✓ |

use crate::mcp::protocol::ToolAnnotations;

/// Read-only, idempotent, bounded — safe to call any number of times.
///
/// Use for: `ax_find`, `ax_get_value`, `ax_screenshot`, `ax_list_windows`,
/// `ax_get_tree`, `ax_is_accessible`, `ax_list_apps`.
pub const READ_ONLY: ToolAnnotations = ToolAnnotations {
    read_only: true,
    destructive: false,
    idempotent: true,
    open_world: false,
};

/// State-changing but idempotent — connecting twice is safe.
///
/// Use for: `ax_connect`.
pub const CONNECT: ToolAnnotations = ToolAnnotations {
    read_only: false,
    destructive: false,
    idempotent: true,
    open_world: false,
};

/// State-changing, not idempotent — clicking twice may click twice.
///
/// Use for: `ax_click`, `ax_click_at`, `ax_scroll`, `ax_wait_idle`.
pub const ACTION: ToolAnnotations = ToolAnnotations {
    read_only: false,
    destructive: false,
    idempotent: false,
    open_world: false,
};

/// Destructive action (e.g., typing overwrites existing text).
///
/// Use for: `ax_type`, `ax_set_value`.
pub const DESTRUCTIVE: ToolAnnotations = ToolAnnotations {
    read_only: false,
    destructive: true,
    idempotent: false,
    open_world: false,
};

/// Open-world action that interacts with external services (e.g. a VLM).
///
/// Use for: `ax_find_visual` — reads the screen and may call an external AI API.
/// The `openWorldHint` signals to the MCP client that results are non-deterministic
/// and may have network or cost implications.
pub const OPEN_WORLD: ToolAnnotations = ToolAnnotations {
    read_only: true,
    destructive: false,
    idempotent: true,
    open_world: true,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn read_only_annotations_are_safe() {
        assert!(READ_ONLY.read_only);
        assert!(!READ_ONLY.destructive);
        assert!(READ_ONLY.idempotent);
        assert!(!READ_ONLY.open_world);
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn connect_annotations_are_not_read_only() {
        assert!(!CONNECT.read_only);
        assert!(CONNECT.idempotent);
        assert!(!CONNECT.destructive);
        assert!(!CONNECT.open_world);
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn action_is_not_idempotent() {
        assert!(!ACTION.idempotent);
        assert!(!ACTION.read_only);
        assert!(!ACTION.open_world);
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn destructive_annotations_flag_correctly() {
        assert!(DESTRUCTIVE.destructive);
        assert!(!DESTRUCTIVE.idempotent);
        assert!(!DESTRUCTIVE.open_world);
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn open_world_sets_open_world_flag() {
        // GIVEN: OPEN_WORLD constant
        // WHEN: inspected
        // THEN: open_world is true, read_only is true (visual find is non-mutating)
        assert!(OPEN_WORLD.open_world);
        assert!(OPEN_WORLD.read_only);
        assert!(!OPEN_WORLD.destructive);
    }
}
