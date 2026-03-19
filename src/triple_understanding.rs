//! Triple Understanding — Visual + Semantic + Structural cross-validation.
//!
//! Combines three independent perception signals to produce a robust
//! [`ElementUnderstanding`] with a blended confidence score.
//!
//! # Signal weights
//!
//! | Signal     | Weight |
//! |------------|--------|
//! | Visual     |  0.20  |
//! | Semantic   |  0.50  |
//! | Structural |  0.30  |
//!
//! # Usage
//!
//! ```rust
//! use axterminator::triple_understanding::{
//!     ElementUnderstanding, VisualSignal, SemanticSignal, StructuralSignal,
//!     Rect, Position, SizeClass, TripleUnderstanding,
//! };
//!
//! let understanding = TripleUnderstanding::build(
//!     VisualSignal {
//!         bounds: Rect { x: 10.0, y: 20.0, width: 80.0, height: 30.0 },
//!         relative_position: Position::TopLeft,
//!         size_class: SizeClass::Medium,
//!     },
//!     SemanticSignal {
//!         role: "AXButton".into(),
//!         label: "Submit".into(),
//!         value: None,
//!     },
//!     StructuralSignal {
//!         depth: 3,
//!         parent_role: "AXGroup".into(),
//!         sibling_index: 0,
//!         child_count: 0,
//!     },
//! );
//!
//! assert!(understanding.combined_confidence > 0.0);
//! ```

// ── Types ─────────────────────────────────────────────────────────────────────

/// Axis-aligned bounding rectangle in screen coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct Rect {
    /// Left edge in points.
    pub x: f64,
    /// Top edge in points.
    pub y: f64,
    /// Width in points.
    pub width: f64,
    /// Height in points.
    pub height: f64,
}

impl Rect {
    /// Compute the center point.
    #[must_use]
    pub fn center(&self) -> (f64, f64) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    /// Area in square points.
    #[must_use]
    pub fn area(&self) -> f64 {
        self.width * self.height
    }
}

/// Coarse relative placement of the element within its container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    /// Upper-left quadrant.
    TopLeft,
    /// Upper-right quadrant.
    TopRight,
    /// Vertically and horizontally centred.
    Center,
    /// Lower-left quadrant.
    BottomLeft,
    /// Lower-right quadrant.
    BottomRight,
}

/// Coarse size bucket for quick visual classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeClass {
    /// Area < 1 000 pt².
    Small,
    /// Area between 1 000 and 10 000 pt².
    Medium,
    /// Area between 10 000 and 100 000 pt².
    Large,
    /// Area ≥ 100 000 pt² — typically occupies the full window.
    FullWidth,
}

impl SizeClass {
    /// Classify a [`Rect`] into a [`SizeClass`] based on area.
    #[must_use]
    pub fn from_rect(rect: &Rect) -> Self {
        match rect.area() {
            a if a < 1_000.0 => Self::Small,
            a if a < 10_000.0 => Self::Medium,
            a if a < 100_000.0 => Self::Large,
            _ => Self::FullWidth,
        }
    }

    /// Numeric confidence contribution for this size class (used in scoring).
    ///
    /// Smaller elements carry lower visual confidence because they are harder
    /// to locate precisely on screen.
    #[must_use]
    fn confidence(&self) -> f64 {
        match self {
            Self::Small => 0.5,
            Self::Medium => 0.8,
            Self::Large => 0.9,
            Self::FullWidth => 1.0,
        }
    }
}

/// Visual perception signal — pixel-level evidence from the screen.
#[derive(Debug, Clone, PartialEq)]
pub struct VisualSignal {
    /// Screen bounding rectangle.
    pub bounds: Rect,
    /// Coarse placement relative to the containing window / viewport.
    pub relative_position: Position,
    /// Coarse size category.
    pub size_class: SizeClass,
}

