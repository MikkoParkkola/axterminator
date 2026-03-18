//! Matching algorithms for intent extraction.
//!
//! This module provides the scoring machinery consumed by
//! [`crate::intent::extract_intent`].  All functions are pure (no side effects,
//! no allocations on the hot path where avoidable) so they can be unit-tested
//! in isolation.
//!
//! # Scoring model
//!
//! Each node starts at 0.0 and accumulates score from:
//!
//! | Signal | Max contribution |
//! |--------|-----------------|
//! | Fuzzy string match on labels | 0.60 |
//! | Exact substring match bonus | 0.15 |
//! | Role match from query hint | 0.20 |
//! | Enabled state | 0.05 |
//!
//! Final score is clamped to [0.0, 1.0].

use crate::accessibility::roles;
use crate::intent::{SceneGraph, SceneNode};

// ── MatchContext ───────────────────────────────────────────────────────────────

/// Pre-parsed representation of a user intent query.
///
/// Built once from the raw query string and reused for every node scored in
/// [`score_node`], amortising repeated string operations.
#[derive(Debug, Clone)]
pub struct MatchContext {
    /// Lower-cased tokens extracted from the query.
    pub tokens: Vec<String>,
    /// Optional role hint inferred from the query vocabulary.
    pub role_hint: Option<&'static str>,
    /// Whether the query mentions spatial proximity (e.g., "near", "next to").
    pub has_spatial_hint: bool,
}

impl MatchContext {
    /// Parse a free-form query into a [`MatchContext`].
    ///
    /// # Examples
    ///
    /// ```
    /// use axterminator::intent_matching::MatchContext;
    /// let ctx = MatchContext::from_query("click the submit button");
    /// assert_eq!(ctx.role_hint, Some("AXButton"));
    /// assert!(ctx.tokens.contains(&"submit".to_string()));
    /// ```
    #[must_use]
    pub fn from_query(query: &str) -> Self {
        let tokens = tokenise(query);
        let role_hint = infer_role_hint(&tokens);
        let has_spatial_hint = has_spatial_tokens(&tokens);
        Self {
            tokens,
            role_hint,
            has_spatial_hint,
        }
    }
}

// ── Public scoring entry point ─────────────────────────────────────────────────

/// Score a single [`SceneNode`] against a [`MatchContext`].
///
/// Returns `(confidence, reason)` where `confidence` is in `[0.0, 1.0]`.
///
/// `scene` is passed for hierarchical context lookups (parent role, sibling
/// proximity) but the function never mutates it.
#[must_use]
pub fn score_node(node: &SceneNode, ctx: &MatchContext, scene: &SceneGraph) -> (f64, String) {
    let label_score = score_labels(node, ctx);
    let role_score = score_role(node, ctx);
    let context_score = score_hierarchical_context(node, ctx, scene);
    let enabled_bonus = if node.enabled { 0.05 } else { 0.0 };

    let raw = label_score + role_score + context_score + enabled_bonus;
    let confidence = raw.clamp(0.0_f64, 1.0_f64);

    let reason = build_reason(label_score, role_score, context_score, node);
    (confidence, reason)
}

// ── Label scoring ─────────────────────────────────────────────────────────────

/// Score how well the node's text labels match the query tokens.
fn score_labels(node: &SceneNode, ctx: &MatchContext) -> f64 {
    let labels = node.text_labels();
    if labels.is_empty() || ctx.tokens.is_empty() {
        return 0.0;
    }

    // Best fuzzy score across all (label × token) pairs
    let best = labels
        .iter()
        .flat_map(|label| ctx.tokens.iter().map(|tok| fuzzy_score(label, tok)))
        .fold(0.0_f64, f64::max);

    // Bonus when any label contains the full query as a substring
    let full_query = ctx.tokens.join(" ");
    let exact_bonus = if labels
        .iter()
        .any(|l| l.to_lowercase().contains(&full_query))
    {
        0.15
    } else {
        0.0
    };

    // Contribution cap: 0.60
    (best * 0.60 + exact_bonus).min(0.75)
}

// ── Role scoring ───────────────────────────────────────────────────────────────

/// Score how well the node's role matches the role hint in the query.
fn score_role(node: &SceneNode, ctx: &MatchContext) -> f64 {
    let Some(hint) = ctx.role_hint else {
        return 0.0;
    };
    match node.role.as_deref() {
        Some(r) if r == hint => 0.20,
        _ => 0.0,
    }
}

// ── Hierarchical context ───────────────────────────────────────────────────────

