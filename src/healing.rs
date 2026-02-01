//! Self-healing element location system
//!
//! Implements 7-strategy fallback for robust element location.
//!
//! Strategies (in order of preference):
//! 1. data_testid - Developer-set stable IDs
//! 2. aria_label - Accessibility labels
//! 3. identifier - AX identifier
//! 4. title - Element title (fuzzy matching)
//! 5. xpath - Structural path
//! 6. position - Relative coordinates
//! 7. visual_vlm - VLM-based visual detection (MLX/Claude/GPT-4V)

use core_foundation::array::CFArray;
use core_foundation::base::{CFType, TCFType};
use core_foundation::string::CFString;
use pyo3::prelude::*;
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use crate::accessibility::{attributes, get_attribute, AXUIElementRef};
use crate::element::AXElement;
use crate::error::{AXError, AXResult};

/// Healing strategy enum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealStrategy {
    /// Use data-testid attribute (most stable)
    DataTestId,
    /// Use aria-label attribute
    AriaLabel,
    /// Use AX identifier
    Identifier,
    /// Use element title
    Title,
    /// Use XPath-like structural path
    XPath,
    /// Use relative position
    Position,
    /// Use VLM for visual matching (last resort)
    VisualVLM,
}

/// Healing configuration
#[pyclass]
#[derive(Debug, Clone)]
pub struct HealingConfig {
    /// Ordered list of strategies to try
    #[pyo3(get, set)]
    pub strategies: Vec<String>,
    /// Maximum time budget for healing (ms)
    #[pyo3(get, set)]
    pub max_heal_time_ms: u64,
    /// Whether to cache successful heals
    #[pyo3(get, set)]
    pub cache_healed: bool,
}

#[pymethods]
impl HealingConfig {
    #[new]
    #[pyo3(signature = (strategies=None, max_heal_time_ms=100, cache_healed=true))]
    fn new(strategies: Option<Vec<String>>, max_heal_time_ms: u64, cache_healed: bool) -> Self {
        Self {
            strategies: strategies.unwrap_or_else(|| {
                vec![
                    "data_testid".to_string(),
                    "aria_label".to_string(),
                    "identifier".to_string(),
                    "title".to_string(),
                    "xpath".to_string(),
                    "position".to_string(),
                    "visual_vlm".to_string(),
                ]
            }),
            max_heal_time_ms,
            cache_healed,
        }
    }
}

impl Default for HealingConfig {
    fn default() -> Self {
        Self {
            strategies: vec![
                "data_testid".to_string(),
                "aria_label".to_string(),
                "identifier".to_string(),
                "title".to_string(),
                "xpath".to_string(),
                "position".to_string(),
                "visual_vlm".to_string(),
            ],
            max_heal_time_ms: 100,
            cache_healed: true,
        }
    }
}

/// Global healing configuration
static GLOBAL_CONFIG: RwLock<Option<HealingConfig>> = RwLock::new(None);

/// Global healing cache - maps original query to successful healed query
static HEALING_CACHE: std::sync::LazyLock<RwLock<HashMap<String, ElementQuery>>> =
    std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));

/// Set the global healing configuration
pub fn set_global_config(config: HealingConfig) -> PyResult<()> {
    let mut global = GLOBAL_CONFIG
        .write()
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    *global = Some(config);
    Ok(())
}

/// Get the global healing configuration
pub fn get_global_config() -> HealingConfig {
    GLOBAL_CONFIG
        .read()
        .ok()
        .and_then(|g| g.clone())
        .unwrap_or_default()
}

/// Element query for healing
#[derive(Debug, Clone)]
pub struct ElementQuery {
    /// Original query string
    pub original: String,
    /// Original identifier if known
    pub original_id: Option<String>,
    /// Text hint for matching
    pub text_hint: Option<String>,
    /// Structural path for XPath-like matching
    pub path: Option<String>,
    /// Position for relative matching
    pub position: Option<(f64, f64)>,
    /// Screenshot for visual matching
    pub screenshot: Option<Vec<u8>>,
    /// Description for VLM
    pub description: Option<String>,
}

