//! macOS Accessibility API bindings
//!
//! Provides safe Rust wrappers around the macOS Accessibility APIs (`AXUIElement`).
//!
//! # Safety
//!
//! These functions wrap low-level macOS APIs that require raw pointers.
//! The callers must ensure the pointers are valid AXUIElement references.

#![allow(clippy::not_unsafe_ptr_arg_deref)]

use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFTypeRef, TCFType};
use core_foundation::boolean::{CFBoolean, CFBooleanRef};
use core_foundation::number::{CFNumber, CFNumberRef};
use core_foundation::string::{CFString, CFStringRef};
use std::ffi::c_void;
use std::ptr;

use crate::error::{AXError, AXResult};

// External declarations for macOS Accessibility APIs
#[link(name = "ApplicationServices", kind = "framework")]
#[allow(dead_code)]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFTypeRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXUIElementSetAttributeValue(
        element: AXUIElementRef,
        attribute: CFTypeRef,
        value: CFTypeRef,
    ) -> i32;
    fn AXUIElementPerformAction(element: AXUIElementRef, action: CFTypeRef) -> i32;
    fn AXUIElementCopyAttributeNames(element: AXUIElementRef, names: *mut CFTypeRef) -> i32;
    fn AXUIElementGetPid(element: AXUIElementRef, pid: *mut i32) -> i32;
    fn AXValueGetType(value: AXValueRef) -> AXValueType;
    fn AXValueGetValue(value: AXValueRef, value_type: AXValueType, value_ptr: *mut c_void) -> bool;
    fn AXValueCreate(value_type: AXValueType, value_ptr: *const c_void) -> AXValueRef;
    fn CFGetTypeID(cf: CFTypeRef) -> CFTypeID;
    fn CFStringGetTypeID() -> CFTypeID;
    fn CFBooleanGetTypeID() -> CFTypeID;
    fn CFNumberGetTypeID() -> CFTypeID;
    fn CFArrayGetTypeID() -> CFTypeID;
    fn CFRetain(cf: CFTypeRef) -> CFTypeRef;
    fn CFRelease(cf: CFTypeRef);
}

/// `CFTypeID` for type checking
pub type CFTypeID = usize;

/// Opaque reference to an `AXValue`
pub type AXValueRef = *const c_void;

/// Opaque reference to an accessibility element
pub type AXUIElementRef = *const c_void;

/// `AXValue` types
#[repr(i32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum AXValueType {
    Illegal = 0,
    CGPoint = 1,
    CGSize = 2,
    CGRect = 3,
    CFRange = 4,
    AXError = 5,
}

/// `CGPoint` structure for coordinates
#[repr(C)]
#[derive(Debug, Copy, Clone, Default)]
pub struct CGPoint {
    pub x: f64,
    pub y: f64,
}

/// `CGSize` structure for dimensions
#[repr(C)]
#[derive(Debug, Copy, Clone, Default)]
pub struct CGSize {
    pub width: f64,
    pub height: f64,
}

/// Error codes from `AXUIElement` functions
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
#[must_use]
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
            "Failed to create element for pid {pid}"
        )));
    }
    Ok(element)
}

/// Get an attribute value from an element
pub fn get_attribute(element: AXUIElementRef, attribute: &str) -> AXResult<CFTypeRef> {
    let attr = CFString::new(attribute);
    let mut value: CFTypeRef = ptr::null();

    let result = unsafe {
        AXUIElementCopyAttributeValue(
            element,
            attr.as_concrete_TypeRef() as CFTypeRef,
            &raw mut value,
        )
    };

    if result != AX_ERROR_SUCCESS {
        return Err(ax_error_to_result(result, attribute));
    }

    Ok(value)
}

/// Perform an action on an element
///
/// This is the core function that enables BACKGROUND testing.
/// `AXUIElementPerformAction` works on unfocused windows!
pub fn perform_action(element: AXUIElementRef, action: &str) -> AXResult<()> {
    let action_str = CFString::new(action);

    let result =
        unsafe { AXUIElementPerformAction(element, action_str.as_concrete_TypeRef() as CFTypeRef) };

    if result != AX_ERROR_SUCCESS {
        return Err(ax_error_to_result(result, action));
    }

    Ok(())
}

