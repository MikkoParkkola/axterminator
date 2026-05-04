//! Two-stage screen intent extraction pipeline.
//!
//! # Architecture
//!
//! ## Stage 1 — Fast structural scan (`scan_scene`)
//!
//! Traverses the accessibility tree and snapshots all elements into a `SceneGraph`.
//! Every node captures role, labels, bounds, and parent/child relationships in one
//! breadth-first pass.  Because the graph stores plain Rust values (no CF refs) it
//! is `Send + Sync` and can be passed freely across threads.
//!
//! ## Stage 2 — Intent extraction (`extract_intent`)
//!
//! Given a natural-language query such as `"click the submit button"`, the engine
//! scores every node in the `SceneGraph` using the algorithms in
//! [`crate::intent_matching`] and returns ranked [`IntentMatch`] candidates.
//!
//! # Example
//!
//! ```rust,ignore
//! use axterminator::intent::{scan_scene, extract_intent};
//!
//! // Stage 1 — snapshot the UI tree (requires live AXUIElementRef)
//! let scene = scan_scene(app_element)?;
//!
//! // Stage 2 — find what the user wants
//! let matches = extract_intent(&scene, "click the submit button");
//! if let Some(best) = matches.first() {
//!     println!("Best match: {:?} (confidence {:.2})", best.node_id, best.confidence);
//! }
//! ```

use std::collections::VecDeque;

use crate::accessibility::{self, AXUIElementRef, attributes};
use crate::error::{AXError, AXResult};
use crate::intent_matching::{MatchContext, score_node};

// ── Public types ──────────────────────────────────────────────────────────────

/// Stable identifier for a node in a [`SceneGraph`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub usize);

/// Snapshot of a single accessibility element.
///
/// All string attributes are cloned from CF strings at scan time so the node
/// owns its data with no raw-pointer lifetime ties.
#[derive(Debug, Clone)]
pub struct SceneNode {
    /// Stable ID within the owning [`SceneGraph`].
    pub id: NodeId,
    /// Parent node (absent for the root).
    pub parent: Option<NodeId>,
    /// Direct children.
    pub children: Vec<NodeId>,
    /// Accessibility role (e.g., `AXButton`).
    pub role: Option<String>,
    /// Title attribute.
    pub title: Option<String>,
    /// Label attribute (AXLabel).
    pub label: Option<String>,
    /// Value attribute (AXValue).
    pub value: Option<String>,
    /// Description attribute (AXDescription).
    pub description: Option<String>,
    /// Unique identifier (AXIdentifier).
    pub identifier: Option<String>,
    /// Bounding rect as `(x, y, width, height)`.
    pub bounds: Option<(f64, f64, f64, f64)>,
    /// Whether the element is enabled.
    pub enabled: bool,
    /// Nesting depth from the root (root = 0).
    pub depth: usize,
}

impl SceneNode {
    /// Collect all non-empty text labels associated with this node.
    ///
    /// Useful for scoring — callers can iterate the returned slice and compare
    /// each label against a query without manually unpacking `Option`s.
    #[must_use]
    pub fn text_labels(&self) -> Vec<&str> {
        [
            self.title.as_deref(),
            self.label.as_deref(),
            self.description.as_deref(),
            self.value.as_deref(),
            self.identifier.as_deref(),
        ]
        .into_iter()
        .flatten()
        .filter(|s| !s.is_empty())
        .collect()
    }

    /// Return the center point of the element's bounding rect, if known.
    #[must_use]
    pub fn center(&self) -> Option<(f64, f64)> {
        self.bounds.map(|(x, y, w, h)| (x + w / 2.0, y + h / 2.0))
    }
}

/// Cached snapshot of an application's accessibility tree.
///
/// Owns all node data — no live CF references are retained after `scan_scene`
/// returns.  The graph is cheap to clone and safe to send across threads.
#[derive(Debug, Clone, Default)]
pub struct SceneGraph {
    /// All nodes indexed by [`NodeId`].
    nodes: Vec<SceneNode>,
}

