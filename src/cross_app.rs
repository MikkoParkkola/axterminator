//! Cross-App Workflow Intelligence — Issue #10
//!
//! Tracks user workflows that span multiple applications, detects repeated
//! cross-app patterns, and suggests automation opportunities.
//!
//! # Design
//!
//! A [`CrossAppTracker`] records every app focus event as an [`AppTransition`].
//! Over time it accumulates a transition log that is mined for repeated
//! subsequences (length ≥ 3) using a sliding-window frequency counter.
//! Subsequences that occur at least `min_frequency` times are surfaced as
//! [`DetectedWorkflow`]s.  Each workflow carries a list of
//! [`AutomationStep`]s describing what an agent could automate.
//!
//! # Example
//!
//! ```rust
//! use axterminator::cross_app::{CrossAppTracker, TransitionTrigger};
//!
//! let mut tracker = CrossAppTracker::new();
//!
//! // Simulate: Figma → Linear → VS Code (three times)
//! for _ in 0..3 {
//!     tracker.record_focus("Figma", TransitionTrigger::UserSwitch);
//!     tracker.record_focus("Linear", TransitionTrigger::UserSwitch);
//!     tracker.record_focus("VSCode", TransitionTrigger::UserSwitch);
//! }
//!
//! let workflows = tracker.detect_workflows(2);
//! assert!(!workflows.is_empty());
//! assert_eq!(workflows[0].apps, vec!["Figma", "Linear", "VSCode"]);
//! ```

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

// ── Public types ──────────────────────────────────────────────────────────────

/// What caused the user to switch to a different application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TransitionTrigger {
    /// Manual switch (Cmd+Tab, Dock click, window click).
    UserSwitch,
    /// Programmatic / scripted focus change.
    Automation,
    /// A notification or alert pulled focus away.
    Notification,
    /// Cause could not be determined.
    #[default]
    Unknown,
}

/// Accumulated state for a single application.
#[derive(Debug, Clone, PartialEq)]
pub struct AppState {
    /// Application name as provided to [`CrossAppTracker::record_focus`].
    pub name: String,
    /// Whether this app is currently in the foreground.
    pub focused: bool,
    /// Label of the most recently recorded action within this app (if any).
    pub last_action: Option<String>,
    /// Number of times this app has received focus.
    pub focus_count: u64,
    /// Cumulative foreground time in milliseconds.
    pub total_time_ms: u64,
}

impl AppState {
    fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            focused: false,
            last_action: None,
            focus_count: 0,
            total_time_ms: 0,
        }
    }
}

/// A single recorded app-to-app focus change.
#[derive(Debug, Clone, PartialEq)]
pub struct AppTransition {
    /// App that lost focus (`None` for the very first focus event).
    pub from_app: String,
    /// App that gained focus.
    pub to_app: String,
    /// Unix timestamp in milliseconds.
    pub timestamp: u64,
    /// What triggered the switch.
    pub trigger: TransitionTrigger,
}

/// A recurring cross-app workflow detected from the transition history.
#[derive(Debug, Clone, PartialEq)]
pub struct DetectedWorkflow {
    /// Human-readable name derived from the app sequence.
    pub name: String,
    /// Ordered list of app names that form the workflow.
    pub apps: Vec<String>,
    /// Representative transition pattern (one occurrence).
    pub pattern: Vec<AppTransition>,
    /// How many times this exact app sequence was observed.
    pub frequency: u32,
    /// Average duration of one workflow cycle in milliseconds.
    pub avg_duration_ms: u64,
}

/// A single automatable step within a detected workflow.
#[derive(Debug, Clone, PartialEq)]
pub struct AutomationStep {
    /// Which app this step targets.
    pub app: String,
    /// Human-readable description of what to automate.
    pub description: String,
    /// Zero-based index of this step in the workflow.
    pub step_index: usize,
}

/// Aggregate statistics across all tracked apps.
#[derive(Debug, Clone, PartialEq)]
pub struct CrossAppStats {
    /// Total number of focus transitions recorded.
    pub total_transitions: usize,
    /// Number of distinct apps seen.
    pub distinct_apps: usize,
    /// Most-focused app (by focus count), if any.
    pub top_app: Option<String>,
    /// Most frequent `(from, to)` pair.
    pub top_transition: Option<(String, String)>,
}

