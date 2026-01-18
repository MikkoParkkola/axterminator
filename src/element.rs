//! Accessibility element wrapper

use pyo3::prelude::*;
use std::time::Duration;

use crate::accessibility::{
    self, actions, attributes, get_attribute, perform_action, AXUIElementRef,
};
use crate::error::{AXError, AXResult};
use crate::ActionMode;

/// Wrapper for an accessibility element
#[pyclass]
#[derive(Debug)]
pub struct AXElement {
    /// Raw accessibility element reference
    pub(crate) element: AXUIElementRef,
    /// Cached role
    pub(crate) role: Option<String>,
    /// Cached title
    pub(crate) title: Option<String>,
}

unsafe impl Send for AXElement {}
unsafe impl Sync for AXElement {}

#[pymethods]
impl AXElement {
    /// Get the element's role (e.g., "AXButton", "AXTextField")
    fn role(&self) -> Option<String> {
        self.get_string_attribute(attributes::AX_ROLE)
    }

    /// Get the element's title
    fn title(&self) -> Option<String> {
        self.get_string_attribute(attributes::AX_TITLE)
    }

    /// Get the element's value
    fn value(&self) -> Option<String> {
        self.get_string_attribute(attributes::AX_VALUE)
    }

    /// Get the element's description
    fn description(&self) -> Option<String> {
        self.get_string_attribute(attributes::AX_DESCRIPTION)
    }

    /// Get the element's label
    fn label(&self) -> Option<String> {
        self.get_string_attribute(attributes::AX_LABEL)
    }

    /// Get the element's identifier
    fn identifier(&self) -> Option<String> {
        self.get_string_attribute(attributes::AX_IDENTIFIER)
    }

    /// Check if the element is enabled
    fn enabled(&self) -> bool {
        self.get_bool_attribute(attributes::AX_ENABLED)
            .unwrap_or(false)
    }

    /// Check if the element is focused
    fn focused(&self) -> bool {
        self.get_bool_attribute(attributes::AX_FOCUSED)
            .unwrap_or(false)
    }

    /// Check if the element exists in the UI
    fn exists(&self) -> bool {
        self.role().is_some()
    }

    /// Get the element's bounds (x, y, width, height)
    fn bounds(&self) -> Option<(f64, f64, f64, f64)> {
        // TODO: Implement bounds retrieval from AXPosition and AXSize
        None
    }

    /// Click the element
    ///
    /// # Arguments
    /// * `mode` - Action mode (BACKGROUND or FOCUS). Default is BACKGROUND.
    ///
    /// # Example
    /// ```python
    /// # Background click (no focus stealing!) - DEFAULT
    /// element.click()
    ///
    /// # Focus click (brings app to foreground)
    /// element.click(mode=ax.FOCUS)
    /// ```
    #[pyo3(signature = (mode=None))]
    fn click(&self, mode: Option<ActionMode>) -> PyResult<()> {
        let mode = mode.unwrap_or(ActionMode::Background);
        self.perform_click(mode)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Double-click the element
    #[pyo3(signature = (mode=None))]
    fn double_click(&self, mode: Option<ActionMode>) -> PyResult<()> {
        let mode = mode.unwrap_or(ActionMode::Background);
        self.perform_click(mode)?;
        std::thread::sleep(Duration::from_millis(50));
        self.perform_click(mode)
    }

    /// Right-click the element
    #[pyo3(signature = (mode=None))]
    fn right_click(&self, mode: Option<ActionMode>) -> PyResult<()> {
        let mode = mode.unwrap_or(ActionMode::Background);
        self.perform_show_menu(mode)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Type text into the element
    ///
    /// NOTE: Text input requires focus mode.
    ///
    /// # Arguments
    /// * `text` - Text to type
    /// * `mode` - Action mode (FOCUS required for text input)
    #[pyo3(signature = (text, mode=None))]
    fn type_text(&self, text: &str, mode: Option<ActionMode>) -> PyResult<()> {
        let mode = mode.unwrap_or(ActionMode::Focus);
        if mode == ActionMode::Background {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "Text input requires FOCUS mode",
            ));
        }
        self.perform_type_text(text)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Set the element's value
    fn set_value(&self, value: &str) -> PyResult<()> {
        self.perform_set_value(value)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Take a screenshot of just this element
    fn screenshot(&self) -> PyResult<Vec<u8>> {
        self.capture_element_screenshot()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Find a child element
    #[pyo3(signature = (query, timeout_ms=None))]
    fn find(&self, query: &str, timeout_ms: Option<u64>) -> PyResult<AXElement> {
        // TODO: Implement child search
        Err(pyo3::exceptions::PyRuntimeError::new_err(
            "Child element search not yet implemented",
        ))
    }
}

impl AXElement {
    /// Create a new element wrapper
    pub fn new(element: AXUIElementRef) -> Self {
        Self {
            element,
            role: None,
            title: None,
        }
    }

    /// Get a string attribute value
    fn get_string_attribute(&self, attribute: &str) -> Option<String> {
        // TODO: Implement proper CFString conversion
        get_attribute(self.element, attribute)
            .ok()
            .map(|_| String::new()) // Placeholder
    }

    /// Get a boolean attribute value
    fn get_bool_attribute(&self, attribute: &str) -> Option<bool> {
        // TODO: Implement proper CFBoolean conversion
        get_attribute(self.element, attribute).ok().map(|_| true) // Placeholder
    }

    /// Perform click action
    fn perform_click(&self, mode: ActionMode) -> PyResult<()> {
        match mode {
            ActionMode::Background => {
                // WORLD FIRST: Background click without focus stealing!
                perform_action(self.element, actions::AX_PRESS)
                    .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
            }
            ActionMode::Focus => {
                // Focus the element first, then click
                self.bring_to_focus()?;
                perform_action(self.element, actions::AX_PRESS)
                    .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
            }
        }
    }

    /// Perform show menu action (right-click)
    fn perform_show_menu(&self, mode: ActionMode) -> AXResult<()> {
        if mode == ActionMode::Focus {
            self.bring_to_focus()?;
        }
        perform_action(self.element, actions::AX_SHOW_MENU)
    }

    /// Type text into the element
    fn perform_type_text(&self, text: &str) -> AXResult<()> {
        // Focus the element
        self.bring_to_focus()?;

        // Use CGEvent to simulate keyboard input
        // TODO: Implement proper keyboard simulation
        Err(AXError::ActionFailed(
            "Text input not yet implemented".into(),
        ))
    }

    /// Set the element's value directly
    fn perform_set_value(&self, value: &str) -> AXResult<()> {
        // TODO: Implement AXUIElementSetAttributeValue
        Err(AXError::ActionFailed(
            "Set value not yet implemented".into(),
        ))
    }

    /// Bring the element to focus
    fn bring_to_focus(&self) -> PyResult<()> {
        // Set AXFocused attribute to true
        // TODO: Implement proper focus
        Ok(())
    }

    /// Capture screenshot of the element
    fn capture_element_screenshot(&self) -> AXResult<Vec<u8>> {
        // TODO: Implement element-level screenshot using bounds
        Err(AXError::ActionFailed(
            "Element screenshot not yet implemented".into(),
        ))
    }
}

impl Drop for AXElement {
    fn drop(&mut self) {
        accessibility::release_cf(self.element as _);
    }
}
