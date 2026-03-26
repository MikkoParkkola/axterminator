//! System context extraction for AI agent situational awareness.
//!
//! Provides a rich environmental snapshot so AI agents can adapt their
//! behaviour to the user's current context — location, connectivity,
//! power state, display configuration, locale, and more.
//!
//! ## Privacy
//!
//! - **Clipboard**: Read from `NSPasteboard` — no network, no persistence.
//! - **System info**: All local queries (IOKit, CoreAudio, AppKit, sysctl).
//! - **Geolocation**: Requires Location Services TCC consent (feature-gated).
//!
//! ## Feature flag
//!
//! The `context` cargo feature enables all context tools. Geolocation
//! additionally requires Location Services permission at runtime.

pub mod clipboard;
#[cfg(feature = "context")]
pub mod location;
pub mod system;