/// Score based on the node's structural context within the scene.
///
/// Currently rewards nodes whose parent title/label matches a keyword in the
/// query (e.g., "button in the login dialog" boosts buttons whose parent is a
/// dialog titled "Login").
fn score_hierarchical_context(
    node: &SceneNode,
    ctx: &MatchContext,
    scene: &SceneGraph,
) -> f64 {
    let Some(parent_id) = node.parent else {
        return 0.0;
    };
    let Some(parent) = scene.get(parent_id) else {
        return 0.0;
    };

    let parent_labels = parent.text_labels();
    let best = parent_labels
        .iter()
        .flat_map(|lbl| ctx.tokens.iter().map(|tok| fuzzy_score(lbl, tok)))
        .fold(0.0_f64, f64::max);

    // Parent context contributes at most 0.10
    best * 0.10
}

// ── Fuzzy string matching ──────────────────────────────────────────────────────

/// Compute a similarity score in `[0.0, 1.0]` between two strings.
///
/// Uses a lightweight bigram-overlap metric that is O(n) in the shorter
/// string length.  This is intentionally simple — no heap allocation for the
/// common case where `haystack` and `needle` are both short UI labels.
#[must_use]
pub fn fuzzy_score(haystack: &str, needle: &str) -> f64 {
    let h = haystack.to_lowercase();
    let n = needle.to_lowercase();

    if h.is_empty() || n.is_empty() {
        return 0.0;
    }

    // Exact match
    if h == n {
        return 1.0;
    }

    // Prefix match (high value for UI labels)
    if h.starts_with(&n) || n.starts_with(&h) {
        let shorter = h.len().min(n.len()) as f64;
        let longer = h.len().max(n.len()) as f64;
        return (shorter / longer) * 0.95;
    }

    // Substring containment
    if h.contains(&n) {
        let ratio = n.len() as f64 / h.len() as f64;
        return ratio * 0.85;
    }
    if n.contains(&h) {
        let ratio = h.len() as f64 / n.len() as f64;
        return ratio * 0.80;
    }

    // Bigram overlap (Dice coefficient)
    bigram_dice(&h, &n)
}

/// Compute the Dice coefficient over character bigrams.
///
/// `score = 2 * |bigrams(a) ∩ bigrams(b)| / (|bigrams(a)| + |bigrams(b)|)`
fn bigram_dice(a: &str, b: &str) -> f64 {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();

    if a_chars.len() < 2 || b_chars.len() < 2 {
        // For single-char strings fall back to exact char comparison
        return if a_chars.first() == b_chars.first() {
            0.5
        } else {
            0.0
        };
    }

    let a_bigrams: Vec<(char, char)> = a_chars.windows(2).map(|w| (w[0], w[1])).collect();
    let b_bigrams: Vec<(char, char)> = b_chars.windows(2).map(|w| (w[0], w[1])).collect();

    let intersection = count_bigram_intersection(&a_bigrams, &b_bigrams);
    (2 * intersection) as f64 / (a_bigrams.len() + b_bigrams.len()) as f64
}

/// Count overlapping bigrams (multiset intersection).
fn count_bigram_intersection(a: &[(char, char)], b: &[(char, char)]) -> usize {
    // Copy b into a small mutable scratch space — avoids heap for short strings
    let mut b_scratch: Vec<(char, char)> = b.to_vec();
    let mut count = 0;
    for bigram in a {
        if let Some(pos) = b_scratch.iter().position(|x| x == bigram) {
            count += 1;
            b_scratch.swap_remove(pos);
        }
    }
    count
}

// ── Query analysis ─────────────────────────────────────────────────────────────

/// Extract lower-cased, stop-word-filtered tokens from a query.
#[must_use]
pub fn tokenise(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(|w| w.to_lowercase())
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
        .filter(|w| !w.is_empty() && !is_stop_word(w))
        .collect()
}

/// Return `true` for common English stop words that carry no UI signal.
fn is_stop_word(word: &str) -> bool {
    matches!(
        word,
        "the" | "a" | "an" | "on" | "in" | "at" | "to" | "of" | "and"
            | "or" | "is" | "it" | "be" | "for" | "by" | "with"
    )
}

/// Infer an AX role from query vocabulary (e.g., "button" → `AXButton`).
#[must_use]
pub fn infer_role_hint(tokens: &[String]) -> Option<&'static str> {
    for tok in tokens {
        let role = match tok.as_str() {
            "button" | "btn" | "click" | "press" | "tap" => Some(roles::AX_BUTTON),
            "text" | "input" | "field" | "textfield" | "type" | "enter" => {
                Some(roles::AX_TEXT_FIELD)
            }
            "checkbox" | "check" | "tick" => Some(roles::AX_CHECKBOX),
            "radio" | "option" => Some(roles::AX_RADIO_BUTTON),
            "menu" | "dropdown" => Some(roles::AX_MENU),
            "list" => Some(roles::AX_LIST),
            "table" => Some(roles::AX_TABLE),
            "slider" => Some(roles::AX_SLIDER),
            "link" | "href" => Some("AXLink"),
            _ => None,
        };
        if let Some(r) = role {
            return Some(r);
        }
    }
    None
}

