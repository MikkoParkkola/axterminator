//! `useCopilotReadable`-style state injection for native macOS apps.
//!
//! Exposes real-time application state as structured context that AI agents
//! can consume. Analogous to React's `useCopilotReadable` hook but driven by
//! macOS accessibility APIs instead of React component trees.
//!
//! # Architecture
//!
//! ```text
//! AXUIElement tree  →  read_copilot_state()  →  CopilotState
//!                   →  watch_state_changes()  →  notifications
//! CopilotState × 2  →  diff_states()          →  Vec<StateChange>
//! ```
//!
//! # Example
//!
//! ```ignore
//! use axterminator::copilot_state::read_copilot_state;
//! use axterminator::create_application_element;
//!
//! let app_ref = create_application_element(12345).unwrap();
//! let state = read_copilot_state(app_ref);
//! println!("App: {}", state.app.name.as_deref().unwrap_or("unknown"));
//! ```

use serde::{Deserialize, Serialize};
use std::time::SystemTime;

use crate::accessibility::{AXUIElementRef, get_children};
use crate::copilot_extract::{
    extract_app_context, extract_content_context, extract_navigation_context,
    extract_selection_context, find_focused_window, first_window_ref,
};

// ---------------------------------------------------------------------------
// Context sub-structs
// ---------------------------------------------------------------------------

/// High-level identity of the running application.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AppContext {
    /// Application name (e.g. "Safari")
    pub name: Option<String>,
    /// Focused window title
    pub focused_window: Option<String>,
    /// Active tab or document title
    pub active_tab: Option<String>,
    /// Active document or file name
    pub active_document: Option<String>,
}

/// What the user currently has selected.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SelectionContext {
    /// Selected text (truncated to 512 chars)
    pub selected_text: Option<String>,
    /// Index of the selected list row (0-based)
    pub selected_list_row: Option<usize>,
    /// `(row, column)` of the selected table cell
    pub selected_table_cell: Option<(usize, usize)>,
    /// Titles of all currently-selected items
    pub selected_items: Vec<String>,
}

/// Where in the application the user is located.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct NavigationContext {
    /// Breadcrumb path from root to current location
    pub breadcrumb: Vec<String>,
    /// Currently selected sidebar item
    pub sidebar_selection: Option<String>,
    /// Active tab bar item
    pub tab_bar_selection: Option<String>,
    /// Depth of the current location in the UI tree
    pub depth: usize,
}

/// Visible content the user is looking at.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ContentContext {
    /// Document or page title
    pub document_title: Option<String>,
    /// Short excerpt of visible text (first 256 chars)
    pub visible_text_excerpt: Option<String>,
    /// Non-empty form field label → value pairs (max 20 fields)
    pub form_fields: Vec<(String, String)>,
    /// Role of the currently focused element
    pub focused_element_role: Option<String>,
    /// Title of the currently focused element
    pub focused_element_title: Option<String>,
}

// ---------------------------------------------------------------------------
// Top-level snapshot
// ---------------------------------------------------------------------------

/// Complete structured snapshot of an application's UI state.
///
/// Analogous to the union of all `useCopilotReadable` registrations in a
/// React app but derived from live AX attributes rather than component props.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CopilotState {
    /// High-level application identity
    pub app: AppContext,
    /// Current selection
    pub selection: SelectionContext,
    /// Navigation position
    pub navigation: NavigationContext,
    /// Visible content
    pub content: ContentContext,
    /// Snapshot timestamp (Unix seconds)
    pub timestamp: u64,
}

impl CopilotState {
    /// Create an empty state with the current timestamp.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            app: AppContext::default(),
            selection: SelectionContext::default(),
            navigation: NavigationContext::default(),
            content: ContentContext::default(),
            timestamp: unix_now(),
        }
    }
}

// ---------------------------------------------------------------------------
// State change diff
// ---------------------------------------------------------------------------

/// A single field that changed between two state snapshots.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateChange {
    /// Dot-path identifying the changed field (e.g. `"app.focused_window"`)
    pub field: String,
    /// Previous value serialised to JSON string (`null` when absent)
    pub old_value: String,
    /// New value serialised to JSON string (`null` when absent)
    pub new_value: String,
}

