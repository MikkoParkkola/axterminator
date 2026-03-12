//! Integration tests for the two-stage intent extraction pipeline.
//!
//! All tests use synthetic [`SceneGraph`] data built with
//! [`build_scene_from_nodes`] so they run without accessibility permissions and
//! without a live macOS application.

use axterminator::intent::{
    build_scene_from_nodes, extract_intent, NodeId, SceneGraph, SceneNode,
};
use axterminator::intent_matching::{fuzzy_score, infer_role_hint, tokenise};

// ── Helpers ────────────────────────────────────────────────────────────────────

fn button(id: usize, title: &str) -> SceneNode {
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
        bounds: Some((10.0, f64::from(id as u32) * 40.0 + 10.0, 80.0, 30.0)),
        enabled: true,
        depth: 1,
    }
}

fn text_field(id: usize, label: &str) -> SceneNode {
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
        bounds: Some((10.0, f64::from(id as u32) * 40.0 + 10.0, 200.0, 25.0)),
        enabled: true,
        depth: 1,
    }
}

fn static_text(id: usize, value: &str) -> SceneNode {
    SceneNode {
        id: NodeId(id),
        parent: None,
        children: vec![],
        role: Some("AXStaticText".into()),
        title: None,
        label: None,
        value: Some(value.into()),
        description: None,
        identifier: None,
        bounds: Some((10.0, f64::from(id as u32) * 20.0, 300.0, 16.0)),
        enabled: true,
        depth: 2,
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

// ── SceneGraph structure tests ─────────────────────────────────────────────────

#[test]
fn scene_graph_empty_has_zero_nodes() {
    // GIVEN / WHEN: Empty graph
    let graph = SceneGraph::empty();
    // THEN: No nodes
    assert_eq!(graph.len(), 0);
    assert!(graph.is_empty());
}

#[test]
fn scene_graph_len_matches_node_count() {
    // GIVEN: 5 nodes
    let nodes: Vec<SceneNode> = (0..5).map(|i| button(i, "OK")).collect();
    let graph = build_scene_from_nodes(nodes);
    // THEN: len is 5
    assert_eq!(graph.len(), 5);
}

#[test]
fn scene_graph_get_by_id_retrieves_correct_node() {
    // GIVEN: Graph with Save and Cancel buttons
    let graph = build_scene_from_nodes(vec![button(0, "Save"), button(1, "Cancel")]);
    // THEN: Lookup by explicit ID works
    assert_eq!(graph.get(NodeId(0)).unwrap().title.as_deref(), Some("Save"));
    assert_eq!(
        graph.get(NodeId(1)).unwrap().title.as_deref(),
        Some("Cancel")
    );
}

#[test]
fn scene_graph_get_out_of_bounds_returns_none() {
    // GIVEN: Single-node graph
    let graph = build_scene_from_nodes(vec![button(0, "OK")]);
    // THEN: Index 1 is absent
    assert!(graph.get(NodeId(1)).is_none());
}

#[test]
fn scene_graph_nodes_by_role_returns_only_matching() {
    // GIVEN: 2 buttons and 1 text field
    let nodes = vec![button(0, "OK"), text_field(1, "Email"), button(2, "Cancel")];
    let graph = build_scene_from_nodes(nodes);
    // WHEN: Filtering buttons
    let buttons = graph.nodes_by_role("AXButton");
    // THEN: 2 buttons
    assert_eq!(buttons.len(), 2);
    assert!(buttons.iter().all(|n| n.role.as_deref() == Some("AXButton")));
}

#[test]
fn scene_graph_root_is_first_inserted_node() {
    // GIVEN: Window + children
    let graph = build_scene_from_nodes(vec![window(0, "Main"), button(1, "OK")]);
    // THEN: Root is the window
    let root = graph.root().unwrap();
    assert_eq!(root.role.as_deref(), Some("AXWindow"));
}

// ── SceneNode helper tests ─────────────────────────────────────────────────────

#[test]
fn scene_node_text_labels_collects_all_non_empty_fields() {
    // GIVEN: Node with title + description + identifier
    let node = SceneNode {
        id: NodeId(0),
        parent: None,
        children: vec![],
        role: Some("AXButton".into()),
        title: Some("OK".into()),
        label: None,
        value: None,
        description: Some("Confirm action".into()),
        identifier: Some("btn_ok".into()),
        bounds: None,
        enabled: true,
        depth: 0,
    };
    let labels = node.text_labels();
    assert!(labels.contains(&"OK"));
    assert!(labels.contains(&"Confirm action"));
    assert!(labels.contains(&"btn_ok"));
    assert_eq!(labels.len(), 3);
}

#[test]
fn scene_node_center_is_midpoint_of_bounds() {
    // GIVEN: Node at (20, 40) with size (60, 20)
    let node = SceneNode {
        id: NodeId(0),
        parent: None,
        children: vec![],
        role: None,
        title: None,
        label: None,
        value: None,
        description: None,
        identifier: None,
        bounds: Some((20.0, 40.0, 60.0, 20.0)),
        enabled: true,
        depth: 0,
    };
    let (cx, cy) = node.center().unwrap();
    assert_eq!(cx, 50.0);
    assert_eq!(cy, 50.0);
}

// ── fuzzy_score tests ──────────────────────────────────────────────────────────

#[test]
fn fuzzy_score_identical_strings_score_one() {
    assert_eq!(fuzzy_score("login", "login"), 1.0);
}

#[test]
fn fuzzy_score_case_insensitive_identical_is_one() {
    assert_eq!(fuzzy_score("LOGIN", "login"), 1.0);
}

#[test]
fn fuzzy_score_prefix_is_high() {
    let s = fuzzy_score("submit form", "submit");
    assert!(s > 0.5, "prefix should score > 0.5, got {s}");
}

#[test]
fn fuzzy_score_unrelated_strings_is_low() {
    let s = fuzzy_score("qqqq", "zzzz");
    assert!(s < 0.2, "unrelated strings should score < 0.2, got {s}");
}

#[test]
fn fuzzy_score_empty_inputs_are_zero() {
    assert_eq!(fuzzy_score("", "ok"), 0.0);
    assert_eq!(fuzzy_score("ok", ""), 0.0);
    assert_eq!(fuzzy_score("", ""), 0.0);
}

// ── tokenise tests ─────────────────────────────────────────────────────────────

#[test]
fn tokenise_removes_english_stop_words() {
    let tokens = tokenise("click the submit button");
    assert!(!tokens.contains(&"the".to_string()));
    assert!(tokens.contains(&"click".to_string()));
}

#[test]
fn tokenise_lowercases_all_tokens() {
    let tokens = tokenise("SAVE FILE");
    assert!(tokens.iter().all(|t| t == t.to_lowercase().as_str()));
}

#[test]
fn tokenise_empty_string_is_empty_vec() {
    assert!(tokenise("").is_empty());
}

// ── infer_role_hint tests ──────────────────────────────────────────────────────

#[test]
fn infer_role_hint_click_maps_to_button() {
    let hint = infer_role_hint(&["click".to_string()]);
    assert_eq!(hint, Some("AXButton"));
}

#[test]
fn infer_role_hint_type_maps_to_text_field() {
    let hint = infer_role_hint(&["type".to_string()]);
    assert_eq!(hint, Some("AXTextField"));
}

#[test]
fn infer_role_hint_unknown_token_returns_none() {
    let hint = infer_role_hint(&["foobar".to_string()]);
    assert!(hint.is_none());
}

// ── extract_intent behaviour tests ────────────────────────────────────────────

#[test]
fn extract_intent_finds_exact_title_match() {
    // GIVEN: Submit and Cancel buttons
    let graph = build_scene_from_nodes(vec![button(0, "Submit"), button(1, "Cancel")]);
    // WHEN: Query for "submit"
    let results = extract_intent(&graph, "submit");
    // THEN: Submit is first
    assert!(!results.is_empty());
    assert_eq!(results[0].node_id, NodeId(0));
}

#[test]
fn extract_intent_results_sorted_descending_by_confidence() {
    // GIVEN: Multiple buttons
    let graph = build_scene_from_nodes(vec![
        button(0, "Submit"),
        button(1, "Cancel"),
        button(2, "Submit Form"),
    ]);
    // WHEN: Query for "submit"
    let results = extract_intent(&graph, "submit");
    // THEN: Sorted descending
    for window in results.windows(2) {
        assert!(
            window[0].confidence >= window[1].confidence,
            "results not sorted: {:.3} < {:.3}",
            window[0].confidence,
            window[1].confidence
        );
    }
}

#[test]
fn extract_intent_empty_scene_returns_empty() {
    let graph = SceneGraph::empty();
    let results = extract_intent(&graph, "click submit");
    assert!(results.is_empty());
}

#[test]
fn extract_intent_all_confidences_in_unit_interval() {
    // GIVEN: Mixed scene
    let graph = build_scene_from_nodes(vec![
        button(0, "OK"),
        text_field(1, "Username"),
        static_text(2, "Welcome"),
    ]);
    let results = extract_intent(&graph, "ok");
    for m in &results {
        assert!(
            (0.0..=1.0).contains(&m.confidence),
            "confidence {:.3} out of range",
            m.confidence
        );
    }
}

#[test]
fn extract_intent_match_reason_is_non_empty() {
    let graph = build_scene_from_nodes(vec![button(0, "Save")]);
    let results = extract_intent(&graph, "save");
    assert!(!results.is_empty());
    assert!(
        !results[0].match_reason.is_empty(),
        "match_reason must not be blank"
    );
}

#[test]
fn extract_intent_role_hint_prefers_button_for_click_query() {
    // GIVEN: Button and text field both labeled "Login"
    let graph = build_scene_from_nodes(vec![button(0, "Login"), text_field(1, "Login")]);
    // WHEN: Query contains "click … button"
    let results = extract_intent(&graph, "click the login button");
    // THEN: Button ranks first
    assert!(!results.is_empty());
    assert_eq!(
        results[0].node_id,
        NodeId(0),
        "Button should beat text field when query says 'button'"
    );
}

#[test]
fn extract_intent_role_hint_prefers_text_field_for_type_query() {
    // GIVEN: Button and text field both labeled "Password"
    let graph = build_scene_from_nodes(vec![button(0, "Password"), text_field(1, "Password")]);
    // WHEN: Query contains "type" — role hint → AXTextField
    let results = extract_intent(&graph, "type password");
    // THEN: Text field ranks first
    assert!(!results.is_empty());
    assert_eq!(
        results[0].node_id,
        NodeId(1),
        "TextField should beat button when query says 'type'"
    );
}

#[test]
fn extract_intent_case_insensitive_matching() {
    // GIVEN: Button with mixed-case title
    let graph = build_scene_from_nodes(vec![button(0, "Save File")]);
    // WHEN: Lower-case query
    let results = extract_intent(&graph, "save file");
    assert!(!results.is_empty());
    assert_eq!(results[0].node_id, NodeId(0));
}

#[test]
fn extract_intent_fuzzy_partial_match_returns_result() {
    // GIVEN: Button titled "Submit"
    let graph = build_scene_from_nodes(vec![button(0, "Submit")]);
    // WHEN: Query is a prefix
    let results = extract_intent(&graph, "subm");
    assert!(!results.is_empty(), "fuzzy prefix should find 'Submit'");
}

#[test]
fn extract_intent_disabled_element_ranks_lower_than_enabled() {
    // GIVEN: Enabled and disabled buttons with same label
    let mut disabled = button(1, "OK");
    disabled.enabled = false;
    let graph = build_scene_from_nodes(vec![button(0, "OK"), disabled]);

    let results = extract_intent(&graph, "ok");
    assert!(results.len() >= 2);
    // Enabled (id 0) should rank first
    assert_eq!(results[0].node_id, NodeId(0));
}

#[test]
fn extract_intent_returns_node_id_stable_reference() {
    // GIVEN: 3-node graph
    let graph = build_scene_from_nodes(vec![
        button(0, "A"),
        button(1, "B"),
        button(2, "C"),
    ]);
    let results = extract_intent(&graph, "b");
    // The returned NodeId must be valid in the original graph
    if let Some(m) = results.first() {
        assert!(graph.get(m.node_id).is_some(), "NodeId must be valid");
    }
}
