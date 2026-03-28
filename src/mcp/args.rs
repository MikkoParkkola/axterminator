//! Shared argument-extraction helpers for MCP tool handlers.
//!
//! All handler modules import from here so that field-extraction logic and
//! user-visible error strings stay in one place and cannot drift as the tool
//! surface grows.  The helpers are pure functions of `serde_json::Value` with
//! no side effects — easy to unit-test in isolation.

use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Core macro
// ---------------------------------------------------------------------------

/// Unwrap an `Ok(value)`, or return early from the enclosing handler with
/// [`crate::mcp::protocol::ToolCallResult::error`] when the expression
/// evaluates to `Err(msg)`.
///
/// # Usage
/// ```ignore
/// let app = extract_or_return!(extract_required_string_field(args, "app"));
/// ```
macro_rules! extract_or_return {
    ($result:expr) => {
        match $result {
            Ok(value) => value,
            Err(error) => return crate::mcp::protocol::ToolCallResult::error(error),
        }
    };
}

pub(crate) use extract_or_return;

// ---------------------------------------------------------------------------
// Scalar field extractors
// ---------------------------------------------------------------------------

pub(crate) fn extract_required_string_field(args: &Value, field: &str) -> Result<String, String> {
    args[field]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| format!("Missing required field: {field}"))
}

pub(crate) fn extract_optional_string_field(args: &Value, field: &str) -> Option<String> {
    args[field].as_str().map(str::to_string)
}

pub(crate) fn extract_string_field_or<'a>(
    args: &'a Value,
    field: &str,
    default: &'a str,
) -> &'a str {
    args[field].as_str().unwrap_or(default)
}

pub(crate) fn extract_u64_field_or(args: &Value, field: &str, default: u64) -> u64 {
    args[field].as_u64().unwrap_or(default)
}

/// Extract a required `u64` field. Returns `Err` with the existing MCP
/// integer-field wording when the field is absent or not an unsigned integer.
#[cfg(any(test, feature = "spaces"))]
pub(crate) fn extract_required_u64_field(args: &Value, field: &str) -> Result<u64, String> {
    args[field]
        .as_u64()
        .ok_or_else(|| format!("Missing required field: {field} (integer)"))
}

pub(crate) fn extract_bool_field_or(args: &Value, field: &str, default: bool) -> bool {
    args[field].as_bool().unwrap_or(default)
}

#[cfg(any(test, feature = "camera"))]
pub(crate) fn extract_string_array_field<'a>(args: &'a Value, field: &str) -> Vec<&'a str> {
    parse_json_string_array(&args[field])
}

pub(crate) fn extract_clamped_u64_field_or(
    args: &Value,
    field: &str,
    default: u64,
    min: u64,
    max: u64,
) -> u64 {
    extract_u64_field_or(args, field, default).clamp(min, max)
}

/// Extract a required `i64` field.  Returns `Err` with a human-readable
/// message when the field is absent or not numeric.
pub(crate) fn extract_required_i64_field(args: &Value, field: &str) -> Result<i64, String> {
    args[field]
        .as_i64()
        .ok_or_else(|| format!("Missing required field: {field}"))
}

/// Extract an optional `f64` field, returning `default` when the field is
/// absent or cannot be represented as a float.
pub(crate) fn extract_f64_field_or(args: &Value, field: &str, default: f64) -> f64 {
    args[field].as_f64().unwrap_or(default)
}

// ---------------------------------------------------------------------------
// JSON array helpers
// ---------------------------------------------------------------------------

pub(crate) fn parse_json_array<T, F>(value: &Value, mut parser: F) -> Vec<T>
where
    F: FnMut(&Value) -> Option<T>,
{
    value
        .as_array()
        .map(|arr| arr.iter().filter_map(&mut parser).collect())
        .unwrap_or_default()
}

