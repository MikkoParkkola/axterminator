//! Persistent element references for `AXTerminator`
//!
//! Assigns stable `ref_N` IDs to accessibility elements so multi-step automation
//! workflows can target elements by reference instead of re-walking the a11y tree
//! on every MCP tool call — mirroring Claude's `window.__claudeElementMap` pattern
//! from the Chrome extension.
//!
//! # Design
//!
//! A [`RefStore`] maintains an in-memory map from auto-incremented IDs to
//! [`ElementRef`] descriptors.  The raw `AXUIElementRef` handle is **not** stored
//! (it would become dangling if the element is destroyed); instead a stable
//! *fingerprint* — a hash of `(role, label, path)` — allows re-identification
//! after a tree refresh.
//!
//! # Usage
//!
//! ```rust,no_run
//! use axterminator::persistent_refs::{ElementSnapshot, RefStore};
//!
//! let mut store = RefStore::new();
//!
//! let snap = ElementSnapshot {
//!     role: "AXButton".into(),
//!     label: "Save".into(),
//!     path: vec!["AXWindow:Document".into(), "AXGroup".into(), "AXButton:Save".into()],
//!     bounds: (100.0, 200.0, 80.0, 30.0),
//! };
//!
//! let elem_ref = store.track(snap);
//! assert!(elem_ref.label.contains("Save"));
//!
//! let resolved = store.resolve(elem_ref.id);
//! assert!(resolved.is_some());
//! ```

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Public data types
// ---------------------------------------------------------------------------

/// Bounds rectangle: `(x, y, width, height)` in screen points.
pub type Rect = (f64, f64, f64, f64);

/// A point-in-time snapshot of an element's identifying attributes.
///
/// This is the *input* to [`RefStore::track`] — a plain data bag that callers
/// construct from whatever a11y attributes they have available.
#[derive(Debug, Clone, PartialEq)]
pub struct ElementSnapshot {
    /// Accessibility role, e.g. `"AXButton"`.
    pub role: String,
    /// Human-readable label (title / description / value, whichever is non-empty).
    pub label: String,
    /// Hierarchy path from root to this element, e.g.
    /// `["AXWindow:Document", "AXToolbar", "AXButton:Save"]`.
    pub path: Vec<String>,
    /// Last-known screen bounds `(x, y, width, height)`.
    pub bounds: Rect,
}

/// A persistent reference to an accessibility element.
///
/// The reference survives app state changes as long as the element's
/// *fingerprint* (role + label + path) remains stable.  If the element is
/// destroyed the `alive` flag is set to `false` after the next [`RefStore::refresh`].
#[derive(Debug, Clone, PartialEq)]
pub struct ElementRef {
    /// Stable numeric ID assigned at tracking time (`ref_1`, `ref_2`, …).
    pub id: u64,
    /// Accessibility role.
    pub role: String,
    /// Human-readable label.
    pub label: String,
    /// Hierarchy path from root.
    pub path: Vec<String>,
    /// Last-known screen bounds.
    pub bounds: Rect,
    /// Stable fingerprint: `hash(role || label || path)`.
    pub fingerprint: u64,
    /// Whether the element was found in the most recent [`RefStore::refresh`].
    pub alive: bool,
    /// Unix timestamp (nanoseconds) of the last observed-alive moment.
    pub last_seen_ns: u64,
    /// How many consecutive refreshes the element has been absent.
    pub missed_refreshes: u32,
}

impl ElementRef {
    /// Returns the `ref_N` display string used in tool output.
    #[must_use]
    pub fn ref_name(&self) -> String {
        format!("ref_{}", self.id)
    }
}

// ---------------------------------------------------------------------------
// RefStore
// ---------------------------------------------------------------------------

/// Maximum missed-refresh count before [`RefStore::gc`] removes a ref.
pub const GC_THRESHOLD: u32 = 3;

/// Stores and manages persistent references across MCP tool calls.
///
/// Thread-safety: [`RefStore`] is not `Sync` by default.  Wrap in a `Mutex`
/// or `RwLock` if sharing across threads (e.g. for a session-scoped global).
pub struct RefStore {
    refs: HashMap<u64, ElementRef>,
    /// Secondary index: fingerprint -> `ref_id` for O(1) re-identification.
    by_fingerprint: HashMap<u64, u64>,
    next_id: u64,
}