impl StateChange {
    fn new(field: &str, old: &impl Serialize, new: &impl Serialize) -> Self {
        Self {
            field: field.to_owned(),
            old_value: serde_json::to_string(old).unwrap_or_default(),
            new_value: serde_json::to_string(new).unwrap_or_default(),
        }
    }
}

/// Compute which fields changed between two snapshots.
///
/// Returns an empty `Vec` when the states are identical.
///
/// # Example
///
/// ```
/// use axterminator::copilot_state::{CopilotState, diff_states};
///
/// let old = CopilotState::empty();
/// let mut new = old.clone();
/// new.app.name = Some("Safari".to_owned());
/// let changes = diff_states(&old, &new);
/// assert_eq!(changes.len(), 1);
/// assert_eq!(changes[0].field, "app.name");
/// ```
#[must_use]
pub fn diff_states(old: &CopilotState, new: &CopilotState) -> Vec<StateChange> {
    let mut changes = Vec::new();
    diff_app(&old.app, &new.app, &mut changes);
    diff_selection(&old.selection, &new.selection, &mut changes);
    diff_navigation(&old.navigation, &new.navigation, &mut changes);
    diff_content(&old.content, &new.content, &mut changes);
    changes
}

// ---------------------------------------------------------------------------
// Primary extraction entry point
// ---------------------------------------------------------------------------

/// Extract the full `CopilotState` from an application element.
///
/// The function never panics; any attribute that cannot be read is simply
/// left as `None` / empty.
///
/// # Safety
///
/// `app_ref` must be a valid `AXUIElementRef` for a running application.
/// Passing a null or stale pointer results in `None` fields, not a crash,
/// because every inner accessor checks for null internally.
#[must_use]
pub fn read_copilot_state(app_ref: AXUIElementRef) -> CopilotState {
    let children = get_children(app_ref).unwrap_or_default();

    let focused_window = find_focused_window(app_ref);
    let window_ref = focused_window
        .as_ref()
        .and_then(|_| first_window_ref(app_ref, &children));

    CopilotState {
        app: extract_app_context(app_ref, &children),
        selection: extract_selection_context(window_ref, &children),
        navigation: extract_navigation_context(window_ref, &children),
        content: extract_content_context(app_ref, window_ref, &children),
        timestamp: unix_now(),
    }
}

/// Register a callback that fires whenever the app's `CopilotState` changes.
///
/// The current implementation polls at `interval_ms` milliseconds using a
/// background thread. A future version should use `AXObserver` notifications.
///
/// Returns a handle that stops the watcher when dropped.
///
/// # Arguments
///
/// * `app_ref`     – application element (must outlive the returned handle)
/// * `interval_ms` – polling interval in milliseconds (minimum 50 ms)
/// * `callback`    – called on every detected change, receives the new state
///   and the diff from the previous state
pub fn watch_state_changes<F>(app_ref: AXUIElementRef, interval_ms: u64, callback: F) -> WatchHandle
where
    F: Fn(CopilotState, Vec<StateChange>) + Send + 'static,
{
    // Safety: AXUIElementRef is a raw pointer. The caller guarantees lifetime.
    let ptr = app_ref as usize;
    let interval = std::time::Duration::from_millis(interval_ms.max(50));

    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop_clone = std::sync::Arc::clone(&stop);

    std::thread::spawn(move || {
        // SAFETY: pointer was valid when passed; we trust the caller's lifetime
        // guarantee. All attribute reads are null-safe internally.
        let element = ptr as AXUIElementRef;
        let mut prev = read_copilot_state(element);

        while !stop_clone.load(std::sync::atomic::Ordering::Relaxed) {
            std::thread::sleep(interval);
            let current = read_copilot_state(element);
            let changes = diff_states(&prev, &current);
            if !changes.is_empty() {
                callback(current.clone(), changes);
                prev = current;
            }
        }
    });

    WatchHandle { stop }
}