impl SceneGraph {
    /// Create an empty scene graph (useful for tests or as a default).
    #[must_use]
    pub fn empty() -> Self {
        Self { nodes: Vec::new() }
    }

    /// Number of nodes captured in the snapshot.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Return `true` when the graph contains no nodes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Look up a node by its stable [`NodeId`].
    #[must_use]
    pub fn get(&self, id: NodeId) -> Option<&SceneNode> {
        self.nodes.get(id.0)
    }

    /// Iterate over every node in BFS insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &SceneNode> {
        self.nodes.iter()
    }

    /// Return the root node (first node inserted), if any.
    #[must_use]
    pub fn root(&self) -> Option<&SceneNode> {
        self.nodes.first()
    }

    /// Append a node and return its assigned [`NodeId`].
    pub(crate) fn push(&mut self, mut node: SceneNode) -> NodeId {
        let id = NodeId(self.nodes.len());
        node.id = id;
        self.nodes.push(node);
        id
    }

    /// Retrieve a mutable reference to a node (used during graph construction).
    pub(crate) fn get_mut(&mut self, id: NodeId) -> Option<&mut SceneNode> {
        self.nodes.get_mut(id.0)
    }

    /// Return every node that has `role == target_role`.
    #[must_use]
    pub fn nodes_by_role(&self, target_role: &str) -> Vec<&SceneNode> {
        self.nodes
            .iter()
            .filter(|n| n.role.as_deref() == Some(target_role))
            .collect()
    }
}

/// A ranked result from [`extract_intent`].
#[derive(Debug, Clone)]
pub struct IntentMatch {
    /// The matching node's stable ID.
    pub node_id: NodeId,
    /// Confidence in [0.0, 1.0] — higher is better.
    pub confidence: f64,
    /// Human-readable explanation of why this node matched.
    pub match_reason: String,
}

// ── Stage 1: scan_scene ───────────────────────────────────────────────────────

/// **Stage 1** — Build a `SceneGraph` from a live accessibility element.
///
/// Performs a breadth-first traversal starting at `root_element`.  Each
/// element's string attributes and bounds are read once and stored in a
/// [`SceneNode`].  The traversal is bounded by `max_nodes` to prevent runaway
/// scans on deeply nested trees.
///
/// # Errors
///
/// Returns [`AXError::AccessibilityNotEnabled`] when permissions are absent, or
/// [`AXError::SystemError`] for unexpected API failures.
pub fn scan_scene(root_element: AXUIElementRef) -> AXResult<SceneGraph> {
    scan_scene_bounded(root_element, 2_000)
}

/// Like [`scan_scene`] but with an explicit node limit.
pub fn scan_scene_bounded(root_element: AXUIElementRef, max_nodes: usize) -> AXResult<SceneGraph> {
    if !accessibility::check_accessibility_enabled() {
        return Err(AXError::AccessibilityNotEnabled);
    }

    let mut graph = SceneGraph::default();
    let mut queue: VecDeque<QueuedElement> = VecDeque::new();
    queue.push_back(QueuedElement::borrowed(root_element, None, 0));

    while graph.len() < max_nodes {
        let Some(queued) = queue.pop_front() else {
            break;
        };

        let node = snapshot_element(queued.element, queued.parent, queued.depth);
        let node_id = graph.push(node);

        // Register this child with its parent
        if let Some(pid) = queued.parent {
            if let Some(parent) = graph.get_mut(pid) {
                parent.children.push(node_id);
            }
        }

        // Enqueue children. get_children() returns +1 retained refs; the queue
        // tracks ownership so each child is released after it is snapshotted.
        if let Ok(children) = accessibility::get_children(queued.element) {
            for child_ref in children {
                queue.push_back(QueuedElement::owned(
                    child_ref,
                    Some(node_id),
                    queued.depth + 1,
                ));
            }
        }

        queued.release_if_owned();
    }

    release_queued_elements(queue);

    Ok(graph)
}

/// Work item for [`scan_scene_bounded`].
///
/// The root element is borrowed from the caller. Child refs returned by
/// `accessibility::get_children` are retained and must be released after use.
struct QueuedElement {
    element: AXUIElementRef,
    parent: Option<NodeId>,
    depth: usize,
    owned: bool,
}