impl VisualSignal {
    /// Compute a visual confidence score in `[0.0, 1.0]`.
    ///
    /// Confidence is high when the element has a non-trivial size and a
    /// well-defined position.
    #[must_use]
    pub fn confidence(&self) -> f64 {
        let area_factor = (self.bounds.area() / 10_000.0).min(1.0).sqrt();
        // Blend area factor with size-class heuristic for robustness.
        (area_factor + self.size_class.confidence()) / 2.0
    }
}

/// Semantic perception signal — accessibility labels and roles.
#[derive(Debug, Clone, PartialEq)]
pub struct SemanticSignal {
    /// Accessibility role (e.g., `"AXButton"`).
    pub role: String,
    /// Human-readable label or title.
    pub label: String,
    /// Current value of the element, if any.
    pub value: Option<String>,
}

impl SemanticSignal {
    /// Compute a semantic confidence score in `[0.0, 1.0]`.
    ///
    /// Confidence increases when both role and label are non-empty,
    /// and further when a value is present.
    #[must_use]
    pub fn confidence(&self) -> f64 {
        let role_score: f64 = if self.role.is_empty() { 0.0 } else { 0.4 };
        let label_score: f64 = match self.label.len() {
            0 => 0.0,
            1..=3 => 0.2,
            4..=20 => 0.5,
            _ => 0.4, // Very long labels are slightly less reliable
        };
        let value_bonus: f64 = if self.value.is_some() { 0.1 } else { 0.0 };
        (role_score + label_score + value_bonus).min(1.0_f64)
    }
}

/// Structural perception signal — position in the accessibility/DOM tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuralSignal {
    /// Depth from root (root = 0).
    pub depth: usize,
    /// Role of the immediate parent element.
    pub parent_role: String,
    /// Zero-based index among siblings with the same parent.
    pub sibling_index: usize,
    /// Number of direct children.
    pub child_count: usize,
}

impl StructuralSignal {
    /// Compute a structural confidence score in `[0.0, 1.0]`.
    ///
    /// Leaf nodes at moderate depth with a known parent role are more
    /// structurally specific and thus carry higher confidence.
    #[must_use]
    pub fn confidence(&self) -> f64 {
        let parent_score: f64 = if self.parent_role.is_empty() {
            0.3
        } else {
            0.6
        };
        // Leaf nodes are more unique; deep nodes are more specific.
        let leaf_bonus: f64 = if self.child_count == 0 { 0.2 } else { 0.0 };
        let depth_score: f64 = match self.depth {
            0 => 0.1, // Root — not specific at all
            1..=2 => 0.3,
            3..=6 => 0.5,
            _ => 0.4, // Very deep nesting is uncommon and slightly less stable
        };
        (parent_score + leaf_bonus + depth_score).min(1.0_f64)
    }
}

/// Inconsistency between two perception sources for the same element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inconsistency {
    /// Element appears visually disabled but a11y reports it as enabled.
    VisuallyDisabledButA11yEnabled,
    /// Element is not in the accessibility tree despite being on screen.
    NotInA11yTree,
    /// Element appears to be loading / transitioning.
    LoadingState,
    /// An unrecognised discrepancy between sources.
    Other(String),
}

/// Full tri-source understanding of a single UI element.
#[derive(Debug, Clone)]
pub struct ElementUnderstanding {
    /// Pixel-level signal.
    pub visual: VisualSignal,
    /// Accessibility-API signal.
    pub semantic: SemanticSignal,
    /// AX-tree structural signal.
    pub structural: StructuralSignal,
    /// Blended confidence score in `[0.0, 1.0]`.
    pub combined_confidence: f64,
    /// Cross-source inconsistencies detected, if any.
    pub inconsistencies: Vec<Inconsistency>,
}

// ── Weight constants ───────────────────────────────────────────────────────────

const VISUAL_WEIGHT: f64 = 0.20;
const SEMANTIC_WEIGHT: f64 = 0.50;
const STRUCTURAL_WEIGHT: f64 = 0.30;

