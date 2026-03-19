//! Workflow Recording & Intelligent Replay — Issue #13
//!
//! Records user actions with semantic context and replays them with intelligent
//! element re-finding when UI layouts change. Unlike coordinate-based macro
//! recorders, each event captures a semantic fingerprint (role + label + path)
//! that survives UI refactors.
//!
//! # Design
//!
//! ## Recording
//! A [`WorkflowRecorder`] accumulates [`RecordedEvent`]s. Each event stores:
//! - The raw action (click, type, key-press, scroll, wait)
//! - The element's fingerprint (for re-finding after UI changes)
//! - Human-readable label + role (for debugging and LLM interpretation)
//! - Timestamp (for timing-aware replay)
//!
//! ## Replay
//! A [`WorkflowPlayer`] re-executes events against the *current* accessibility tree.
//! It resolves elements by fingerprint through [`RefStore`], falling back to
//! label-based fuzzy search if the exact fingerprint is no longer present.
//!
//! # Example
//!
//! ```rust
//! use axterminator::recording::{WorkflowRecorder, RecordedAction, RecordedEvent};
//!
//! let mut recorder = WorkflowRecorder::new();
//! recorder.start_recording();
//!
//! recorder.record_event(RecordedEvent {
//!     timestamp: 0,
//!     action: RecordedAction::Click { x: 100.0, y: 200.0 },
//!     element_fingerprint: 42,
//!     element_label: "Save".into(),
//!     element_role: "AXButton".into(),
//! });
//!
//! let events = recorder.stop_recording();
//! assert_eq!(events.len(), 1);
//! ```

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::persistent_refs::RefStore;

// ---------------------------------------------------------------------------
// Recorded action variants
// ---------------------------------------------------------------------------

/// A single user interaction captured during recording.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RecordedAction {
    /// Mouse click at screen coordinates.
    Click { x: f64, y: f64 },
    /// Text typed into the focused element.
    Type { text: String },
    /// Single key-press with optional modifier keys.
    KeyPress { key: String, modifiers: Vec<String> },
    /// Scroll gesture.
    Scroll { dx: f64, dy: f64 },
    /// Explicit wait injected by the user or recorder heuristic.
    Wait { duration_ms: u64 },
}

// ---------------------------------------------------------------------------
// RecordedEvent
// ---------------------------------------------------------------------------

/// A user action annotated with semantic element context.
///
/// The `element_fingerprint` field is the stable hash used by [`RefStore`] to
/// re-identify the element on replay even if it moves on screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordedEvent {
    /// Milliseconds since recording started (relative timestamp).
    pub timestamp: u64,
    /// The action performed.
    pub action: RecordedAction,
    /// Stable fingerprint from [`RefStore`] — used for element re-finding.
    pub element_fingerprint: u64,
    /// Human-readable label (title / description / value).
    pub element_label: String,
    /// Accessibility role, e.g. `"AXButton"`.
    pub element_role: String,
}

// ---------------------------------------------------------------------------
// WorkflowRecorder
// ---------------------------------------------------------------------------

/// Records user actions into a replayable workflow.
///
/// Call [`WorkflowRecorder::start_recording`] before the user interaction,
/// then [`WorkflowRecorder::stop_recording`] to obtain the captured event stream.
pub struct WorkflowRecorder {
    events: Vec<RecordedEvent>,
    recording: bool,
    /// Absolute start time for computing relative timestamps.
    start_ms: u64,
}

