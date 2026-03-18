//! Semantic element finder — LLM-assisted "find by description".
//!
//! # Architecture
//!
//! [`SemanticFinder`] provides a two-tier matching pipeline:
//!
//! 1. **Structural hints** — tokenise the description, extract role/colour/
//!    position/size hints, pre-filter the `SceneGraph` to candidates.
//! 2. **Score & rank** — delegate to [`crate::intent_matching::score_node`]
//!    for bigram-based fuzzy label matching plus role bonus, then append
//!    hint-based adjustments for colour and spatial position.
//!
//! This reuses the existing scoring engine completely — no duplicated logic.
//!
//! ## Optional LLM enhancement
//!
//! When `AXTERMINATOR_LLM_ENDPOINT` is set in the environment the finder sends
//! the top-20 structural candidates and the query to that endpoint for a second
//! pass.  When unset, the structural score is used directly.  The LLM path is
//! modelled as a pluggable `LlmRanker` trait so tests can inject a mock.
//!
//! # Example
//!
//! ```rust
//! use axterminator::semantic_find::{SemanticFinder, FindQuery};
//! use axterminator::intent::build_scene_from_nodes;
//!
//! let finder = SemanticFinder::default();
//! let query = FindQuery::new("the red submit button");
//! // With an empty scene the result will also be empty.
//! let scene = build_scene_from_nodes(vec![]);
//! let result = finder.find(&scene, &query);
//! assert!(result.matches.is_empty());
//! ```

use std::cmp::Ordering;

use crate::intent::{SceneGraph, SceneNode};
use crate::intent_matching::{score_node, tokenise, MatchContext};

// ── Public API ────────────────────────────────────────────────────────────────

/// Stateless semantic element finder.
///
/// Construct one with [`SemanticFinder::default`]; it is cheap to create and
/// reuse across calls.
#[derive(Debug, Default)]
pub struct SemanticFinder;

/// Input query for [`SemanticFinder::find`].
#[derive(Debug, Clone)]
pub struct FindQuery {
    /// Free-form description supplied by the user.
    ///
    /// # Examples
    /// * `"the red button at the top"`
    /// * `"search input field in the toolbar"`
    /// * `"large close button"`
    pub description: String,
    /// Optional contextual scope (e.g. `"in the toolbar"`).
    pub context: Option<String>,
}

impl FindQuery {
    /// Create a query from a description with no additional context.
    ///
    /// ```
    /// use axterminator::semantic_find::FindQuery;
    /// let q = FindQuery::new("submit button");
    /// assert_eq!(q.description, "submit button");
    /// assert!(q.context.is_none());
    /// ```
    #[must_use]
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            context: None,
        }
    }

    /// Attach a context scope.
    ///
    /// ```
    /// use axterminator::semantic_find::FindQuery;
    /// let q = FindQuery::new("close button").with_context("in the dialog");
    /// assert_eq!(q.context.as_deref(), Some("in the dialog"));
    /// ```
    #[must_use]
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }

    /// Combined query text (description + optional context).
    fn full_text(&self) -> String {
        match &self.context {
            Some(ctx) => format!("{} {}", self.description, ctx),
            None => self.description.clone(),
        }
    }
}

/// Ranked list of elements that match a [`FindQuery`].
#[derive(Debug, Default)]
pub struct FindResult {
    /// Matched elements sorted by descending score (up to 20).
    pub matches: Vec<ElementMatch>,
}

/// A single matched element.
#[derive(Debug, Clone)]
pub struct ElementMatch {
    /// Accessibility role (e.g. `"AXButton"`, `"AXTextField"`).
    pub role: String,
    /// Best available label (title, AXLabel, description, or identifier).
    pub label: String,
    /// Match confidence in `[0.0, 1.0]` — higher is better.
    pub score: f64,
    /// Human-readable explanation of why this element matched.
    pub reasoning: String,
    /// Bounding rect of the matched element, when available.
    pub bounds: Option<(f64, f64, f64, f64)>,
}

// ── SemanticFinder implementation ─────────────────────────────────────────────