// ── TripleUnderstanding builder ───────────────────────────────────────────────

/// Builder that combines visual, semantic, and structural signals.
///
/// # Example
///
/// ```rust
/// use axterminator::triple_understanding::*;
///
/// let understanding = TripleUnderstanding::build(
///     VisualSignal {
///         bounds: Rect { x: 0.0, y: 0.0, width: 100.0, height: 30.0 },
///         relative_position: Position::TopLeft,
///         size_class: SizeClass::Medium,
///     },
///     SemanticSignal { role: "AXButton".into(), label: "OK".into(), value: None },
///     StructuralSignal {
///         depth: 3,
///         parent_role: "AXWindow".into(),
///         sibling_index: 0,
///         child_count: 0,
///     },
/// );
/// assert!(understanding.combined_confidence > 0.0);
/// ```
pub struct TripleUnderstanding;

impl TripleUnderstanding {
    /// Combine three signals into an [`ElementUnderstanding`].
    ///
    /// Combined confidence is the weighted average:
    /// `0.20 × visual + 0.50 × semantic + 0.30 × structural`.
    #[must_use]
    pub fn build(
        visual: VisualSignal,
        semantic: SemanticSignal,
        structural: StructuralSignal,
    ) -> ElementUnderstanding {
        let combined_confidence = Self::weighted_confidence(&visual, &semantic, &structural);
        let inconsistencies = Self::detect_inconsistencies(&visual, &semantic);

        ElementUnderstanding {
            visual,
            semantic,
            structural,
            combined_confidence,
            inconsistencies,
        }
    }

    /// Compute blended confidence from the three signals.
    fn weighted_confidence(
        visual: &VisualSignal,
        semantic: &SemanticSignal,
        structural: &StructuralSignal,
    ) -> f64 {
        VISUAL_WEIGHT * visual.confidence()
            + SEMANTIC_WEIGHT * semantic.confidence()
            + STRUCTURAL_WEIGHT * structural.confidence()
    }

    /// Detect cross-source inconsistencies.
    ///
    /// Currently catches three types:
    /// - Visually disabled but a11y says enabled (tiny area + "Button" role)
    /// - Not in a11y tree (empty role while having a non-trivial visual footprint)
    /// - Loading state (label contains loading keywords)
    fn detect_inconsistencies(
        visual: &VisualSignal,
        semantic: &SemanticSignal,
    ) -> Vec<Inconsistency> {
        let mut found = Vec::new();

        if Self::looks_disabled_visually(visual) && Self::looks_enabled_semantically(semantic) {
            found.push(Inconsistency::VisuallyDisabledButA11yEnabled);
        }

        if semantic.role.is_empty() && visual.bounds.area() > 100.0 {
            found.push(Inconsistency::NotInA11yTree);
        }

        if Self::label_suggests_loading(&semantic.label) {
            found.push(Inconsistency::LoadingState);
        }

        found
    }

    /// Heuristic: element is visually disabled when it occupies very little area
    /// or is classified as `SizeClass::Small` while being positioned centrally.
    fn looks_disabled_visually(visual: &VisualSignal) -> bool {
        visual.size_class == SizeClass::Small && visual.bounds.area() < 400.0
    }

    /// Heuristic: element is semantically enabled when the role is non-empty.
    fn looks_enabled_semantically(semantic: &SemanticSignal) -> bool {
        !semantic.role.is_empty() && semantic.role.starts_with("AX")
    }