/// Get the PID of the application owning an element
pub fn get_element_pid(element: AXUIElementRef) -> AXResult<i32> {
    let mut pid: i32 = 0;
    let result = unsafe { AXUIElementGetPid(element, &raw mut pid) };

    if result != AX_ERROR_SUCCESS {
        return Err(AXError::SystemError("Failed to get PID".into()));
    }

    Ok(pid)
}

/// Release a `CFTypeRef`
pub fn release_cf(cf: CFTypeRef) {
    if !cf.is_null() {
        unsafe { CFRelease(cf) };
    }
}

/// Retain a `CFTypeRef` (increment reference count)
/// Returns the same pointer for convenience
#[must_use]
pub fn retain_cf(cf: CFTypeRef) -> CFTypeRef {
    if cf.is_null() {
        cf
    } else {
        unsafe { CFRetain(cf) }
    }
}

/// Get string attribute value from an element
#[must_use]
pub fn get_string_attribute_value(element: AXUIElementRef, attribute: &str) -> Option<String> {
    let value = get_attribute(element, attribute).ok()?;
    if value.is_null() {
        return None;
    }

    let type_id = unsafe { CFGetTypeID(value) };
    let string_type_id = unsafe { CFStringGetTypeID() };

    if type_id == string_type_id {
        let cf_string = unsafe { CFString::wrap_under_get_rule(value as CFStringRef) };
        let result = Some(cf_string.to_string());
        release_cf(value);
        result
    } else {
        release_cf(value);
        None
    }
}

/// Get boolean attribute value from an element
#[must_use]
pub fn get_bool_attribute_value(element: AXUIElementRef, attribute: &str) -> Option<bool> {
    let value = get_attribute(element, attribute).ok()?;
    if value.is_null() {
        return None;
    }

    let type_id = unsafe { CFGetTypeID(value) };
    let bool_type_id = unsafe { CFBooleanGetTypeID() };

    if type_id == bool_type_id {
        let cf_bool = unsafe { CFBoolean::wrap_under_get_rule(value as CFBooleanRef) };
        let result = Some(cf_bool.into());
        release_cf(value);
        result
    } else {
        release_cf(value);
        None
    }
}

/// Get number attribute value from an element
#[must_use]
pub fn get_number_attribute_value(element: AXUIElementRef, attribute: &str) -> Option<f64> {
    let value = get_attribute(element, attribute).ok()?;
    if value.is_null() {
        return None;
    }

    let type_id = unsafe { CFGetTypeID(value) };
    let number_type_id = unsafe { CFNumberGetTypeID() };

    if type_id == number_type_id {
        let cf_number = unsafe { CFNumber::wrap_under_get_rule(value as CFNumberRef) };
        let result = cf_number.to_f64();
        release_cf(value);
        result
    } else {
        release_cf(value);
        None
    }
}

/// Get position (`CGPoint`) from `AXValue`
#[must_use]
pub fn get_position_attribute(element: AXUIElementRef) -> Option<CGPoint> {
    let value = get_attribute(element, attributes::AX_POSITION).ok()?;
    if value.is_null() {
        return None;
    }

    let value_type = unsafe { AXValueGetType(value as AXValueRef) };
    if value_type != AXValueType::CGPoint {
        release_cf(value);
        return None;
    }

    let mut point = CGPoint::default();
    let success = unsafe {
        AXValueGetValue(
            value as AXValueRef,
            AXValueType::CGPoint,
            (&raw mut point).cast::<c_void>(),
        )
    };

    release_cf(value);

    if success {
        Some(point)
    } else {
        None
    }
}

/// Get size (`CGSize`) from `AXValue`
#[must_use]
pub fn get_size_attribute(element: AXUIElementRef) -> Option<CGSize> {
    let value = get_attribute(element, attributes::AX_SIZE).ok()?;
    if value.is_null() {
        return None;
    }

    let value_type = unsafe { AXValueGetType(value as AXValueRef) };
    if value_type != AXValueType::CGSize {
        release_cf(value);
        return None;
    }

    let mut size = CGSize::default();
    let success = unsafe {
        AXValueGetValue(
            value as AXValueRef,
            AXValueType::CGSize,
            (&raw mut size).cast::<c_void>(),
        )
    };

    release_cf(value);

    if success {
        Some(size)
    } else {
        None
    }
}

