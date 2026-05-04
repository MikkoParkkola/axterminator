//! Query-Based Scene Understanding
//!
//! This module provides a high-level query interface over the accessibility tree.
//! It answers natural-language questions about the current screen state with a
//! single-pass encode then many-query model — matching the encode-once architecture
//! described in issue #4.
//!
//! # Architecture
//!
//! ```text
//! raw query string
//!       │
//!       ▼
//! SceneEngine::query()
//!       │
//!       ├─► parse_query_type()   — classifies intent (Find / Describe / Check / Count)
//!       │
//!       ├─► FindElement   — score every SceneNode via intent_matching, rank, threshold
//!       ├─► DescribeScene — enumerate top-level elements into a human sentence
//!       ├─► CheckState    — score + interpret top match as a boolean predicate
//!       └─► CountElements — score + count matches above threshold
//! ```
//!
//! The [`SceneGraph`] built by [`crate::intent::scan_scene`] is the single
//! encoded representation reused across every query — no re-scan required.
//!
//! # Example
//!
//! ```rust
//! use axterminator::scene::{SceneEngine, QueryType};
//! use axterminator::intent::{build_scene_from_nodes, NodeId, SceneNode};
//!
//! let node = SceneNode {
//!     id: NodeId(0),
//!     parent: None,
//!     children: vec![],
//!     role: Some("AXButton".into()),
//!     title: Some("Login".into()),
//!     label: None, value: None, description: None, identifier: None,
//!     bounds: Some((10.0, 20.0, 80.0, 30.0)),
//!     enabled: true,
//!     depth: 1,
//! };
//! let scene = build_scene_from_nodes(vec![node]);
//! let engine = SceneEngine::new();
//! let result = engine.query("find the login button", &scene);
//! assert!(!result.matches.is_empty());
//! assert!(result.confidence > 0.5);
//! ```

use crate::intent::{SceneGraph, SceneNode};
use crate::intent_matching::{MatchContext, score_node};

// ── Public types ──────────────────────────────────────────────────────────────

/// Classification of a natural-language scene query.
///
/// Parsed from the raw query string by [`SceneEngine::query`] before dispatch.
#[derive(Debug, Clone, PartialEq)]
pub enum QueryType {
    /// Locate one or more elements matching a description.
    ///
    /// Example: `"find the submit button"`, `"where is the password field?"`
    FindElement(String),
    /// Produce a human-readable overview of all top-level UI elements.
    ///
    /// Example: `"what's on screen?"`, `"describe the interface"`
    DescribeScene,
    /// Evaluate whether a named element or state condition is active.
    ///
    /// Example: `"is the dialog open?"`, `"is save enabled?"`
    CheckState(String),
    /// Return the cardinality of elements matching a description.
    ///
    /// Example: `"how many list items are there?"`, `"count the buttons"`
    CountElements(String),
}

/// A single element that matched a scene query.
#[derive(Debug, Clone)]
pub struct SceneMatch {
    /// Accessibility role of the matched element (e.g., `"AXButton"`).
    pub element_role: String,
    /// Primary text label of the element (title, label, or description).
    pub element_label: String,
    /// Ancestor role chain from root down to (but not including) this node.
    pub element_path: Vec<String>,
    /// Bounding rect as `(x, y, width, height)`, if known.
    pub bounds: Option<(f64, f64, f64, f64)>,
    /// Normalised match score in `[0.0, 1.0]`.
    pub match_score: f64,
    /// Human-readable explanation of why this element was selected.
    pub match_reason: String,
}

/// The result returned by [`SceneEngine::query`].
#[derive(Debug, Clone)]
pub struct SceneResult {
    /// Ranked matches (best first); empty when nothing passes the threshold.
    pub matches: Vec<SceneMatch>,
    /// Overall confidence of the top match, or `0.0` when `matches` is empty.
    pub confidence: f64,
    /// Scene description prose produced by [`QueryType::DescribeScene`].
    pub scene_description: Option<String>,
}

/// Stateless engine for answering scene queries.
///
/// The engine holds no mutable state — all scene data is passed per call via
/// [`SceneGraph`].  Create one instance and reuse it freely across threads.
///
/// # Example
///
/// ```rust
/// use axterminator::scene::SceneEngine;
/// use axterminator::intent::SceneGraph;
///
/// let engine = SceneEngine::new();
/// let result = engine.query("describe the screen", &SceneGraph::empty());
/// assert!(result.scene_description.is_some());
/// ```
#[derive(Debug, Default)]
pub struct SceneEngine;