impl QueuedElement {
    fn borrowed(element: AXUIElementRef, parent: Option<NodeId>, depth: usize) -> Self {
        Self {
            element,
            parent,
            depth,
            owned: false,
        }
    }

    fn owned(element: AXUIElementRef, parent: Option<NodeId>, depth: usize) -> Self {
        Self {
            element,
            parent,
            depth,
            owned: true,
        }
    }

    fn release_if_owned(self) {
        if self.owned {
            accessibility::release_cf(self.element.cast());
        }
    }
}

fn release_queued_elements(queue: VecDeque<QueuedElement>) {
    for queued in queue {
        queued.release_if_owned();
    }
}

/// Snapshot a single element into a [`SceneNode`] without retaining any CF refs.
fn snapshot_element(elem_ref: AXUIElementRef, parent: Option<NodeId>, depth: usize) -> SceneNode {
    let bounds = read_bounds(elem_ref);
    let enabled =
        accessibility::get_bool_attribute_value(elem_ref, attributes::AX_ENABLED).unwrap_or(true);

    SceneNode {
        id: NodeId(0), // Assigned by SceneGraph::push
        parent,
        children: Vec::new(),
        role: accessibility::get_string_attribute_value(elem_ref, attributes::AX_ROLE),
        title: accessibility::get_string_attribute_value(elem_ref, attributes::AX_TITLE),
        label: accessibility::get_string_attribute_value(elem_ref, attributes::AX_LABEL),
        value: accessibility::get_string_attribute_value(elem_ref, attributes::AX_VALUE),
        description: accessibility::get_string_attribute_value(
            elem_ref,
            attributes::AX_DESCRIPTION,
        ),
        identifier: accessibility::get_string_attribute_value(elem_ref, attributes::AX_IDENTIFIER),
        bounds,
        enabled,
        depth,
    }
}

/// Read position + size from an element and combine into a bounds tuple.
fn read_bounds(elem_ref: AXUIElementRef) -> Option<(f64, f64, f64, f64)> {
    let pos = accessibility::get_position_attribute(elem_ref)?;
    let size = accessibility::get_size_attribute(elem_ref)?;
    Some((pos.x, pos.y, size.width, size.height))
}

// ── Stage 2: extract_intent ───────────────────────────────────────────────────

/// **Stage 2** — Map a user intent query to ranked [`IntentMatch`] candidates.
///
/// Scores every node in `scene` against `query` using the algorithms in
/// [`crate::intent_matching`].  Returns matches sorted by descending confidence;
/// matches below `MIN_CONFIDENCE` are excluded.
///
/// The result vector is empty when nothing in the scene is a plausible match.
///
/// # Arguments
///
/// * `scene`  — Snapshot built by [`scan_scene`].
/// * `query`  — Free-form intent description (e.g., `"click submit button"`).
#[must_use]
pub fn extract_intent(scene: &SceneGraph, query: &str) -> Vec<IntentMatch> {
    let ctx = MatchContext::from_query(query);
    let mut matches: Vec<IntentMatch> = scene
        .iter()
        .filter_map(|node| score_to_match(node, &ctx, scene))
        .collect();

    matches.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    matches
}

/// Minimum confidence threshold for a node to appear in results.
const MIN_CONFIDENCE: f64 = 0.05;

/// Score a single node and return an [`IntentMatch`] if confidence ≥ threshold.
fn score_to_match(node: &SceneNode, ctx: &MatchContext, scene: &SceneGraph) -> Option<IntentMatch> {
    let (confidence, reason) = score_node(node, ctx, scene);
    if confidence >= MIN_CONFIDENCE {
        Some(IntentMatch {
            node_id: node.id,
            confidence,
            match_reason: reason,
        })
    } else {
        None
    }
}

// ── Public helpers ────────────────────────────────────────────────────────────