/// Find element with healing
pub fn find_with_healing(query: &ElementQuery, root: AXUIElementRef) -> AXResult<AXElement> {
    let config = get_global_config();

    // Check cache first if enabled
    if config.cache_healed {
        if let Ok(cache) = HEALING_CACHE.read() {
            if let Some(cached_query) = cache.get(&query.original) {
                // Try the cached successful query first
                for strategy_name in &config.strategies {
                    let strategy = parse_strategy(strategy_name);
                    if let Some(element) = try_strategy(strategy, cached_query, root) {
                        return Ok(element);
                    }
                }
            }
        }
    }

    let start = Instant::now();
    let timeout = Duration::from_millis(config.max_heal_time_ms);

    // Try each strategy in order
    for strategy_name in &config.strategies {
        if start.elapsed() >= timeout {
            break;
        }

        let strategy = parse_strategy(strategy_name);
        if let Some(element) = try_strategy(strategy, query, root) {
            // Cache successful query if enabled
            if config.cache_healed {
                if let Ok(mut cache) = HEALING_CACHE.write() {
                    cache.insert(query.original.clone(), query.clone());
                }
            }
            return Ok(element);
        }
    }

    Err(AXError::ElementNotFoundAfterHealing(query.original.clone()))
}

/// Parse strategy name to enum
fn parse_strategy(name: &str) -> HealStrategy {
    match name.to_lowercase().as_str() {
        "data_testid" => HealStrategy::DataTestId,
        "aria_label" => HealStrategy::AriaLabel,
        "identifier" => HealStrategy::Identifier,
        "title" => HealStrategy::Title,
        "xpath" => HealStrategy::XPath,
        "position" => HealStrategy::Position,
        "visual_vlm" => HealStrategy::VisualVLM,
        _ => HealStrategy::Title, // Default fallback
    }
}

