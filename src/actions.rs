//! Action execution for `AXTerminator`
//!
//! Implements background and focus mode actions.

use crate::accessibility::{actions as ax_actions, perform_action, AXUIElementRef};
use crate::error::AXResult;
use crate::ActionMode;

/// Execute an action on an element
pub fn execute_action(element: AXUIElementRef, action: &str, mode: ActionMode) -> AXResult<()> {
    match mode {
        ActionMode::Background => {
            // WORLD FIRST: Perform action without stealing focus
            perform_action(element, action)
        }
        ActionMode::Focus => {
            // Bring to focus first (raises window)
            let _ = perform_action(element, ax_actions::AX_RAISE);
            // Then perform the action
            perform_action(element, action)
        }
    }
}

/// Check if an action can be performed in background mode
#[must_use]
pub fn can_perform_in_background(action: &str) -> bool {
    matches!(
        action,
        ax_actions::AX_PRESS
            | ax_actions::AX_PICK
            | ax_actions::AX_INCREMENT
            | ax_actions::AX_DECREMENT
            | ax_actions::AX_SHOW_MENU
            | ax_actions::AX_CONFIRM
            | ax_actions::AX_CANCEL
    )
}

/// Actions that require focus mode
#[must_use]
pub fn requires_focus(action: &str) -> bool {
    matches!(action, "type_text" | "drag" | "multi_touch")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_background_action_check() {
        assert!(can_perform_in_background(ax_actions::AX_PRESS));
        assert!(can_perform_in_background(ax_actions::AX_PICK));
        assert!(!can_perform_in_background(ax_actions::AX_RAISE));
    }

    #[test]
    fn test_focus_required_check() {
        assert!(requires_focus("type_text"));
        assert!(requires_focus("drag"));
        assert!(!requires_focus("click"));
    }
}
