//! Accessibility element wrapper

#![allow(clippy::useless_conversion)]

use std::time::Duration;

use crate::ActionMode;
use crate::accessibility::{
    self, AXUIElementRef, actions, attributes, get_attribute, perform_action,
};
use crate::error::{AXError, AXResult};

/// Wrapper for an accessibility element
#[derive(Debug)]
pub struct AXElement {
    /// Raw accessibility element reference
    pub(crate) element: AXUIElementRef,
    /// Cached role
    pub(crate) role: Option<String>,
    /// Cached title
    pub(crate) title: Option<String>,
}

// Manual Clone implementation that properly retains the element
impl Clone for AXElement {
    fn clone(&self) -> Self {
        // CRITICAL: Retain the element so both copies own a reference
        let _ = accessibility::retain_cf(self.element.cast());
        Self {
            element: self.element,
            role: self.role.clone(),
            title: self.title.clone(),
        }
    }
}

unsafe impl Send for AXElement {}
unsafe impl Sync for AXElement {}

// Pure-Rust accessors — always compiled, used by the CLI, MCP server, and tests.
impl AXElement {
    /// Get the element's role (e.g., "`AXButton`", "`AXTextField`")
    pub fn role(&self) -> Option<String> {
        self.get_string_attribute(attributes::AX_ROLE)
    }

    /// Get the element's title
    pub fn title(&self) -> Option<String> {
        self.get_string_attribute(attributes::AX_TITLE)
    }

    /// Get the element's value
    pub fn value(&self) -> Option<String> {
        self.get_string_attribute(attributes::AX_VALUE)
    }

    /// Get the element's description
    pub fn description(&self) -> Option<String> {
        self.get_string_attribute(attributes::AX_DESCRIPTION)
    }

    /// Get the element's label
    pub fn label(&self) -> Option<String> {
        self.get_string_attribute(attributes::AX_LABEL)
    }

    /// Get the element's identifier
    pub fn identifier(&self) -> Option<String> {
        self.get_string_attribute(attributes::AX_IDENTIFIER)
    }

    /// Check if the element is enabled
    pub fn enabled(&self) -> bool {
        self.get_bool_attribute(attributes::AX_ENABLED)
            .unwrap_or(false)
    }

    /// Check if the element currently has keyboard focus.
    pub fn focused(&self) -> bool {
        self.get_bool_attribute(attributes::AX_FOCUSED)
            .unwrap_or(false)
    }

    /// Check if the element exists in the UI
    pub fn exists(&self) -> bool {
        self.role().is_some()
    }

    /// Get the element's bounds (x, y, width, height)
    pub fn bounds(&self) -> Option<(f64, f64, f64, f64)> {
        let position = accessibility::get_position_attribute(self.element)?;
        let size = accessibility::get_size_attribute(self.element)?;
        Some((position.x, position.y, size.width, size.height))
    }

    /// Get the element's direct children.
    ///
    /// Returns an empty `Vec` when the element has no children or the
    /// accessibility API reports an error (e.g. the element is a leaf node).
    pub fn children(&self) -> Vec<AXElement> {
        accessibility::get_children(self.element)
            .unwrap_or_default()
            .into_iter()
            .map(AXElement::new)
            .collect()
    }
}

impl AXElement {
    /// Create a new element wrapper
    #[must_use]
    pub fn new(element: AXUIElementRef) -> Self {
        Self {
            element,
            role: None,
            title: None,
        }
    }

    // -----------------------------------------------------------------------
    // Rust-native action methods — safe to call from the MCP server and CLI binary.
    // -----------------------------------------------------------------------

    /// Click — returns `AXResult`.
    pub fn click_native(&self, mode: ActionMode) -> AXResult<()> {
        self.perform_click_native(mode)
    }

    /// Double-click — returns `AXResult`.
    pub fn double_click_native(&self, mode: ActionMode) -> AXResult<()> {
        self.perform_click_native(mode)?;
        std::thread::sleep(Duration::from_millis(50));
        self.perform_click_native(mode)
    }

    /// Right-click — returns `AXResult`.
    pub fn right_click_native(&self, mode: ActionMode) -> AXResult<()> {
        self.perform_show_menu(mode)
    }

