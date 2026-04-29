use serde::{Deserialize, Serialize};

/// Represents an Accessibility (AX) element in the macOS UI tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AXElement {
    /// The element's accessibility role (AXButton, AXTextField, etc.)
    pub role: String,
    /// Human-readable title or label
    pub title: Option<String>,
    /// Accessibility description
    pub description: Option<String>,
    /// Element's value (current text, number, etc.)
    pub value: Option<String>,
    /// Whether the element is enabled
    pub enabled: bool,
    /// Whether the element is focused
    pub focused: bool,
    /// Screen position (x, y)
    pub position: Option<(f64, f64)>,
    /// Element size (width, height)
    pub size: Option<(f64, f64)>,
    /// Unique identifier (if available)
    pub identifier: Option<String>,
}

impl AXElement {
    /// Create a minimal element with just a role.
    pub fn new(role: &str) -> Self {
        Self {
            role: role.to_string(),
            title: None,
            description: None,
            value: None,
            enabled: true,
            focused: false,
            position: None,
            size: None,
            identifier: None,
        }
    }

    /// Whether this element can receive click events.
    pub fn is_clickable(&self) -> bool {
        matches!(
            self.role.as_str(),
            "AXButton" | "AXLink" | "AXMenuItem" | "AXCheckBox" | "AXRadioButton"
        )
    }

    /// Whether this element can receive text input.
    pub fn is_text_field(&self) -> bool {
        matches!(self.role.as_str(), "AXTextField" | "AXTextArea" | "AXSearchField")
    }
}