#[cfg(any(test, feature = "audio", feature = "camera"))]
pub(crate) fn parse_json_string_array(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .map(|arr| arr.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// JSON output helpers
// ---------------------------------------------------------------------------

pub(crate) fn format_bounds(bounds: Option<(f64, f64, f64, f64)>) -> Option<Value> {
    bounds.map(|(x, y, w, h)| json!([x, y, w, h]))
}

// ---------------------------------------------------------------------------
// Composite app-query extractors
// ---------------------------------------------------------------------------

/// Extract the mandatory `app` and `query` string fields from an argument object.
pub(crate) fn extract_app_query(args: &Value) -> Result<(String, String), String> {
    Ok((
        extract_required_string_field(args, "app")?,
        extract_required_string_field(args, "query")?,
    ))
}

/// Extract the mandatory `app` field plus an optional `query` string field.
pub(crate) fn extract_app_optional_query(args: &Value) -> Result<(String, Option<String>), String> {
    Ok((
        extract_required_string_field(args, "app")?,
        extract_optional_string_field(args, "query"),
    ))
}

/// Extract the mandatory `app`, `from_query`, and `to_query` string fields.
pub(crate) fn extract_app_from_to_queries(
    args: &Value,
) -> Result<(String, String, String), String> {
    Ok((
        extract_required_string_field(args, "app")?,
        extract_required_string_field(args, "from_query")?,
        extract_required_string_field(args, "to_query")?,
    ))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::protocol::ToolCallResult;
    use serde_json::json;

    // ------------------------------------------------------------------
    // extract_or_return! macro
    // ------------------------------------------------------------------

    #[test]
    fn extract_or_return_macro_preserves_error_text() {
        fn extract_app(args: &Value) -> ToolCallResult {
            let app = extract_or_return!(extract_required_string_field(args, "app"));
            ToolCallResult::ok_json(json!({ "app": app }))
        }

        let result = extract_app(&json!({}));
        assert!(result.is_error);
        assert_eq!(result.content[0].text, "Missing required field: app");
    }

    // ------------------------------------------------------------------
    // extract_app_query
    // ------------------------------------------------------------------

    #[test]
    fn extract_app_query_succeeds_with_valid_args() {
        let args = json!({"app": "Safari", "query": "Save"});
        let (app, query) = extract_app_query(&args).unwrap();
        assert_eq!(app, "Safari");
        assert_eq!(query, "Save");
    }

    #[test]
    fn extract_app_query_fails_without_app() {
        let args = json!({"query": "Save"});
        assert_eq!(
            extract_app_query(&args).unwrap_err(),
            "Missing required field: app"
        );
    }

    #[test]
    fn extract_app_query_fails_without_query() {
        let args = json!({"app": "Safari"});
        assert_eq!(
            extract_app_query(&args).unwrap_err(),
            "Missing required field: query"
        );
    }

    // ------------------------------------------------------------------
    // extract_app_optional_query
    // ------------------------------------------------------------------

    #[test]
    fn extract_app_optional_query_succeeds_with_query() {
        let args = json!({"app": "Safari", "query": "Save"});
        let (app, query) = extract_app_optional_query(&args).unwrap();
        assert_eq!(app, "Safari");
        assert_eq!(query.as_deref(), Some("Save"));
    }

    #[test]
    fn extract_app_optional_query_succeeds_without_query() {
        let args = json!({"app": "Safari"});
        let (app, query) = extract_app_optional_query(&args).unwrap();
        assert_eq!(app, "Safari");
        assert_eq!(query, None);
    }

    #[test]
    fn extract_app_optional_query_fails_without_app() {
        let args = json!({"query": "Save"});
        assert_eq!(
            extract_app_optional_query(&args).unwrap_err(),
            "Missing required field: app"
        );
    }

    // ------------------------------------------------------------------
    // extract_app_from_to_queries
    // ------------------------------------------------------------------

    #[test]
    fn extract_app_from_to_queries_succeeds_with_valid_args() {
        let args = json!({
            "app": "Finder",
            "from_query": "Downloads",
            "to_query": "Desktop"
        });
        let (app, from_query, to_query) = extract_app_from_to_queries(&args).unwrap();
        assert_eq!(app, "Finder");
        assert_eq!(from_query, "Downloads");
        assert_eq!(to_query, "Desktop");
    }

    #[test]
    fn extract_app_from_to_queries_fails_without_app() {
        let args = json!({"from_query": "Downloads", "to_query": "Desktop"});
        assert_eq!(
            extract_app_from_to_queries(&args).unwrap_err(),
            "Missing required field: app"
        );
    }

    #[test]
    fn extract_app_from_to_queries_fails_without_from_query() {
        let args = json!({"app": "Finder", "to_query": "Desktop"});
        assert_eq!(
            extract_app_from_to_queries(&args).unwrap_err(),
            "Missing required field: from_query"
        );
    }

    #[test]
    fn extract_app_from_to_queries_fails_without_to_query() {
        let args = json!({"app": "Finder", "from_query": "Downloads"});
        assert_eq!(
            extract_app_from_to_queries(&args).unwrap_err(),
            "Missing required field: to_query"
        );
    }

    // ------------------------------------------------------------------
    // Scalar extractors
    // ------------------------------------------------------------------

    #[test]
    fn extract_optional_string_field_returns_some_when_present() {
        let args = json!({"query": "Save"});
        assert_eq!(
            extract_optional_string_field(&args, "query").as_deref(),
            Some("Save")
        );
    }

    #[test]
    fn extract_optional_string_field_returns_none_when_absent() {
        let args = json!({});
        assert_eq!(extract_optional_string_field(&args, "query"), None);
    }

    #[test]
    fn extract_string_field_or_uses_value_then_default() {
        let args = json!({"mode": "focus"});
        assert_eq!(
            extract_string_field_or(&args, "mode", "background"),
            "focus"
        );
        assert_eq!(
            extract_string_field_or(&json!({}), "mode", "background"),
            "background"
        );
    }

    #[test]
    fn extract_u64_field_or_uses_value_then_default() {
        let args = json!({"timeout_ms": 123});
        assert_eq!(extract_u64_field_or(&args, "timeout_ms", 5000), 123);
        assert_eq!(extract_u64_field_or(&json!({}), "timeout_ms", 5000), 5000);
    }

    #[test]
    fn extract_required_u64_field_returns_value_when_present() {
        let args = json!({"space_id": 42});
        assert_eq!(extract_required_u64_field(&args, "space_id").unwrap(), 42);
    }

    #[test]
    fn extract_required_u64_field_errors_when_absent() {
        let err = extract_required_u64_field(&json!({}), "space_id").unwrap_err();
        assert_eq!(err, "Missing required field: space_id (integer)");
    }

    #[test]
    fn extract_required_u64_field_errors_when_wrong_type() {
        let err = extract_required_u64_field(&json!({"space_id": "abc"}), "space_id").unwrap_err();
        assert_eq!(err, "Missing required field: space_id (integer)");
    }

    #[test]
    fn extract_bool_field_or_uses_value_then_default() {
        let args = json!({"confirm": true});
        assert!(extract_bool_field_or(&args, "confirm", false));
        assert!(!extract_bool_field_or(&json!({}), "confirm", false));
    }

    #[test]
    fn extract_string_array_field_filters_non_strings_and_defaults_empty() {
        let args = json!({"gestures": ["wave", 7, "pinch", null]});
        assert_eq!(
            extract_string_array_field(&args, "gestures"),
            vec!["wave", "pinch"]
        );
        assert!(extract_string_array_field(&json!({}), "gestures").is_empty());
    }

    #[test]
    fn extract_clamped_u64_field_or_applies_default_and_bounds() {
        assert_eq!(
            extract_clamped_u64_field_or(&json!({"depth": 0}), "depth", 3, 1, 10),
            1
        );
        assert_eq!(
            extract_clamped_u64_field_or(&json!({"depth": 20}), "depth", 3, 1, 10),
            10
        );
        assert_eq!(
            extract_clamped_u64_field_or(&json!({}), "depth", 3, 1, 10),
            3
        );
    }

    #[test]
    fn extract_required_i64_field_returns_value_when_present() {
        let args = json!({"x": -5, "y": 42});
        assert_eq!(extract_required_i64_field(&args, "x").unwrap(), -5_i64);
        assert_eq!(extract_required_i64_field(&args, "y").unwrap(), 42_i64);
    }

    #[test]
    fn extract_required_i64_field_errors_when_absent() {
        let err = extract_required_i64_field(&json!({}), "x").unwrap_err();
        assert_eq!(err, "Missing required field: x");
    }

    #[test]
    fn extract_required_i64_field_errors_when_wrong_type() {
        let err = extract_required_i64_field(&json!({"x": "not-a-number"}), "x").unwrap_err();
        assert_eq!(err, "Missing required field: x");
    }

    #[test]
    fn extract_f64_field_or_uses_value_then_default() {
        let args = json!({"timeout": 2.5});
        assert!((extract_f64_field_or(&args, "timeout", 10.0) - 2.5).abs() < f64::EPSILON);
        assert!((extract_f64_field_or(&json!({}), "timeout", 10.0) - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn extract_f64_field_or_accepts_integer_json_value() {
        // JSON integers coerce to f64 via serde_json::Value::as_f64.
        let args = json!({"n": 7});
        assert!((extract_f64_field_or(&args, "n", 0.0) - 7.0).abs() < f64::EPSILON);
    }

    #[test]
    fn extract_f64_field_or_with_clamp_mimics_inline_pattern() {
        // Mirrors the idiom used at call sites: extract_f64_field_or(...).clamp(min, max).
        // Below minimum → clamped to min.
        assert!(
            (extract_f64_field_or(&json!({"t": 0.0}), "t", 5.0).clamp(1.0, 10.0) - 1.0).abs()
                < f64::EPSILON
        );
        // Above maximum → clamped to max.
        assert!(
            (extract_f64_field_or(&json!({"t": 99.0}), "t", 5.0).clamp(1.0, 10.0) - 10.0).abs()
                < f64::EPSILON
        );
        // In-range value preserved.
        assert!(
            (extract_f64_field_or(&json!({"t": 3.5}), "t", 5.0).clamp(1.0, 10.0) - 3.5).abs()
                < f64::EPSILON
        );
        // Absent field → default (also clamped).
        assert!(
            (extract_f64_field_or(&json!({}), "t", 5.0).clamp(1.0, 10.0) - 5.0).abs()
                < f64::EPSILON
        );
    }

    // ------------------------------------------------------------------
    // Array helpers
    // ------------------------------------------------------------------

    #[test]
    fn parse_json_array_returns_empty_for_non_array() {
        let values: Vec<u64> = parse_json_array(&json!({"not": "an array"}), Value::as_u64);
        assert!(values.is_empty());
    }

    #[test]
    fn parse_json_array_filters_out_invalid_items() {
        let values = parse_json_array(&json!([1, "two", 3, null]), Value::as_u64);
        assert_eq!(values, vec![1, 3]);
    }

    #[test]
    fn parse_json_string_array_filters_non_strings() {
        assert_eq!(
            parse_json_string_array(&json!(["text", 9, "role", false])),
            vec!["text", "role"]
        );
        assert!(parse_json_string_array(&json!({"not": "an array"})).is_empty());
    }

    // ------------------------------------------------------------------
    // format_bounds
    // ------------------------------------------------------------------

    #[test]
    fn format_bounds_serialises_array_shape() {
        assert_eq!(
            format_bounds(Some((1.0, 2.0, 3.0, 4.0))),
            Some(json!([1.0, 2.0, 3.0, 4.0]))
        );
        assert_eq!(format_bounds(None), None);
    }

    // ------------------------------------------------------------------
    // Destructive gate helpers
    // ------------------------------------------------------------------

    #[test]
    fn confirm_arg_false_is_treated_as_unconfirmed() {
        // GIVEN: args with explicit confirm=false (same as absent)
        let args = json!({"app": "x", "query": "q", "confirm": false});
        // WHEN: confirm is extracted
        let confirmed = extract_bool_field_or(&args, "confirm", false);
        // THEN: treated as not confirmed
        assert!(!confirmed);
    }

    #[test]
    fn confirm_arg_true_is_treated_as_confirmed() {
        // GIVEN: args with explicit confirm=true
        let args = json!({"app": "x", "query": "q", "confirm": true});
        // WHEN: confirm is extracted
        let confirmed = extract_bool_field_or(&args, "confirm", false);
        // THEN: treated as confirmed
        assert!(confirmed);
    }

    #[test]
    fn confirm_arg_absent_defaults_to_false() {
        // GIVEN: args without a confirm field
        let args = json!({"app": "x", "query": "q"});
        // WHEN: confirm is extracted with default
        let confirmed = extract_bool_field_or(&args, "confirm", false);
        // THEN: defaults to false (unconfirmed)
        assert!(!confirmed);
    }
}