/// RAII handle that stops the background watcher when dropped.
pub struct WatchHandle {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Drop for WatchHandle {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------------------
// Diff helpers
// ---------------------------------------------------------------------------

fn diff_opt_str(
    prefix: &str,
    old: &Option<String>,
    new: &Option<String>,
    changes: &mut Vec<StateChange>,
) {
    if old != new {
        changes.push(StateChange::new(prefix, old, new));
    }
}

fn diff_app(old: &AppContext, new: &AppContext, changes: &mut Vec<StateChange>) {
    diff_opt_str("app.name", &old.name, &new.name, changes);
    diff_opt_str(
        "app.focused_window",
        &old.focused_window,
        &new.focused_window,
        changes,
    );
    diff_opt_str("app.active_tab", &old.active_tab, &new.active_tab, changes);
    diff_opt_str(
        "app.active_document",
        &old.active_document,
        &new.active_document,
        changes,
    );
}

fn diff_selection(old: &SelectionContext, new: &SelectionContext, changes: &mut Vec<StateChange>) {
    diff_opt_str(
        "selection.selected_text",
        &old.selected_text,
        &new.selected_text,
        changes,
    );
    if old.selected_list_row != new.selected_list_row {
        changes.push(StateChange::new(
            "selection.selected_list_row",
            &old.selected_list_row,
            &new.selected_list_row,
        ));
    }
    if old.selected_items != new.selected_items {
        changes.push(StateChange::new(
            "selection.selected_items",
            &old.selected_items,
            &new.selected_items,
        ));
    }
}

fn diff_navigation(
    old: &NavigationContext,
    new: &NavigationContext,
    changes: &mut Vec<StateChange>,
) {
    if old.breadcrumb != new.breadcrumb {
        changes.push(StateChange::new(
            "navigation.breadcrumb",
            &old.breadcrumb,
            &new.breadcrumb,
        ));
    }
    diff_opt_str(
        "navigation.sidebar_selection",
        &old.sidebar_selection,
        &new.sidebar_selection,
        changes,
    );
    diff_opt_str(
        "navigation.tab_bar_selection",
        &old.tab_bar_selection,
        &new.tab_bar_selection,
        changes,
    );
    if old.depth != new.depth {
        changes.push(StateChange::new("navigation.depth", &old.depth, &new.depth));
    }
}

fn diff_content(old: &ContentContext, new: &ContentContext, changes: &mut Vec<StateChange>) {
    diff_opt_str(
        "content.document_title",
        &old.document_title,
        &new.document_title,
        changes,
    );
    diff_opt_str(
        "content.visible_text_excerpt",
        &old.visible_text_excerpt,
        &new.visible_text_excerpt,
        changes,
    );
    if old.form_fields != new.form_fields {
        changes.push(StateChange::new(
            "content.form_fields",
            &old.form_fields,
            &new.form_fields,
        ));
    }
    diff_opt_str(
        "content.focused_element_role",
        &old.focused_element_role,
        &new.focused_element_role,
        changes,
    );
    diff_opt_str(
        "content.focused_element_title",
        &old.focused_element_title,
        &new.focused_element_title,
        changes,
    );
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Truncate a string to at most `max_chars` Unicode scalar values.
///
/// Exposed as `pub(crate)` so extraction helpers in sibling modules can share
/// the same implementation without duplicating it.
#[must_use]
pub(crate) fn truncate_str(s: String, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s
    } else {
        s.chars().take(max_chars).collect()
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- CopilotState::empty -------------------------------------------------

    #[test]
    fn empty_state_has_default_contexts() {
        // GIVEN / WHEN: empty state
        let state = CopilotState::empty();

        // THEN: all contexts are default
        assert!(state.app.name.is_none());
        assert!(state.selection.selected_items.is_empty());
        assert!(state.navigation.breadcrumb.is_empty());
        assert!(state.content.form_fields.is_empty());
    }

    #[test]
    fn empty_state_has_nonzero_timestamp() {
        // GIVEN / WHEN
        let state = CopilotState::empty();

        // THEN: timestamp is set (Unix epoch was 2001 for macOS but >0)
        assert!(state.timestamp > 0);
    }

    // -- diff_states: no change ---------------------------------------------

    #[test]
    fn diff_identical_states_produces_empty_vec() {
        // GIVEN: two identical states
        let s = CopilotState::empty();

        // WHEN
        let changes = diff_states(&s, &s);

        // THEN
        assert!(changes.is_empty());
    }

    // -- diff_states: AppContext changes ------------------------------------

    #[test]
    fn diff_detects_app_name_change() {
        // GIVEN
        let mut old = CopilotState::empty();
        let mut new = CopilotState::empty();
        old.app.name = None;
        new.app.name = Some("Safari".to_owned());

        // WHEN
        let changes = diff_states(&old, &new);

        // THEN
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "app.name");
        assert!(changes[0].new_value.contains("Safari"));
    }

    #[test]
    fn diff_detects_focused_window_change() {
        // GIVEN
        let mut old = CopilotState::empty();
        let mut new = CopilotState::empty();
        old.app.focused_window = Some("Window A".to_owned());
        new.app.focused_window = Some("Window B".to_owned());

        // WHEN
        let changes = diff_states(&old, &new);

        // THEN
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "app.focused_window");
    }

