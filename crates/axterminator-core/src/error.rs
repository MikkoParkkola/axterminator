use thiserror::Error;

#[derive(Error, Debug)]
pub enum AXError {
    #[error("Accessibility permission not granted")]
    PermissionDenied,
    
    #[error("Application not found: {0}")]
    AppNotFound(String),
    
    #[error("Element not found: {0}")]
    ElementNotFound(String),
    
    #[error("AX API error: {0}")]
    ApiError(String),
    
    #[error("Internal error: {0}")]
    Internal(String),
}