/// Build a [`SceneGraph`] entirely from owned data (no CF calls).
///
/// This constructor is the intended way to create graphs for testing and for
/// offline processing where the live accessibility tree is not available.
///
/// ```rust
/// use axterminator::intent::{build_scene_from_nodes, SceneNode, NodeId};
///
/// let node = SceneNode {
///     id: NodeId(0),
///     parent: None,
///     children: vec![],
///     role: Some("AXButton".into()),
///     title: Some("Submit".into()),
///     label: None,
///     value: None,
///     description: None,
///     identifier: None,
///     bounds: Some((10.0, 20.0, 80.0, 30.0)),
///     enabled: true,
///     depth: 0,
/// };
/// let graph = build_scene_from_nodes(vec![node]);
/// assert_eq!(graph.len(), 1);
/// assert_eq!(graph.get(NodeId(0)).unwrap().title.as_deref(), Some("Submit"));
/// ```
pub fn build_scene_from_nodes(nodes: Vec<SceneNode>) -> SceneGraph {
    let mut graph = SceneGraph::default();
    for mut node in nodes {
        let id = NodeId(graph.nodes.len());
        node.id = id;
        graph.nodes.push(node);
    }
    graph
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_button(id: usize, title: &str) -> SceneNode {
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
            bounds: Some((0.0, f64::from(id as u32) * 40.0, 100.0, 30.0)),
            enabled: true,
            depth: 1,
        }
    }

    fn make_text_field(id: usize, label: &str) -> SceneNode {
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
            bounds: Some((0.0, f64::from(id as u32) * 40.0, 200.0, 25.0)),
            enabled: true,
            depth: 1,
        }
    }

    // ── SceneGraph ─────────────────────────────────────────────────────────

    #[test]
    fn scene_graph_empty_reports_zero_length() {
        // GIVEN: Empty graph
        let graph = SceneGraph::empty();
        // THEN: len and is_empty are consistent
        assert_eq!(graph.len(), 0);
        assert!(graph.is_empty());
    }

    #[test]
    fn scene_graph_push_assigns_sequential_ids() {
        // GIVEN: Two nodes
        let mut graph = SceneGraph::empty();
        let id0 = graph.push(make_button(99, "OK"));
        let id1 = graph.push(make_button(42, "Cancel"));
        // THEN: IDs are 0 and 1
        assert_eq!(id0, NodeId(0));
        assert_eq!(id1, NodeId(1));
        assert_eq!(graph.get(id0).unwrap().id, NodeId(0));
        assert_eq!(graph.get(id1).unwrap().id, NodeId(1));
    }

    #[test]
    fn scene_graph_get_returns_correct_node() {
        // GIVEN: Graph with one button
        let mut graph = SceneGraph::empty();
        let id = graph.push(make_button(0, "Save"));
        // WHEN: Looking up by ID
        let node = graph.get(id).unwrap();
        // THEN: Title matches
        assert_eq!(node.title.as_deref(), Some("Save"));
    }

    #[test]
    fn scene_graph_get_out_of_range_returns_none() {
        // GIVEN: Empty graph
        let graph = SceneGraph::empty();
        // THEN: Any lookup returns None
        assert!(graph.get(NodeId(99)).is_none());
    }

    #[test]
    fn scene_graph_nodes_by_role_filters_correctly() {
        // GIVEN: Mixed roles
        let nodes = vec![
            make_button(0, "OK"),
            make_text_field(1, "Email"),
            make_button(2, "Cancel"),
        ];
        let graph = build_scene_from_nodes(nodes);
        // WHEN: Filtering by AXButton
        let buttons = graph.nodes_by_role("AXButton");
        // THEN: Only buttons returned
        assert_eq!(buttons.len(), 2);
    }

    #[test]
    fn scene_graph_root_is_first_node() {
        // GIVEN: Graph with multiple nodes
        let graph = build_scene_from_nodes(vec![make_button(0, "Root"), make_button(1, "Child")]);
        // THEN: Root is node 0
        assert_eq!(graph.root().unwrap().id, NodeId(0));
    }

    // ── SceneNode helpers ──────────────────────────────────────────────────

    #[test]
    fn scene_node_text_labels_returns_non_empty_fields() {
        // GIVEN: Node with title + description, no label
        let node = SceneNode {
            id: NodeId(0),
            parent: None,
            children: vec![],
            role: Some("AXButton".into()),
            title: Some("Submit".into()),
            label: None,
            value: None,
            description: Some("Submit the form".into()),
            identifier: Some("btn_submit".into()),
            bounds: None,
            enabled: true,
            depth: 0,
        };
        let labels = node.text_labels();
        // THEN: title, description, identifier appear; label does not
        assert!(labels.contains(&"Submit"));
        assert!(labels.contains(&"Submit the form"));
        assert!(labels.contains(&"btn_submit"));
        assert_eq!(labels.len(), 3);
    }

    #[test]
    fn scene_node_center_computed_correctly() {
        // GIVEN: Node at (10, 20) with size 80×30
        let node = make_button(0, "OK");
        // WHEN: Computing center — bounds are (0, 0, 100, 30)
        let (cx, cy) = node.center().unwrap();
        // THEN: center = (50, 15)
        assert_eq!(cx, 50.0);
        assert_eq!(cy, 15.0);
    }

    #[test]
    fn scene_node_center_returns_none_without_bounds() {
        // GIVEN: Node without bounds
        let mut node = make_button(0, "OK");
        node.bounds = None;
        // THEN: center is None
        assert!(node.center().is_none());
    }

    // ── extract_intent ─────────────────────────────────────────────────────

    #[test]
    fn extract_intent_finds_exact_title_match() {
        // GIVEN: Scene with submit button
        let graph =
            build_scene_from_nodes(vec![make_button(0, "Submit"), make_button(1, "Cancel")]);
        // WHEN: Intent targets submit
        let results = extract_intent(&graph, "submit");
        // THEN: Submit is ranked first
        assert!(!results.is_empty());
        assert_eq!(results[0].node_id, NodeId(0));
    }

    #[test]
    fn extract_intent_returns_sorted_by_confidence_descending() {
        // GIVEN: Scene with several buttons
        let graph = build_scene_from_nodes(vec![
            make_button(0, "Submit"),
            make_button(1, "Cancel"),
            make_button(2, "Submit Form"),
        ]);
        // WHEN: Query for "submit"
        let results = extract_intent(&graph, "submit");
        // THEN: Results sorted descending
        for window in results.windows(2) {
            assert!(window[0].confidence >= window[1].confidence);
        }
    }

    #[test]
    fn extract_intent_empty_scene_returns_empty_results() {
        // GIVEN: Empty scene
        let graph = SceneGraph::empty();
        // THEN: No matches
        let results = extract_intent(&graph, "click ok");
        assert!(results.is_empty());
    }

    #[test]
    fn extract_intent_match_reason_is_non_empty() {
        // GIVEN: Scene with one button
        let graph = build_scene_from_nodes(vec![make_button(0, "OK")]);
        // WHEN: Querying
        let results = extract_intent(&graph, "ok");
        // THEN: Reason is not blank
        assert!(!results.is_empty());
        assert!(!results[0].match_reason.is_empty());
    }

    #[test]
    fn extract_intent_role_hint_boosts_button_for_click_query() {
        // GIVEN: Scene with button and text field labeled "Login"
        let graph =
            build_scene_from_nodes(vec![make_button(0, "Login"), make_text_field(1, "Login")]);
        // WHEN: Intent says "click the login button"
        let results = extract_intent(&graph, "click the login button");
        // THEN: Button should rank first (role hint)
        assert!(!results.is_empty());
        assert_eq!(
            results[0].node_id,
            NodeId(0),
            "Button should beat text field for click-button query"
        );
    }

    #[test]
    fn build_scene_from_nodes_assigns_ids_sequentially() {
        // GIVEN: 3 pre-built nodes
        let nodes: Vec<SceneNode> = (0..3).map(|i| make_button(i, "X")).collect();
        let graph = build_scene_from_nodes(nodes);
        // THEN: IDs are 0, 1, 2
        for i in 0..3 {
            assert_eq!(graph.get(NodeId(i)).unwrap().id, NodeId(i));
        }
    }
}
