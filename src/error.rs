//! Error types for `AXTerminator`

use thiserror::Error;

/// `AXTerminator` error types
#[derive(Error, Debug)]
pub enum AXError {
    #[error("Accessibility not enabled. Enable in System Preferences > Privacy > Accessibility")]
    AccessibilityNotEnabled,

    #[error("Application not found: {0}")]
    AppNotFound(String),

    #[error("Application not running: {0}")]
    AppNotRunning(String),

    #[error("Element not found: {0}")]
    ElementNotFound(String),

    #[error("Element not found after healing attempts: {0}")]
    ElementNotFoundAfterHealing(String),

    #[error("Action failed: {0}")]
    ActionFailed(String),

    #[error("Action not supported in background mode: {0}")]
    BackgroundNotSupported(String),

    #[error("Timeout waiting for element: {0}")]
    Timeout(String),

    #[error("Invalid query: {0}")]
    InvalidQuery(String),

    #[error("macOS API error: {0}")]
    SystemError(String),
}

#[cfg(feature = "python-ext")]
impl From<AXError> for pyo3::PyErr {
    fn from(err: AXError) -> pyo3::PyErr {
        pyo3::exceptions::PyRuntimeError::new_err(err.to_string())
    }
}

/// Result type for `AXTerminator` operations
pub type AXResult<T> = Result<T, AXError>;