/// Set attribute value on an element
pub fn set_attribute_value(
    element: AXUIElementRef,
    attribute: &str,
    value: CFTypeRef,
) -> AXResult<()> {
    let attr = CFString::new(attribute);

    let result = unsafe {
        AXUIElementSetAttributeValue(element, attr.as_concrete_TypeRef() as CFTypeRef, value)
    };

    if result != AX_ERROR_SUCCESS {
        return Err(ax_error_to_result(result, attribute));
    }

    Ok(())
}

/// Set a string attribute value on an element
///
/// Convenience wrapper for setting text values.
/// Commonly used for `AXValue` attribute to set text field contents.
pub fn set_string_attribute_value(
    element: AXUIElementRef,
    attribute: &str,
    value: &str,
) -> AXResult<()> {
    let cf_string = CFString::new(value);
    set_attribute_value(
        element,
        attribute,
        cf_string.as_concrete_TypeRef() as CFTypeRef,
    )
}

/// Set a boolean attribute value on an element
///
/// Convenience wrapper for setting boolean values.
/// Commonly used for `AXFocused` attribute.
pub fn set_bool_attribute_value(
    element: AXUIElementRef,
    attribute: &str,
    value: bool,
) -> AXResult<()> {
    let cf_boolean = CFBoolean::from(value);
    set_attribute_value(
        element,
        attribute,
        cf_boolean.as_concrete_TypeRef() as CFTypeRef,
    )
}

/// Get a point attribute (generic version)
///
/// Unlike `get_position_attribute`, this accepts any attribute name.
#[must_use]
pub fn get_point_attribute(element: AXUIElementRef, attribute: &str) -> Option<CGPoint> {
    let value = get_attribute(element, attribute).ok()?;
    if value.is_null() {
        return None;
    }

    let value_type = unsafe { AXValueGetType(value as AXValueRef) };
    if value_type != AXValueType::CGPoint {
        release_cf(value);
        return None;
    }

    let mut point = CGPoint::default();
    let success = unsafe {
        AXValueGetValue(
            value as AXValueRef,
            AXValueType::CGPoint,
            (&raw mut point).cast::<c_void>(),
        )
    };

    release_cf(value);

    if success {
        Some(point)
    } else {
        None
    }
}

/// Get a size attribute (generic version)
///
/// Unlike `get_size_attribute`, this accepts any attribute name.
#[must_use]
pub fn get_size_attribute_generic(element: AXUIElementRef, attribute: &str) -> Option<CGSize> {
    let value = get_attribute(element, attribute).ok()?;
    if value.is_null() {
        return None;
    }

    let value_type = unsafe { AXValueGetType(value as AXValueRef) };
    if value_type != AXValueType::CGSize {
        release_cf(value);
        return None;
    }

    let mut size = CGSize::default();
    let success = unsafe {
        AXValueGetValue(
            value as AXValueRef,
            AXValueType::CGSize,
            (&raw mut size).cast::<c_void>(),
        )
    };

    release_cf(value);

    if success {
        Some(size)
    } else {
        None
    }
}

/// Get children of an element
pub fn get_children(element: AXUIElementRef) -> AXResult<Vec<AXUIElementRef>> {
    let value = get_attribute(element, attributes::AX_CHILDREN)?;
    if value.is_null() {
        return Ok(vec![]);
    }

    let type_id = unsafe { CFGetTypeID(value) };
    let array_type_id = unsafe { CFArrayGetTypeID() };

    if type_id != array_type_id {
        release_cf(value);
        return Ok(vec![]);
    }

    // Cast to CFArray of void pointers
    let cf_array = unsafe { CFArray::<*const c_void>::wrap_under_get_rule(value as CFArrayRef) };
    let mut children = Vec::new();

    for i in 0..cf_array.len() {
        if let Some(child_ref) = cf_array.get(i) {
            // Get the raw pointer value
            let child_ptr: AXUIElementRef = *child_ref;
            // Manually retain the element since we're storing it
            unsafe {
                CFRetain(child_ptr as CFTypeRef);
            }
            children.push(child_ptr);
        }
    }

    release_cf(value);
    Ok(children)
}