// ── CrossAppTracker ───────────────────────────────────────────────────────────

/// Tracks cross-app focus events, detects workflows, and suggests automation.
///
/// # Thread Safety
///
/// `CrossAppTracker` is `Send` but not `Sync`; wrap in a `Mutex` for shared use.
#[derive(Debug, Default)]
pub struct CrossAppTracker {
    /// Per-app accumulated state, keyed by app name.
    apps: HashMap<String, AppState>,
    /// Ordered log of every focus transition.
    transitions: Vec<AppTransition>,
    /// Currently focused app name, used to compute dwell time.
    current_app: Option<String>,
    /// Timestamp when the current app gained focus.
    current_focus_start_ms: u64,
}

impl CrossAppTracker {
    /// Create an empty tracker.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a focus event: `app` gained foreground with `trigger` as cause.
    ///
    /// The previous foreground app's [`AppState::total_time_ms`] is updated
    /// based on wall-clock elapsed time since it gained focus.
    pub fn record_focus(&mut self, app: &str, trigger: TransitionTrigger) {
        let now = now_ms();
        self.flush_dwell_time(now);

        let from_app = self.current_app.clone().unwrap_or_default();
        let is_first = self.current_app.is_none();

        // Defocus previous app
        if let Some(prev) = &from_app
            .is_empty()
            .then_some(())
            .map(|_| None)
            .unwrap_or(Some(from_app.clone()))
        {
            if let Some(state) = self.apps.get_mut(prev) {
                state.focused = false;
            }
        }

        // Ensure destination app entry exists
        let state = self
            .apps
            .entry(app.to_owned())
            .or_insert_with(|| AppState::new(app));
        state.focused = true;
        state.focus_count += 1;

        // Record transition (skip phantom "" → first_app edge)
        if !is_first {
            self.transitions.push(AppTransition {
                from_app: from_app.clone(),
                to_app: app.to_owned(),
                timestamp: now,
                trigger,
            });
        }

        self.current_app = Some(app.to_owned());
        self.current_focus_start_ms = now;
    }

    /// Set an action label on the currently focused app.
    ///
    /// Useful for richer workflow names when the agent knows what the user
    /// was doing (e.g. `"editing component"`).
    pub fn set_last_action(&mut self, app: &str, action: impl Into<String>) {
        if let Some(state) = self.apps.get_mut(app) {
            state.last_action = Some(action.into());
        }
    }

    /// Snapshot of all tracked [`AppState`]s.
    #[must_use]
    pub fn app_states(&self) -> Vec<&AppState> {
        self.apps.values().collect()
    }

    /// All recorded [`AppTransition`]s in chronological order.
    #[must_use]
    pub fn transitions(&self) -> &[AppTransition] {
        &self.transitions
    }

    /// Detect repeated cross-app sequences of length ≥ 3.
    ///
    /// Reconstructs the full ordered app focus chain from the transition log
    /// (`from_app` of the first transition prepended to all `to_app` values),
    /// then applies a sliding window to find repeated subsequences.
    ///
    /// Windows of length 3–6 are checked in descending order so that longer
    /// (more specific) workflows are preferred over their shorter sub-sequences.
    #[must_use]
    pub fn detect_workflows(&self, min_frequency: u32) -> Vec<DetectedWorkflow> {
        if self.transitions.len() < 2 {
            return vec![];
        }

        let app_seq = self.full_app_chain();
        if app_seq.len() < 3 {
            return vec![];
        }

        let app_seq_refs: Vec<&str> = app_seq.iter().map(String::as_str).collect();
        let max_window = app_seq_refs.len().min(6);

        let mut results: Vec<DetectedWorkflow> = vec![];

        // Ascending order: shorter (more canonical) workflows are added first.
        // The is_subsequence_of_existing guard then prevents longer windows
        // that merely repeat a shorter pattern from being added again.
        for window in 3..=max_window {
            let candidates = count_subsequences(&app_seq_refs, window);

            // Sort for determinism: higher frequency first, then lexicographic.
            let mut sorted: Vec<_> = candidates.into_iter().collect();
            sorted.sort_by(|(a, ca), (b, cb)| cb.cmp(ca).then_with(|| a.cmp(b)));

            for (seq, freq) in sorted {
                if freq < min_frequency {
                    continue;
                }
                // Skip if a shorter workflow already covers this sequence
                if is_subsequence_of_existing(&seq, &results) {
                    continue;
                }
                let workflow = self.build_workflow(seq, freq);
                results.push(workflow);
            }
        }

        results.sort_by_key(|b| std::cmp::Reverse(b.frequency));
        results
    }