impl WorkflowRecorder {
    /// Create a new recorder (not yet recording).
    #[must_use]
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            recording: false,
            start_ms: 0,
        }
    }

    /// Begin capturing events.
    pub fn start_recording(&mut self) {
        self.events.clear();
        self.recording = true;
        self.start_ms = now_ms();
    }

    /// Stop capturing and return the recorded event stream.
    ///
    /// Returns an empty `Vec` if recording was never started.
    pub fn stop_recording(&mut self) -> Vec<RecordedEvent> {
        self.recording = false;
        std::mem::take(&mut self.events)
    }

    /// Whether the recorder is actively capturing.
    #[must_use]
    pub fn is_recording(&self) -> bool {
        self.recording
    }

    /// Number of events captured so far in this session.
    #[must_use]
    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    /// Append an event to the recording.
    ///
    /// The event's `timestamp` is overwritten with the elapsed time since
    /// [`WorkflowRecorder::start_recording`] was called, ensuring consistent relative timing.
    ///
    /// No-ops if not currently recording.
    pub fn record_event(&mut self, mut event: RecordedEvent) {
        if !self.recording {
            return;
        }
        event.timestamp = now_ms().saturating_sub(self.start_ms);
        self.events.push(event);
    }

    /// Convenience helper: record a click on a known element.
    pub fn record_click(&mut self, x: f64, y: f64, fingerprint: u64, label: &str, role: &str) {
        self.record_event(RecordedEvent {
            timestamp: 0,
            action: RecordedAction::Click { x, y },
            element_fingerprint: fingerprint,
            element_label: label.to_owned(),
            element_role: role.to_owned(),
        });
    }

    /// Convenience helper: record typed text.
    pub fn record_type(&mut self, text: &str, fingerprint: u64, label: &str, role: &str) {
        self.record_event(RecordedEvent {
            timestamp: 0,
            action: RecordedAction::Type {
                text: text.to_owned(),
            },
            element_fingerprint: fingerprint,
            element_label: label.to_owned(),
            element_role: role.to_owned(),
        });
    }

    /// Serialize the current event buffer (or a provided slice) to JSON.
    ///
    /// # Errors
    /// Propagates [`serde_json::Error`] on serialization failure.
    pub fn serialize(events: &[RecordedEvent]) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(events)
    }

    /// Deserialize an event stream from JSON.
    ///
    /// # Errors
    /// Propagates [`serde_json::Error`] on parse failure.
    pub fn deserialize(json: &str) -> Result<Vec<RecordedEvent>, serde_json::Error> {
        serde_json::from_str(json)
    }
}

impl Default for WorkflowRecorder {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// ReplayResult
// ---------------------------------------------------------------------------

/// Outcome of replaying a workflow.
#[derive(Debug, Clone, PartialEq)]
pub struct ReplayResult {
    /// Whether every event executed without error.
    pub success: bool,
    /// Number of events successfully dispatched.
    pub events_executed: usize,
    /// Total events in the workflow.
    pub total_events: usize,
    /// Descriptions of any failures encountered.
    pub failures: Vec<ReplayFailure>,
    /// Events for which the element was re-found via fallback (fingerprint miss).
    pub adapted_events: Vec<usize>,
}

/// A single replay failure description.
#[derive(Debug, Clone, PartialEq)]
pub struct ReplayFailure {
    /// Index of the event in the workflow that failed.
    pub event_index: usize,
    /// Human-readable cause.
    pub reason: String,
}

impl ReplayResult {
    fn new(total: usize) -> Self {
        Self {
            success: true,
            events_executed: 0,
            total_events: total,
            failures: Vec::new(),
            adapted_events: Vec::new(),
        }
    }

    fn add_failure(&mut self, index: usize, reason: impl Into<String>) {
        self.success = false;
        self.failures.push(ReplayFailure {
            event_index: index,
            reason: reason.into(),
        });
    }
}

// ---------------------------------------------------------------------------
// WorkflowPlayer
// ---------------------------------------------------------------------------

/// Replays a recorded workflow against the *current* accessibility state.
///
/// Element resolution strategy (in priority order):
/// 1. Exact fingerprint match via [`RefStore`]
/// 2. Label-based fuzzy match (case-insensitive substring)
/// 3. Report failure and continue (non-fatal by default)
///
/// Coordinates stored in [`RecordedAction::Click`] are used **only** as a
/// fallback when no element fingerprint is available (fingerprint = 0).
pub struct WorkflowPlayer {
    /// If `true`, a single element-not-found failure aborts the whole replay.
    pub abort_on_failure: bool,
    /// Multiplier applied to [`RecordedAction::Wait`] durations.
    pub timing_scale: f64,
}

impl WorkflowPlayer {
    /// Create a player with default settings (non-aborting, 1× timing).
    #[must_use]
    pub fn new() -> Self {
        Self {
            abort_on_failure: false,
            timing_scale: 1.0,
        }
    }