    #[test]
    fn diff_detects_active_tab_change() {
        // GIVEN
        let old = CopilotState::empty();
        let mut new = CopilotState::empty();
        new.app.active_tab = Some("Tab 2".to_owned());

        // WHEN / THEN
        let changes = diff_states(&old, &new);
        assert!(changes.iter().any(|c| c.field == "app.active_tab"));
    }

    // -- diff_states: SelectionContext changes ------------------------------

    #[test]
    fn diff_detects_selected_text_change() {
        // GIVEN
        let mut old = CopilotState::empty();
        let mut new = CopilotState::empty();
        old.selection.selected_text = Some("hello".to_owned());
        new.selection.selected_text = Some("hello world".to_owned());

        // WHEN
        let changes = diff_states(&old, &new);

        // THEN
        assert!(changes.iter().any(|c| c.field == "selection.selected_text"));
    }

    #[test]
    fn diff_detects_selected_list_row_change() {
        // GIVEN
        let mut old = CopilotState::empty();
        let mut new = CopilotState::empty();
        old.selection.selected_list_row = Some(0);
        new.selection.selected_list_row = Some(5);

        // WHEN / THEN
        let changes = diff_states(&old, &new);
        assert!(
            changes
                .iter()
                .any(|c| c.field == "selection.selected_list_row")
        );
    }

    #[test]
    fn diff_detects_selected_items_change() {
        // GIVEN
        let old = CopilotState::empty();
        let mut new = CopilotState::empty();
        new.selection.selected_items = vec!["Item A".to_owned()];

        // WHEN / THEN
        let changes = diff_states(&old, &new);
        assert!(
            changes
                .iter()
                .any(|c| c.field == "selection.selected_items")
        );
    }

    // -- diff_states: NavigationContext changes -----------------------------

    #[test]
    fn diff_detects_breadcrumb_change() {
        // GIVEN
        let old = CopilotState::empty();
        let mut new = CopilotState::empty();
        new.navigation.breadcrumb = vec!["Home".to_owned(), "Settings".to_owned()];

        // WHEN / THEN
        let changes = diff_states(&old, &new);
        assert!(changes.iter().any(|c| c.field == "navigation.breadcrumb"));
    }

    #[test]
    fn diff_detects_depth_change() {
        // GIVEN
        let mut old = CopilotState::empty();
        let mut new = CopilotState::empty();
        old.navigation.depth = 0;
        new.navigation.depth = 4;

        // WHEN / THEN
        let changes = diff_states(&old, &new);
        assert!(changes.iter().any(|c| c.field == "navigation.depth"));
    }

    // -- diff_states: ContentContext changes --------------------------------