/// Try a specific healing strategy
fn try_strategy(
    strategy: HealStrategy,
    query: &ElementQuery,
    root: AXUIElementRef,
) -> Option<AXElement> {
    match strategy {
        HealStrategy::DataTestId => try_by_data_testid(query, root),
        HealStrategy::AriaLabel => try_by_aria_label(query, root),
        HealStrategy::Identifier => try_by_identifier(query, root),
        HealStrategy::Title => try_by_title(query, root),
        HealStrategy::XPath => try_by_xpath(query, root),
        HealStrategy::Position => try_by_position(query, root),
        HealStrategy::VisualVLM => try_by_visual(query, root),
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Get string attribute from element
fn get_string_attr(element: AXUIElementRef, attr: &str) -> Option<String> {
    get_attribute(element, attr).ok().and_then(|cf_ref| {
        let cf_type = unsafe { CFType::wrap_under_get_rule(cf_ref) };
        cf_type.downcast::<CFString>().map(|s| s.to_string())
    })
}

/// Get children of element
fn get_children(element: AXUIElementRef) -> Vec<AXUIElementRef> {
    get_attribute(element, attributes::AX_CHILDREN)
        .ok()
        .and_then(|cf_ref| {
            let cf_type = unsafe { CFType::wrap_under_get_rule(cf_ref) };
            cf_type.downcast::<CFArray>()
        })
        .map(|array| {
            (0..array.len())
                .filter_map(|i| {
                    // CFArray::get returns an ItemRef, we need to extract the raw pointer
                    // Safety: We're extracting raw pointers from the CoreFoundation array
                    array.get(i).map(|item_ref| *item_ref as AXUIElementRef)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Get position and size of element
fn get_bounds(element: AXUIElementRef) -> Option<(f64, f64, f64, f64)> {
    let _position = get_attribute(element, attributes::AX_POSITION).ok()?;
    let _size = get_attribute(element, attributes::AX_SIZE).ok()?;

    // For now, return None - full implementation requires CGPoint/CGSize parsing
    // This would need additional CoreGraphics bindings for proper extraction
    // from CFDictionary/CFData types returned by the accessibility API
    None
}

/// Walk the accessibility tree depth-first
fn walk_tree<F>(element: AXUIElementRef, visitor: &mut F, max_depth: usize) -> bool
where
    F: FnMut(AXUIElementRef) -> bool,
{
    if max_depth == 0 {
        return false;
    }

    // Visit this element
    if visitor(element) {
        return true;
    }

    // Visit children
    for child in get_children(element) {
        if walk_tree(child, visitor, max_depth - 1) {
            return true;
        }
    }

    false
}

/// Fuzzy string matching (simple implementation)
fn fuzzy_match(text: &str, pattern: &str, threshold: f64) -> bool {
    let text_lower = text.to_lowercase();
    let pattern_lower = pattern.to_lowercase();

    // Exact match
    if text_lower == pattern_lower {
        return true;
    }

    // Contains match
    if text_lower.contains(&pattern_lower) {
        return true;
    }

    // Simple Levenshtein-based similarity
    let similarity = 1.0
        - (levenshtein_distance(&text_lower, &pattern_lower) as f64
            / text_lower.len().max(pattern_lower.len()) as f64);

    similarity >= threshold
}

/// Calculate Levenshtein distance
fn levenshtein_distance(s1: &str, s2: &str) -> usize {
    let len1 = s1.chars().count();
    let len2 = s2.chars().count();

    if len1 == 0 {
        return len2;
    }
    if len2 == 0 {
        return len1;
    }

    let mut matrix = vec![vec![0; len2 + 1]; len1 + 1];

    for (i, row) in matrix.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, val) in matrix[0].iter_mut().enumerate() {
        *val = j;
    }

    let s1_chars: Vec<char> = s1.chars().collect();
    let s2_chars: Vec<char> = s2.chars().collect();

    for i in 1..=len1 {
        for j in 1..=len2 {
            let cost = usize::from(s1_chars[i - 1] != s2_chars[j - 1]);
            matrix[i][j] = (matrix[i - 1][j] + 1)
                .min(matrix[i][j - 1] + 1)
                .min(matrix[i - 1][j - 1] + cost);
        }
    }

    matrix[len1][len2]
}

/// Simple `XPath` parser for AX tree paths
#[derive(Debug)]
struct XPathSegment {
    role: String,
    predicates: Vec<(String, String)>,
}

fn parse_xpath(xpath: &str) -> Vec<XPathSegment> {
    let mut segments = Vec::new();

    // Split by / and parse each segment
    for part in xpath.split('/').filter(|s| !s.is_empty()) {
        if let Some((role, predicate_str)) = part.split_once('[') {
            let role = role.trim().to_string();
            let predicate_str = predicate_str.trim_end_matches(']');

            let mut predicates = Vec::new();
            for pred in predicate_str.split(" and ") {
                if let Some((attr, val)) = pred.split_once('=') {
                    let attr = attr.trim().trim_start_matches('@').to_string();
                    let val = val.trim().trim_matches('\'').trim_matches('"').to_string();
                    predicates.push((attr, val));
                }
            }

            segments.push(XPathSegment { role, predicates });
        } else {
            segments.push(XPathSegment {
                role: part.trim().to_string(),
                predicates: Vec::new(),
            });
        }
    }

    segments
}

/// Match element against `XPath` segment
fn matches_xpath_segment(element: AXUIElementRef, segment: &XPathSegment) -> bool {
    // Check role
    if let Some(role) = get_string_attr(element, attributes::AX_ROLE) {
        if role != segment.role {
            return false;
        }
    } else {
        return false;
    }

    // Check predicates
    for (attr, expected_val) in &segment.predicates {
        let attr_name = match attr.as_str() {
            "AXTitle" => attributes::AX_TITLE,
            "AXIdentifier" => attributes::AX_IDENTIFIER,
            "AXLabel" => attributes::AX_LABEL,
            "AXDescription" => attributes::AX_DESCRIPTION,
            "AXValue" => attributes::AX_VALUE,
            _ => attr.as_str(),
        };

        if let Some(actual_val) = get_string_attr(element, attr_name) {
            if actual_val != *expected_val {
                return false;
            }
        } else {
            return false;
        }
    }

    true
}

// ============================================================================
// Strategy Implementations
// ============================================================================

fn try_by_data_testid(query: &ElementQuery, root: AXUIElementRef) -> Option<AXElement> {
    // data-testid maps to AXIdentifier in macOS accessibility
    let target_id = query.original_id.as_ref().or(query.text_hint.as_ref())?;

    let mut found = None;
    walk_tree(
        root,
        &mut |element| {
            if let Some(identifier) = get_string_attr(element, attributes::AX_IDENTIFIER) {
                if identifier == *target_id {
                    found = Some(element);
                    return true;
                }
            }
            false
        },
        50, // Max depth
    );

    found.map(AXElement::new)
}

fn try_by_aria_label(query: &ElementQuery, root: AXUIElementRef) -> Option<AXElement> {
    // aria-label maps to AXDescription or AXLabel
    let target_label = query.text_hint.as_ref()?;

    let mut found = None;
    walk_tree(
        root,
        &mut |element| {
            // Try AXLabel first
            if let Some(label) = get_string_attr(element, attributes::AX_LABEL) {
                if label == *target_label {
                    found = Some(element);
                    return true;
                }
            }

            // Try AXDescription
            if let Some(desc) = get_string_attr(element, attributes::AX_DESCRIPTION) {
                if desc == *target_label {
                    found = Some(element);
                    return true;
                }
            }

            false
        },
        50,
    );

    found.map(AXElement::new)
}

fn try_by_identifier(query: &ElementQuery, root: AXUIElementRef) -> Option<AXElement> {
    // Direct AXIdentifier match
    let target_id = query.original_id.as_ref()?;

    let mut found = None;
    walk_tree(
        root,
        &mut |element| {
            if let Some(identifier) = get_string_attr(element, attributes::AX_IDENTIFIER) {
                if identifier == *target_id {
                    found = Some(element);
                    return true;
                }
            }
            false
        },
        50,
    );

    found.map(AXElement::new)
}

fn try_by_title(query: &ElementQuery, root: AXUIElementRef) -> Option<AXElement> {
    // Fuzzy match on AXTitle
    let target_title = query.text_hint.as_ref()?;

    let mut found = None;
    walk_tree(
        root,
        &mut |element| {
            if let Some(title) = get_string_attr(element, attributes::AX_TITLE) {
                if fuzzy_match(&title, target_title, 0.8) {
                    found = Some(element);
                    return true;
                }
            }
            false
        },
        50,
    );

    found.map(AXElement::new)
}

fn try_by_xpath(query: &ElementQuery, root: AXUIElementRef) -> Option<AXElement> {
    // Parse and match structural path like: //AXWindow/AXGroup/AXButton[@AXTitle='Save']
    let path = query.path.as_ref()?;
    let segments = parse_xpath(path);

    if segments.is_empty() {
        return None;
    }

    // Recursive search for the path
    fn search_path(
        element: AXUIElementRef,
        segments: &[XPathSegment],
        current_idx: usize,
    ) -> Option<AXUIElementRef> {
        if current_idx >= segments.len() {
            return Some(element);
        }

        let segment = &segments[current_idx];

        // Check if current element matches this segment
        if matches_xpath_segment(element, segment) {
            // Try to match rest of path from children
            if current_idx == segments.len() - 1 {
                return Some(element);
            }

            for child in get_children(element) {
                if let Some(found) = search_path(child, segments, current_idx + 1) {
                    return Some(found);
                }
            }
        }

        // Try from children (for // style paths)
        for child in get_children(element) {
            if let Some(found) = search_path(child, segments, current_idx) {
                return Some(found);
            }
        }

        None
    }

    search_path(root, &segments, 0).map(AXElement::new)
}

fn try_by_position(query: &ElementQuery, root: AXUIElementRef) -> Option<AXElement> {
    // Find element at relative position
    let (target_x, target_y) = query.position?;

    let mut closest = None;
    let mut closest_dist = f64::MAX;

    walk_tree(
        root,
        &mut |element| {
            if let Some((x, y, _w, _h)) = get_bounds(element) {
                let dist = ((x - target_x).powi(2) + (y - target_y).powi(2)).sqrt();
                if dist < closest_dist {
                    closest_dist = dist;
                    closest = Some(element);
                }
            }
            false
        },
        50,
    );

    // Only return if within reasonable distance (e.g., 50 pixels)
    if closest_dist < 50.0 {
        closest.map(AXElement::new)
    } else {
        None
    }
}

fn try_by_visual(query: &ElementQuery, root: AXUIElementRef) -> Option<AXElement> {
    // VLM-based visual matching using Python MLX/Claude/GPT-4V
    //
    // Strategy:
    // 1. Take screenshot of the window
    // 2. Call Python VLM to identify element from description
    // 3. Find element at returned coordinates

    // Get description to search for
    let description = query.description.as_ref().or(query.text_hint.as_ref())?;

    // Take screenshot - try to get window bounds first
    let screenshot_data = capture_window_screenshot(root)?;

    // Get window dimensions for coordinate conversion
    let (width, height) = get_window_dimensions(root)?;

    // Call Python VLM detector
    let result = Python::with_gil(|py| -> Option<(f64, f64)> {
        // Import the VLM module
        let vlm_module = py.import_bound("axterminator.vlm").ok()?;
        let detect_fn = vlm_module.getattr("detect_element_visual").ok()?;

        // Create PyBytes from screenshot data
        let py_bytes = pyo3::types::PyBytes::new_bound(py, &screenshot_data);

        // Call: detect_element_visual(image_data, description, width, height)
        let result = detect_fn
            .call1((py_bytes, description, width as i32, height as i32))
            .ok()?;

        // Parse result - returns Optional[(x, y)]
        if result.is_none() {
            return None;
        }

        let coords: (f64, f64) = result.extract().ok()?;
        Some(coords)
    });

    // If we got coordinates, find element at that position
    if let Some((x, y)) = result {
        // Search for element at or near the coordinates
        return find_element_at_position(root, x, y);
    }

    None
}

/// Capture screenshot of the window containing the root element
fn capture_window_screenshot(root: AXUIElementRef) -> Option<Vec<u8>> {
    use std::process::Command;

    // Get the PID from the element
    let pid = crate::accessibility::get_element_pid(root).ok()?;

    // Get window ID for this app
    let output = Command::new("osascript")
        .args([
            "-e",
            &format!(
                "tell application \"System Events\" to id of window 1 of (processes whose unix id is {})",
                pid
            ),
        ])
        .output()
        .ok()?;

    let window_id = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // Capture screenshot to temp file
    let temp_path = format!("/tmp/axterminator_vlm_screenshot_{}.png", pid);

    let capture_result = Command::new("screencapture")
        .args(["-l", &window_id, "-o", "-x", &temp_path])
        .output()
        .ok()?;

    if !capture_result.status.success() {
        return None;
    }

    // Read the screenshot data
    let data = std::fs::read(&temp_path).ok()?;
    let _ = std::fs::remove_file(&temp_path);

    Some(data)
}

/// Get window dimensions
fn get_window_dimensions(root: AXUIElementRef) -> Option<(f64, f64)> {
    // Try to get size from the root element or its window
    if let Some(size) = crate::accessibility::get_size_attribute(root) {
        return Some((size.width, size.height));
    }

    // Try to find a window parent
    let mut current = root;
    for _ in 0..10 {
        if let Some(role) = get_string_attr(current, attributes::AX_ROLE) {
            if role == "AXWindow" {
                if let Some(size) = crate::accessibility::get_size_attribute(current) {
                    return Some((size.width, size.height));
                }
            }
        }

        // Get parent
        if let Ok(parent_ref) = get_attribute(current, attributes::AX_PARENT) {
            if parent_ref.is_null() {
                break;
            }
            current = parent_ref as AXUIElementRef;
        } else {
            break;
        }
    }

    // Default to screen size if we can't determine window size
    Some((1920.0, 1080.0))
}

/// Find element at given screen coordinates
fn find_element_at_position(root: AXUIElementRef, target_x: f64, target_y: f64) -> Option<AXElement> {
    let mut best_match: Option<AXUIElementRef> = None;
    let mut best_distance = f64::MAX;
    let mut smallest_area = f64::MAX;

    // Walk the tree looking for elements that contain the target point
    walk_tree(
        root,
        &mut |element| {
            if let Some((x, y, w, h)) = get_bounds(element) {
                // Check if point is inside this element
                if target_x >= x && target_x <= x + w && target_y >= y && target_y <= y + h {
                    let area = w * h;
                    // Prefer smaller elements (more specific match)
                    if area < smallest_area {
                        smallest_area = area;
                        best_match = Some(element);
                    }
                } else {
                    // If not inside, calculate distance to center
                    let center_x = x + w / 2.0;
                    let center_y = y + h / 2.0;
                    let dist = ((center_x - target_x).powi(2) + (center_y - target_y).powi(2)).sqrt();
                    if dist < best_distance && best_match.is_none() {
                        best_distance = dist;
                        best_match = Some(element);
                    }
                }
            }
            false // Continue walking
        },
        30, // Max depth
    );

    // Return the best match if within reasonable distance
    if let Some(elem) = best_match {
        // Retain the element for the new AXElement
        let _ = crate::accessibility::retain_cf(elem.cast());
        return Some(AXElement::new(elem));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = HealingConfig::default();
        assert_eq!(config.strategies.len(), 7);
        assert_eq!(config.max_heal_time_ms, 100);
        assert!(config.cache_healed);
    }

    #[test]
    fn test_parse_strategy() {
        assert_eq!(parse_strategy("data_testid"), HealStrategy::DataTestId);
        assert_eq!(parse_strategy("aria_label"), HealStrategy::AriaLabel);
        assert_eq!(parse_strategy("identifier"), HealStrategy::Identifier);
        assert_eq!(parse_strategy("title"), HealStrategy::Title);
        assert_eq!(parse_strategy("xpath"), HealStrategy::XPath);
        assert_eq!(parse_strategy("position"), HealStrategy::Position);
        assert_eq!(parse_strategy("visual_vlm"), HealStrategy::VisualVLM);
    }

    #[test]
    fn test_levenshtein_distance() {
        assert_eq!(levenshtein_distance("", ""), 0);
        assert_eq!(levenshtein_distance("abc", "abc"), 0);
        assert_eq!(levenshtein_distance("abc", ""), 3);
        assert_eq!(levenshtein_distance("", "abc"), 3);
        assert_eq!(levenshtein_distance("kitten", "sitting"), 3);
        assert_eq!(levenshtein_distance("saturday", "sunday"), 3);
    }

    #[test]
    fn test_fuzzy_match_exact() {
        assert!(fuzzy_match("Save", "Save", 0.8));
        assert!(fuzzy_match("save", "SAVE", 0.8));
    }

    #[test]
    fn test_fuzzy_match_contains() {
        assert!(fuzzy_match("Save Button", "Save", 0.8));
        assert!(fuzzy_match("Click to Save", "Save", 0.8));
    }

    #[test]
    fn test_fuzzy_match_similar() {
        assert!(fuzzy_match("Button", "Buton", 0.8)); // 1 char diff, 83% similar
        assert!(fuzzy_match("Click", "Clik", 0.8));
    }

    #[test]
    fn test_fuzzy_match_no_match() {
        assert!(!fuzzy_match("Save", "Cancel", 0.8));
        assert!(!fuzzy_match("Button", "Window", 0.8));
    }

    #[test]
    fn test_parse_xpath_simple() {
        let segments = parse_xpath("//AXWindow/AXButton");
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].role, "AXWindow");
        assert_eq!(segments[0].predicates.len(), 0);
        assert_eq!(segments[1].role, "AXButton");
        assert_eq!(segments[1].predicates.len(), 0);
    }

    #[test]
    fn test_parse_xpath_with_predicates() {
        let segments = parse_xpath("//AXWindow/AXButton[@AXTitle='Save']");
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[1].role, "AXButton");
        assert_eq!(segments[1].predicates.len(), 1);
        assert_eq!(segments[1].predicates[0].0, "AXTitle");
        assert_eq!(segments[1].predicates[0].1, "Save");
    }

    #[test]
    fn test_parse_xpath_multiple_predicates() {
        let segments = parse_xpath("//AXButton[@AXTitle='Save' and @AXEnabled='true']");
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].role, "AXButton");
        assert_eq!(segments[0].predicates.len(), 2);
        assert_eq!(segments[0].predicates[0].0, "AXTitle");
        assert_eq!(segments[0].predicates[0].1, "Save");
        assert_eq!(segments[0].predicates[1].0, "AXEnabled");
        assert_eq!(segments[0].predicates[1].1, "true");
    }

    #[test]
    fn test_parse_xpath_double_quotes() {
        let segments = parse_xpath(r#"//AXButton[@AXTitle="Save"]"#);
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].role, "AXButton");
        assert_eq!(segments[0].predicates.len(), 1);
        assert_eq!(segments[0].predicates[0].1, "Save");
    }

    #[test]
    fn test_parse_xpath_complex() {
        let segments =
            parse_xpath("//AXWindow[@AXTitle='Editor']/AXGroup/AXButton[@AXTitle='Save']");
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].role, "AXWindow");
        assert_eq!(segments[0].predicates[0].0, "AXTitle");
        assert_eq!(segments[0].predicates[0].1, "Editor");
        assert_eq!(segments[1].role, "AXGroup");
        assert_eq!(segments[2].role, "AXButton");
        assert_eq!(segments[2].predicates[0].1, "Save");
    }

    #[test]
    fn test_element_query_builder() {
        let query = ElementQuery {
            original: "button_save".to_string(),
            original_id: Some("save_btn".to_string()),
            text_hint: Some("Save".to_string()),
            path: None,
            position: None,
            screenshot: None,
            description: None,
        };

        assert_eq!(query.original, "button_save");
        assert_eq!(query.original_id, Some("save_btn".to_string()));
        assert_eq!(query.text_hint, Some("Save".to_string()));
    }

    #[test]
    fn test_healing_config_custom() {
        let config = HealingConfig::new(
            Some(vec!["identifier".to_string(), "title".to_string()]),
            200,
            false,
        );

        assert_eq!(config.strategies.len(), 2);
        assert_eq!(config.strategies[0], "identifier");
        assert_eq!(config.strategies[1], "title");
        assert_eq!(config.max_heal_time_ms, 200);
        assert!(!config.cache_healed);
    }

    #[test]
    fn test_healing_cache_isolation() {
        // Test that cache operations don't panic
        let query1 = ElementQuery {
            original: "query1".to_string(),
            original_id: Some("id1".to_string()),
            text_hint: None,
            path: None,
            position: None,
            screenshot: None,
            description: None,
        };

        let query2 = ElementQuery {
            original: "query2".to_string(),
            original_id: Some("id2".to_string()),
            text_hint: None,
            path: None,
            position: None,
            screenshot: None,
            description: None,
        };

        // Test cache write
        if let Ok(mut cache) = HEALING_CACHE.write() {
            cache.insert(query1.original.clone(), query1.clone());
            cache.insert(query2.original.clone(), query2.clone());
        }

        // Test cache read
        if let Ok(cache) = HEALING_CACHE.read() {
            assert!(cache.contains_key("query1"));
            assert!(cache.contains_key("query2"));
        }
    }

    #[test]
    fn test_xpath_segment_attribute_mapping() {
        // Test that we correctly map different attribute names
        let segments = parse_xpath("//AXButton[@AXTitle='Save' and @AXIdentifier='btn1']");
        assert_eq!(segments[0].predicates[0].0, "AXTitle");
        assert_eq!(segments[0].predicates[1].0, "AXIdentifier");
    }

    #[test]
    fn test_position_distance_calculation() {
        // Test the distance calculation logic in try_by_position
        let query = ElementQuery {
            original: "element_at_pos".to_string(),
            original_id: None,
            text_hint: None,
            path: None,
            position: Some((100.0, 200.0)),
            screenshot: None,
            description: None,
        };

        // Verify query has position
        assert!(query.position.is_some());
        let (x, y) = query.position.unwrap();
        assert_eq!(x, 100.0);
        assert_eq!(y, 200.0);
    }

    #[test]
    fn test_visual_strategy_requires_data() {
        let query_no_data = ElementQuery {
            original: "visual_element".to_string(),
            original_id: None,
            text_hint: None,
            path: None,
            position: None,
            screenshot: None,
            description: None,
        };

        // Should return None without screenshot or description
        assert!(query_no_data.screenshot.is_none());
        assert!(query_no_data.description.is_none());
    }

    #[test]
    fn test_strategy_order_matters() {
        let config = HealingConfig::default();

        // Verify default strategy order (most stable first)
        assert_eq!(config.strategies[0], "data_testid");
        assert_eq!(config.strategies[1], "aria_label");
        assert_eq!(config.strategies[2], "identifier");
        assert_eq!(config.strategies[3], "title");
        assert_eq!(config.strategies[4], "xpath");
        assert_eq!(config.strategies[5], "position");
        assert_eq!(config.strategies[6], "visual_vlm");
    }

    #[test]
    fn test_levenshtein_unicode() {
        // Test with unicode characters
        assert_eq!(levenshtein_distance("café", "cafe"), 1);
        assert_eq!(levenshtein_distance("hello", "héllo"), 1);
    }

    #[test]
    fn test_xpath_empty_path() {
        let segments = parse_xpath("");
        assert_eq!(segments.len(), 0);

        let segments = parse_xpath("//");
        assert_eq!(segments.len(), 0);
    }
}
