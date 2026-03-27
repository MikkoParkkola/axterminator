use serde::Serialize;
use serde_json::{json, Value};

use crate::mcp::protocol::ToolCallResult;

pub(crate) fn ok_apps(apps: Vec<Value>) -> ToolCallResult {
    ToolCallResult::ok_json(json!({ "apps": apps }))
}

pub(crate) fn ok_assertion(
    passed: bool,
    actual: &str,
    expected: &str,
    property: &str,
) -> ToolCallResult {
    ToolCallResult::ok_json(json!({
        "passed": passed,
        "actual": actual,
        "expected": expected,
        "property": property
    }))
}

pub(crate) fn ok_found_attributes(attributes: Value) -> ToolCallResult {
    ToolCallResult::ok_json(json!({
        "found": true,
        "attributes": attributes
    }))
}

pub(crate) fn ok_found_false() -> ToolCallResult {
    ToolCallResult::ok_json(json!({ "found": false }))
}

pub(crate) fn ok_found_tree(tree: Value) -> ToolCallResult {
    ToolCallResult::ok_json(json!({
        "found": true,
        "tree": tree
    }))
}

pub(crate) fn ok_found_value<T: Serialize>(value: T) -> ToolCallResult {
    ToolCallResult::ok_json(json!({
        "found": true,
        "value": value
    }))
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::{
        ok_apps, ok_assertion, ok_found_attributes, ok_found_false, ok_found_tree, ok_found_value,
    };

    fn parse_result(result: crate::mcp::protocol::ToolCallResult) -> Value {
        assert!(!result.is_error);
        assert_eq!(result.content.len(), 1);
        serde_json::from_str(&result.content[0].text).expect("valid JSON payload")
    }

    #[test]
    fn ok_found_false_preserves_shape() {
        assert_eq!(parse_result(ok_found_false()), json!({ "found": false }));
    }

    #[test]
    fn ok_found_tree_preserves_shape() {
        assert_eq!(
            parse_result(ok_found_tree(json!({ "role": "window" }))),
            json!({
                "found": true,
                "tree": { "role": "window" }
            })
        );
    }

    #[test]
    fn ok_assertion_preserves_shape() {
        assert_eq!(
            parse_result(ok_assertion(true, "actual", "expected", "title")),
            json!({
                "passed": true,
                "actual": "actual",
                "expected": "expected",
                "property": "title"
            })
        );
    }

    #[test]
    fn ok_found_value_serializes_null() {
        assert_eq!(
            parse_result(ok_found_value(Option::<String>::None)),
            json!({
                "found": true,
                "value": null
            })
        );
    }

    #[test]
    fn ok_found_attributes_preserves_shape() {
        assert_eq!(
            parse_result(ok_found_attributes(json!({ "role": "button" }))),
            json!({
                "found": true,
                "attributes": { "role": "button" }
            })
        );
    }

    #[test]
    fn ok_apps_preserves_shape() {
        assert_eq!(
            parse_result(ok_apps(vec![json!({ "name": "Finder" })])),
            json!({
                "apps": [{ "name": "Finder" }]
            })
        );
    }
}