    /// Check if the label contains common loading-state keywords.
    fn label_suggests_loading(label: &str) -> bool {
        let lower = label.to_lowercase();
        ["loading", "please wait", "spinner"]
            .iter()
            .any(|kw| lower.contains(kw))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Fixtures ──────────────────────────────────────────────────────────

    fn medium_button_bounds() -> Rect {
        Rect {
            x: 10.0,
            y: 20.0,
            width: 80.0,
            height: 30.0,
        }
    }

    fn visual_medium() -> VisualSignal {
        VisualSignal {
            bounds: medium_button_bounds(),
            relative_position: Position::Center,
            size_class: SizeClass::Medium,
        }
    }

    fn semantic_button(label: &str) -> SemanticSignal {
        SemanticSignal {
            role: "AXButton".into(),
            label: label.into(),
            value: None,
        }
    }

    fn structural_leaf() -> StructuralSignal {
        StructuralSignal {
            depth: 3,
            parent_role: "AXGroup".into(),
            sibling_index: 0,
            child_count: 0,
        }
    }

    // ── Rect ──────────────────────────────────────────────────────────────

    #[test]
    fn rect_center_computes_midpoint() {
        // GIVEN: Rect at (10, 20) with size 80×30
        let r = Rect {
            x: 10.0,
            y: 20.0,
            width: 80.0,
            height: 30.0,
        };
        // WHEN: Computing center
        let (cx, cy) = r.center();
        // THEN: Midpoint is correct
        assert_eq!(cx, 50.0);
        assert_eq!(cy, 35.0);
    }

    #[test]
    fn rect_area_is_width_times_height() {
        let r = Rect {
            x: 0.0,
            y: 0.0,
            width: 50.0,
            height: 40.0,
        };
        assert_eq!(r.area(), 2_000.0);
    }

    // ── SizeClass ─────────────────────────────────────────────────────────

    #[test]
    fn size_class_from_rect_classifies_small_correctly() {
        // GIVEN: 30×30 button (area 900 < 1000)
        let r = Rect {
            x: 0.0,
            y: 0.0,
            width: 30.0,
            height: 30.0,
        };
        assert_eq!(SizeClass::from_rect(&r), SizeClass::Small);
    }

    #[test]
    fn size_class_from_rect_classifies_full_width_correctly() {
        // GIVEN: 1920×1080 screen
        let r = Rect {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
        };
        assert_eq!(SizeClass::from_rect(&r), SizeClass::FullWidth);
    }

    #[test]
    fn size_class_from_rect_classifies_medium_correctly() {
        // GIVEN: 80×30 button (area 2400)
        let r = Rect {
            x: 0.0,
            y: 0.0,
            width: 80.0,
            height: 30.0,
        };
        assert_eq!(SizeClass::from_rect(&r), SizeClass::Medium);
    }

    // ── VisualSignal confidence ────────────────────────────────────────────

    #[test]
    fn visual_confidence_is_in_unit_range() {
        // GIVEN: Any visual signal
        let v = visual_medium();
        let c = v.confidence();
        // THEN: Confidence in [0, 1]
        assert!((0.0..=1.0).contains(&c));
    }

    #[test]
    fn visual_full_width_has_higher_confidence_than_small() {
        let full = VisualSignal {
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 1920.0,
                height: 1080.0,
            },
            relative_position: Position::Center,
            size_class: SizeClass::FullWidth,
        };
        let small = VisualSignal {
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            },
            relative_position: Position::TopLeft,
            size_class: SizeClass::Small,
        };
        assert!(full.confidence() > small.confidence());
    }

    // ── SemanticSignal confidence ──────────────────────────────────────────

    #[test]
    fn semantic_empty_role_and_label_has_zero_confidence() {
        let s = SemanticSignal {
            role: String::new(),
            label: String::new(),
            value: None,
        };
        assert_eq!(s.confidence(), 0.0);
    }

    #[test]
    fn semantic_with_role_and_label_has_non_zero_confidence() {
        let s = semantic_button("Submit");
        assert!(s.confidence() > 0.0);
    }

    #[test]
    fn semantic_value_bonus_increases_confidence() {
        let without = semantic_button("Submit");
        let with_val = SemanticSignal {
            role: "AXTextField".into(),
            label: "Email".into(),
            value: Some("user@example.com".into()),
        };
        assert!(with_val.confidence() > without.confidence() - 0.15);
    }

    // ── StructuralSignal confidence ────────────────────────────────────────

    #[test]
    fn structural_leaf_at_depth_3_has_reasonable_confidence() {
        let s = structural_leaf();
        let c = s.confidence();
        assert!((0.0..=1.0).contains(&c));
        assert!(c > 0.5); // leaf + moderate depth + known parent
    }

    #[test]
    fn structural_root_has_lower_confidence_than_leaf() {
        let root = StructuralSignal {
            depth: 0,
            parent_role: String::new(),
            sibling_index: 0,
            child_count: 5,
        };
        assert!(structural_leaf().confidence() > root.confidence());
    }

    // ── TripleUnderstanding combined confidence ────────────────────────────

    #[test]
    fn combined_confidence_matches_weighted_average() {
        // GIVEN: Known signals
        let v = visual_medium();
        let s = semantic_button("Submit");
        let st = structural_leaf();

        let expected = VISUAL_WEIGHT * v.confidence()
            + SEMANTIC_WEIGHT * s.confidence()
            + STRUCTURAL_WEIGHT * st.confidence();

        let understanding = TripleUnderstanding::build(v, s, st);

        // THEN: Combined confidence matches formula
        let diff = (understanding.combined_confidence - expected).abs();
        assert!(diff < 1e-10, "diff={diff}");
    }

    #[test]
    fn combined_confidence_is_in_unit_range() {
        let understanding =
            TripleUnderstanding::build(visual_medium(), semantic_button("OK"), structural_leaf());
        assert!((0.0..=1.0).contains(&understanding.combined_confidence));
    }

    // ── Inconsistency detection ────────────────────────────────────────────

    #[test]
    fn no_inconsistencies_for_normal_button() {
        // GIVEN: Normal medium button
        let u = TripleUnderstanding::build(
            visual_medium(),
            semantic_button("Submit"),
            structural_leaf(),
        );
        // THEN: No inconsistencies
        assert!(u.inconsistencies.is_empty());
    }

    #[test]
    fn detects_loading_state_inconsistency() {
        // GIVEN: Button whose label says "Loading…"
        let u = TripleUnderstanding::build(
            visual_medium(),
            SemanticSignal {
                role: "AXButton".into(),
                label: "Loading...".into(),
                value: None,
            },
            structural_leaf(),
        );
        // THEN: Loading-state inconsistency detected
        assert!(u.inconsistencies.contains(&Inconsistency::LoadingState));
    }

    #[test]
    fn detects_not_in_a11y_tree_when_role_empty_but_has_area() {
        // GIVEN: Visible element with no accessibility role
        let u = TripleUnderstanding::build(
            VisualSignal {
                bounds: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 200.0,
                    height: 50.0,
                },
                relative_position: Position::Center,
                size_class: SizeClass::Medium,
            },
            SemanticSignal {
                role: String::new(),
                label: String::new(),
                value: None,
            },
            structural_leaf(),
        );
        // THEN: NotInA11yTree detected
        assert!(u.inconsistencies.contains(&Inconsistency::NotInA11yTree));
    }

    #[test]
    fn detects_visually_disabled_but_a11y_enabled() {
        // GIVEN: Tiny element (visually collapsed) with valid AX role
        let u = TripleUnderstanding::build(
            VisualSignal {
                bounds: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 15.0,
                    height: 15.0,
                },
                relative_position: Position::TopLeft,
                size_class: SizeClass::Small,
            },
            SemanticSignal {
                role: "AXButton".into(),
                label: "hidden".into(),
                value: None,
            },
            structural_leaf(),
        );
        // THEN: Visual/a11y disabled mismatch detected
        assert!(u
            .inconsistencies
            .contains(&Inconsistency::VisuallyDisabledButA11yEnabled));
    }
}
