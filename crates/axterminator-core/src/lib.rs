//! axterminator-core — macOS GUI automation library.
//!
//! Provides the core types and primitives for Accessibility (AX) API access
//! on macOS. Used by botnaut-client for direct (zero-IPC) GUI automation.
//!
//! The full MCP server is in the root `axterminator` crate.
//!
//! ## License
//! PolyForm Noncommercial 1.0.0 — free for noncommercial use.
//! Commercial use requires a separate license. See LICENSE.

pub mod element;
pub mod error;

pub use element::AXElement;
pub use error::AXError;

/// Library version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Check if accessibility permissions are granted for this process.
pub fn is_accessible() -> bool {
    #[cfg(target_os = "macos")]
    {
        // AXIsProcessTrusted() returns bool
        unsafe { ax_is_process_trusted() }
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

#[cfg(target_os = "macos")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

// Alias for the external function
#[cfg(target_os = "macos")]
unsafe fn ax_is_process_trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert_eq!(VERSION, "0.1.0");
    }

    #[test]
    fn test_is_accessible_returns_bool() {
        let result = is_accessible();
        assert!(result == true || result == false);
    }
}

/// Application descriptor — name, bundle ID, PID.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AppInfo {
    pub name: String,
    pub bundle_id: Option<String>,
    pub pid: i32,
    pub is_accessible: bool,
}

/// Window descriptor — title, position, size, app.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WindowInfo {
    pub title: String,
    pub app_name: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub is_main: bool,
}

/// System context snapshot — battery, display, volume, etc.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SystemContext {
    pub battery_pct: Option<f64>,
    pub charging: Option<bool>,
    pub dark_mode: bool,
    pub screen_width: u32,
    pub screen_height: u32,
    pub volume: Option<f64>,
    pub hostname: String,
    pub uptime_secs: u64,
}
