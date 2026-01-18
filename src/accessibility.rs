//! macOS Accessibility API bindings
//!
//! Provides safe Rust wrappers around the macOS Accessibility APIs (AXUIElement).

use core_foundation::base::{CFTypeRef, TCFType};
use core_foundation::string::CFString;
use std::ffi::c_void;
use std::ptr;

use crate::error::{AXError, AXResult};

// External declarations for macOS Accessibility APIs
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFTypeRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXUIElementPerformAction(element: AXUIElementRef, action: CFTypeRef) -> i32;
    fn AXUIElementCopyAttributeNames(element: AXUIElementRef, names: *mut CFTypeRef) -> i32;
    fn AXUIElementGetPid(element: AXUIElementRef, pid: *mut i32) -> i32;
    fn CFRelease(cf: CFTypeRef);
}

/// Opaque reference to an accessibility element
pub type AXUIElementRef = *const c_void;

/// Error codes from AXUIElement functions
pub const AX_ERROR_SUCCESS: i32 = 0;
pub const AX_ERROR_FAILURE: i32 = -25200;
pub const AX_ERROR_ILLEGAL_ARGUMENT: i32 = -25201;
pub const AX_ERROR_INVALID_ELEMENT: i32 = -25202;
pub const AX_ERROR_INVALID_OBSERVER: i32 = -25203;
pub const AX_ERROR_CANNOT_COMPLETE: i32 = -25204;
pub const AX_ERROR_ATTRIBUTE_UNSUPPORTED: i32 = -25205;
pub const AX_ERROR_ACTION_UNSUPPORTED: i32 = -25206;
pub const AX_ERROR_NOT_IMPLEMENTED: i32 = -25207;
pub const AX_ERROR_NOTIFICATION_UNSUPPORTED: i32 = -25208;
pub const AX_ERROR_NOT_PERMITTED: i32 = -25209;
pub const AX_ERROR_API_DISABLED: i32 = -25210;
pub const AX_ERROR_NO_VALUE: i32 = -25211;
pub const AX_ERROR_PARAMETERIZED_ATTRIBUTE_UNSUPPORTED: i32 = -25212;
pub const AX_ERROR_NOT_ENOUGH_PRECISION: i32 = -25213;

/// Check if accessibility permissions are enabled
pub fn check_accessibility_enabled() -> bool {
    unsafe { AXIsProcessTrusted() }
}

/// Create a system-wide accessibility element
pub fn create_system_wide_element() -> AXResult<AXUIElementRef> {
    if !check_accessibility_enabled() {
        return Err(AXError::AccessibilityNotEnabled);
    }
    let element = unsafe { AXUIElementCreateSystemWide() };
    if element.is_null() {
        return Err(AXError::SystemError(
            "Failed to create system-wide element".into(),
        ));
    }
    Ok(element)
}

/// Create an accessibility element for an application
pub fn create_application_element(pid: i32) -> AXResult<AXUIElementRef> {
    if !check_accessibility_enabled() {
        return Err(AXError::AccessibilityNotEnabled);
    }
    let element = unsafe { AXUIElementCreateApplication(pid) };
    if element.is_null() {
        return Err(AXError::SystemError(format!(
            "Failed to create element for pid {}",
            pid
        )));
    }
    Ok(element)
}

/// Get an attribute value from an element
pub fn get_attribute(element: AXUIElementRef, attribute: &str) -> AXResult<CFTypeRef> {
    let attr = CFString::new(attribute);
    let mut value: CFTypeRef = ptr::null();

    let result =
        unsafe { AXUIElementCopyAttributeValue(element, attr.as_concrete_TypeRef(), &mut value) };

    if result != AX_ERROR_SUCCESS {
        return Err(ax_error_to_result(result, attribute));
    }

    Ok(value)
}

/// Perform an action on an element
///
/// This is the core function that enables BACKGROUND testing.
/// AXUIElementPerformAction works on unfocused windows!
pub fn perform_action(element: AXUIElementRef, action: &str) -> AXResult<()> {
    let action_str = CFString::new(action);

    let result = unsafe { AXUIElementPerformAction(element, action_str.as_concrete_TypeRef()) };

    if result != AX_ERROR_SUCCESS {
        return Err(ax_error_to_result(result, action));
    }

    Ok(())
}

/// Get the PID of the application owning an element
pub fn get_element_pid(element: AXUIElementRef) -> AXResult<i32> {
    let mut pid: i32 = 0;
    let result = unsafe { AXUIElementGetPid(element, &mut pid) };

    if result != AX_ERROR_SUCCESS {
        return Err(AXError::SystemError("Failed to get PID".into()));
    }

    Ok(pid)
}