impl RefStore {
    /// Create an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            refs: HashMap::new(),
            by_fingerprint: HashMap::new(),
            next_id: 1,
        }
    }

    /// Track an element, returning a (possibly updated) [`ElementRef`].
    ///
    /// - If the fingerprint already exists, the existing ref is updated with the
    ///   new bounds and marked alive.
    /// - Otherwise a new ref is created and assigned the next sequential ID.
    pub fn track(&mut self, snap: ElementSnapshot) -> ElementRef {
        let fp = fingerprint(&snap.role, &snap.label, &snap.path);
        let now = unix_ns();

        if let Some(&existing_id) = self.by_fingerprint.get(&fp) {
            // Update bounds + liveness on existing ref.
            if let Some(entry) = self.refs.get_mut(&existing_id) {
                entry.bounds = snap.bounds;
                entry.alive = true;
                entry.last_seen_ns = now;
                entry.missed_refreshes = 0;
                return entry.clone();
            }
        }

        // New element — assign next ID.
        let id = self.next_id;
        self.next_id += 1;

        let elem_ref = ElementRef {
            id,
            role: snap.role,
            label: snap.label,
            path: snap.path,
            bounds: snap.bounds,
            fingerprint: fp,
            alive: true,
            last_seen_ns: now,
            missed_refreshes: 0,
        };

        self.by_fingerprint.insert(fp, id);
        self.refs.insert(id, elem_ref.clone());
        elem_ref
    }

    /// Resolve a ref ID to its [`ElementRef`], if it exists.
    #[must_use]
    pub fn resolve(&self, ref_id: u64) -> Option<&ElementRef> {
        self.refs.get(&ref_id)
    }

    /// Resolve a ref by its stable fingerprint, if it exists.
    ///
    /// This is the primary lookup used by [`crate::recording::WorkflowPlayer`]
    /// for exact-match replay: if the element's role+label+path hash matches a
    /// live ref, return it directly without a label search.
    #[must_use]
    pub fn resolve_by_fingerprint(&self, fingerprint: u64) -> Option<&ElementRef> {
        self.by_fingerprint
            .get(&fingerprint)
            .and_then(|id| self.refs.get(id))
    }

    /// Find refs whose label contains `needle` (case-insensitive).
    #[must_use]
    pub fn find_by_label(&self, needle: &str) -> Vec<&ElementRef> {
        let needle_lower = needle.to_lowercase();
        self.refs
            .values()
            .filter(|r| r.label.to_lowercase().contains(&needle_lower))
            .collect()
    }

    /// Update liveness of all tracked refs against a fresh set of snapshots.
    ///
    /// Elements whose fingerprint appears in `current_elements` are marked alive;
    /// all others have their `missed_refreshes` incremented and `alive` set to
    /// `false`.
    pub fn refresh(&mut self, current_elements: &[ElementSnapshot]) {
        // Build a set of fingerprints present in the current tree.
        let live_fps: HashMap<u64, &ElementSnapshot> = current_elements
            .iter()
            .map(|s| (fingerprint(&s.role, &s.label, &s.path), s))
            .collect();

        let now = unix_ns();

        for entry in self.refs.values_mut() {
            if let Some(snap) = live_fps.get(&entry.fingerprint) {
                entry.bounds = snap.bounds;
                entry.alive = true;
                entry.last_seen_ns = now;
                entry.missed_refreshes = 0;
            } else {
                entry.alive = false;
                entry.missed_refreshes = entry.missed_refreshes.saturating_add(1);
            }
        }
    }

    /// Remove refs that have been absent for more than [`GC_THRESHOLD`]
    /// consecutive refreshes.
    ///
    /// Returns the number of refs removed.
    pub fn gc(&mut self) -> usize {
        let stale_ids: Vec<u64> = self
            .refs
            .values()
            .filter(|r| r.missed_refreshes >= GC_THRESHOLD)
            .map(|r| r.id)
            .collect();

        for id in &stale_ids {
            if let Some(removed) = self.refs.remove(id) {
                self.by_fingerprint.remove(&removed.fingerprint);
            }
        }

        stale_ids.len()
    }

    /// Total number of tracked refs (alive + stale).
    #[must_use]
    pub fn len(&self) -> usize {
        self.refs.len()
    }

    /// Whether the store has no tracked refs.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.refs.is_empty()
    }

    /// Number of refs currently marked alive.
    #[must_use]
    pub fn alive_count(&self) -> usize {
        self.refs.values().filter(|r| r.alive).count()
    }
}