    /// Reconstruct the full focus chain as an ordered `Vec<String>`.
    ///
    /// The first app in the chain is the `from_app` of the first transition;
    /// every subsequent app is the `to_app` of each transition in order.
    fn full_app_chain(&self) -> Vec<String> {
        let Some(first) = self.transitions.first() else {
            return vec![];
        };
        let mut chain = Vec::with_capacity(self.transitions.len() + 1);
        chain.push(first.from_app.clone());
        for t in &self.transitions {
            chain.push(t.to_app.clone());
        }
        chain
    }

    /// Suggest [`AutomationStep`]s for each app in `workflow`.
    ///
    /// Returns one step per app with a generic description.  Callers can enrich
    /// the descriptions using domain knowledge from the accessibility tree.
    #[must_use]
    pub fn suggest_automation(workflow: &DetectedWorkflow) -> Vec<AutomationStep> {
        workflow
            .apps
            .iter()
            .enumerate()
            .map(|(i, app)| AutomationStep {
                app: app.clone(),
                description: automation_description(app, i, workflow.apps.len()),
                step_index: i,
            })
            .collect()
    }

    /// Aggregate statistics across all tracked apps and transitions.
    #[must_use]
    pub fn stats(&self) -> CrossAppStats {
        let top_app = self
            .apps
            .values()
            .max_by_key(|s| s.focus_count)
            .map(|s| s.name.clone());

        let top_transition = top_transition_pair(&self.transitions);

        CrossAppStats {
            total_transitions: self.transitions.len(),
            distinct_apps: self.apps.len(),
            top_app,
            top_transition,
        }
    }

    // ── Private helpers ──────────────────────────────────────────────────────

    /// Credit elapsed dwell time to the current app.
    fn flush_dwell_time(&mut self, now_ms: u64) {
        if let Some(app) = &self.current_app.clone() {
            if let Some(state) = self.apps.get_mut(app) {
                let elapsed = now_ms.saturating_sub(self.current_focus_start_ms);
                state.total_time_ms = state.total_time_ms.saturating_add(elapsed);
            }
        }
    }

    /// Construct a [`DetectedWorkflow`] from a sequence and its frequency.
    fn build_workflow(&self, seq: Vec<&str>, frequency: u32) -> DetectedWorkflow {
        let apps: Vec<String> = seq.iter().map(|s| s.to_string()).collect();
        let name = apps.join(" → ");

        // Find a representative run in the transition log
        let pattern = self.find_first_occurrence(&apps);

        let avg_duration_ms = self.avg_workflow_duration(&apps);

        DetectedWorkflow {
            name,
            apps,
            pattern,
            frequency,
            avg_duration_ms,
        }
    }

    /// Locate the first occurrence of `apps` as a contiguous run in the
    /// full focus chain and return the corresponding transitions.
    ///
    /// A sequence of N app names spans N-1 transitions.
    fn find_first_occurrence(&self, apps: &[String]) -> Vec<AppTransition> {
        let n = apps.len();
        if n < 2 || self.transitions.len() < n - 1 {
            return vec![];
        }
        // Each window of (n-1) consecutive transitions covers n apps:
        // from_app[i], to_app[i], to_app[i+1], ..., to_app[i+n-2]
        let window_len = n - 1;
        self.transitions
            .windows(window_len)
            .find(|w| {
                let first_app = w[0].from_app.as_str();
                let rest: Vec<&str> = w.iter().map(|t| t.to_app.as_str()).collect();
                let full: Vec<&str> = std::iter::once(first_app).chain(rest).collect();
                full.iter().zip(apps.iter()).all(|(a, b)| *a == b.as_str())
            })
            .map(|w| w.to_vec())
            .unwrap_or_default()
    }

