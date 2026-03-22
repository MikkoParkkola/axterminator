//! AX Observer — real-time accessibility notifications (stub).
//!
//! Full implementation requires a `CFRunLoop` (async server mode). This module
//! provides the type definitions and a gracefully degrading no-op implementation
//! for the current synchronous stdio transport.
//!
//! ## Why a stub?
//!
//! The macOS AX Observer API (`AXObserverCreate`, `AXObserverAddNotification`)
//! delivers callbacks on a `CFRunLoop`. The synchronous stdio server has no
//! run loop, so observers can be created but notifications are never pumped.
//!
//! When the server moves to an async transport (Phase 6), this module can be
//! upgraded to a real implementation without changing any call sites: the
//! public API is intentionally identical to what a full implementation would
//! expose.
//!
//! ## Usage (future async mode)
//!
//! ```rust,ignore
//! use axterminator::mcp::observer::{AXNotification, AXObserverHandle};
//!
//! let mut handle = AXObserverHandle::new(pid);
//! handle.subscribe(AXNotification::FocusedUIElementChanged)?;
//! // … run loop delivers callbacks via channel …
//! ```

// ---------------------------------------------------------------------------
// Notification types
// ---------------------------------------------------------------------------

/// The set of AX notification strings we would observe.
///
/// Each variant maps to the Core Foundation string constant used in
/// `AXObserverAddNotification`. The mapping is provided by [`AXNotification::as_cf_str`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AXNotification {
    /// An accessible value changed (e.g. text field content, slider position).
    ValueChanged,
    /// The UI element was destroyed (e.g. window or widget closed).
    UIElementDestroyed,
    /// A new window was created.
    WindowCreated,
    /// An existing window was moved.
    WindowMoved,
    /// An existing window was resized.
    WindowResized,
    /// The focused window changed.
    FocusedWindowChanged,
    /// The focused UI element within the focused window changed.
    FocusedUIElementChanged,
    /// The selected children of a container changed (e.g. list selection).
    SelectedChildrenChanged,
    /// The title of a UI element changed.
    TitleChanged,
}

impl AXNotification {
    /// Return the Core Foundation string constant for this notification type.
    ///
    /// The returned string is suitable for passing to `AXObserverAddNotification`
    /// once the full implementation is wired to the macOS AX API.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use axterminator::mcp::observer::AXNotification;
    ///
    /// assert_eq!(AXNotification::ValueChanged.as_cf_str(), "AXValueChanged");
    /// assert_eq!(AXNotification::WindowCreated.as_cf_str(), "AXWindowCreated");
    /// ```
    #[must_use]
    pub fn as_cf_str(self) -> &'static str {
        match self {
            Self::ValueChanged => "AXValueChanged",
            Self::UIElementDestroyed => "AXUIElementDestroyed",
            Self::WindowCreated => "AXWindowCreated",
            Self::WindowMoved => "AXWindowMoved",
            Self::WindowResized => "AXWindowResized",
            Self::FocusedWindowChanged => "AXFocusedWindowChanged",
            Self::FocusedUIElementChanged => "AXFocusedUIElementChanged",
            Self::SelectedChildrenChanged => "AXSelectedChildrenChanged",
            Self::TitleChanged => "AXTitleChanged",
        }
    }
}

// ---------------------------------------------------------------------------
// Observer handle
// ---------------------------------------------------------------------------

/// Observer handle — a no-op stub in synchronous server mode.
///
/// In a future async implementation this struct will hold the `AXObserverRef`
/// and a `Sender` for delivering notifications to the server loop. The current
/// stub allows all call sites to be written against the final API without
/// requiring an active `CFRunLoop`.
///
/// # Examples
///
/// ```rust
/// use axterminator::mcp::observer::{AXNotification, AXObserverHandle};
///
/// let mut handle = AXObserverHandle::new(0);
/// assert!(!handle.is_active());
/// assert!(handle.subscribe(AXNotification::FocusedWindowChanged).is_ok());
/// ```
pub struct AXObserverHandle {
    active: bool,
}

impl AXObserverHandle {
    /// Create a new observer for the process with the given PID.
    ///
    /// In synchronous mode this is a no-op: no `AXObserverRef` is allocated
    /// and `is_active()` returns `false`. The handle is still valid to call
    /// `subscribe` on — subscriptions are recorded but silently discarded.
    #[must_use]
    pub fn new(_pid: i32) -> Self {
        tracing::debug!("AX Observer created (stub — synchronous mode, no CFRunLoop)");
        Self { active: false }
    }

    /// Subscribe to an AX notification type.
    ///
    /// In synchronous mode this logs the subscription request and returns
    /// `Ok(())`. No actual `CFRunLoop` observer is registered, so no
    /// callbacks will be delivered.
    ///
    /// # Errors
    ///
    /// Currently infallible in stub mode. A real implementation may return
    /// `Err(String)` when `AXObserverAddNotification` fails (e.g. the PID is
    /// invalid or accessibility permissions are denied).
    pub fn subscribe(&mut self, notification: AXNotification) -> Result<(), String> {
        tracing::debug!(
            notification = notification.as_cf_str(),
            "AX Observer subscribe (stub — no CFRunLoop, notification discarded)"
        );
        Ok(())
    }

    /// Return `true` when the observer is actively delivering notifications.
    ///
    /// Always `false` in synchronous mode — the `CFRunLoop` is not running.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.active
    }
}