impl Default for RefStore {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Session-scoped global store
// ---------------------------------------------------------------------------

/// Global session-scoped [`RefStore`] protected by a `Mutex`.
///
/// This persists across MCP tool calls for the lifetime of the process.
static GLOBAL_REF_STORE: std::sync::OnceLock<std::sync::Mutex<RefStore>> =
    std::sync::OnceLock::new();

/// Obtain a lock on the global [`RefStore`].
///
/// # Panics
///
/// Panics if the mutex is poisoned (which only happens on a thread panic while
/// holding the lock — effectively unrecoverable).
pub fn global_ref_store() -> std::sync::MutexGuard<'static, RefStore> {
    GLOBAL_REF_STORE
        .get_or_init(|| std::sync::Mutex::new(RefStore::new()))
        .lock()
        .expect("global RefStore mutex poisoned")
}

// ---------------------------------------------------------------------------
// Counter for ref IDs (used by tests to verify monotonicity)
// ---------------------------------------------------------------------------

/// Atomic counter exposed so callers can peek at the next-to-be-assigned ID.
/// Only meaningful on the *global* store; local stores manage their own counter.
static GLOBAL_NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Reserve the next available ref ID from the global counter.
///
/// This is a monotonically increasing sequence — IDs are never reused.
pub fn next_global_ref_id() -> u64 {
    GLOBAL_NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Compute a stable fingerprint for `(role, label, path)`.
///
/// Uses [`std::collections::hash_map::DefaultHasher`] — zero external deps,
/// zero runtime overhead, deterministic within a single process invocation.
fn fingerprint(role: &str, label: &str, path: &[String]) -> u64 {
    use std::collections::hash_map::DefaultHasher;

    let mut h = DefaultHasher::new();
    role.hash(&mut h);
    label.hash(&mut h);
    path.hash(&mut h);
    h.finish()
}

/// Current time as nanoseconds since the Unix epoch.
///
/// Truncation from `u128` to `u64` is intentional: nanoseconds since the Unix
/// epoch fit in a `u64` until approximately year 2554.
#[allow(clippy::cast_possible_truncation)]
fn unix_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn button_snap(label: &str) -> ElementSnapshot {
        ElementSnapshot {
            role: "AXButton".into(),
            label: label.into(),
            path: vec!["AXWindow:Document".into(), format!("AXButton:{label}")],
            bounds: (10.0, 20.0, 80.0, 30.0),
        }
    }

    fn textfield_snap(label: &str) -> ElementSnapshot {
        ElementSnapshot {
            role: "AXTextField".into(),
            label: label.into(),
            path: vec!["AXWindow:Document".into(), format!("AXTextField:{label}")],
            bounds: (10.0, 60.0, 200.0, 24.0),
        }
    }

    // -----------------------------------------------------------------------
    // Tests
    // -----------------------------------------------------------------------

    #[test]
    fn track_new_element_assigns_sequential_id() {
        // GIVEN: Empty store
        let mut store = RefStore::new();

        // WHEN: Tracking two elements
        let r1 = store.track(button_snap("Save"));
        let r2 = store.track(button_snap("Cancel"));

        // THEN: IDs are assigned sequentially starting from 1
        assert_eq!(r1.id, 1);
        assert_eq!(r2.id, 2);
    }

    #[test]
    fn track_same_fingerprint_returns_existing_id() {
        // GIVEN: Store with one tracked element
        let mut store = RefStore::new();
        let first = store.track(button_snap("Save"));

        // WHEN: Tracking the same logical element again
        let second = store.track(button_snap("Save"));

        // THEN: Same ID is reused (no new ref created)
        assert_eq!(first.id, second.id);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn track_updates_bounds_on_revisit() {
        // GIVEN: Tracked element with initial bounds
        let mut store = RefStore::new();
        store.track(button_snap("Save"));

        // WHEN: Tracking same element with moved bounds
        let moved = ElementSnapshot {
            bounds: (999.0, 888.0, 80.0, 30.0),
            ..button_snap("Save")
        };
        let updated = store.track(moved);

        // THEN: Bounds reflect the new position
        assert_eq!(updated.bounds.0, 999.0);
        assert_eq!(updated.bounds.1, 888.0);
    }

    #[test]
    fn resolve_returns_none_for_unknown_id() {
        // GIVEN: Empty store
        let store = RefStore::new();

        // WHEN: Resolving non-existent ID
        let result = store.resolve(42);

        // THEN: None returned
        assert!(result.is_none());
    }

    #[test]
    fn resolve_returns_tracked_element() {
        // GIVEN: Store with one element
        let mut store = RefStore::new();
        let tracked = store.track(button_snap("OK"));

        // WHEN: Resolving by ID
        let resolved = store.resolve(tracked.id);

        // THEN: Element is found and matches
        assert!(resolved.is_some());
        assert_eq!(resolved.unwrap().label, "OK");
        assert_eq!(resolved.unwrap().role, "AXButton");
    }

    #[test]
    fn find_by_label_is_case_insensitive() {
        // GIVEN: Store with mixed-case labels
        let mut store = RefStore::new();
        store.track(button_snap("Save Document"));
        store.track(button_snap("Cancel"));
        store.track(textfield_snap("save path"));

        // WHEN: Searching for lowercase "save"
        let mut results = store.find_by_label("save");
        results.sort_by_key(|r| r.id);

        // THEN: Both "Save Document" and "save path" match; "Cancel" does not
        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|r| r.label == "Save Document"));
        assert!(results.iter().any(|r| r.label == "save path"));
    }

    #[test]
    fn find_by_label_returns_empty_when_no_match() {
        // GIVEN: Store with unrelated labels
        let mut store = RefStore::new();
        store.track(button_snap("OK"));

        // WHEN: Searching for non-existent label
        let results = store.find_by_label("nonexistent");

        // THEN: Empty result
        assert!(results.is_empty());
    }

    #[test]
    fn refresh_marks_missing_elements_not_alive() {
        // GIVEN: Store with two elements
        let mut store = RefStore::new();
        let save_ref = store.track(button_snap("Save"));
        let cancel_ref = store.track(button_snap("Cancel"));

        // WHEN: Refreshing with only "Save" present
        store.refresh(&[button_snap("Save")]);

        // THEN: "Save" is alive, "Cancel" is not
        assert!(store.resolve(save_ref.id).unwrap().alive);
        assert!(!store.resolve(cancel_ref.id).unwrap().alive);
    }

    #[test]
    fn refresh_increments_missed_refreshes_for_absent_elements() {
        // GIVEN: Store with one element, refreshed with empty tree
        let mut store = RefStore::new();
        let r = store.track(button_snap("Vanished"));

        // WHEN: Two consecutive refreshes without the element
        store.refresh(&[]);
        store.refresh(&[]);

        // THEN: missed_refreshes counts both absences
        assert_eq!(store.resolve(r.id).unwrap().missed_refreshes, 2);
    }

    #[test]
    fn refresh_resets_missed_refreshes_when_element_reappears() {
        // GIVEN: Element that was absent for one refresh
        let mut store = RefStore::new();
        let r = store.track(button_snap("Flicker"));
        store.refresh(&[]); // missed_refreshes = 1

        // WHEN: Element reappears in next refresh
        store.refresh(&[button_snap("Flicker")]);

        // THEN: missed_refreshes reset to 0
        assert_eq!(store.resolve(r.id).unwrap().missed_refreshes, 0);
        assert!(store.resolve(r.id).unwrap().alive);
    }

    #[test]
    fn gc_removes_elements_exceeding_threshold() {
        // GIVEN: Store with one element absent for GC_THRESHOLD refreshes
        let mut store = RefStore::new();
        let r = store.track(button_snap("Stale"));

        for _ in 0..GC_THRESHOLD {
            store.refresh(&[]);
        }

        // WHEN: Running GC
        let removed = store.gc();

        // THEN: Stale element removed, store is empty
        assert_eq!(removed, 1);
        assert!(store.is_empty());
        assert!(store.resolve(r.id).is_none());
    }

    #[test]
    fn gc_does_not_remove_live_or_below_threshold_elements() {
        // GIVEN: Store with a live element and one with only 1 missed refresh
        let mut store = RefStore::new();
        let live = store.track(button_snap("Active"));
        let near_stale = store.track(button_snap("NearStale"));

        // Miss one refresh for NearStale (threshold is 3)
        store.refresh(&[button_snap("Active")]);

        // WHEN: Running GC
        let removed = store.gc();

        // THEN: Nothing removed yet
        assert_eq!(removed, 0);
        assert!(store.resolve(live.id).is_some());
        assert!(store.resolve(near_stale.id).is_some());
    }

    #[test]
    fn fingerprint_differs_for_different_role() {
        // GIVEN: Two snapshots identical except for role
        let fp1 = fingerprint("AXButton", "OK", &["AXWindow".to_string()]);
        let fp2 = fingerprint("AXTextField", "OK", &["AXWindow".to_string()]);

        // THEN: Fingerprints differ
        assert_ne!(fp1, fp2);
    }

    #[test]
    fn fingerprint_differs_for_different_label() {
        let fp1 = fingerprint("AXButton", "Save", &["AXWindow".to_string()]);
        let fp2 = fingerprint("AXButton", "Cancel", &["AXWindow".to_string()]);
        assert_ne!(fp1, fp2);
    }

    #[test]
    fn fingerprint_differs_for_different_path() {
        let fp1 = fingerprint("AXButton", "OK", &["AXWindow:A".to_string()]);
        let fp2 = fingerprint("AXButton", "OK", &["AXWindow:B".to_string()]);
        assert_ne!(fp1, fp2);
    }

    #[test]
    fn fingerprint_is_stable_for_same_inputs() {
        let fp1 = fingerprint(
            "AXButton",
            "Save",
            &["AXWindow".to_string(), "AXToolbar".to_string()],
        );
        let fp2 = fingerprint(
            "AXButton",
            "Save",
            &["AXWindow".to_string(), "AXToolbar".to_string()],
        );
        assert_eq!(fp1, fp2);
    }

    #[test]
    fn ref_name_formats_correctly() {
        // GIVEN: A ref with id=7
        let elem_ref = ElementRef {
            id: 7,
            role: "AXButton".into(),
            label: "Save".into(),
            path: vec![],
            bounds: (0.0, 0.0, 0.0, 0.0),
            fingerprint: 0,
            alive: true,
            last_seen_ns: 0,
            missed_refreshes: 0,
        };

        // THEN: ref_name returns "ref_7"
        assert_eq!(elem_ref.ref_name(), "ref_7");
    }

    #[test]
    fn alive_count_tracks_only_live_elements() {
        // GIVEN: Store with 3 elements, then refresh removes 1
        let mut store = RefStore::new();
        store.track(button_snap("A"));
        store.track(button_snap("B"));
        store.track(button_snap("C"));

        store.refresh(&[button_snap("A"), button_snap("B")]);

        // THEN: alive_count == 2, len == 3
        assert_eq!(store.alive_count(), 2);
        assert_eq!(store.len(), 3);
    }

    #[test]
    fn store_default_is_empty() {
        let store = RefStore::default();
        assert!(store.is_empty());
        assert_eq!(store.alive_count(), 0);
    }

    #[test]
    fn gc_cleans_secondary_fingerprint_index() {
        // GIVEN: Element tracked, then removed by GC
        let mut store = RefStore::new();
        let snap = button_snap("Ephemeral");
        store.track(snap.clone());

        for _ in 0..GC_THRESHOLD {
            store.refresh(&[]);
        }
        store.gc();

        // WHEN: Re-tracking the same element
        let retracked = store.track(snap);

        // THEN: Gets a fresh ID (fingerprint index was cleared)
        assert_eq!(retracked.id, 2); // id=1 was used before GC, id=2 is next
    }
}
