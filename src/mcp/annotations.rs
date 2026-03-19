//! Canonical tool annotations for every Phase 1 tool.
//!
//! Each constant captures the semantic hints defined in MCP 2025-11-05 §6.3.
//! Centralising them here ensures the CLI help text and MCP `tools/list` response
//! stay consistent with the design document.

use crate::mcp::protocol::ToolAnnotations;

/// Read-only, idempotent, bounded — safe to call any number of times.
pub const READ_ONLY: ToolAnnotations = ToolAnnotations {
    read_only: true,
    destructive: false,
    idempotent: true,
    open_world: false,
};

/// State-changing but idempotent — connecting twice is safe.
pub const CONNECT: ToolAnnotations = ToolAnnotations {
    read_only: false,
    destructive: false,
    idempotent: true,
    open_world: false,
};

/// State-changing, not idempotent — clicking twice may click twice.
pub const ACTION: ToolAnnotations = ToolAnnotations {
    read_only: false,
    destructive: false,
    idempotent: false,
    open_world: false,
};

/// Destructive action (e.g., typing overwrites existing text).
pub const DESTRUCTIVE: ToolAnnotations = ToolAnnotations {
    read_only: false,
    destructive: true,
    idempotent: false,
    open_world: false,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_annotations_are_safe() {
        assert!(READ_ONLY.read_only);
        assert!(!READ_ONLY.destructive);
        assert!(READ_ONLY.idempotent);
    }

    #[test]
    fn connect_annotations_are_not_read_only() {
        assert!(!CONNECT.read_only);
        assert!(CONNECT.idempotent);
        assert!(!CONNECT.destructive);
    }

    #[test]
    fn action_is_not_idempotent() {
        assert!(!ACTION.idempotent);
        assert!(!ACTION.read_only);
    }

    #[test]
    fn destructive_annotations_flag_correctly() {
        assert!(DESTRUCTIVE.destructive);
        assert!(!DESTRUCTIVE.idempotent);
    }
}
