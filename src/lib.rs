//! # `AXTerminator` - macOS GUI Testing Framework
//!
//! A macOS GUI testing framework with background testing, sub-millisecond
//! element access, and self-healing locators.
//!
//! ## Key Features
//!
//! - **Background Testing**: Test apps without stealing focus
//! - **Sub-millisecond Access**: ~379 us element access (measured on M1 `MacBook` Pro)
//! - **Self-Healing**: 7-strategy fallback for element location
//! - **Unified API**: Works with Native, Electron, and `WebView` apps

#![allow(hidden_glob_reexports)]
#![allow(clippy::useless_conversion)]

#[cfg(feature = "audio")]
pub mod audio;
#[cfg(feature = "camera")]
pub mod camera;
#[cfg(feature = "audio")]
pub mod capture;
pub mod context;
pub mod display;
pub mod mcp;
#[cfg(feature = "spaces")]
pub mod spaces;
#[cfg(feature = "watch")]
pub mod watch;

mod accessibility;
mod actions;
mod app;
pub mod blackbox;
pub mod copilot;
pub mod copilot_extract;
pub mod copilot_format;
pub mod copilot_state;
pub mod cross_app;
pub mod docker_browser;
pub mod durable_steps;
pub mod electron_cdp;
pub mod electron_profiles;
mod element;
mod error;
mod healing;
pub mod healing_match;
pub mod intent;
pub mod intent_matching;
pub mod persistent_refs;
pub mod recording;
mod router;
pub mod scene;
pub mod semantic_find;
mod sync;
#[cfg(test)]
mod test_sync;
pub mod triple_understanding;

pub use accessibility::*;
pub use actions::*;
pub use app::*;
pub use element::*;
pub use error::*;
pub use healing::*;
pub use router::*;
pub use sync::*;

/// Action mode for element interactions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActionMode {
    /// Perform action in background without stealing focus (DEFAULT)
    #[default]
    Background,
    /// Bring app to foreground and focus element
    Focus,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_mode_default() {
        assert_eq!(ActionMode::default(), ActionMode::Background);
    }
}