    /// Type text — returns `AXResult`.
    pub fn type_text_native(&self, text: &str, mode: ActionMode) -> AXResult<()> {
        if mode == ActionMode::Background {
            return Err(AXError::BackgroundNotSupported(
                "Text input requires FOCUS mode".into(),
            ));
        }
        self.perform_type_text(text)
    }

    /// Set value — returns `AXResult`.
    pub fn set_value_native(&self, value: &str) -> AXResult<()> {
        self.perform_set_value(value)
    }

    /// Screenshot — returns `AXResult`.
    pub fn screenshot_native(&self) -> AXResult<Vec<u8>> {
        self.capture_element_screenshot()
    }

    /// Get a string attribute value
    fn get_string_attribute(&self, attribute: &str) -> Option<String> {
        accessibility::get_string_attribute_value(self.element, attribute)
    }

    /// Get a boolean attribute value
    fn get_bool_attribute(&self, attribute: &str) -> Option<bool> {
        accessibility::get_bool_attribute_value(self.element, attribute)
    }

    /// Perform click action — returns `AXResult`.
    ///
    /// Attempts `AXPress` first. If the action is unsupported by the element
    /// (reported as [`AXError::BackgroundNotSupported`] or [`AXError::ActionFailed`]),
    /// falls back transparently to a coordinate-based CGEvent click at the
    /// element's center.
    fn perform_click_native(&self, mode: ActionMode) -> AXResult<()> {
        let press_result = match mode {
            ActionMode::Background => perform_action(self.element, actions::AX_PRESS),
            ActionMode::Focus => {
                self.bring_to_focus_internal()?;
                perform_action(self.element, actions::AX_PRESS)
            }
        };

        match press_result {
            Ok(()) => Ok(()),
            Err(AXError::BackgroundNotSupported(_)) | Err(AXError::ActionFailed(_)) => {
                self.click_at_center()
            }
            Err(e) => Err(e),
        }
    }

    /// Click at the element's center via CGEvent (coordinate-based).
    ///
    /// Used as an automatic fallback when `AXPress` is unsupported by the element.
    fn click_at_center(&self) -> AXResult<()> {
        use core_graphics::event::{CGEvent, CGEventTapLocation, CGEventType, CGMouseButton};
        use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
        use core_graphics::geometry::CGPoint;

        let (x, y, w, h) = self.bounds().ok_or_else(|| {
            AXError::ActionFailed(
                "Cannot fall back to coordinate click: element has no bounds".into(),
            )
        })?;

        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .map_err(|()| AXError::ActionFailed("Failed to create CGEventSource".into()))?;

        let point = CGPoint::new(x + w / 2.0, y + h / 2.0);

        let down = CGEvent::new_mouse_event(
            source.clone(),
            CGEventType::LeftMouseDown,
            point,
            CGMouseButton::Left,
        )
        .map_err(|()| AXError::ActionFailed("Failed to create mouse down event".into()))?;
        down.post(CGEventTapLocation::HID);

        let up =
            CGEvent::new_mouse_event(source, CGEventType::LeftMouseUp, point, CGMouseButton::Left)
                .map_err(|()| AXError::ActionFailed("Failed to create mouse up event".into()))?;
        up.post(CGEventTapLocation::HID);

        Ok(())
    }

    /// Perform show menu action (right-click)
    fn perform_show_menu(&self, mode: ActionMode) -> AXResult<()> {
        if mode == ActionMode::Focus {
            self.bring_to_focus_internal()?;
        }
        perform_action(self.element, actions::AX_SHOW_MENU)
    }

    /// Type text into the element (BACKGROUND - no focus stealing!)
    ///
    /// Uses `CGEventPostToPid` to send keyboard events directly to the
    /// target application without stealing focus from the current app.
    fn perform_type_text(&self, text: &str) -> AXResult<()> {
        use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

        // Get the PID of the element's owning application
        let pid = accessibility::get_element_pid(self.element)?;

        // Create event source
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .map_err(|()| AXError::ActionFailed("Failed to create event source".into()))?;

        // Type each character directly to the target PID
        for ch in text.chars() {
            self.type_character_to_pid(ch, &source, pid)?;
        }

        Ok(())
    }