/// Convert AX error code to `AXResult`
fn ax_error_to_result(code: i32, context: &str) -> AXError {
    match code {
        AX_ERROR_FAILURE => AXError::ActionFailed(context.into()),
        AX_ERROR_ILLEGAL_ARGUMENT => AXError::InvalidQuery(format!("Illegal argument: {context}")),
        AX_ERROR_INVALID_ELEMENT => AXError::ElementNotFound(context.into()),
        AX_ERROR_CANNOT_COMPLETE => AXError::ActionFailed(format!("Cannot complete: {context}")),
        AX_ERROR_ATTRIBUTE_UNSUPPORTED => {
            AXError::InvalidQuery(format!("Attribute unsupported: {context}"))
        }
        AX_ERROR_ACTION_UNSUPPORTED => {
            AXError::BackgroundNotSupported(format!("Action unsupported: {context}"))
        }
        AX_ERROR_NOT_PERMITTED => AXError::AccessibilityNotEnabled,
        AX_ERROR_API_DISABLED => AXError::AccessibilityNotEnabled,
        AX_ERROR_NO_VALUE => AXError::ElementNotFound(format!("No value for: {context}")),
        _ => AXError::SystemError(format!("Unknown error {code}: {context}")),
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

    #[test]
    fn test_create_system_wide_element_requires_permissions() {
        // GIVEN: System may or may not have accessibility enabled
        // WHEN: Creating system-wide element
        let result = create_system_wide_element();

        // THEN: Either succeeds or fails with accessibility error
        match result {
            Ok(element) => {
                assert!(!element.is_null());
                release_cf(element as CFTypeRef);
            }
            Err(AXError::AccessibilityNotEnabled) => {
                // Expected if permissions not granted
            }
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[test]
    fn test_create_application_element_requires_valid_pid() {
        // GIVEN: Accessibility permissions (or test will skip)
        if !check_accessibility_enabled() {
            return;
        }

        // WHEN: Creating element for invalid PID
        let result = create_application_element(-1);

        // THEN: Should fail (invalid PID should not create element)
        // Note: macOS may still create element but it will be invalid
        match result {
            Ok(element) => {
                assert!(!element.is_null());
                release_cf(element as CFTypeRef);
            }
            Err(_) => {
                // Expected for invalid PID
            }
        }
    }

    #[test]
    fn test_get_string_attribute_value_null_safety() {
        // GIVEN: Null element
        let null_element: AXUIElementRef = ptr::null();

        // WHEN: Getting string attribute
        let result = get_string_attribute_value(null_element, attributes::AX_TITLE);

        // THEN: Should return None, not crash
        assert!(result.is_none());
    }

    #[test]
    fn test_get_bool_attribute_value_null_safety() {
        // GIVEN: Null element
        let null_element: AXUIElementRef = ptr::null();

        // WHEN: Getting bool attribute
        let result = get_bool_attribute_value(null_element, attributes::AX_FOCUSED);

        // THEN: Should return None, not crash
        assert!(result.is_none());
    }

    #[test]
    fn test_get_number_attribute_value_null_safety() {
        // GIVEN: Null element
        let null_element: AXUIElementRef = ptr::null();

        // WHEN: Getting number attribute
        let result = get_number_attribute_value(null_element, "AXValue");

        // THEN: Should return None, not crash
        assert!(result.is_none());
    }

    #[test]
    fn test_get_position_attribute_null_safety() {
        // GIVEN: Null element
        let null_element: AXUIElementRef = ptr::null();

        // WHEN: Getting position
        let result = get_position_attribute(null_element);

        // THEN: Should return None, not crash
        assert!(result.is_none());
    }

    #[test]
    fn test_get_size_attribute_null_safety() {
        // GIVEN: Null element
        let null_element: AXUIElementRef = ptr::null();

        // WHEN: Getting size
        let result = get_size_attribute(null_element);

        // THEN: Should return None, not crash
        assert!(result.is_none());
    }

    #[test]
    fn test_get_point_attribute_null_safety() {
        // GIVEN: Null element
        let null_element: AXUIElementRef = ptr::null();

        // WHEN: Getting generic point
        let result = get_point_attribute(null_element, attributes::AX_POSITION);

        // THEN: Should return None, not crash
        assert!(result.is_none());
    }

    #[test]
    fn test_get_size_attribute_generic_null_safety() {
        // GIVEN: Null element
        let null_element: AXUIElementRef = ptr::null();

        // WHEN: Getting generic size
        let result = get_size_attribute_generic(null_element, attributes::AX_SIZE);

        // THEN: Should return None, not crash
        assert!(result.is_none());
    }

    #[test]
    fn test_get_children_null_safety() {
        // GIVEN: Null element
        let null_element: AXUIElementRef = ptr::null();

        // WHEN: Getting children
        let result = get_children(null_element);

        // THEN: Should fail gracefully
        assert!(result.is_err());
    }

    #[test]
    fn test_set_string_attribute_value_null_safety() {
        // GIVEN: Null element
        let null_element: AXUIElementRef = ptr::null();

        // WHEN: Setting string attribute
        let result = set_string_attribute_value(null_element, attributes::AX_VALUE, "test");

        // THEN: Should fail gracefully, not crash
        assert!(result.is_err());
    }

    #[test]
    fn test_set_bool_attribute_value_null_safety() {
        // GIVEN: Null element
        let null_element: AXUIElementRef = ptr::null();

        // WHEN: Setting bool attribute
        let result = set_bool_attribute_value(null_element, attributes::AX_FOCUSED, true);

        // THEN: Should fail gracefully, not crash
        assert!(result.is_err());
    }

    #[test]
    fn test_perform_action_null_safety() {
        // GIVEN: Null element
        let null_element: AXUIElementRef = ptr::null();

        // WHEN: Performing action
        let result = perform_action(null_element, actions::AX_PRESS);

        // THEN: Should fail gracefully, not crash
        assert!(result.is_err());
    }

    #[test]
    fn test_get_element_pid_null_safety() {
        // GIVEN: Null element
        let null_element: AXUIElementRef = ptr::null();

        // WHEN: Getting PID
        let result = get_element_pid(null_element);

        // THEN: Should fail gracefully, not crash
        assert!(result.is_err());
    }

    #[test]
    fn test_cgpoint_default() {
        // GIVEN: Default CGPoint
        let point = CGPoint::default();

        // THEN: Should be (0, 0)
        assert_eq!(point.x, 0.0);
        assert_eq!(point.y, 0.0);
    }

    #[test]
    fn test_cgsize_default() {
        // GIVEN: Default CGSize
        let size = CGSize::default();

        // THEN: Should be (0, 0)
        assert_eq!(size.width, 0.0);
        assert_eq!(size.height, 0.0);
    }

    #[test]
    fn test_ax_value_type_equality() {
        // GIVEN: AXValueType variants
        // THEN: Equality checks should work
        assert_eq!(AXValueType::CGPoint, AXValueType::CGPoint);
        assert_eq!(AXValueType::CGSize, AXValueType::CGSize);
        assert_ne!(AXValueType::CGPoint, AXValueType::CGSize);
    }

    #[test]
    fn test_error_code_conversion_failure() {
        // GIVEN: AX_ERROR_FAILURE code
        // WHEN: Converting to error
        let error = ax_error_to_result(AX_ERROR_FAILURE, "test");

        // THEN: Should be ActionFailed
        match error {
            AXError::ActionFailed(msg) => assert_eq!(msg, "test"),
            _ => panic!("Expected ActionFailed"),
        }
    }

    #[test]
    fn test_error_code_conversion_invalid_element() {
        // GIVEN: AX_ERROR_INVALID_ELEMENT code
        // WHEN: Converting to error
        let error = ax_error_to_result(AX_ERROR_INVALID_ELEMENT, "test");

        // THEN: Should be ElementNotFound
        match error {
            AXError::ElementNotFound(msg) => assert_eq!(msg, "test"),
            _ => panic!("Expected ElementNotFound"),
        }
    }

    #[test]
    fn test_error_code_conversion_not_permitted() {
        // GIVEN: AX_ERROR_NOT_PERMITTED code
        // WHEN: Converting to error
        let error = ax_error_to_result(AX_ERROR_NOT_PERMITTED, "test");

        // THEN: Should be AccessibilityNotEnabled
        match error {
            AXError::AccessibilityNotEnabled => {}
            _ => panic!("Expected AccessibilityNotEnabled"),
        }
    }

    #[test]
    fn test_error_code_conversion_attribute_unsupported() {
        // GIVEN: AX_ERROR_ATTRIBUTE_UNSUPPORTED code
        // WHEN: Converting to error
        let error = ax_error_to_result(AX_ERROR_ATTRIBUTE_UNSUPPORTED, "AXFoo");

        // THEN: Should be InvalidQuery
        match error {
            AXError::InvalidQuery(msg) => assert!(msg.contains("AXFoo")),
            _ => panic!("Expected InvalidQuery"),
        }
    }

    #[test]
    fn test_release_cf_null_safety() {
        // GIVEN: Null CFTypeRef
        let null_ref: CFTypeRef = ptr::null();

        // WHEN: Releasing null reference
        release_cf(null_ref);

        // THEN: Should not crash (function handles null check)
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_system_wide_element_integration() {
        // GIVEN: Accessibility enabled (skip if not)
        if !check_accessibility_enabled() {
            eprintln!("Skipping integration test: accessibility not enabled");
            return;
        }

        // WHEN: Creating system-wide element
        let element = create_system_wide_element().expect("Failed to create system element");

        // THEN: Element should be valid
        assert!(!element.is_null());

        // WHEN: Getting role attribute
        let role = get_string_attribute_value(element, attributes::AX_ROLE);

        // THEN: Should return a role (likely "AXSystemWide")
        if let Some(role_str) = role {
            assert!(!role_str.is_empty());
        }

        // Cleanup
        release_cf(element as CFTypeRef);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_get_children_integration() {
        // GIVEN: Accessibility enabled (skip if not)
        if !check_accessibility_enabled() {
            eprintln!("Skipping integration test: accessibility not enabled");
            return;
        }

        // WHEN: Creating system-wide element
        let element = create_system_wide_element().expect("Failed to create system element");

        // WHEN: Getting children
        let children_result = get_children(element);

        // THEN: Should succeed (may be empty)
        match children_result {
            Ok(children) => {
                // Children vector may be empty or contain elements
                for child in &children {
                    assert!(!child.is_null());
                }
                // Cleanup children
                for child in children {
                    release_cf(child as CFTypeRef);
                }
            }
            Err(e) => {
                // Some elements don't have children, which is ok
                println!(
                    "Note: get_children returned error (may be expected): {:?}",
                    e
                );
            }
        }

        // Cleanup
        release_cf(element as CFTypeRef);
    }

    #[test]
    fn test_memory_safety_double_release() {
        // GIVEN: Accessibility enabled (skip if not)
        if !check_accessibility_enabled() {
            return;
        }

        // WHEN: Creating and releasing element multiple times
        let element = match create_system_wide_element() {
            Ok(e) => e,
            Err(_) => return,
        };

        // THEN: Single release should be safe
        release_cf(element as CFTypeRef);

        // NOTE: Double release would be unsafe, so we don't test it
        // This test documents expected usage pattern
    }

    #[test]
    fn test_attribute_constants_exist() {
        // GIVEN: Attribute constants
        // THEN: Should all be non-empty strings
        assert!(!attributes::AX_ROLE.is_empty());
        assert!(!attributes::AX_TITLE.is_empty());
        assert!(!attributes::AX_VALUE.is_empty());
        assert!(!attributes::AX_CHILDREN.is_empty());
        assert!(!attributes::AX_POSITION.is_empty());
        assert!(!attributes::AX_SIZE.is_empty());
        assert!(!attributes::AX_FOCUSED.is_empty());
        assert!(!attributes::AX_ENABLED.is_empty());
    }

    #[test]
    fn test_action_constants_exist() {
        // GIVEN: Action constants
        // THEN: Should all be non-empty strings
        assert!(!actions::AX_PRESS.is_empty());
        assert!(!actions::AX_PICK.is_empty());
        assert!(!actions::AX_INCREMENT.is_empty());
        assert!(!actions::AX_DECREMENT.is_empty());
        assert!(!actions::AX_SHOW_MENU.is_empty());
    }

    #[test]
    fn test_role_constants_exist() {
        // GIVEN: Role constants
        // THEN: Should all be non-empty strings
        assert!(!roles::AX_APPLICATION.is_empty());
        assert!(!roles::AX_WINDOW.is_empty());
        assert!(!roles::AX_BUTTON.is_empty());
        assert!(!roles::AX_TEXT_FIELD.is_empty());
        assert!(!roles::AX_MENU.is_empty());
    }
}