// Minimum score for a node to appear in `FindElement` / `CheckState` results.
// Set at 0.25 to suppress pure bigram-noise matches (a single shared bigram in
// a 7-bigram pair gives ~0.22, which is not a meaningful match).
const FIND_THRESHOLD: f64 = 0.25;
// Minimum score for a node to be counted in `CountElements`.
const COUNT_THRESHOLD: f64 = 0.25;
// Maximum matches returned for `FindElement`.
const MAX_FIND_RESULTS: usize = 10;

impl SceneEngine {
    /// Create a new engine instance.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Answer a natural-language query about `scene`.
    ///
    /// The query string is parsed to determine intent, then dispatched to the
    /// appropriate handler.  All work runs synchronously on the calling thread;
    /// callers wanting parallelism should issue concurrent calls from a thread
    /// pool.
    ///
    /// # Arguments
    ///
    /// * `query` — Free-form question or instruction (e.g., `"find the ok button"`).
    /// * `scene` — Encoded scene snapshot built by [`crate::intent::scan_scene`].
    ///
    /// # Returns
    ///
    /// A [`SceneResult`] with ranked matches and optional prose description.
    #[must_use]
    pub fn query(&self, query: &str, scene: &SceneGraph) -> SceneResult {
        match parse_query_type(query) {
            QueryType::DescribeScene => self.describe_scene(scene),
            QueryType::FindElement(subject) => self.find_elements(&subject, scene),
            QueryType::CheckState(subject) => self.check_state(&subject, scene),
            QueryType::CountElements(subject) => self.count_elements(&subject, scene),
        }
    }
}

// ── Query type parsing ────────────────────────────────────────────────────────

/// Parse a raw query string into a [`QueryType`].
///
/// Uses keyword-prefix heuristics so no regex is needed on the hot path.
#[must_use]
pub fn parse_query_type(query: &str) -> QueryType {
    let lower = query.trim().to_lowercase();

    if is_describe_query(&lower) {
        return QueryType::DescribeScene;
    }
    if let Some(subject) = strip_prefix(&lower, CHECK_PREFIXES) {
        return QueryType::CheckState(subject.trim().to_string());
    }
    if let Some(subject) = strip_prefix(&lower, COUNT_PREFIXES) {
        return QueryType::CountElements(subject.trim().to_string());
    }
    // Default: treat entire query as a FindElement subject
    let subject = strip_prefix(&lower, FIND_PREFIXES)
        .unwrap_or(lower.as_str())
        .trim()
        .to_string();
    QueryType::FindElement(subject)
}

// Keyword sets used by `parse_query_type`.
const DESCRIBE_KEYWORDS: &[&str] = &[
    "what's on screen",
    "what is on screen",
    "describe the screen",
    "describe the interface",
    "describe the ui",
    "what do you see",
    "what's visible",
    "what is visible",
    "show me the screen",
    "list elements",
    "list all elements",
];
const CHECK_PREFIXES: &[&str] = &[
    "is the ",
    "is there a ",
    "is there an ",
    "are there ",
    "check if ",
    "check whether ",
    "does the ",
];
const COUNT_PREFIXES: &[&str] = &["how many ", "count the ", "count all ", "number of "];
const FIND_PREFIXES: &[&str] = &[
    "find the ",
    "find a ",
    "find an ",
    "locate the ",
    "locate a ",
    "locate an ",
    "where is the ",
    "where is a ",
    "get the ",
    "click the ",
    "press the ",
];

fn is_describe_query(lower: &str) -> bool {
    DESCRIBE_KEYWORDS.iter().any(|&kw| lower.contains(kw))
}

/// Remove the first matching prefix from `s`, returning the remainder.
fn strip_prefix<'s>(s: &'s str, prefixes: &[&str]) -> Option<&'s str> {
    prefixes.iter().find_map(|&prefix| s.strip_prefix(prefix))
}

// ── Handler implementations ────────────────────────────────────────────────────