    /// Average wall-clock duration of all occurrences of the workflow.
    ///
    /// Duration is measured from the first to last transition in a matching window.
    fn avg_workflow_duration(&self, apps: &[String]) -> u64 {
        let n = apps.len();
        if n < 2 || self.transitions.len() < n - 1 {
            return 0;
        }

        let mut total: u64 = 0;
        let mut count: u64 = 0;
        let window_len = n - 1;

        for window in self.transitions.windows(window_len) {
            let first_app = window[0].from_app.as_str();
            let rest: Vec<&str> = window.iter().map(|t| t.to_app.as_str()).collect();
            let full: Vec<&str> = std::iter::once(first_app).chain(rest).collect();

            if full.iter().zip(apps.iter()).all(|(a, b)| *a == b.as_str()) {
                let duration = window
                    .last()
                    .and_then(|last| {
                        window
                            .first()
                            .map(|first| last.timestamp.saturating_sub(first.timestamp))
                    })
                    .unwrap_or(0);
                total = total.saturating_add(duration);
                count += 1;
            }
        }

        total.checked_div(count).unwrap_or(0)
    }
}

// ── Free functions ────────────────────────────────────────────────────────────

/// Count how many times each `window`-length subsequence appears in `seq`.
fn count_subsequences<'a>(seq: &[&'a str], window: usize) -> HashMap<Vec<&'a str>, u32> {
    let mut counts: HashMap<Vec<&str>, u32> = HashMap::new();
    for chunk in seq.windows(window) {
        *counts.entry(chunk.to_vec()).or_insert(0) += 1;
    }
    counts
}

/// Return `true` if `seq` is a contiguous sub-slice of any workflow in `existing`.
fn is_subsequence_of_existing(seq: &[&str], existing: &[DetectedWorkflow]) -> bool {
    existing.iter().any(|wf| {
        let wf_apps: Vec<&str> = wf.apps.iter().map(String::as_str).collect();
        wf_apps.windows(seq.len()).any(|w| w == seq)
    })
}

/// Build an automation step description for an app at position `i` of `total`.
fn automation_description(app: &str, i: usize, total: usize) -> String {
    match i {
        0 => format!("Read/extract data from {app}"),
        n if n == total - 1 => format!("Write/inject result into {app}"),
        _ => format!("Transform and pass context through {app}"),
    }
}

/// Find the most frequent `(from, to)` transition pair.
fn top_transition_pair(transitions: &[AppTransition]) -> Option<(String, String)> {
    let mut counts: HashMap<(&str, &str), u32> = HashMap::new();
    for t in transitions {
        *counts
            .entry((t.from_app.as_str(), t.to_app.as_str()))
            .or_insert(0) += 1;
    }
    counts
        .into_iter()
        .max_by_key(|(_, c)| *c)
        .map(|((from, to), _)| (from.to_owned(), to.to_owned()))
}