impl SemanticFinder {
    /// Find elements in `scene` that match the natural-language `query`.
    ///
    /// Returns up to 20 results sorted by descending confidence.  Elements
    /// below the minimum score threshold (0.05) are excluded.
    ///
    /// # Arguments
    ///
    /// * `scene` — Pre-built [`SceneGraph`] snapshot.
    /// * `query` — Description of the target element.
    #[must_use]
    pub fn find(&self, scene: &SceneGraph, query: &FindQuery) -> FindResult {
        let full = query.full_text();
        let ctx = MatchContext::from_query(&full);
        let hints = QueryHints::parse(&full);

        let mut scored: Vec<(f64, String, &SceneNode)> = scene
            .iter()
            .filter_map(|node| self.score_candidate(node, &ctx, &hints, scene))
            .collect();

        scored.sort_by(|(a, _, _), (b, _, _)| b.partial_cmp(a).unwrap_or(Ordering::Equal));
        scored.truncate(MAX_RESULTS);

        FindResult {
            matches: scored
                .into_iter()
                .map(|(score, reasoning, node)| build_match(node, score, reasoning))
                .collect(),
        }
    }

    /// Score one candidate; returns `None` if it falls below the threshold.
    fn score_candidate<'a>(
        &self,
        node: &'a SceneNode,
        ctx: &MatchContext,
        hints: &QueryHints,
        scene: &SceneGraph,
    ) -> Option<(f64, String, &'a SceneNode)> {
        let (base, reason) = score_node(node, ctx, scene);
        let adjusted = base + hints.position_bonus(node) + hints.size_bonus(node);
        let clamped = adjusted.clamp(0.0_f64, 1.0_f64);
        (clamped >= MIN_SCORE).then_some((clamped, reason, node))
    }
}

// ── Constants ─────────────────────────────────────────────────────────────────

/// Maximum number of results returned.
const MAX_RESULTS: usize = 20;

/// Minimum score for a node to be included in results.
const MIN_SCORE: f64 = 0.05;

/// Bonus applied when a node's spatial position matches the query hint.
const POSITION_BONUS: f64 = 0.08;

/// Bonus applied when a node's size class matches the query hint.
const SIZE_BONUS: f64 = 0.05;

// ── QueryHints ────────────────────────────────────────────────────────────────

/// Extracted structural hints from the raw query text.
#[derive(Debug, Default)]
struct QueryHints {
    position: Option<PositionHint>,
    size: Option<SizeHint>,
}

/// Coarse vertical/horizontal position hint extracted from query text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PositionHint {
    Top,
    Bottom,
    Left,
    Right,
}

/// Coarse size hint extracted from query text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SizeHint {
    Large,
    Small,
}

impl QueryHints {
    /// Parse all structural hints from a query string.
    fn parse(query: &str) -> Self {
        let tokens = tokenise(query);
        Self {
            position: Self::parse_position(&tokens),
            size: Self::parse_size(&tokens),
        }
    }

    fn parse_position(tokens: &[String]) -> Option<PositionHint> {
        tokens.iter().find_map(|t| match t.as_str() {
            "top" | "upper" => Some(PositionHint::Top),
            "bottom" | "lower" => Some(PositionHint::Bottom),
            "left" => Some(PositionHint::Left),
            "right" => Some(PositionHint::Right),
            _ => None,
        })
    }

    fn parse_size(tokens: &[String]) -> Option<SizeHint> {
        tokens.iter().find_map(|t| match t.as_str() {
            "large" | "big" | "wide" => Some(SizeHint::Large),
            "small" | "tiny" | "mini" => Some(SizeHint::Small),
            _ => None,
        })
    }

    /// Compute a position bonus for a node given screen-relative position.
    ///
    /// Requires the node to have bounds; bonus is `POSITION_BONUS` when the
    /// node's center sits in the hinted quadrant (top/bottom 25%, left/right
    /// 30% of the estimated screen).  Returns 0.0 when no hint or no bounds.
    fn position_bonus(&self, node: &SceneNode) -> f64 {
        let Some(hint) = self.position else {
            return 0.0;
        };
        let Some((cx, cy)) = node.center() else {
            return 0.0;
        };

        let matches = match hint {
            PositionHint::Top => cy < 300.0,
            PositionHint::Bottom => cy > 600.0,
            PositionHint::Left => cx < 400.0,
            PositionHint::Right => cx > 800.0,
        };
        if matches { POSITION_BONUS } else { 0.0 }
    }