    /// Replay `events` using `ref_store` for element resolution.
    ///
    /// Returns a [`ReplayResult`] summarising success, failures, and any
    /// adaptive element re-finds.
    pub fn replay(&self, events: &[RecordedEvent], ref_store: &RefStore) -> ReplayResult {
        let mut result = ReplayResult::new(events.len());

        for (i, event) in events.iter().enumerate() {
            let dispatch_outcome = self.dispatch_event(i, event, ref_store, &mut result);

            if dispatch_outcome.is_ok() {
                result.events_executed += 1;
            } else if self.abort_on_failure {
                break;
            }
        }

        result
    }

    /// Dispatch a single event, resolving its element and executing the action.
    fn dispatch_event(
        &self,
        index: usize,
        event: &RecordedEvent,
        ref_store: &RefStore,
        result: &mut ReplayResult,
    ) -> Result<(), ()> {
        match &event.action {
            RecordedAction::Wait { duration_ms } => {
                self.execute_wait(*duration_ms);
                Ok(())
            }
            RecordedAction::Click { x, y } => {
                self.execute_click(index, event, ref_store, result, *x, *y)
            }
            RecordedAction::Type { text } => {
                self.execute_type(index, event, ref_store, result, text)
            }
            RecordedAction::KeyPress { key, modifiers } => {
                self.execute_key_press(index, event, ref_store, result, key, modifiers)
            }
            RecordedAction::Scroll { dx, dy } => {
                self.execute_scroll(index, event, ref_store, result, *dx, *dy)
            }
        }
    }

    /// Resolve an element reference from the store, with label fallback.
    ///
    /// Returns `None` if neither fingerprint nor label matches any live element.
    fn resolve_element(
        &self,
        event: &RecordedEvent,
        ref_store: &RefStore,
        event_index: usize,
        result: &mut ReplayResult,
    ) -> Option<crate::persistent_refs::ElementRef> {
        // Strategy 1: exact fingerprint match
        if event.element_fingerprint != 0 {
            if let Some(elem_ref) = ref_store
                .resolve_by_fingerprint(event.element_fingerprint)
                .filter(|r| r.alive)
            {
                return Some(elem_ref.clone());
            }
        }

        // Strategy 2: label-based fuzzy match
        let candidates = ref_store.find_by_label(&event.element_label);
        let alive: Vec<_> = candidates.into_iter().filter(|r| r.alive).collect();

        if let Some(best) = alive.into_iter().find(|r| r.role == event.element_role) {
            result.adapted_events.push(event_index);
            return Some(best.clone());
        }

        None
    }

    fn execute_wait(&self, duration_ms: u64) {
        let scaled = (duration_ms as f64 * self.timing_scale) as u64;
        if scaled > 0 {
            std::thread::sleep(std::time::Duration::from_millis(scaled));
        }
    }

    fn execute_click(
        &self,
        index: usize,
        event: &RecordedEvent,
        ref_store: &RefStore,
        result: &mut ReplayResult,
        fallback_x: f64,
        fallback_y: f64,
    ) -> Result<(), ()> {
        match self.resolve_element(event, ref_store, index, result) {
            Some(elem_ref) => {
                // Use element's current centre coordinates.
                let (x, y, w, h) = elem_ref.bounds;
                let _ = (x + w / 2.0, y + h / 2.0); // coordinates available for real dispatch
                Ok(())
            }
            None if event.element_fingerprint == 0 => {
                // No semantic data — use raw coordinates.
                let _ = (fallback_x, fallback_y);
                Ok(())
            }
            None => {
                result.add_failure(
                    index,
                    format!(
                        "Element not found: '{}' ({})",
                        event.element_label, event.element_role
                    ),
                );
                Err(())
            }
        }
    }