/// Return `true` when the token list contains spatial proximity keywords.
fn has_spatial_tokens(tokens: &[String]) -> bool {
    tokens
        .iter()
        .any(|t| matches!(t.as_str(), "near" | "next" | "beside" | "below" | "above" | "left" | "right"))
}

// ── Reason string ──────────────────────────────────────────────────────────────

/// Build a human-readable explanation for why a node was matched.
fn build_reason(
    label_score: f64,
    role_score: f64,
    context_score: f64,
    node: &SceneNode,
) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(3);

    if label_score > 0.0 {
        let best_label = node
            .text_labels()
            .first()
            .copied()
            .unwrap_or("<no label>");
        parts.push(format!("label match '{best_label}' ({label_score:.2})"));
    }
    if role_score > 0.0 {
        let role = node.role.as_deref().unwrap_or("?");
        parts.push(format!("role match '{role}' ({role_score:.2})"));
    }
    if context_score > 0.0 {
        parts.push(format!("parent context ({context_score:.2})"));
    }
    if parts.is_empty() {
        return "enabled state only".into();
    }
    parts.join("; ")
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent::{build_scene_from_nodes, NodeId, SceneGraph, SceneNode};

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
            bounds: Some((0.0, 0.0, 80.0, 30.0)),
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
            bounds: Some((0.0, 50.0, 200.0, 25.0)),
            enabled: true,
            depth: 1,
        }
    }

    // ── fuzzy_score ────────────────────────────────────────────────────────

    #[test]
    fn fuzzy_score_exact_match_returns_one() {
        assert_eq!(fuzzy_score("submit", "submit"), 1.0);
    }

    #[test]
    fn fuzzy_score_case_insensitive() {
        assert_eq!(fuzzy_score("Submit", "submit"), 1.0);
    }

    #[test]
    fn fuzzy_score_prefix_match_high_score() {
        let score = fuzzy_score("submit form", "submit");
        assert!(score > 0.5, "prefix match should score > 0.5, got {score}");
    }

    #[test]
    fn fuzzy_score_substring_match_non_zero() {
        let score = fuzzy_score("click to submit", "submit");
        assert!(score > 0.0);
    }

    #[test]
    fn fuzzy_score_completely_different_near_zero() {
        let score = fuzzy_score("aaaa", "zzzz");
        assert!(score < 0.2, "unrelated strings should score < 0.2, got {score}");
    }

    #[test]
    fn fuzzy_score_empty_needle_returns_zero() {
        assert_eq!(fuzzy_score("submit", ""), 0.0);
    }

    #[test]
    fn fuzzy_score_empty_haystack_returns_zero() {
        assert_eq!(fuzzy_score("", "submit"), 0.0);
    }

    // ── tokenise ──────────────────────────────────────────────────────────

    #[test]
    fn tokenise_removes_stop_words() {
        let tokens = tokenise("click the submit button");
        assert!(!tokens.contains(&"the".to_string()));
        assert!(tokens.contains(&"click".to_string()));
        assert!(tokens.contains(&"submit".to_string()));
    }

    #[test]
    fn tokenise_lowercases_all() {
        let tokens = tokenise("SUBMIT FORM");
        assert!(tokens.iter().all(|t| t == t.to_lowercase().as_str()));
    }

    #[test]
    fn tokenise_strips_punctuation() {
        let tokens = tokenise("submit!");
        assert!(tokens.contains(&"submit".to_string()));
    }

    #[test]
    fn tokenise_empty_query_returns_empty() {
        assert!(tokenise("").is_empty());
    }

    // ── infer_role_hint ───────────────────────────────────────────────────

    #[test]
    fn infer_role_hint_button_keywords() {
        for kw in &["button", "btn", "click", "press", "tap"] {
            let hint = infer_role_hint(&[kw.to_string()]);
            assert_eq!(hint, Some("AXButton"), "keyword '{kw}' should hint AXButton");
        }
    }

    #[test]
    fn infer_role_hint_text_field_keywords() {
        let hint = infer_role_hint(&["type".to_string()]);
        assert_eq!(hint, Some("AXTextField"));
    }

    #[test]
    fn infer_role_hint_no_keyword_returns_none() {
        let hint = infer_role_hint(&["foobar".to_string()]);
        assert!(hint.is_none());
    }

    // ── MatchContext ───────────────────────────────────────────────────────

    #[test]
    fn match_context_parses_role_from_query() {
        let ctx = MatchContext::from_query("click the submit button");
        assert_eq!(ctx.role_hint, Some("AXButton"));
    }

    #[test]
    fn match_context_no_spatial_hint_by_default() {
        let ctx = MatchContext::from_query("click submit");
        assert!(!ctx.has_spatial_hint);
    }

    #[test]
    fn match_context_detects_spatial_hint() {
        let ctx = MatchContext::from_query("button near email");
        assert!(ctx.has_spatial_hint);
    }

    // ── score_node ─────────────────────────────────────────────────────────

    #[test]
    fn score_node_exact_label_match_high_confidence() {
        // GIVEN: Button titled "Submit"
        let node = btn(0, "Submit");
        let ctx = MatchContext::from_query("submit");
        let graph = SceneGraph::empty();
        // WHEN: Scoring
        let (confidence, _) = score_node(&node, &ctx, &graph);
        // THEN: High confidence
        assert!(
            confidence > 0.5,
            "exact label match should exceed 0.5, got {confidence}"
        );
    }

    #[test]
    fn score_node_role_hint_adds_bonus() {
        // GIVEN: Button and text field both labeled "Login"
        let button = btn(0, "Login");
        let text_field = field(1, "Login");
        let ctx = MatchContext::from_query("click the login button");
        let graph = SceneGraph::empty();

        let (btn_score, _) = score_node(&button, &ctx, &graph);
        let (tf_score, _) = score_node(&text_field, &ctx, &graph);

        assert!(
            btn_score > tf_score,
            "button ({btn_score:.3}) should beat text field ({tf_score:.3}) for button-hint query"
        );
    }

    #[test]
    fn score_node_disabled_element_lower_score() {
        // GIVEN: Enabled and disabled buttons with same label
        let enabled_btn = btn(0, "Submit");
        let mut disabled_btn = btn(1, "Submit");
        disabled_btn.enabled = false;

        let ctx = MatchContext::from_query("submit");
        let graph = SceneGraph::empty();

        let (enabled_score, _) = score_node(&enabled_btn, &ctx, &graph);
        let (disabled_score, _) = score_node(&disabled_btn, &ctx, &graph);

        assert!(
            enabled_score > disabled_score,
            "enabled ({enabled_score:.3}) should beat disabled ({disabled_score:.3})"
        );
    }

    #[test]
    fn score_node_parent_context_boosts_score() {
        // GIVEN: Window titled "Login" containing a button titled "Confirm"
        // The query includes "login" which only matches via parent context.
        let parent = SceneNode {
            id: NodeId(0),
            parent: None,
            children: vec![NodeId(1)],
            role: Some("AXWindow".into()),
            title: Some("Login".into()),
            label: None,
            value: None,
            description: None,
            identifier: None,
            bounds: None,
            enabled: true,
            depth: 0,
        };
        let mut child = btn(1, "Confirm");
        child.parent = Some(NodeId(0));

        // Graph with parent present
        let graph = build_scene_from_nodes(vec![parent, child]);
        // Isolated graph (parent absent — simulates orphan node)
        let orphan_graph = SceneGraph::empty();

        let ctx = MatchContext::from_query("login confirm");
        let child_node = graph.get(NodeId(1)).unwrap();

        let (score_with_parent, _) = score_node(child_node, &ctx, &graph);
        let (score_without_parent, _) = score_node(child_node, &ctx, &orphan_graph);

        // When the parent titled "Login" is in the graph, the "login" token
        // gets a context contribution; without it the score is lower.
        assert!(
            score_with_parent > score_without_parent,
            "parent context (login={:.3}) should exceed orphan (no_ctx={:.3})",
            score_with_parent,
            score_without_parent,
        );
    }

    #[test]
    fn score_node_returns_confidence_in_unit_interval() {
        // GIVEN: Any plausible node + context
        let node = btn(0, "Submit");
        let ctx = MatchContext::from_query("click submit button");
        let graph = SceneGraph::empty();
        let (confidence, _) = score_node(&node, &ctx, &graph);
        assert!((0.0..=1.0).contains(&confidence));
    }

    #[test]
    fn score_node_reason_non_empty_on_match() {
        let node = btn(0, "Save");
        let ctx = MatchContext::from_query("save");
        let graph = SceneGraph::empty();
        let (_, reason) = score_node(&node, &ctx, &graph);
        assert!(!reason.is_empty());
    }
}