impl SceneEngine {
    /// Handle `FindElement` queries — score all nodes and return ranked matches.
    fn find_elements(&self, subject: &str, scene: &SceneGraph) -> SceneResult {
        let ctx = MatchContext::from_query(subject);
        let mut scored = score_all_nodes(scene, &ctx);
        scored.retain(|(score, _)| *score >= FIND_THRESHOLD);
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(MAX_FIND_RESULTS);

        let confidence = scored.first().map_or(0.0, |(s, _)| *s);
        let matches = scored
            .into_iter()
            .map(|(score, node)| build_scene_match(score, node, scene))
            .collect();

        SceneResult {
            matches,
            confidence,
            scene_description: None,
        }
    }

    /// Handle `DescribeScene` — enumerate top-level elements into prose.
    fn describe_scene(&self, scene: &SceneGraph) -> SceneResult {
        let description = build_scene_description(scene);
        SceneResult {
            matches: vec![],
            confidence: if scene.is_empty() { 0.0 } else { 1.0 },
            scene_description: Some(description),
        }
    }

    /// Handle `CheckState` — interpret the top-scoring match as a boolean.
    fn check_state(&self, subject: &str, scene: &SceneGraph) -> SceneResult {
        let mut base = self.find_elements(subject, scene);
        base.scene_description = build_state_description(subject, &base);
        base
    }