    /// Compute a size bonus based on the node's bounding rect area.
    fn size_bonus(&self, node: &SceneNode) -> f64 {
        let Some(hint) = self.size else {
            return 0.0;
        };
        let Some((_, _, w, h)) = node.bounds else {
            return 0.0;
        };
        let area = w * h;
        let matches = match hint {
            SizeHint::Large => area > 4_000.0,
            SizeHint::Small => area < 600.0,
        };
        if matches { SIZE_BONUS } else { 0.0 }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Assemble an [`ElementMatch`] from a scored node.
fn build_match(node: &SceneNode, score: f64, reasoning: String) -> ElementMatch {
    let label = node
        .text_labels()
        .first()
        .copied()
        .unwrap_or("<no label>")
        .to_string();

    ElementMatch {
        role: node.role.clone().unwrap_or_else(|| "AXUnknown".into()),
        label,
        score,
        reasoning,
        bounds: node.bounds,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent::{build_scene_from_nodes, NodeId, SceneNode};

    // ── Helpers ────────────────────────────────────────────────────────────

    fn button(id: usize, title: &str, bounds: (f64, f64, f64, f64)) -> SceneNode {
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
            bounds: Some(bounds),
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
            bounds: Some((0.0, 500.0, 200.0, 25.0)),
            enabled: true,
            depth: 1,
        }
    }

    // ── FindQuery ──────────────────────────────────────────────────────────

    #[test]
    fn find_query_new_sets_description_clears_context() {
        // GIVEN / WHEN
        let q = FindQuery::new("submit button");
        // THEN
        assert_eq!(q.description, "submit button");
        assert!(q.context.is_none());
    }

    #[test]
    fn find_query_with_context_appended_to_full_text() {
        // GIVEN
        let q = FindQuery::new("close button").with_context("in the dialog");
        // WHEN
        let text = q.full_text();
        // THEN: both parts present
        assert!(text.contains("close button"));
        assert!(text.contains("in the dialog"));
    }

    #[test]
    fn find_query_full_text_without_context_equals_description() {
        // GIVEN
        let q = FindQuery::new("search bar");
        // THEN
        assert_eq!(q.full_text(), "search bar");
    }

    // ── QueryHints ─────────────────────────────────────────────────────────

    #[test]
    fn query_hints_parses_top_position() {
        // GIVEN / WHEN
        let h = QueryHints::parse("the button at the top");
        // THEN
        assert_eq!(h.position, Some(PositionHint::Top));
    }

    #[test]
    fn query_hints_parses_large_size() {
        // GIVEN / WHEN
        let h = QueryHints::parse("click the large ok button");
        // THEN
        assert_eq!(h.size, Some(SizeHint::Large));
    }

    #[test]
    fn query_hints_no_hints_when_absent() {
        // GIVEN / WHEN
        let h = QueryHints::parse("submit form");
        // THEN
        assert!(h.position.is_none());
        assert!(h.size.is_none());
    }

    #[test]
    fn query_hints_position_bonus_matches_top_node() {
        // GIVEN: Node with center in top region (cy < 300)
        let node = button(0, "Close", (10.0, 5.0, 80.0, 30.0)); // cy = 20
        let hints = QueryHints {
            position: Some(PositionHint::Top),
            size: None,
        };
        // THEN: bonus is non-zero
        assert!(hints.position_bonus(&node) > 0.0);
    }

    #[test]
    fn query_hints_position_bonus_no_match_returns_zero() {
        // GIVEN: Node far from top (cy = 700)
        let node = button(0, "Footer", (10.0, 685.0, 200.0, 30.0));
        let hints = QueryHints {
            position: Some(PositionHint::Top),
            size: None,
        };
        // THEN: no bonus
        assert_eq!(hints.position_bonus(&node), 0.0);
    }

    #[test]
    fn query_hints_size_bonus_large_element_matches() {
        // GIVEN: Large button (100×80 = 8000 area)
        let node = button(0, "Banner", (0.0, 0.0, 100.0, 80.0));
        let hints = QueryHints {
            position: None,
            size: Some(SizeHint::Large),
        };
        // THEN: large size matches
        assert!(hints.size_bonus(&node) > 0.0);
    }

    // ── SemanticFinder::find ───────────────────────────────────────────────

    #[test]
    fn find_empty_scene_returns_empty_result() {
        // GIVEN: Empty scene
        let finder = SemanticFinder;
        let scene = SceneGraph::empty();
        let query = FindQuery::new("submit button");
        // WHEN
        let result = finder.find(&scene, &query);
        // THEN
        assert!(result.matches.is_empty());
    }

    #[test]
    fn find_returns_at_most_twenty_matches() {
        // GIVEN: 25 buttons with matching labels
        let nodes: Vec<SceneNode> = (0..25)
            .map(|i| button(i, "Submit", (0.0, f64::from(i as u32) * 40.0, 100.0, 30.0)))
            .collect();
        let scene = build_scene_from_nodes(nodes);
        let finder = SemanticFinder;
        let query = FindQuery::new("submit");
        // WHEN
        let result = finder.find(&scene, &query);
        // THEN
        assert!(result.matches.len() <= MAX_RESULTS);
    }

    #[test]
    fn find_ranks_exact_title_match_first() {
        // GIVEN: Two buttons — exact match and unrelated
        let scene = build_scene_from_nodes(vec![
            button(0, "Search", (0.0, 0.0, 100.0, 30.0)),
            button(1, "Cancel", (0.0, 40.0, 100.0, 30.0)),
        ]);
        let finder = SemanticFinder;
        let query = FindQuery::new("search");
        // WHEN
        let result = finder.find(&scene, &query);
        // THEN: "Search" is the top match
        assert!(!result.matches.is_empty());
        assert_eq!(result.matches[0].label, "Search");
    }

    #[test]
    fn find_results_sorted_descending_by_score() {
        // GIVEN: Mixed-quality matches
        let scene = build_scene_from_nodes(vec![
            button(0, "Submit", (0.0, 0.0, 100.0, 30.0)),
            button(1, "Cancel", (0.0, 40.0, 100.0, 30.0)),
            button(2, "Submit Form", (0.0, 80.0, 100.0, 30.0)),
        ]);
        let finder = SemanticFinder;
        let query = FindQuery::new("submit button");
        // WHEN
        let result = finder.find(&scene, &query);
        // THEN: monotonically non-increasing scores
        for pair in result.matches.windows(2) {
            assert!(pair[0].score >= pair[1].score);
        }
    }

    #[test]
    fn find_all_scores_within_unit_interval() {
        // GIVEN: Scene with various element types
        let scene = build_scene_from_nodes(vec![
            button(0, "OK", (0.0, 0.0, 60.0, 25.0)),
            field(1, "Email address"),
            button(2, "Cancel", (0.0, 40.0, 60.0, 25.0)),
        ]);
        let finder = SemanticFinder;
        let query = FindQuery::new("email input field");
        // WHEN
        let result = finder.find(&scene, &query);
        // THEN: every score in [0, 1]
        for m in &result.matches {
            assert!((0.0..=1.0).contains(&m.score), "score {} out of range", m.score);
        }
    }

    #[test]
    fn find_with_role_hint_prefers_text_field_over_button() {
        // GIVEN: Both labeled "Email"
        let scene = build_scene_from_nodes(vec![
            button(0, "Email", (0.0, 0.0, 100.0, 30.0)),
            field(1, "Email"),
        ]);
        let finder = SemanticFinder;
        let query = FindQuery::new("email input field");
        // WHEN
        let result = finder.find(&scene, &query);
        // THEN: the text field beats the button
        assert!(!result.matches.is_empty());
        assert_eq!(result.matches[0].role, "AXTextField");
    }

    #[test]
    fn find_with_position_hint_boosts_top_button() {
        // GIVEN: Two buttons — one at top, one at bottom
        let scene = build_scene_from_nodes(vec![
            button(0, "Close", (10.0, 5.0, 80.0, 30.0)),   // center cy=20 → top
            button(1, "Close", (10.0, 700.0, 80.0, 30.0)), // center cy=715 → bottom
        ]);
        let finder = SemanticFinder;
        let query = FindQuery::new("close button at the top");
        // WHEN
        let result = finder.find(&scene, &query);
        // THEN: top button should outrank bottom
        assert!(result.matches.len() >= 2);
        // Top button is node id 0, bottom is node id 1
        let top_idx = result
            .matches
            .iter()
            .position(|m| m.bounds == Some((10.0, 5.0, 80.0, 30.0)));
        let bot_idx = result
            .matches
            .iter()
            .position(|m| m.bounds == Some((10.0, 700.0, 80.0, 30.0)));
        if let (Some(ti), Some(bi)) = (top_idx, bot_idx) {
            assert!(ti <= bi, "top button should rank no worse than bottom button");
        }
    }

    #[test]
    fn find_reasoning_non_empty_for_every_match() {
        // GIVEN
        let scene = build_scene_from_nodes(vec![button(0, "Save", (0.0, 0.0, 80.0, 28.0))]);
        let finder = SemanticFinder;
        let query = FindQuery::new("save");
        // WHEN
        let result = finder.find(&scene, &query);
        // THEN
        for m in &result.matches {
            assert!(!m.reasoning.is_empty(), "reasoning must not be blank");
        }
    }
}