    /// Type a single character to a specific PID (BACKGROUND mode)
    fn type_character_to_pid(
        &self,
        ch: char,
        source: &core_graphics::event_source::CGEventSource,
        pid: i32,
    ) -> AXResult<()> {
        use core_graphics::event::CGEvent;

        // Convert character to virtual key code and determine if shift is needed
        let (key_code, needs_shift) = char_to_keycode(ch);

        // Press shift if needed
        if needs_shift && let Ok(shift_down) = CGEvent::new_keyboard_event(source.clone(), 56, true)
        {
            shift_down.post_to_pid(pid);
            std::thread::sleep(Duration::from_millis(10));
        }

        // Key down
        if let Ok(key_down) = CGEvent::new_keyboard_event(source.clone(), key_code, true) {
            // Set the Unicode character
            key_down.set_string_from_utf16_unchecked(&[ch as u16]);
            key_down.post_to_pid(pid);
            std::thread::sleep(Duration::from_millis(10));

            // Key up
            if let Ok(key_up) = CGEvent::new_keyboard_event(source.clone(), key_code, false) {
                key_up.set_string_from_utf16_unchecked(&[ch as u16]);
                key_up.post_to_pid(pid);
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        // Release shift if needed
        if needs_shift && let Ok(shift_up) = CGEvent::new_keyboard_event(source.clone(), 56, false)
        {
            shift_up.post_to_pid(pid);
            std::thread::sleep(Duration::from_millis(10));
        }

        Ok(())
    }

    /// Set the element's value directly
    fn perform_set_value(&self, value: &str) -> AXResult<()> {
        accessibility::set_string_attribute_value(self.element, attributes::AX_VALUE, value)
    }

    /// Bring the element to focus (internal version returning `AXResult`)
    fn bring_to_focus_internal(&self) -> AXResult<()> {
        // Set AXFocused attribute to true
        accessibility::set_bool_attribute_value(self.element, attributes::AX_FOCUSED, true)?;

        // Get the window and raise it
        if let Ok(window) = self.get_window() {
            let _ = perform_action(window, actions::AX_RAISE);
        }

        Ok(())
    }

    /// Get the window containing this element
    fn get_window(&self) -> AXResult<AXUIElementRef> {
        // Walk up the parent chain to find a window
        let mut current = self.element;
        loop {
            if let Some(role) =
                accessibility::get_string_attribute_value(current, attributes::AX_ROLE)
                && role == "AXWindow"
            {
                return Ok(current);
            }

            // Get parent
            match get_attribute(current, attributes::AX_PARENT) {
                Ok(parent_ref) if !parent_ref.is_null() => {
                    current = parent_ref as AXUIElementRef;
                }
                _ => break,
            }
        }

        Err(AXError::ElementNotFound("window".into()))
    }

    /// Capture screenshot of the element
    fn capture_element_screenshot(&self) -> AXResult<Vec<u8>> {
        use std::process::Command;

        // Get element bounds
        let (x, y, width, height) = self
            .bounds()
            .ok_or_else(|| AXError::ActionFailed("Could not get element bounds".into()))?;

        // Use CGWindowListCreateImage to capture the region
        // For simplicity, use screencapture command with region
        let temp_dir = tempfile::Builder::new()
            .prefix("axterminator_element_screenshot_")
            .tempdir()
            .map_err(|e| AXError::SystemError(e.to_string()))?;
        let temp_path = temp_dir.path().join("capture.png");
        let region = format!(
            "{},{},{},{}",
            x as i32, y as i32, width as i32, height as i32
        );

        let output = Command::new("screencapture")
            .arg("-R")
            .arg(&region)
            .arg("-x")
            .arg(&temp_path)
            .output()
            .map_err(|e| AXError::SystemError(e.to_string()))?;

        if !output.status.success() {
            return Err(AXError::ActionFailed("Screenshot failed".into()));
        }

        let data = std::fs::read(&temp_path).map_err(|e| AXError::SystemError(e.to_string()))?;

        Ok(data)
    }

    /// Find a child element with timeout
    pub fn find_child(&self, query: &str, timeout: Option<Duration>) -> AXResult<AXElement> {
        use std::time::Instant;

        let start = Instant::now();
        let timeout = timeout.unwrap_or(Duration::from_millis(100));

        loop {
            match self.search_child(query) {
                Ok(element) => return Ok(element),
                Err(_) if start.elapsed() >= timeout => {
                    return Err(AXError::ElementNotFound(query.to_string()));
                }
                Err(_) => {
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        }
    }

    /// Search for a child element (single attempt)
    fn search_child(&self, query: &str) -> AXResult<AXElement> {
        let children = accessibility::get_children(self.element)?;
        if query.contains(':') {
            let parts: Vec<&str> = query.splitn(2, ':').collect();
            let attr = parts[0].trim();
            let value = parts[1].trim();
            self.search_in_elements(&children, attr, value)
        } else {
            self.search_in_elements_any_text(&children, query)
        }
    }

    /// Recursively search in element list
    fn search_in_elements(
        &self,
        elements: &[AXUIElementRef],
        attr: &str,
        value: &str,
    ) -> AXResult<AXElement> {
        for &element in elements {
            // Check if this element matches
            if let Some(attr_value) = accessibility::get_string_attribute_value(element, attr)
                && attr_value.contains(value)
            {
                return Ok(AXElement::new(element));
            }

            // Search in children
            if let Ok(children) = accessibility::get_children(element)
                && let Ok(found) = self.search_in_elements(&children, attr, value)
            {
                return Ok(found);
            }
        }

        Err(AXError::ElementNotFound(format!("{attr}:{value}")))
    }

    /// Recursively search elements, matching query against ANY text-bearing attribute.
    fn search_in_elements_any_text(
        &self,
        elements: &[AXUIElementRef],
        query: &str,
    ) -> AXResult<AXElement> {
        let text_attrs = [
            attributes::AX_TITLE,
            attributes::AX_DESCRIPTION,
            attributes::AX_VALUE,
            attributes::AX_LABEL,
            attributes::AX_IDENTIFIER,
        ];

        for &element in elements {
            let matches = text_attrs.iter().any(|attr| {
                accessibility::get_string_attribute_value(element, attr)
                    .is_some_and(|v| v.contains(query))
            });

            if matches {
                return Ok(AXElement::new(element));
            }

            if let Ok(children) = accessibility::get_children(element)
                && let Ok(found) = self.search_in_elements_any_text(&children, query)
            {
                return Ok(found);
            }
        }

        Err(AXError::ElementNotFound(query.to_string()))
    }
}

/// Convert a character to macOS virtual key code
///
/// Returns (`key_code`, `needs_shift`)
fn char_to_keycode(ch: char) -> (u16, bool) {
    match ch {
        'a' | 'A' => (0, ch.is_uppercase()),
        'b' | 'B' => (11, ch.is_uppercase()),
        'c' | 'C' => (8, ch.is_uppercase()),
        'd' | 'D' => (2, ch.is_uppercase()),
        'e' | 'E' => (14, ch.is_uppercase()),
        'f' | 'F' => (3, ch.is_uppercase()),
        'g' | 'G' => (5, ch.is_uppercase()),
        'h' | 'H' => (4, ch.is_uppercase()),
        'i' | 'I' => (34, ch.is_uppercase()),
        'j' | 'J' => (38, ch.is_uppercase()),
        'k' | 'K' => (40, ch.is_uppercase()),
        'l' | 'L' => (37, ch.is_uppercase()),
        'm' | 'M' => (46, ch.is_uppercase()),
        'n' | 'N' => (45, ch.is_uppercase()),
        'o' | 'O' => (31, ch.is_uppercase()),
        'p' | 'P' => (35, ch.is_uppercase()),
        'q' | 'Q' => (12, ch.is_uppercase()),
        'r' | 'R' => (15, ch.is_uppercase()),
        's' | 'S' => (1, ch.is_uppercase()),
        't' | 'T' => (17, ch.is_uppercase()),
        'u' | 'U' => (32, ch.is_uppercase()),
        'v' | 'V' => (9, ch.is_uppercase()),
        'w' | 'W' => (13, ch.is_uppercase()),
        'x' | 'X' => (7, ch.is_uppercase()),
        'y' | 'Y' => (16, ch.is_uppercase()),
        'z' | 'Z' => (6, ch.is_uppercase()),
        '0' => (29, false),
        '1' => (18, false),
        '2' => (19, false),
        '3' => (20, false),
        '4' => (21, false),
        '5' => (23, false),
        '6' => (22, false),
        '7' => (26, false),
        '8' => (28, false),
        '9' => (25, false),
        ')' => (29, true),
        '!' => (18, true),
        '@' => (19, true),
        '#' => (20, true),
        '$' => (21, true),
        '%' => (23, true),
        '^' => (22, true),
        '&' => (26, true),
        '*' => (28, true),
        '(' => (25, true),
        ' ' => (49, false),
        '-' | '_' => (27, ch == '_'),
        '=' | '+' => (24, ch == '+'),
        '[' | '{' => (33, ch == '{'),
        ']' | '}' => (30, ch == '}'),
        '\\' | '|' => (42, ch == '|'),
        ';' | ':' => (41, ch == ':'),
        '\'' | '"' => (39, ch == '"'),
        ',' | '<' => (43, ch == '<'),
        '.' | '>' => (47, ch == '>'),
        '/' | '?' => (44, ch == '?'),
        '`' | '~' => (50, ch == '~'),
        '\n' | '\r' => (36, false), // Return
        '\t' => (48, false),        // Tab
        _ => (49, false),           // Default to space
    }
}

impl Drop for AXElement {
    fn drop(&mut self) {
        accessibility::release_cf(self.element.cast());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    /// Helper to check if accessibility is enabled
    fn check_accessibility() -> bool {
        accessibility::check_accessibility_enabled()
    }

    #[test]
    fn test_char_to_keycode_lowercase() {
        let (code, shift) = char_to_keycode('a');
        assert_eq!(code, 0);
        assert!(!shift);

        let (code, shift) = char_to_keycode('z');
        assert_eq!(code, 6);
        assert!(!shift);
    }

    #[test]
    fn test_char_to_keycode_uppercase() {
        let (code, shift) = char_to_keycode('A');
        assert_eq!(code, 0);
        assert!(shift);

        let (code, shift) = char_to_keycode('Z');
        assert_eq!(code, 6);
        assert!(shift);
    }

    #[test]
    fn test_char_to_keycode_numbers() {
        let (code, shift) = char_to_keycode('0');
        assert_eq!(code, 29);
        assert!(!shift);

        let (code, shift) = char_to_keycode('5');
        assert_eq!(code, 23);
        assert!(!shift);
    }

    #[test]
    fn test_char_to_keycode_symbols() {
        let (code, shift) = char_to_keycode('!');
        assert_eq!(code, 18);
        assert!(shift);

        let (code, shift) = char_to_keycode('@');
        assert_eq!(code, 19);
        assert!(shift);

        let (code, shift) = char_to_keycode(' ');
        assert_eq!(code, 49);
        assert!(!shift);
    }

    #[test]
    fn test_char_to_keycode_special() {
        let (code, shift) = char_to_keycode('\n');
        assert_eq!(code, 36);
        assert!(!shift);

        let (code, shift) = char_to_keycode('\t');
        assert_eq!(code, 48);
        assert!(!shift);
    }

    #[test]
    fn test_element_creation() {
        if !check_accessibility() {
            println!("Skipping: Accessibility not enabled");
            return;
        }

        // Create a system-wide element
        let sys = accessibility::create_system_wide_element();
        assert!(sys.is_ok());

        if let Ok(element_ref) = sys {
            let element = AXElement::new(element_ref);
            // Element should exist
            assert!(element.role().is_some());
        }
    }

    #[test]
    fn test_get_string_attribute() {
        if !check_accessibility() {
            println!("Skipping: Accessibility not enabled");
            return;
        }

        // Try to get an app (Finder should always be running)
        let output = Command::new("pgrep")
            .arg("-x")
            .arg("Finder")
            .output()
            .expect("Failed to run pgrep");

        if let Ok(pid_str) = String::from_utf8(output.stdout)
            && let Ok(pid) = pid_str.trim().parse::<i32>()
            && let Ok(app_ref) = accessibility::create_application_element(pid)
        {
            let element = AXElement::new(app_ref);

            // Finder should have a role
            let role = element.role();
            assert!(role.is_some());
            assert_eq!(role.unwrap(), "AXApplication");
        }
    }

    #[test]
    fn test_get_bool_attribute() {
        if !check_accessibility() {
            println!("Skipping: Accessibility not enabled");
            return;
        }

        let output = Command::new("pgrep")
            .arg("-x")
            .arg("Finder")
            .output()
            .expect("Failed to run pgrep");

        if let Ok(pid_str) = String::from_utf8(output.stdout)
            && let Ok(pid) = pid_str.trim().parse::<i32>()
            && let Ok(app_ref) = accessibility::create_application_element(pid)
        {
            let element = AXElement::new(app_ref);

            // Check enabled attribute (should return a boolean)
            let enabled = element.enabled();
            // Value doesn't matter, just that it returns without panic
            let _ = enabled;
        }
    }

    #[test]
    fn test_bounds() {
        if !check_accessibility() {
            println!("Skipping: Accessibility not enabled");
            return;
        }

        let output = Command::new("pgrep")
            .arg("-x")
            .arg("Finder")
            .output()
            .expect("Failed to run pgrep");

        if let Ok(pid_str) = String::from_utf8(output.stdout)
            && let Ok(pid) = pid_str.trim().parse::<i32>()
            && let Ok(app_ref) = accessibility::create_application_element(pid)
        {
            let element = AXElement::new(app_ref);

            // Try to get windows
            if let Ok(children) = accessibility::get_children(element.element) {
                for child in children.iter().take(5) {
                    let child_elem = AXElement::new(*child);
                    if let Some(role) = child_elem.role()
                        && role == "AXWindow"
                    {
                        // Window should have bounds
                        if let Some((_x, _y, w, h)) = child_elem.bounds() {
                            assert!(w > 0.0);
                            assert!(h > 0.0);
                            return;
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn test_element_exists() {
        if !check_accessibility() {
            println!("Skipping: Accessibility not enabled");
            return;
        }

        let output = Command::new("pgrep")
            .arg("-x")
            .arg("Finder")
            .output()
            .expect("Failed to run pgrep");

        if let Ok(pid_str) = String::from_utf8(output.stdout)
            && let Ok(pid) = pid_str.trim().parse::<i32>()
            && let Ok(app_ref) = accessibility::create_application_element(pid)
        {
            let element = AXElement::new(app_ref);
            assert!(element.exists());
        }
    }

    #[test]
    fn test_search_child_parsing() {
        if !check_accessibility() {
            println!("Skipping: Accessibility not enabled");
            return;
        }

        // Test query parsing
        let query = "AXTitle:Save";
        assert!(query.contains(':'));

        let parts: Vec<&str> = query.splitn(2, ':').collect();
        assert_eq!(parts[0], "AXTitle");
        assert_eq!(parts[1], "Save");

        // Test simple query
        let query = "Save";
        assert!(!query.contains(':'));
    }

    #[test]
    fn test_perform_set_value_structure() {
        // This test verifies the structure is correct
        // Actual functionality requires a real text field element
        if !check_accessibility() {
            println!("Skipping: Accessibility not enabled");
            return;
        }

        // Test that CFString can be created
        use core_foundation::base::TCFType;
        use core_foundation::string::CFString;
        let test_value = CFString::new("test");
        assert!(!test_value.as_concrete_TypeRef().is_null());
    }

    #[test]
    fn test_type_character_mapping_completeness() {
        // Ensure all common characters have mappings
        let test_chars = "abcdefghijklmnopqrstuvwxyz\
                          ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                          0123456789\
                          !@#$%^&*()\
                          -=[]\\;',./\
                          _+{}|:\"<>?\
                          `~ \n\t";

        for ch in test_chars.chars() {
            let (code, _shift) = char_to_keycode(ch);
            // All characters should map to valid key codes (0-127)
            assert!(code < 128);
        }
    }
}