    #[test]
    fn diff_detects_document_title_change() {
        // GIVEN
        let mut old = CopilotState::empty();
        let mut new = CopilotState::empty();
        old.content.document_title = Some("Old Doc".to_owned());
        new.content.document_title = Some("New Doc".to_owned());

        // WHEN / THEN
        let changes = diff_states(&old, &new);
        assert!(changes.iter().any(|c| c.field == "content.document_title"));
    }

    #[test]
    fn diff_detects_form_fields_change() {
        // GIVEN
        let old = CopilotState::empty();
        let mut new = CopilotState::empty();
        new.content.form_fields = vec![("Email".to_owned(), "a@b.com".to_owned())];

        // WHEN / THEN
        let changes = diff_states(&old, &new);
        assert!(changes.iter().any(|c| c.field == "content.form_fields"));
    }

    #[test]
    fn diff_multiple_simultaneous_changes() {
        // GIVEN: several fields changed at once
        let old = CopilotState::empty();
        let mut new = CopilotState::empty();
        new.app.name = Some("Finder".to_owned());
        new.navigation.depth = 3;
        new.content.document_title = Some("Documents".to_owned());

        // WHEN
        let changes = diff_states(&old, &new);

        // THEN: three changes reported
        assert_eq!(changes.len(), 3);
    }

    // -- StateChange field path --------------------------------------------

    #[test]
    fn state_change_carries_old_and_new_json() {
        // GIVEN
        let mut old = CopilotState::empty();
        let mut new = CopilotState::empty();
        old.app.name = Some("Before".to_owned());
        new.app.name = Some("After".to_owned());

        // WHEN
        let changes = diff_states(&old, &new);

        // THEN
        assert_eq!(changes.len(), 1);
        assert!(changes[0].old_value.contains("Before"));
        assert!(changes[0].new_value.contains("After"));
    }

    // -- Serialisation ------------------------------------------------------

    #[test]
    fn copilot_state_serialises_to_json() {
        // GIVEN
        let mut state = CopilotState::empty();
        state.app.name = Some("Xcode".to_owned());
        state.navigation.breadcrumb = vec!["Project".to_owned()];

        // WHEN
        let json = serde_json::to_string(&state).unwrap();

        // THEN: key fields present
        assert!(json.contains("Xcode"));
        assert!(json.contains("Project"));
    }

    #[test]
    fn copilot_state_round_trips_through_json() {
        // GIVEN
        let mut state = CopilotState::empty();
        state.app.name = Some("TextEdit".to_owned());
        state.content.form_fields = vec![("Name".to_owned(), "Alice".to_owned())];

        // WHEN
        let json = serde_json::to_string(&state).unwrap();
        let restored: CopilotState = serde_json::from_str(&json).unwrap();

        // THEN: round-trip preserves data (timestamps may differ)
        assert_eq!(restored.app.name, state.app.name);
        assert_eq!(restored.content.form_fields, state.content.form_fields);
    }

    // -- Utilities ----------------------------------------------------------

    #[test]
    fn truncate_short_string_unchanged() {
        // GIVEN / WHEN / THEN
        assert_eq!(truncate_str("hello".to_owned(), 10), "hello");
    }

    #[test]
    fn truncate_long_string_capped_at_max() {
        // GIVEN
        let s = "a".repeat(100);

        // WHEN
        let result = truncate_str(s, 20);

        // THEN
        assert_eq!(result.chars().count(), 20);
    }

    #[test]
    fn unix_now_returns_reasonable_timestamp() {
        // GIVEN / WHEN
        let ts = unix_now();

        // THEN: must be > 2020-01-01 (1577836800) and < 2100-01-01 (4102444800)
        assert!(ts > 1_577_836_800);
        assert!(ts < 4_102_444_800);
    }

    // -- read_copilot_state with null ref -----------------------------------

    #[test]
    fn read_copilot_state_null_ref_returns_empty() {
        // GIVEN: null element (accessibility may or may not be enabled)
        let null_ref: AXUIElementRef = std::ptr::null();

        // WHEN: extracting state – must not crash
        let state = read_copilot_state(null_ref);

        // THEN: state is populated with empty/None fields
        assert!(state.app.name.is_none());
    }
}