/// Release a CFTypeRef
pub fn release_cf(cf: CFTypeRef) {
    if !cf.is_null() {
        unsafe { CFRelease(cf) };
    }
}

/// Convert AX error code to AXResult
fn ax_error_to_result(code: i32, context: &str) -> AXError {
    match code {
        AX_ERROR_FAILURE => AXError::ActionFailed(context.into()),
        AX_ERROR_ILLEGAL_ARGUMENT => {
            AXError::InvalidQuery(format!("Illegal argument: {}", context))
        }
        AX_ERROR_INVALID_ELEMENT => AXError::ElementNotFound(context.into()),
        AX_ERROR_CANNOT_COMPLETE => AXError::ActionFailed(format!("Cannot complete: {}", context)),
        AX_ERROR_ATTRIBUTE_UNSUPPORTED => {
            AXError::InvalidQuery(format!("Attribute unsupported: {}", context))
        }
        AX_ERROR_ACTION_UNSUPPORTED => {
            AXError::BackgroundNotSupported(format!("Action unsupported: {}", context))
        }
        AX_ERROR_NOT_PERMITTED => AXError::AccessibilityNotEnabled,
        AX_ERROR_API_DISABLED => AXError::AccessibilityNotEnabled,
        AX_ERROR_NO_VALUE => AXError::ElementNotFound(format!("No value for: {}", context)),
        _ => AXError::SystemError(format!("Unknown error {}: {}", code, context)),
    }
}

/// Standard accessibility attributes
pub mod attributes {
    pub const AX_ROLE: &str = "AXRole";
    pub const AX_TITLE: &str = "AXTitle";
    pub const AX_VALUE: &str = "AXValue";
    pub const AX_DESCRIPTION: &str = "AXDescription";
    pub const AX_CHILDREN: &str = "AXChildren";
    pub const AX_PARENT: &str = "AXParent";
    pub const AX_FOCUSED: &str = "AXFocused";
    pub const AX_ENABLED: &str = "AXEnabled";
    pub const AX_POSITION: &str = "AXPosition";
    pub const AX_SIZE: &str = "AXSize";
    pub const AX_IDENTIFIER: &str = "AXIdentifier";
    pub const AX_LABEL: &str = "AXLabel";
    pub const AX_WINDOWS: &str = "AXWindows";
    pub const AX_MAIN_WINDOW: &str = "AXMainWindow";
    pub const AX_FOCUSED_WINDOW: &str = "AXFocusedWindow";
}

/// Standard accessibility actions
pub mod actions {
    /// Press action - works in BACKGROUND!
    pub const AX_PRESS: &str = "AXPress";
    /// Pick action for selection - works in BACKGROUND!
    pub const AX_PICK: &str = "AXPick";
    /// Increment action - works in BACKGROUND!
    pub const AX_INCREMENT: &str = "AXIncrement";
    /// Decrement action - works in BACKGROUND!
    pub const AX_DECREMENT: &str = "AXDecrement";
    /// Show menu action - works in BACKGROUND!
    pub const AX_SHOW_MENU: &str = "AXShowMenu";
    /// Confirm action - works in BACKGROUND!
    pub const AX_CONFIRM: &str = "AXConfirm";
    /// Cancel action - works in BACKGROUND!
    pub const AX_CANCEL: &str = "AXCancel";
    /// Raise action - brings window to front (NOT background)
    pub const AX_RAISE: &str = "AXRaise";
}

/// Accessibility roles
pub mod roles {
    pub const AX_APPLICATION: &str = "AXApplication";
    pub const AX_WINDOW: &str = "AXWindow";
    pub const AX_BUTTON: &str = "AXButton";
    pub const AX_TEXT_FIELD: &str = "AXTextField";
    pub const AX_TEXT_AREA: &str = "AXTextArea";
    pub const AX_STATIC_TEXT: &str = "AXStaticText";
    pub const AX_MENU: &str = "AXMenu";
    pub const AX_MENU_ITEM: &str = "AXMenuItem";
    pub const AX_MENU_BAR: &str = "AXMenuBar";
    pub const AX_CHECKBOX: &str = "AXCheckBox";
    pub const AX_RADIO_BUTTON: &str = "AXRadioButton";
    pub const AX_SLIDER: &str = "AXSlider";
    pub const AX_TABLE: &str = "AXTable";
    pub const AX_LIST: &str = "AXList";
    pub const AX_WEB_AREA: &str = "AXWebArea";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_accessibility() {
        // This will return true if running with accessibility permissions
        let _ = check_accessibility_enabled();
    }
}