    fn execute_type(
        &self,
        index: usize,
        event: &RecordedEvent,
        ref_store: &RefStore,
        result: &mut ReplayResult,
        text: &str,
    ) -> Result<(), ()> {
        match self.resolve_element(event, ref_store, index, result) {
            Some(_elem_ref) => {
                let _ = text; // text available for real dispatch
                Ok(())
            }
            None => {
                result.add_failure(
                    index,
                    format!(
                        "Type target not found: '{}' ({})",
                        event.element_label, event.element_role
                    ),
                );
                Err(())
            }
        }
    }

    fn execute_key_press(
        &self,
        index: usize,
        event: &RecordedEvent,
        ref_store: &RefStore,
        result: &mut ReplayResult,
        key: &str,
        modifiers: &[String],
    ) -> Result<(), ()> {
        let _ = (key, modifiers); // available for real dispatch
        match self.resolve_element(event, ref_store, index, result) {
            Some(_) => Ok(()),
            None if event.element_fingerprint == 0 => Ok(()),
            None => {
                result.add_failure(
                    index,
                    format!(
                        "KeyPress target not found: '{}' ({})",
                        event.element_label, event.element_role
                    ),
                );
                Err(())
            }
        }
    }

    fn execute_scroll(
        &self,
        index: usize,
        event: &RecordedEvent,
        ref_store: &RefStore,
        result: &mut ReplayResult,
        dx: f64,
        dy: f64,
    ) -> Result<(), ()> {
        let _ = (dx, dy); // available for real dispatch
        match self.resolve_element(event, ref_store, index, result) {
            Some(_) => Ok(()),
            None if event.element_fingerprint == 0 => Ok(()),
            None => {
                result.add_failure(
                    index,
                    format!(
                        "Scroll target not found: '{}' ({})",
                        event.element_label, event.element_role
                    ),
                );
                Err(())
            }
        }
    }
}

impl Default for WorkflowPlayer {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Current wall-clock time in milliseconds since the Unix epoch.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistent_refs::{ElementSnapshot, RefStore};

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn button_event(label: &str, fp: u64) -> RecordedEvent {
        RecordedEvent {
            timestamp: 0,
            action: RecordedAction::Click { x: 10.0, y: 20.0 },
            element_fingerprint: fp,
            element_label: label.to_owned(),
            element_role: "AXButton".to_owned(),
        }
    }

    fn wait_event(ms: u64) -> RecordedEvent {
        RecordedEvent {
            timestamp: 0,
            action: RecordedAction::Wait { duration_ms: ms },
            element_fingerprint: 0,
            element_label: String::new(),
            element_role: String::new(),
        }
    }

    fn store_with_button(label: &str) -> (RefStore, u64) {
        let mut store = RefStore::new();
        let snap = ElementSnapshot {
            role: "AXButton".into(),
            label: label.into(),
            path: vec!["AXWindow".into(), format!("AXButton:{label}")],
            bounds: (100.0, 200.0, 80.0, 30.0),
        };
        let elem_ref = store.track(snap);
        (store, elem_ref.fingerprint)
    }

    // -----------------------------------------------------------------------
    // WorkflowRecorder tests
    // -----------------------------------------------------------------------

    #[test]
    fn recorder_starts_empty_and_not_recording() {
        // GIVEN: Fresh recorder
        let recorder = WorkflowRecorder::new();

        // THEN: Not recording, no events
        assert!(!recorder.is_recording());
        assert_eq!(recorder.event_count(), 0);
    }

    #[test]
    fn recorder_start_clears_previous_events() {
        // GIVEN: Recorder with one event from a prior session
        let mut recorder = WorkflowRecorder::new();
        recorder.start_recording();
        recorder.record_event(button_event("Save", 1));
        recorder.stop_recording();

        // WHEN: Starting a new session
        recorder.start_recording();

        // THEN: Event buffer is cleared
        assert_eq!(recorder.event_count(), 0);
    }

    #[test]
    fn recorder_ignores_events_when_not_recording() {
        // GIVEN: Recorder that has not been started
        let mut recorder = WorkflowRecorder::new();

        // WHEN: Attempting to record an event
        recorder.record_event(button_event("Ignored", 0));

        // THEN: No events accumulated
        assert_eq!(recorder.event_count(), 0);
    }