    /// Handle `CountElements` — score nodes and report count above threshold.
    fn count_elements(&self, subject: &str, scene: &SceneGraph) -> SceneResult {
        let ctx = MatchContext::from_query(subject);
        let count = scene
            .iter()
            .filter(|node| {
                let (score, _) = score_node(node, &ctx, scene);
                score >= COUNT_THRESHOLD
            })
            .count();

        let description = format!(
            "Found {count} element{} matching \"{}\".",
            if count == 1 { "" } else { "s" },
            subject
        );
        SceneResult {
            matches: vec![],
            confidence: if count > 0 { 1.0 } else { 0.0 },
            scene_description: Some(description),
        }
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────────

/// Score every node in `scene` returning `(score, &node)` pairs.
fn score_all_nodes<'g>(scene: &'g SceneGraph, ctx: &MatchContext) -> Vec<(f64, &'g SceneNode)> {
    scene
        .iter()
        .map(|node| {
            let (score, _) = score_node(node, ctx, scene);
            (score, node)
        })
        .collect()
}

/// Convert a scored node into a [`SceneMatch`].
fn build_scene_match(score: f64, node: &SceneNode, scene: &SceneGraph) -> SceneMatch {
    let (_, reason) = score_node(node, &MatchContext::from_query(""), scene);
    let element_label = node
        .text_labels()
        .first()
        .copied()
        .unwrap_or("")
        .to_string();
    let element_role = node.role.clone().unwrap_or_default();
    let element_path = ancestor_roles(node, scene);

    // Re-derive reason from original scoring context for this specific score.
    // `score_node` with empty query gives us structural reasons; use a richer
    // label here so the caller always gets meaningful text.
    let match_reason = if reason == "enabled state only" || reason.is_empty() {
        format!("score {score:.2}")
    } else {
        reason
    };

    SceneMatch {
        element_role,
        element_label,
        element_path,
        bounds: node.bounds,
        match_score: score,
        match_reason,
    }
}

/// Walk the parent chain and collect ancestor roles (root-first order).
fn ancestor_roles(node: &SceneNode, scene: &SceneGraph) -> Vec<String> {
    let mut path: Vec<String> = Vec::new();
    let mut current_parent = node.parent;
    while let Some(parent_id) = current_parent {
        let Some(parent_node) = scene.get(parent_id) else {
            break;
        };
        let role = parent_node.role.clone().unwrap_or_default();
        path.push(role);
        current_parent = parent_node.parent;
    }
    path.reverse();
    path
}

/// Build a human-readable description of the top-level elements in `scene`.
fn build_scene_description(scene: &SceneGraph) -> String {
    if scene.is_empty() {
        return "The screen appears to be empty.".into();
    }

    let mut parts: Vec<String> = Vec::new();
    for node in scene.iter() {
        // Only emit top-level nodes (depth ≤ 1) to keep the description concise.
        if node.depth > 1 {
            continue;
        }
        let role = node.role.as_deref().unwrap_or("element");
        let label = node
            .text_labels()
            .first()
            .copied()
            .unwrap_or("(unlabelled)");
        parts.push(format!("{role} \"{label}\""));
    }

    if parts.is_empty() {
        return "No visible top-level elements found.".into();
    }

    format!("On screen: {}.", parts.join(", "))
}

/// Build a prose state description (used by `CheckState`).
fn build_state_description(subject: &str, result: &SceneResult) -> Option<String> {
    let present = !result.matches.is_empty() && result.confidence >= FIND_THRESHOLD;
    Some(format!(
        "\"{}\" is {}.",
        subject,
        if present { "present" } else { "not found" }
    ))
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent::{NodeId, SceneNode, build_scene_from_nodes};

    // ── Fixture helpers ────────────────────────────────────────────────────

    fn btn(id: usize, title: &str) -> SceneNode {
        SceneNode {
            id: NodeId(id),
            parent: None,
            children: vec![],
            role: Some("AXButton".into()),
            title: Some(title.into()),
            label: None,
            value: None,
            description: None,
            identifier: None,
            bounds: Some((0.0, f64::from(id as u32) * 40.0, 80.0, 30.0)),
            enabled: true,
            depth: 1,
        }
    }

    fn field(id: usize, label: &str) -> SceneNode {
        SceneNode {
            id: NodeId(id),
            parent: None,
            children: vec![],
            role: Some("AXTextField".into()),
            title: None,
            label: Some(label.into()),
            value: None,
            description: None,
            identifier: None,
            bounds: Some((100.0, f64::from(id as u32) * 40.0, 200.0, 25.0)),
            enabled: true,
            depth: 1,
        }
    }

    fn window(id: usize, title: &str) -> SceneNode {
        SceneNode {
            id: NodeId(id),
            parent: None,
            children: vec![],
            role: Some("AXWindow".into()),
            title: Some(title.into()),
            label: None,
            value: None,
            description: None,
            identifier: None,
            bounds: Some((0.0, 0.0, 800.0, 600.0)),
            enabled: true,
            depth: 0,
        }
    }

    fn child_btn(id: usize, parent_id: usize, title: &str) -> SceneNode {
        let mut node = btn(id, title);
        node.parent = Some(NodeId(parent_id));
        node.depth = 2;
        node
    }

    fn engine() -> SceneEngine {
        SceneEngine::new()
    }

    // ── parse_query_type ───────────────────────────────────────────────────

    #[test]
    fn parse_query_type_find_element_by_default() {
        // GIVEN: A plain element description
        // WHEN: Parsed
        let qt = parse_query_type("submit button");
        // THEN: Classified as FindElement
        assert_eq!(qt, QueryType::FindElement("submit button".into()));
    }

    #[test]
    fn parse_query_type_find_strips_find_prefix() {
        // GIVEN: "find the X" query
        let qt = parse_query_type("find the login button");
        // THEN: Subject is extracted without prefix
        assert_eq!(qt, QueryType::FindElement("login button".into()));
    }

    #[test]
    fn parse_query_type_describe_scene_variants() {
        // GIVEN: Several "describe" phrasings
        for query in &[
            "what's on screen?",
            "describe the screen",
            "describe the interface",
            "list elements",
        ] {
            // WHEN: Parsed
            let qt = parse_query_type(query);
            // THEN: All map to DescribeScene
            assert_eq!(qt, QueryType::DescribeScene, "failed for: {query}");
        }
    }

    #[test]
    fn parse_query_type_check_state_strips_prefix() {
        // GIVEN: "is the X" query
        let qt = parse_query_type("is the dialog open?");
        // THEN: Subject stripped of prefix
        assert_eq!(qt, QueryType::CheckState("dialog open?".into()));
    }

    #[test]
    fn parse_query_type_count_elements_strips_prefix() {
        // GIVEN: "how many X" query
        let qt = parse_query_type("how many buttons are there?");
        // THEN: Subject extracted
        assert_eq!(qt, QueryType::CountElements("buttons are there?".into()));
    }

    // ── SceneEngine::query — FindElement ───────────────────────────────────

    #[test]
    fn find_element_exact_label_match_returns_match() {
        // GIVEN: Scene with a "Submit" button
        let scene = build_scene_from_nodes(vec![btn(0, "Submit"), btn(1, "Cancel")]);
        // WHEN: Query for submit
        let result = engine().query("find the submit button", &scene);
        // THEN: At least one match with high confidence
        assert!(!result.matches.is_empty());
        assert!(result.confidence > 0.5, "confidence={}", result.confidence);
        assert_eq!(result.matches[0].element_role, "AXButton");
    }

    #[test]
    fn find_element_fuzzy_label_match_submit_order() {
        // GIVEN: Button titled "Submit Order"
        let scene = build_scene_from_nodes(vec![btn(0, "Submit Order"), btn(1, "Cancel")]);
        // WHEN: Query for "Submit"
        let result = engine().query("Submit", &scene);
        // THEN: "Submit Order" matches
        assert!(!result.matches.is_empty());
        let top = &result.matches[0];
        assert!(
            top.element_label.to_lowercase().contains("submit"),
            "expected label containing 'submit', got '{}'",
            top.element_label
        );
    }

    #[test]
    fn find_element_role_and_label_returns_button() {
        // GIVEN: Scene with a button and text field both labeled "Login"
        let scene = build_scene_from_nodes(vec![btn(0, "Login"), field(1, "Login")]);
        // WHEN: Query with role hint "button"
        let result = engine().query("login button", &scene);
        // THEN: AXButton ranked first
        assert!(!result.matches.is_empty());
        assert_eq!(
            result.matches[0].element_role, "AXButton",
            "button should outrank text field for 'button' query"
        );
    }

    #[test]
    fn find_element_no_match_returns_empty_low_confidence() {
        // GIVEN: Scene with unrelated elements
        let scene = build_scene_from_nodes(vec![btn(0, "Foo"), btn(1, "Bar")]);
        // WHEN: Query for something completely absent — tokens have no label overlap
        // with "Foo" or "Bar"; only the enabled-state bonus (0.05) fires, which is
        // below FIND_THRESHOLD (0.10), so matches should be empty.
        let result = engine().query("find the purple wizard hat", &scene);
        // THEN: Either no matches at all, or very low confidence (< 0.15)
        assert!(
            result.matches.is_empty() || result.confidence < 0.15,
            "expected low confidence for unrelated query, got confidence={}",
            result.confidence
        );
    }

    #[test]
    fn find_element_matches_ranked_descending() {
        // GIVEN: Several buttons with varying relevance
        let scene = build_scene_from_nodes(vec![
            btn(0, "Submit"),
            btn(1, "Submit Form"),
            btn(2, "Cancel"),
        ]);
        // WHEN: Query for "submit"
        let result = engine().query("submit", &scene);
        // THEN: Sorted descending by match_score
        let scores: Vec<f64> = result.matches.iter().map(|m| m.match_score).collect();
        for window in scores.windows(2) {
            assert!(
                window[0] >= window[1],
                "scores not sorted: {:.3} < {:.3}",
                window[0],
                window[1]
            );
        }
    }

    #[test]
    fn find_element_match_reason_non_empty() {
        // GIVEN: A matching button
        let scene = build_scene_from_nodes(vec![btn(0, "OK")]);
        // WHEN: Query
        let result = engine().query("ok", &scene);
        // THEN: All matches have non-empty reason
        for m in &result.matches {
            assert!(!m.match_reason.is_empty(), "match_reason must not be empty");
        }
    }

    #[test]
    fn find_element_hierarchy_match_path_populated() {
        // GIVEN: Window containing a child button
        let parent = window(0, "Login");
        let child = child_btn(1, 0, "Confirm");
        let scene = build_scene_from_nodes(vec![parent, child]);
        // WHEN: Query for the child
        let result = engine().query("confirm button", &scene);
        // THEN: element_path contains ancestor roles
        let confirm_match = result.matches.iter().find(|m| m.element_label == "Confirm");
        assert!(confirm_match.is_some(), "should find 'Confirm' button");
        let path = &confirm_match.unwrap().element_path;
        assert!(!path.is_empty(), "path should list ancestors");
        assert!(
            path.iter().any(|r| r == "AXWindow"),
            "path should include AXWindow, got: {path:?}"
        );
    }

    #[test]
    fn find_element_bounds_propagated() {
        // GIVEN: Button with known bounds
        let scene = build_scene_from_nodes(vec![btn(0, "Save")]);
        // WHEN: Query
        let result = engine().query("save", &scene);
        // THEN: bounds are returned
        assert!(result.matches[0].bounds.is_some());
    }

    // ── SceneEngine::query — DescribeScene ────────────────────────────────

    #[test]
    fn describe_scene_lists_top_level_elements() {
        // GIVEN: Scene with two buttons at depth 1
        let scene = build_scene_from_nodes(vec![btn(0, "OK"), btn(1, "Cancel")]);
        // WHEN: Describe query
        let result = engine().query("what's on screen?", &scene);
        // THEN: Description mentions both labels
        let desc = result
            .scene_description
            .expect("description should be present");
        assert!(desc.contains("OK"), "should mention 'OK'; got: {desc}");
        assert!(
            desc.contains("Cancel"),
            "should mention 'Cancel'; got: {desc}"
        );
    }

    #[test]
    fn describe_scene_empty_screen_message() {
        // GIVEN: Empty scene
        let result = engine().query("describe the screen", &SceneGraph::empty());
        // THEN: Meaningful empty message
        let desc = result.scene_description.expect("description present");
        assert!(!desc.is_empty());
        assert!(desc.to_lowercase().contains("empty"));
    }

    #[test]
    fn describe_scene_no_matches_returned() {
        // GIVEN: Non-empty scene
        let scene = build_scene_from_nodes(vec![btn(0, "OK")]);
        // WHEN: Describe query
        let result = engine().query("what's on screen?", &scene);
        // THEN: matches vec is empty (describe doesn't rank individual elements)
        assert!(result.matches.is_empty());
    }

    // ── SceneEngine::query — CheckState ───────────────────────────────────

    #[test]
    fn check_state_present_element_returns_present_description() {
        // GIVEN: Scene with a dialog button
        let scene = build_scene_from_nodes(vec![btn(0, "Close Dialog"), btn(1, "OK")]);
        // WHEN: Check state for "dialog"
        let result = engine().query("is the dialog open?", &scene);
        // THEN: Description says present
        let desc = result
            .scene_description
            .expect("check state should produce description");
        assert!(desc.contains("present"), "expected 'present' in: {desc}");
    }

    #[test]
    fn check_state_absent_element_returns_not_found() {
        // GIVEN: Scene with unrelated elements
        let scene = build_scene_from_nodes(vec![btn(0, "OK")]);
        // WHEN: Check state for something absent
        let result = engine().query("is the export wizard shown?", &scene);
        let desc = result.scene_description.unwrap_or_default();
        // THEN: Either low confidence or description says not found
        let absent = result.matches.is_empty() || desc.contains("not found");
        assert!(absent, "should indicate absence; desc: {desc}");
    }

    // ── SceneEngine::query — CountElements ────────────────────────────────

    #[test]
    fn count_elements_returns_correct_count_description() {
        // GIVEN: Scene with 3 buttons sharing the label prefix "Apply"
        let scene = build_scene_from_nodes(vec![
            btn(0, "Apply Settings"),
            btn(1, "Apply Changes"),
            btn(2, "Apply All"),
            field(3, "Username"),
        ]);
        // WHEN: Count using the shared label token "apply"
        let result = engine().query("how many apply buttons", &scene);
        // THEN: Description is produced with a non-zero count
        let desc = result
            .scene_description
            .expect("count should produce description");
        // At least 1 element must be counted (the "apply" token matches all 3 btns)
        assert!(
            !desc.contains("Found 0"),
            "expected non-zero count; desc: {desc}"
        );
        assert!(result.confidence > 0.0);
    }

    #[test]
    fn count_elements_zero_match_description() {
        // GIVEN: Scene with no sliders
        let scene = build_scene_from_nodes(vec![btn(0, "OK")]);
        // WHEN: Count sliders
        let result = engine().query("how many sliders are visible?", &scene);
        let desc = result.scene_description.unwrap_or_default();
        // THEN: Zero reported
        assert!(
            desc.contains('0') || result.confidence == 0.0,
            "expected zero count; desc: {desc}"
        );
    }

    // ── Regression: empty scene never panics ──────────────────────────────

    #[test]
    fn all_query_types_safe_on_empty_scene() {
        let e = engine();
        let g = SceneGraph::empty();
        // GIVEN / WHEN / THEN: No panic on any query type against empty scene
        let _ = e.query("find the submit button", &g);
        let _ = e.query("what's on screen?", &g);
        let _ = e.query("is the save button visible?", &g);
        let _ = e.query("how many items are in the list?", &g);
    }
}
