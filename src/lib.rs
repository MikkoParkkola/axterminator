//! # `AXTerminator` - macOS GUI Testing Framework
//!
//! A macOS GUI testing framework with background testing, sub-millisecond
//! element access, and self-healing locators.
//!
//! ## Key Features
//!
//! - **Background Testing**: Test apps without stealing focus
//! - **Sub-millisecond Access**: ~379 us element access (measured on M1 MacBook Pro)
//! - **Self-Healing**: 7-strategy fallback for element location
//! - **Unified API**: Works with Native, Electron, and `WebView` apps
//!
//! ## Quick Start
//!
//! ```python
//! import axterminator as ax
//!
//! # Connect to app
//! safari = ax.app(bundle_id="com.apple.Safari")
//!
//! # Background click (no focus stealing!)
//! safari.find("New Tab").click()
//!
//! # Text input (requires focus)
//! safari.find("URL").type_text("https://example.com", mode=ax.FOCUS)
//! ```

#![allow(hidden_glob_reexports)]
#![allow(clippy::useless_conversion)]

use pyo3::prelude::*;

mod accessibility;
mod actions;
mod app;
mod cache;
mod element;
mod error;
mod healing;
pub mod blackbox;
pub mod copilot;
pub mod docker_browser;
pub mod copilot_extract;
pub mod cross_app;
pub mod copilot_format;
pub mod copilot_state;
pub mod durable_steps;
pub mod electron_cdp;
pub mod electron_profiles;
pub mod healing_match;
pub mod intent;
pub mod intent_matching;
pub mod persistent_refs;
pub mod recording;
pub mod scene;
pub mod semantic_find;
mod router;
mod sync;
pub mod triple_understanding;

pub use accessibility::*;
pub use actions::*;
pub use app::*;
pub use cache::*;
pub use element::*;
pub use error::*;
pub use healing::*;
pub use router::*;
pub use sync::*;

/// Action mode for element interactions
#[pyclass(eq, eq_int)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActionMode {
    /// Perform action in background without stealing focus (DEFAULT)
    #[default]
    Background,
    /// Bring app to foreground and focus element
    Focus,
}

/// Initialize the Python module
#[pymodule]
fn axterminator(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<ActionMode>()?;
    m.add_class::<AXApp>()?;
    m.add_class::<AXElement>()?;
    m.add_class::<HealingConfig>()?;

    // Top-level functions
    m.add_function(wrap_pyfunction!(connect_app, m)?)?;
    m.add_function(wrap_pyfunction!(is_accessibility_enabled, m)?)?;
    m.add_function(wrap_pyfunction!(configure_healing, m)?)?;

    // Constants
    m.add("BACKGROUND", ActionMode::Background)?;
    m.add("FOCUS", ActionMode::Focus)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;

    Ok(())
}

/// Connect to an application
///
/// # Arguments
/// * `name` - Application name (e.g., "Safari")
/// * `bundle_id` - Bundle identifier (e.g., "com.apple.Safari")
/// * `pid` - Process ID
///
/// # Returns
/// * `AXApp` - Application wrapper
///
/// # Example
/// ```python
/// # By name
/// safari = ax.app(name="Safari")
///
/// # By bundle ID (recommended)
/// safari = ax.app(bundle_id="com.apple.Safari")
///
/// # By PID
/// safari = ax.app(pid=12345)
/// ```
#[pyfunction(name = "app")]
#[pyo3(signature = (name=None, bundle_id=None, pid=None))]
fn connect_app(name: Option<&str>, bundle_id: Option<&str>, pid: Option<u32>) -> PyResult<AXApp> {
    AXApp::connect(name, bundle_id, pid)
}

/// Check if accessibility permissions are enabled
///
/// # Returns
/// * `bool` - True if accessibility is enabled
///
/// # Example
/// ```python
/// if not ax.is_accessibility_enabled():
///     print("Enable in System Preferences > Privacy > Accessibility")
/// ```
#[pyfunction]
fn is_accessibility_enabled() -> bool {
    accessibility::check_accessibility_enabled()
}

/// Configure the self-healing system
///
/// # Arguments
/// * `config` - Healing configuration
#[pyfunction]
fn configure_healing(config: HealingConfig) -> PyResult<()> {
    healing::set_global_config(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_mode_default() {
        assert_eq!(ActionMode::default(), ActionMode::Background);
    }
}