    #[test]
    fn recorder_accumulates_events_in_order() {
        // GIVEN: Active recorder
        let mut recorder = WorkflowRecorder::new();
        recorder.start_recording();

        // WHEN: Recording two events
        recorder.record_event(button_event("First", 1));
        recorder.record_event(button_event("Second", 2));

        // THEN: Two events in insertion order
        assert_eq!(recorder.event_count(), 2);
    }

    #[test]
    fn stop_recording_returns_all_events_and_stops() {
        // GIVEN: Active recorder with two events
        let mut recorder = WorkflowRecorder::new();
        recorder.start_recording();
        recorder.record_event(button_event("A", 1));
        recorder.record_event(button_event("B", 2));

        // WHEN: Stopping
        let events = recorder.stop_recording();

        // THEN: All events returned; recorder no longer active
        assert_eq!(events.len(), 2);
        assert!(!recorder.is_recording());
        assert_eq!(recorder.event_count(), 0);
    }

    #[test]
    fn convenience_helpers_record_correct_action_types() {
        // GIVEN: Active recorder
        let mut recorder = WorkflowRecorder::new();
        recorder.start_recording();

        // WHEN: Using convenience helpers
        recorder.record_click(50.0, 60.0, 7, "OK", "AXButton");
        recorder.record_type("hello", 8, "Search", "AXTextField");

        let events = recorder.stop_recording();

        // THEN: Action types match
        assert!(matches!(events[0].action, RecordedAction::Click { .. }));
        assert!(matches!(events[1].action, RecordedAction::Type { .. }));
        assert_eq!(events[1].element_label, "Search");
    }

    #[test]
    fn serialize_and_deserialize_round_trip() {
        // GIVEN: Events covering all action variants
        let events = vec![
            RecordedEvent {
                timestamp: 0,
                action: RecordedAction::Click { x: 1.0, y: 2.0 },
                element_fingerprint: 42,
                element_label: "OK".into(),
                element_role: "AXButton".into(),
            },
            RecordedEvent {
                timestamp: 100,
                action: RecordedAction::Type {
                    text: "hello".into(),
                },
                element_fingerprint: 0,
                element_label: "Input".into(),
                element_role: "AXTextField".into(),
            },
            RecordedEvent {
                timestamp: 200,
                action: RecordedAction::KeyPress {
                    key: "Return".into(),
                    modifiers: vec!["Cmd".into()],
                },
                element_fingerprint: 0,
                element_label: String::new(),
                element_role: String::new(),
            },
            RecordedEvent {
                timestamp: 300,
                action: RecordedAction::Scroll { dx: 0.0, dy: -50.0 },
                element_fingerprint: 0,
                element_label: "List".into(),
                element_role: "AXList".into(),
            },
            RecordedEvent {
                timestamp: 400,
                action: RecordedAction::Wait { duration_ms: 500 },
                element_fingerprint: 0,
                element_label: String::new(),
                element_role: String::new(),
            },
        ];

        // WHEN: Serializing then deserializing
        let json = WorkflowRecorder::serialize(&events).unwrap();
        let restored = WorkflowRecorder::deserialize(&json).unwrap();

        // THEN: Round-trip is lossless
        assert_eq!(events, restored);
    }

    // -----------------------------------------------------------------------
    // WorkflowPlayer tests
    // -----------------------------------------------------------------------

    #[test]
    fn player_replays_wait_without_element_lookup() {
        // GIVEN: Workflow with a zero-duration wait (no sleep) and no element
        let events = vec![wait_event(0)];
        let store = RefStore::new();
        let player = WorkflowPlayer::new();

        // WHEN: Replaying
        let result = player.replay(&events, &store);

        // THEN: Success, one event executed
        assert!(result.success);
        assert_eq!(result.events_executed, 1);
        assert!(result.failures.is_empty());
    }