impl Drop for AXObserverHandle {
    fn drop(&mut self) {
        tracing::debug!("AX Observer dropped");
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // AXNotification::as_cf_str
    // -----------------------------------------------------------------------

    #[test]
    fn value_changed_maps_to_correct_cf_str() {
        // GIVEN / WHEN / THEN
        assert_eq!(AXNotification::ValueChanged.as_cf_str(), "AXValueChanged");
    }

    #[test]
    fn ui_element_destroyed_maps_to_correct_cf_str() {
        assert_eq!(
            AXNotification::UIElementDestroyed.as_cf_str(),
            "AXUIElementDestroyed"
        );
    }

    #[test]
    fn window_created_maps_to_correct_cf_str() {
        assert_eq!(AXNotification::WindowCreated.as_cf_str(), "AXWindowCreated");
    }

    #[test]
    fn window_moved_maps_to_correct_cf_str() {
        assert_eq!(AXNotification::WindowMoved.as_cf_str(), "AXWindowMoved");
    }

    #[test]
    fn window_resized_maps_to_correct_cf_str() {
        assert_eq!(AXNotification::WindowResized.as_cf_str(), "AXWindowResized");
    }

    #[test]
    fn focused_window_changed_maps_to_correct_cf_str() {
        assert_eq!(
            AXNotification::FocusedWindowChanged.as_cf_str(),
            "AXFocusedWindowChanged"
        );
    }

    #[test]
    fn focused_ui_element_changed_maps_to_correct_cf_str() {
        assert_eq!(
            AXNotification::FocusedUIElementChanged.as_cf_str(),
            "AXFocusedUIElementChanged"
        );
    }

    #[test]
    fn selected_children_changed_maps_to_correct_cf_str() {
        assert_eq!(
            AXNotification::SelectedChildrenChanged.as_cf_str(),
            "AXSelectedChildrenChanged"
        );
    }

    #[test]
    fn title_changed_maps_to_correct_cf_str() {
        assert_eq!(AXNotification::TitleChanged.as_cf_str(), "AXTitleChanged");
    }

    #[test]
    fn all_notifications_produce_non_empty_cf_str() {
        // GIVEN: every variant
        let variants = [
            AXNotification::ValueChanged,
            AXNotification::UIElementDestroyed,
            AXNotification::WindowCreated,
            AXNotification::WindowMoved,
            AXNotification::WindowResized,
            AXNotification::FocusedWindowChanged,
            AXNotification::FocusedUIElementChanged,
            AXNotification::SelectedChildrenChanged,
            AXNotification::TitleChanged,
        ];
        // WHEN / THEN: all produce a non-empty string
        for n in variants {
            assert!(
                !n.as_cf_str().is_empty(),
                "variant {n:?} produced empty cf_str"
            );
        }
    }

    #[test]
    fn all_cf_strs_start_with_ax_prefix() {
        // GIVEN: every variant
        let variants = [
            AXNotification::ValueChanged,
            AXNotification::UIElementDestroyed,
            AXNotification::WindowCreated,
            AXNotification::WindowMoved,
            AXNotification::WindowResized,
            AXNotification::FocusedWindowChanged,
            AXNotification::FocusedUIElementChanged,
            AXNotification::SelectedChildrenChanged,
            AXNotification::TitleChanged,
        ];
        // WHEN / THEN: all start with "AX"
        for n in variants {
            assert!(
                n.as_cf_str().starts_with("AX"),
                "expected 'AX' prefix on {:?} -> '{}'",
                n,
                n.as_cf_str()
            );
        }
    }

    // -----------------------------------------------------------------------
    // AXObserverHandle
    // -----------------------------------------------------------------------

    #[test]
    fn new_observer_is_not_active() {
        // GIVEN: freshly created observer
        // WHEN: checking active state
        let handle = AXObserverHandle::new(1234);
        // THEN: always false in synchronous mode
        assert!(!handle.is_active());
    }

    #[test]
    fn subscribe_returns_ok_in_stub_mode() {
        // GIVEN: observer for a dummy PID
        let mut handle = AXObserverHandle::new(0);
        // WHEN: subscribing to any notification
        let result = handle.subscribe(AXNotification::FocusedWindowChanged);
        // THEN: stub always succeeds
        assert!(result.is_ok());
    }

    #[test]
    fn subscribe_all_notification_types_without_error() {
        // GIVEN: observer for dummy PID
        let mut handle = AXObserverHandle::new(0);
        // WHEN: subscribing to every notification type
        let results = [
            handle.subscribe(AXNotification::ValueChanged),
            handle.subscribe(AXNotification::UIElementDestroyed),
            handle.subscribe(AXNotification::WindowCreated),
            handle.subscribe(AXNotification::WindowMoved),
            handle.subscribe(AXNotification::WindowResized),
            handle.subscribe(AXNotification::FocusedWindowChanged),
            handle.subscribe(AXNotification::FocusedUIElementChanged),
            handle.subscribe(AXNotification::SelectedChildrenChanged),
            handle.subscribe(AXNotification::TitleChanged),
        ];
        // THEN: all succeed
        for r in results {
            assert!(r.is_ok());
        }
    }

    #[test]
    fn is_active_remains_false_after_subscribe() {
        // GIVEN: fresh observer with a subscription
        let mut handle = AXObserverHandle::new(0);
        handle.subscribe(AXNotification::TitleChanged).unwrap();
        // WHEN: checking active
        // THEN: still false — stub never activates
        assert!(!handle.is_active());
    }

    #[test]
    fn observer_drops_without_panic() {
        // GIVEN: observer in scope
        {
            let _handle = AXObserverHandle::new(0);
        }
        // THEN: no panic on drop
    }
}
