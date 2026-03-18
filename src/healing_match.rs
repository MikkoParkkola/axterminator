//! Fuzzy string matching and XPath parsing for the healing system.
//!
//! Provides `fuzzy_match` for title-based element location and
//! `parse_xpath` / `matches_xpath_segment` for structural path queries.
//! All functions are pure (no I/O) and fully unit-tested in this module.

// ---------------------------------------------------------------------------
// Fuzzy matching
// ---------------------------------------------------------------------------

/// Return `true` when `text` matches `pattern` with at least `threshold`
/// similarity (0.0 – 1.0).
///
/// Matching is tried in order: exact → contains → Levenshtein similarity.
#[allow(dead_code)]
#[must_use]
pub(crate) fn fuzzy_match(text: &str, pattern: &str, threshold: f64) -> bool {
    let text_lower = text.to_lowercase();
    let pattern_lower = pattern.to_lowercase();

    if text_lower == pattern_lower {
        return true;
    }

    if text_lower.contains(&pattern_lower) {
        return true;
    }

    let similarity = 1.0
        - (levenshtein_distance(&text_lower, &pattern_lower) as f64
            / text_lower.len().max(pattern_lower.len()) as f64);

    similarity >= threshold
}

/// Compute the Levenshtein edit distance between two strings.
///
/// Reserved for XPath healing strategy #5 — not yet integrated into healing pipeline.
#[allow(dead_code)]
#[must_use]
pub(crate) fn levenshtein_distance(s1: &str, s2: &str) -> usize {
    let len1 = s1.chars().count();
    let len2 = s2.chars().count();

    if len1 == 0 {
        return len2;
    }
    if len2 == 0 {
        return len1;
    }

    let mut matrix = vec![vec![0usize; len2 + 1]; len1 + 1];

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

// ---------------------------------------------------------------------------
// XPath parsing
// ---------------------------------------------------------------------------

/// One segment of an XPath-style accessibility tree query.
///
/// Example: `AXButton[@AXTitle='Save']` → `role = "AXButton"`,
/// `predicates = [("AXTitle", "Save")]`.
///
/// Reserved for XPath healing strategy #5 — not yet integrated into healing pipeline.
#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct XPathSegment {
    pub(crate) role: String,
    pub(crate) predicates: Vec<(String, String)>,
}

/// Parse a simplified XPath expression into a list of [`XPathSegment`]s.
///
/// Supports `//Role` and `//Role[@Attr='val' and @Attr2='val2']` syntax.
///
/// Reserved for XPath healing strategy #5 — not yet integrated into healing pipeline.
#[allow(dead_code)]
#[must_use]
pub(crate) fn parse_xpath(xpath: &str) -> Vec<XPathSegment> {
    let mut segments = Vec::new();

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

/// Return `true` if `element` matches all role and predicate constraints in `segment`.
///
/// Reserved for XPath healing strategy #5 — not yet integrated into healing pipeline.
#[allow(dead_code)]
#[must_use]
pub(crate) fn matches_xpath_segment(
    element: crate::accessibility::AXUIElementRef,
    segment: &XPathSegment,
) -> bool {
    use crate::accessibility::{attributes, get_string_attribute_value};

    let Some(role) = get_string_attribute_value(element, attributes::AX_ROLE) else {
        return false;
    };
    if role != segment.role {
        return false;
    }

    for (attr, expected_val) in &segment.predicates {
        let attr_name = match attr.as_str() {
            "AXTitle" => attributes::AX_TITLE,
            "AXIdentifier" => attributes::AX_IDENTIFIER,
            "AXLabel" => attributes::AX_LABEL,
            "AXDescription" => attributes::AX_DESCRIPTION,
            "AXValue" => attributes::AX_VALUE,
            _ => attr.as_str(),
        };

        match get_string_attribute_value(element, attr_name) {
            Some(actual) if actual == *expected_val => {}
            _ => return false,
        }
    }

    true
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_parse_xpath_segment_attribute_mapping() {
        let segments = parse_xpath("//AXButton[@AXTitle='Save' and @AXIdentifier='btn1']");
        assert_eq!(segments[0].predicates[0].0, "AXTitle");
        assert_eq!(segments[0].predicates[1].0, "AXIdentifier");
    }

    #[test]
    fn test_xpath_empty_path() {
        let segments = parse_xpath("");
        assert_eq!(segments.len(), 0);

        let segments = parse_xpath("//");
        assert_eq!(segments.len(), 0);
    }

    #[test]
    fn test_levenshtein_unicode() {
        assert_eq!(levenshtein_distance("café", "cafe"), 1);
        assert_eq!(levenshtein_distance("hello", "héllo"), 1);
    }
}