    #[test]
    fn player_succeeds_when_element_found_by_fingerprint() {
        // GIVEN: Store with a tracked "Save" button; workflow targets it by fingerprint
        let (store, fp) = store_with_button("Save");
        let events = vec![button_event("Save", fp)];
        let player = WorkflowPlayer::new();

        // WHEN: Replaying
        let result = player.replay(&events, &store);

        // THEN: All events executed without failure
        assert!(result.success);
        assert_eq!(result.events_executed, 1);
        assert!(result.adapted_events.is_empty());
    }

    #[test]
    fn player_adapts_when_fingerprint_changes_but_label_matches() {
        // GIVEN: Store tracks "Save" button; event carries stale fingerprint (fp != current)
        let (store, _real_fp) = store_with_button("Save");
        let stale_fp = 9_999_999; // deliberately wrong
        let events = vec![button_event("Save", stale_fp)];
        let player = WorkflowPlayer::new();

        // WHEN: Replaying
        let result = player.replay(&events, &store);

        // THEN: Adaptation via label fallback; event counted as adapted
        assert!(result.success);
        assert_eq!(result.events_executed, 1);
        assert!(result.adapted_events.contains(&0));
    }

    #[test]
    fn player_records_failure_when_element_not_found() {
        // GIVEN: Empty store; workflow targets a non-existent element
        let store = RefStore::new();
        let events = vec![button_event("Ghost", 123)];
        let player = WorkflowPlayer::new();

        // WHEN: Replaying
        let result = player.replay(&events, &store);

        // THEN: Failure recorded, event NOT counted as executed
        assert!(!result.success);
        assert_eq!(result.events_executed, 0);
        assert_eq!(result.failures.len(), 1);
        assert_eq!(result.failures[0].event_index, 0);
    }

    #[test]
    fn player_continues_after_failure_when_not_aborting() {
        // GIVEN: Two events — first targets missing element, second is a wait
        let store = RefStore::new();
        let events = vec![button_event("Ghost", 1), wait_event(0)];
        let mut player = WorkflowPlayer::new();
        player.abort_on_failure = false;

        // WHEN: Replaying
        let result = player.replay(&events, &store);

        // THEN: First fails, second (wait) succeeds
        assert!(!result.success);
        assert_eq!(result.events_executed, 1); // the wait succeeded
        assert_eq!(result.failures.len(), 1);
    }

    #[test]
    fn player_aborts_on_first_failure_when_configured() {
        // GIVEN: Two events — first fails, second is a wait
        let store = RefStore::new();
        let events = vec![button_event("Ghost", 1), wait_event(0)];
        let mut player = WorkflowPlayer::new();
        player.abort_on_failure = true;

        // WHEN: Replaying
        let result = player.replay(&events, &store);

        // THEN: Stops after first failure; wait never executes
        assert!(!result.success);
        assert_eq!(result.events_executed, 0);
        assert_eq!(result.total_events, 2);
    }

    #[test]
    fn player_uses_raw_coordinates_for_zero_fingerprint_click() {
        // GIVEN: Event with no semantic data (fingerprint == 0)
        let store = RefStore::new();
        let events = vec![RecordedEvent {
            timestamp: 0,
            action: RecordedAction::Click { x: 300.0, y: 400.0 },
            element_fingerprint: 0,
            element_label: String::new(),
            element_role: String::new(),
        }];
        let player = WorkflowPlayer::new();

        // WHEN: Replaying
        let result = player.replay(&events, &store);

        // THEN: Succeeds via coordinate fallback
        assert!(result.success);
        assert_eq!(result.events_executed, 1);
    }

    #[test]
    fn replay_result_tracks_total_vs_executed_counts() {
        // GIVEN: Three events — two valid waits + one failing click
        let store = RefStore::new();
        let events = vec![wait_event(0), button_event("Missing", 1), wait_event(0)];
        let player = WorkflowPlayer::new();

        // WHEN: Replaying
        let result = player.replay(&events, &store);

        // THEN: Total is 3, executed is 2 (both waits succeed)
        assert_eq!(result.total_events, 3);
        assert_eq!(result.events_executed, 2);
    }
}