/// Current Unix time in milliseconds.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helpers ──────────────────────────────────────────────────────────────

    /// Push `n` repetitions of the A→B→C pattern into `tracker`.
    fn push_abc(tracker: &mut CrossAppTracker, n: u32) {
        for _ in 0..n {
            tracker.record_focus("AppA", TransitionTrigger::UserSwitch);
            tracker.record_focus("AppB", TransitionTrigger::UserSwitch);
            tracker.record_focus("AppC", TransitionTrigger::UserSwitch);
        }
    }

    // ── TransitionTrigger ─────────────────────────────────────────────────

    #[test]
    fn transition_trigger_default_is_unknown() {
        // GIVEN / WHEN / THEN
        assert_eq!(TransitionTrigger::default(), TransitionTrigger::Unknown);
    }

    // ── record_focus: AppState bookkeeping ───────────────────────────────

    #[test]
    fn record_focus_three_apps_creates_three_states() {
        // GIVEN: Empty tracker
        let mut tracker = CrossAppTracker::new();

        // WHEN: Three distinct apps focused
        tracker.record_focus("Safari", TransitionTrigger::UserSwitch);
        tracker.record_focus("Slack", TransitionTrigger::UserSwitch);
        tracker.record_focus("VSCode", TransitionTrigger::UserSwitch);

        // THEN: All three apps present, VSCode is focused
        let stats = tracker.stats();
        assert_eq!(stats.distinct_apps, 3);
        let vscode = tracker.apps.get("VSCode").unwrap();
        assert!(vscode.focused);
    }

    #[test]
    fn record_focus_increments_focus_count_per_app() {
        // GIVEN: Tracker with repeated focuses on AppA
        let mut tracker = CrossAppTracker::new();
        tracker.record_focus("AppA", TransitionTrigger::UserSwitch);
        tracker.record_focus("AppB", TransitionTrigger::UserSwitch);
        tracker.record_focus("AppA", TransitionTrigger::UserSwitch);
        tracker.record_focus("AppB", TransitionTrigger::UserSwitch);
        tracker.record_focus("AppA", TransitionTrigger::UserSwitch);

        // WHEN / THEN: AppA focused 3 times
        assert_eq!(tracker.apps["AppA"].focus_count, 3);
        assert_eq!(tracker.apps["AppB"].focus_count, 2);
    }

    #[test]
    fn record_focus_first_app_generates_no_transition() {
        // GIVEN: Empty tracker
        let mut tracker = CrossAppTracker::new();

        // WHEN: Only one app focused
        tracker.record_focus("AppA", TransitionTrigger::UserSwitch);

        // THEN: No transitions recorded (nothing to transition from)
        assert_eq!(tracker.transitions().len(), 0);
    }

    #[test]
    fn record_focus_second_app_generates_one_transition() {
        // GIVEN: Empty tracker
        let mut tracker = CrossAppTracker::new();

        // WHEN: Two apps focused in sequence
        tracker.record_focus("From", TransitionTrigger::UserSwitch);
        tracker.record_focus("To", TransitionTrigger::Automation);

        // THEN: One transition with correct from/to and trigger
        let transitions = tracker.transitions();
        assert_eq!(transitions.len(), 1);
        assert_eq!(transitions[0].from_app, "From");
        assert_eq!(transitions[0].to_app, "To");
        assert_eq!(transitions[0].trigger, TransitionTrigger::Automation);
    }

    #[test]
    fn record_focus_only_current_app_has_focused_true() {
        // GIVEN / WHEN
        let mut tracker = CrossAppTracker::new();
        tracker.record_focus("First", TransitionTrigger::UserSwitch);
        tracker.record_focus("Second", TransitionTrigger::UserSwitch);

        // THEN: Only Second is focused
        assert!(!tracker.apps["First"].focused);
        assert!(tracker.apps["Second"].focused);
    }

    // ── set_last_action ──────────────────────────────────────────────────

    #[test]
    fn set_last_action_stores_label_on_known_app() {
        // GIVEN: AppA tracked
        let mut tracker = CrossAppTracker::new();
        tracker.record_focus("AppA", TransitionTrigger::UserSwitch);

        // WHEN: Action labelled
        tracker.set_last_action("AppA", "editing component");

        // THEN: Label stored
        assert_eq!(
            tracker.apps["AppA"].last_action.as_deref(),
            Some("editing component")
        );
    }

    #[test]
    fn set_last_action_on_unknown_app_is_noop() {
        // GIVEN: Empty tracker
        let mut tracker = CrossAppTracker::new();

        // WHEN: Action set on non-existent app — no panic
        tracker.set_last_action("Ghost", "whatever");

        // THEN: Nothing inserted
        assert!(!tracker.apps.contains_key("Ghost"));
    }

    // ── detect_workflows ─────────────────────────────────────────────────

    #[test]
    fn detect_workflows_empty_tracker_returns_empty() {
        // GIVEN: No transitions
        let tracker = CrossAppTracker::new();

        // WHEN / THEN
        assert!(tracker.detect_workflows(1).is_empty());
    }

    #[test]
    fn detect_workflows_fewer_than_three_transitions_returns_empty() {
        // GIVEN: Only two transitions (A→B→C needs 3 but we have A→B only)
        let mut tracker = CrossAppTracker::new();
        tracker.record_focus("A", TransitionTrigger::UserSwitch);
        tracker.record_focus("B", TransitionTrigger::UserSwitch);

        // WHEN / THEN: One transition total, too short for any 3-window
        assert!(tracker.detect_workflows(1).is_empty());
    }

    #[test]
    fn detect_workflows_abc_repeated_twice_detected() {
        // GIVEN: A→B→C twice
        let mut tracker = CrossAppTracker::new();
        push_abc(&mut tracker, 2);

        // WHEN
        let workflows = tracker.detect_workflows(2);

        // THEN: Workflow found with the correct app sequence
        assert!(!workflows.is_empty());
        let wf = &workflows[0];
        assert_eq!(wf.apps, vec!["AppA", "AppB", "AppC"]);
        assert_eq!(wf.frequency, 2);
    }

    #[test]
    fn detect_workflows_min_frequency_filters_rare_patterns() {
        // GIVEN: A→B→C three times, D→E→F once
        let mut tracker = CrossAppTracker::new();
        push_abc(&mut tracker, 3);
        tracker.record_focus("AppD", TransitionTrigger::UserSwitch);
        tracker.record_focus("AppE", TransitionTrigger::UserSwitch);
        tracker.record_focus("AppF", TransitionTrigger::UserSwitch);

        // WHEN: min_frequency = 3
        let workflows = tracker.detect_workflows(3);

        // THEN: Only the A→B→C workflow meets the threshold
        assert_eq!(workflows.len(), 1);
        assert_eq!(workflows[0].apps[0], "AppA");
    }

    #[test]
    fn detect_workflows_name_derived_from_app_sequence() {
        // GIVEN: A→B→C three times
        let mut tracker = CrossAppTracker::new();
        push_abc(&mut tracker, 3);

        // WHEN
        let workflows = tracker.detect_workflows(2);

        // THEN: Name is the arrow-joined sequence
        assert_eq!(workflows[0].name, "AppA → AppB → AppC");
    }

    #[test]
    fn detect_workflows_pattern_contains_representative_transitions() {
        // GIVEN: A→B→C three times
        let mut tracker = CrossAppTracker::new();
        push_abc(&mut tracker, 3);

        // WHEN
        let workflows = tracker.detect_workflows(2);

        // THEN: Pattern has 2 transitions covering 3 apps (n apps = n-1 transitions)
        assert!(!workflows.is_empty());
        assert_eq!(workflows[0].pattern.len(), 2);
    }

    // ── suggest_automation ───────────────────────────────────────────────

    #[test]
    fn suggest_automation_returns_one_step_per_app() {
        // GIVEN: A workflow over 4 apps
        let wf = DetectedWorkflow {
            name: "A → B → C → D".into(),
            apps: vec!["A".into(), "B".into(), "C".into(), "D".into()],
            pattern: vec![],
            frequency: 3,
            avg_duration_ms: 0,
        };

        // WHEN
        let steps = CrossAppTracker::suggest_automation(&wf);

        // THEN: Four steps, indices 0-3
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0].step_index, 0);
        assert_eq!(steps[3].step_index, 3);
    }

    #[test]
    fn suggest_automation_first_step_is_read_last_is_write() {
        // GIVEN: Workflow with 3 apps
        let wf = DetectedWorkflow {
            name: "Figma → Linear → VSCode".into(),
            apps: vec!["Figma".into(), "Linear".into(), "VSCode".into()],
            pattern: vec![],
            frequency: 5,
            avg_duration_ms: 0,
        };

        // WHEN
        let steps = CrossAppTracker::suggest_automation(&wf);

        // THEN: First = read, last = write
        assert!(
            steps[0].description.contains("Read"),
            "got: {}",
            steps[0].description
        );
        assert!(
            steps[2].description.contains("Write"),
            "got: {}",
            steps[2].description
        );
    }

    #[test]
    fn suggest_automation_middle_step_is_transform() {
        // GIVEN: 4-app workflow
        let wf = DetectedWorkflow {
            name: "A → B → C → D".into(),
            apps: vec!["A".into(), "B".into(), "C".into(), "D".into()],
            pattern: vec![],
            frequency: 2,
            avg_duration_ms: 0,
        };

        // WHEN
        let steps = CrossAppTracker::suggest_automation(&wf);

        // THEN: Middle steps mention "Transform"
        assert!(
            steps[1].description.contains("Transform"),
            "got: {}",
            steps[1].description
        );
        assert!(
            steps[2].description.contains("Transform"),
            "got: {}",
            steps[2].description
        );
    }

    // ── stats ─────────────────────────────────────────────────────────────

    #[test]
    fn stats_empty_tracker_returns_zero_totals() {
        // GIVEN / WHEN / THEN
        let tracker = CrossAppTracker::new();
        let stats = tracker.stats();
        assert_eq!(stats.total_transitions, 0);
        assert_eq!(stats.distinct_apps, 0);
        assert!(stats.top_app.is_none());
        assert!(stats.top_transition.is_none());
    }

    #[test]
    fn stats_top_app_is_most_focused() {
        // GIVEN: AppA focused 3×, AppB focused 1×
        let mut tracker = CrossAppTracker::new();
        tracker.record_focus("AppA", TransitionTrigger::UserSwitch);
        tracker.record_focus("AppB", TransitionTrigger::UserSwitch);
        tracker.record_focus("AppA", TransitionTrigger::UserSwitch);
        tracker.record_focus("AppB", TransitionTrigger::UserSwitch);
        tracker.record_focus("AppA", TransitionTrigger::UserSwitch);

        // WHEN
        let stats = tracker.stats();

        // THEN: AppA is top
        assert_eq!(stats.top_app.as_deref(), Some("AppA"));
    }

    #[test]
    fn stats_top_transition_is_most_frequent_pair() {
        // GIVEN: A→B appears 5 times, B→A appears 4 times, B→C appears 1 time
        // Sequence: A,B,A,B,A,B,A,B,A,B,C → 10 transitions
        // Transitions: A→B(×5), B→A(×4), B→C(×1) — clear winner: A→B
        let mut tracker = CrossAppTracker::new();
        tracker.record_focus("A", TransitionTrigger::UserSwitch); // first, no trans
        tracker.record_focus("B", TransitionTrigger::UserSwitch); // A→B  [1]
        tracker.record_focus("A", TransitionTrigger::UserSwitch); // B→A  [1]
        tracker.record_focus("B", TransitionTrigger::UserSwitch); // A→B  [2]
        tracker.record_focus("A", TransitionTrigger::UserSwitch); // B→A  [2]
        tracker.record_focus("B", TransitionTrigger::UserSwitch); // A→B  [3]
        tracker.record_focus("A", TransitionTrigger::UserSwitch); // B→A  [3]
        tracker.record_focus("B", TransitionTrigger::UserSwitch); // A→B  [4]
        tracker.record_focus("A", TransitionTrigger::UserSwitch); // B→A  [4]
        tracker.record_focus("B", TransitionTrigger::UserSwitch); // A→B  [5]
        tracker.record_focus("C", TransitionTrigger::UserSwitch); // B→C  [1]

        // WHEN
        let stats = tracker.stats();

        // THEN: A→B (5 occurrences) beats B→A (4) and B→C (1)
        let top = stats.top_transition.unwrap();
        assert_eq!(top.0, "A");
        assert_eq!(top.1, "B");
    }

    #[test]
    fn stats_counts_all_distinct_apps() {
        // GIVEN
        let mut tracker = CrossAppTracker::new();
        tracker.record_focus("X", TransitionTrigger::UserSwitch);
        tracker.record_focus("Y", TransitionTrigger::Notification);
        tracker.record_focus("Z", TransitionTrigger::Automation);

        // WHEN / THEN
        assert_eq!(tracker.stats().distinct_apps, 3);
    }
}
