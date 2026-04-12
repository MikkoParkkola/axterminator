//! Phase 3 GUI interaction tool declarations and handlers.
//!
//! | Tool | Purpose |
//! |------|---------|
//! | `ax_scroll`         | Scroll an element or window in any direction |
//! | `ax_key_press`      | Simulate keyboard shortcuts and key combinations |
//! | `ax_get_attributes` | Read all AX attributes of a matched element |
//! | `ax_get_tree`       | Walk the element hierarchy up to a given depth |
//! | `ax_list_apps`      | Enumerate all accessible running applications |
//! | `ax_drag`           | Drag from one element to another via `CGEvent` |
//! | `ax_assert`         | Assert an element property matches an expected value |

use std::io::Write;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::accessibility::{attributes, perform_action};
use crate::mcp::annotations;
use crate::mcp::progress::ProgressReporter;
use crate::mcp::protocol::{Tool, ToolCallResult};
use crate::mcp::tools::AppRegistry;

// Re-export AX actions so handlers can reference them without the full path.
use crate::accessibility::actions;

// ---------------------------------------------------------------------------
// Tool declarations
// ---------------------------------------------------------------------------

pub(crate) fn tool_ax_scroll() -> Tool {
    Tool {
        name: "ax_scroll",
        title: "Scroll an element or window",
        description: "Scroll an element (e.g. a list, scroll area, or the app window) in the \
            given direction. Uses the AXIncrement/AXDecrement accessibility action and falls \
            back to `CGScrollWheelChanged` CGEvent when the AX action is unavailable.\n\
            \n\
            Examples:\n\
            - Scroll a table down 5 ticks: `{\"app\":\"Finder\",\"direction\":\"down\",\"amount\":5}`\n\
            - Scroll to the top of a sidebar: `{\"app\":\"Notes\",\"query\":\"sidebar\",\"direction\":\"up\",\"amount\":20}`",
        input_schema: json!({
            "type": "object",
            "properties": {
                "app": {
                    "type": "string",
                    "description": "App alias from ax_connect"
                },
                "query": {
                    "type": "string",
                    "description": "Optional element query. Scrolls the front window when omitted."
                },
                "direction": {
                    "type": "string",
                    "enum": ["up", "down", "left", "right"],
                    "description": "Scroll direction"
                },
                "amount": {
                    "type": "integer",
                    "description": "Number of scroll increments (default 3, range 1-100)",
                    "default": 3,
                    "minimum": 1,
                    "maximum": 100
                }
            },
            "required": ["app", "direction"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "scrolled": { "type": "boolean" },
                "direction": { "type": "string" },
                "amount": { "type": "integer" }
            },
            "required": ["scrolled", "direction", "amount"]
        }),
        annotations: annotations::ACTION,
    }
}

pub(crate) fn tool_ax_key_press() -> Tool {
    Tool {
        name: "ax_key_press",
        title: "Press keyboard keys or shortcuts",
        description: "Simulate a keyboard shortcut or key press in a connected application. \
            Sends events to the application's PID using `CGEventPostToPid` (background-safe).\n\
            \n\
            Key syntax (case-insensitive modifiers, `+` separator):\n\
            - Single key:    `enter`, `tab`, `escape`, `space`, `delete`, `return`\n\
            - Arrow keys:    `up`, `down`, `left`, `right`\n\
            - Function keys: `f1`–`f20`\n\
            - Modifier combos: `cmd+s`, `ctrl+c`, `opt+tab`, `shift+cmd+p`\n\
            - Letter/digit:  `a`–`z`, `0`–`9`\n\
            \n\
            Examples:\n\
            - Save file: `{\"app\":\"TextEdit\",\"keys\":\"cmd+s\"}`\n\
            - Select all: `{\"app\":\"Safari\",\"keys\":\"cmd+a\"}`",
        input_schema: json!({
            "type": "object",
            "properties": {
                "app": {
                    "type": "string",
                    "description": "App alias from ax_connect"
                },
                "keys": {
                    "type": "string",
                    "description": "Key or shortcut string, e.g. \"cmd+s\", \"enter\", \"tab\""
                }
            },
            "required": ["app", "keys"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "pressed": { "type": "boolean" },
                "keys":    { "type": "string" }
            },
            "required": ["pressed", "keys"]
        }),
        annotations: annotations::ACTION,
    }
}

pub(crate) fn tool_ax_get_attributes() -> Tool {
    Tool {
        name: "ax_get_attributes",
        title: "Get all accessibility attributes of an element",
        description: "Read every AX attribute of a matched element and return them as a JSON \
            object. Useful for exploring unknown UIs before writing more targeted queries.\n\
            \n\
            Returned fields include (when available): `role`, `title`, `value`, `description`, \
            `label`, `identifier`, `enabled`, `focused`, `bounds` [x,y,w,h].",
        input_schema: json!({
            "type": "object",
            "properties": {
                "app":   { "type": "string", "description": "App alias from ax_connect" },
                "query": { "type": "string", "description": "Element query" }
            },
            "required": ["app", "query"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "found": { "type": "boolean" },
                "attributes": {
                    "type": "object",
                    "description": "Map of attribute name to value",
                    "properties": {
                        "role":        { "type": "string" },
                        "title":       { "type": "string" },
                        "value":       { "type": "string" },
                        "description": { "type": "string" },
                        "label":       { "type": "string" },
                        "identifier":  { "type": "string" },
                        "enabled":     { "type": "boolean" },
                        "focused":     { "type": "boolean" },
                        "bounds": {
                            "type": "array",
                            "items": { "type": "number" },
                            "description": "[x, y, width, height]"
                        }
                    }
                }
            },
            "required": ["found"]
        }),
        annotations: annotations::READ_ONLY,
    }
}

pub(crate) fn tool_ax_get_tree() -> Tool {
    Tool {
        name: "ax_get_tree",
        title: "Get the element hierarchy tree",
        description: "Walk the accessibility element tree starting from the application root \
            (or a specific element matched by `query`) and return a nested JSON structure.\n\
            \n\
            Each node has: `role`, `title`, `value`, `enabled`, and `children`.\n\
            Depth 1 returns only immediate children; depth 3 (default) covers most UIs.\n\
            Emits progress notifications while scanning each depth layer.\n\
            \n\
            When `compact` is `true`, only interactive elements are returned \
            (buttons, links, text fields, menu items, checkboxes, radio buttons, \
            pop-up buttons, sliders, text areas, combo boxes). Tree traversal is \
            limited to `max_depth` levels (default 5). Response includes \
            `element_count` and `total_scanned` metadata. Use compact mode to \
            reduce token usage when you only need actionable controls.\n\
            \n\
            When `format` is `\"llm\"`, returns a token-optimised plain-text summary of the \
            application state (app name, selection, navigation, content) built from the \
            CopilotState snapshot instead of the raw element tree. Use this when you want \
            a compact context for an LLM rather than the full element hierarchy.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "app": {
                    "type": "string",
                    "description": "App alias from ax_connect"
                },
                "query": {
                    "type": "string",
                    "description": "Optional root element query. Starts from app root when omitted."
                },
                "depth": {
                    "type": "integer",
                    "description": "Maximum traversal depth (default 3, range 1-10)",
                    "default": 3,
                    "minimum": 1,
                    "maximum": 10
                },
                "format": {
                    "type": "string",
                    "enum": ["default", "llm"],
                    "description": "Output format. \"llm\" returns a token-optimised CopilotState \
                        summary; \"default\" (or omitted) returns the full element tree.",
                    "default": "default"
                },
                "compact": {
                    "type": "boolean",
                    "description": "When true, returns only interactive elements (buttons, links, \
                        inputs, menus) up to max_depth levels. Reduces token usage significantly.",
                    "default": false
                },
                "max_depth": {
                    "type": "integer",
                    "description": "Maximum depth when compact is true (default 5, range 1-10). \
                        Ignored unless compact is true.",
                    "default": 5,
                    "minimum": 1,
                    "maximum": 10
                }
            },
            "required": ["app"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "found": { "type": "boolean" },
                "tree": {
                    "type": "object",
                    "description": "Nested element tree (default format)"
                },
                "llm_summary": {
                    "type": "string",
                    "description": "Token-optimised plain-text summary (llm format)"
                },
                "element_count": {
                    "type": "integer",
                    "description": "Number of interactive elements returned (compact mode only)"
                },
                "total_scanned": {
                    "type": "integer",
                    "description": "Total elements visited during traversal (compact mode only)"
                }
            },
            "required": ["found"]
        }),
        annotations: annotations::READ_ONLY,
    }
}

pub(crate) fn tool_ax_list_apps() -> Tool {
    Tool {
        name: "ax_list_apps",
        title: "List all accessible running applications",
        description: "Return all running macOS applications that expose an accessibility element. \
            Use this to discover app names and PIDs before calling ax_connect.",
        input_schema: json!({
            "type": "object",
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "apps": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name":      { "type": "string" },
                            "pid":       { "type": "integer" },
                            "bundle_id": { "type": "string" }
                        },
                        "required": ["name", "pid"]
                    }
                }
            },
            "required": ["apps"]
        }),
        annotations: annotations::READ_ONLY,
    }
}

pub(crate) fn tool_ax_drag() -> Tool {
    Tool {
        name: "ax_drag",
        title: "Drag from one element to another",
        description: "Perform a mouse drag from the centre of `from_query` to the centre of \
            `to_query` using `CGEvent` drag events. Both elements must belong to the same \
            connected app. The drag is posted via the HID event tap (background-safe as long as \
            the destination element does not require window focus).",
        input_schema: json!({
            "type": "object",
            "properties": {
                "app":        { "type": "string", "description": "App alias from ax_connect" },
                "from_query": { "type": "string", "description": "Query for the drag source element" },
                "to_query":   { "type": "string", "description": "Query for the drop target element" }
            },
            "required": ["app", "from_query", "to_query"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "dragged":    { "type": "boolean" },
                "from_query": { "type": "string" },
                "to_query":   { "type": "string" }
            },
            "required": ["dragged", "from_query", "to_query"]
        }),
        annotations: annotations::ACTION,
    }
}

pub(crate) fn tool_ax_assert() -> Tool {
    Tool {
        name: "ax_assert",
        title: "Assert an element property against an expected value",
        description: "Verify that a specific accessibility property of a matched element \
            equals an expected string. Returns `passed: true` when the actual value matches, \
            `passed: false` otherwise — never returns an error on mismatch, so callers can \
            use the result for conditional logic without error handling.\n\
            \n\
            Supported properties: `exists`, `value`, `title`, `role`, `enabled`, `focused`.\n\
            \n\
            Example — verify a checkbox is checked:\n\
            `{\"app\":\"Settings\",\"query\":\"Enable feature\",\"property\":\"value\",\"expected\":\"1\"}`",
        input_schema: json!({
            "type": "object",
            "properties": {
                "app":      { "type": "string", "description": "App alias from ax_connect" },
                "query":    { "type": "string", "description": "Element query" },
                "property": {
                    "type": "string",
                    "enum": ["exists", "value", "title", "role", "enabled", "focused"],
                    "description": "Attribute to inspect"
                },
                "expected": { "type": "string", "description": "Expected string value" }
            },
            "required": ["app", "query", "property", "expected"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "passed":   { "type": "boolean" },
                "actual":   { "type": "string" },
                "expected": { "type": "string" },
                "property": { "type": "string" }
            },
            "required": ["passed", "actual", "expected", "property"]
        }),
        annotations: annotations::READ_ONLY,
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

pub(crate) fn handle_scroll(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    let Some(app_name) = args["app"].as_str().map(str::to_string) else {
        return ToolCallResult::error("Missing required field: app");
    };
    let Some(direction) = args["direction"].as_str() else {
        return ToolCallResult::error("Missing required field: direction");
    };
    let amount = args["amount"].as_u64().unwrap_or(3).clamp(1, 100) as u32;
    let query = args["query"].as_str().map(str::to_string);

    registry
        .with_app(&app_name, |app| {
            // Resolve target element (or use app root for window-level scroll).
            let element = if let Some(ref q) = query {
                match app.find_native(q, Some(100)) {
                    Ok(el) => Some(el),
                    Err(_) => return ToolCallResult::error(format!("Element not found: '{q}'")),
                }
            } else {
                None
            };

            let ax_action = match direction {
                "up" | "left" => actions::AX_DECREMENT,
                _ => actions::AX_INCREMENT,
            };

            // Try AX action first (background-safe).
            let mut ax_ok = false;
            if let Some(ref el) = element {
                for _ in 0..amount {
                    if perform_action(el.element, ax_action).is_ok() {
                        ax_ok = true;
                    }
                }
            }

            // Fall back to CGScrollWheel event when AX action unavailable.
            if !ax_ok {
                let (dx, dy) = scroll_deltas(direction, amount);
                if let Err(e) = post_scroll_event(dx, dy) {
                    return ToolCallResult::error(format!("Scroll failed: {e}"));
                }
            }

            ToolCallResult::ok(
                json!({
                    "scrolled":  true,
                    "direction": direction,
                    "amount":    amount
                })
                .to_string(),
            )
        })
        .unwrap_or_else(ToolCallResult::error)
}

pub(crate) fn handle_key_press(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    let Some(app_name) = args["app"].as_str().map(str::to_string) else {
        return ToolCallResult::error("Missing required field: app");
    };
    let Some(keys_str) = args["keys"].as_str().map(str::to_string) else {
        return ToolCallResult::error("Missing required field: keys");
    };

    registry
        .with_app(&app_name, |app| {
            match parse_and_post_key_event(app.pid, &keys_str) {
                Ok(()) => {
                    ToolCallResult::ok(json!({ "pressed": true, "keys": keys_str }).to_string())
                }
                Err(e) => ToolCallResult::error(format!("key_press failed: {e}")),
            }
        })
        .unwrap_or_else(ToolCallResult::error)
}

pub(crate) fn handle_get_attributes(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    let (app_name, query) = match extract_app_query(args) {
        Ok(v) => v,
        Err(e) => return ToolCallResult::error(e),
    };

    registry
        .with_app(&app_name, |app| match app.find_native(&query, Some(100)) {
            Ok(el) => {
                let bounds_val = el.bounds().map(|(x, y, w, h)| json!([x, y, w, h]));
                let attrs = json!({
                    "role":        el.role(),
                    "title":       el.title(),
                    "value":       el.value(),
                    "description": el.description(),
                    "label":       el.label(),
                    "identifier":  el.identifier(),
                    "enabled":     el.enabled(),
                    "focused":     el.focused(),
                    "bounds":      bounds_val
                });
                ToolCallResult::ok(json!({"found": true, "attributes": attrs}).to_string())
            }
            Err(_) => ToolCallResult::ok(json!({"found": false}).to_string()),
        })
        .unwrap_or_else(ToolCallResult::error)
}

pub(crate) fn handle_get_tree<W: Write>(
    args: &Value,
    registry: &Arc<AppRegistry>,
    out: &mut W,
) -> ToolCallResult {
    let Some(app_name) = args["app"].as_str().map(str::to_string) else {
        return ToolCallResult::error("Missing required field: app");
    };

    // When format == "llm", skip the element tree entirely and return a
    // token-optimised CopilotState summary built from the live AX tree.
    if args["format"].as_str() == Some("llm") {
        return handle_get_tree_llm_format(&app_name, registry);
    }

    // Compact mode: return only interactive elements up to max_depth.
    if args["compact"].as_bool().unwrap_or(false) {
        let max_depth = args["max_depth"].as_u64().unwrap_or(5).clamp(1, 10) as usize;
        let query = args["query"].as_str().map(str::to_string);
        return handle_get_tree_compact(&app_name, query, max_depth, registry);
    }

    let depth = args["depth"].as_u64().unwrap_or(3).clamp(1, 10) as usize;
    let query = args["query"].as_str().map(str::to_string);

    // Emit progress when depth ≥ 2 (otherwise it completes too fast to matter).
    #[allow(clippy::cast_possible_truncation)] // depth is clamped to 1..=10 above
    let mut reporter = if depth >= 2 {
        Some(ProgressReporter::new(out, depth as u32))
    } else {
        None
    };

    registry
        .with_app(&app_name, |app| {
            let root_element = if let Some(ref q) = query {
                match app.find_native(q, Some(100)) {
                    Ok(el) => {
                        let tree = build_element_tree(el.element, depth, &mut reporter);
                        return ToolCallResult::ok(
                            json!({"found": true, "tree": tree}).to_string(),
                        );
                    }
                    Err(_) => return ToolCallResult::ok(json!({"found": false}).to_string()),
                }
            } else {
                app.element
            };

            let tree = build_app_root_tree(root_element, depth, &mut reporter);
            ToolCallResult::ok(json!({"found": true, "tree": tree}).to_string())
        })
        .unwrap_or_else(ToolCallResult::error)
}

/// Handle `ax_get_tree` in compact mode: only interactive elements, depth-limited.
fn handle_get_tree_compact(
    app_name: &str,
    query: Option<String>,
    max_depth: usize,
    registry: &Arc<AppRegistry>,
) -> ToolCallResult {
    registry
        .with_app(app_name, |app| {
            let root_element = if let Some(ref q) = query {
                match app.find_native(q, Some(100)) {
                    Ok(el) => el.element,
                    Err(_) => return ToolCallResult::ok(json!({"found": false}).to_string()),
                }
            } else {
                app.element
            };

            let mut stats = CompactStats::default();
            let nodes = collect_compact_nodes(root_element, max_depth, 0, &mut stats);
            ToolCallResult::ok(
                json!({
                    "found":         true,
                    "tree":          nodes,
                    "element_count": stats.matched,
                    "total_scanned": stats.scanned,
                })
                .to_string(),
            )
        })
        .unwrap_or_else(ToolCallResult::error)
}

/// Return a token-optimised CopilotState summary of the application.
///
/// Reads the live AX tree via [`crate::copilot_state::read_copilot_state`] and
/// formats it with [`crate::copilot_format::format_for_llm`].  The resulting
/// plain text is compact enough to drop directly into an LLM system prompt.
fn handle_get_tree_llm_format(app_name: &str, registry: &Arc<AppRegistry>) -> ToolCallResult {
    use crate::copilot_format::{format_for_llm, FormatOptions};
    use crate::copilot_state::read_copilot_state;

    registry
        .with_app(app_name, |app| {
            let state = read_copilot_state(app.element);
            let summary = format_for_llm(&state, &FormatOptions::default());
            ToolCallResult::ok(
                json!({
                    "found": true,
                    "llm_summary": summary
                })
                .to_string(),
            )
        })
        .unwrap_or_else(ToolCallResult::error)
}

pub(crate) fn handle_list_apps() -> ToolCallResult {
    let apps = list_running_apps();
    ToolCallResult::ok(json!({ "apps": apps }).to_string())
}

pub(crate) fn handle_drag(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    let Some(app_name) = args["app"].as_str().map(str::to_string) else {
        return ToolCallResult::error("Missing required field: app");
    };
    let Some(from_query) = args["from_query"].as_str().map(str::to_string) else {
        return ToolCallResult::error("Missing required field: from_query");
    };
    let Some(to_query) = args["to_query"].as_str().map(str::to_string) else {
        return ToolCallResult::error("Missing required field: to_query");
    };

    registry
        .with_app(&app_name, |app| {
            let Ok(from_el) = app.find_native(&from_query, Some(100)) else {
                return ToolCallResult::error(format!("Drag source not found: '{from_query}'"));
            };
            let Ok(to_el) = app.find_native(&to_query, Some(100)) else {
                return ToolCallResult::error(format!("Drag target not found: '{to_query}'"));
            };

            let Some(from_center) = element_center(&from_el) else {
                return ToolCallResult::error(format!(
                    "Cannot determine bounds of source: '{from_query}'"
                ));
            };
            let Some(to_center) = element_center(&to_el) else {
                return ToolCallResult::error(format!(
                    "Cannot determine bounds of target: '{to_query}'"
                ));
            };

            match post_drag_event(from_center, to_center) {
                Ok(()) => ToolCallResult::ok(
                    json!({
                        "dragged":    true,
                        "from_query": from_query,
                        "to_query":   to_query
                    })
                    .to_string(),
                ),
                Err(e) => ToolCallResult::error(format!("Drag failed: {e}")),
            }
        })
        .unwrap_or_else(ToolCallResult::error)
}

pub(crate) fn handle_assert(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    let (app_name, query) = match extract_app_query(args) {
        Ok(v) => v,
        Err(e) => return ToolCallResult::error(e),
    };
    let Some(property) = args["property"].as_str().map(str::to_string) else {
        return ToolCallResult::error("Missing required field: property");
    };
    let Some(expected) = args["expected"].as_str().map(str::to_string) else {
        return ToolCallResult::error("Missing required field: expected");
    };

    registry
        .with_app(&app_name, |app| {
            // The `exists` property is special — element lookup itself is the assertion.
            if property == "exists" {
                let exists = app.find_native(&query, Some(100)).is_ok();
                let actual = if exists { "true" } else { "false" }.to_string();
                let passed = actual == expected;
                return ToolCallResult::ok(
                    json!({
                        "passed":   passed,
                        "actual":   actual,
                        "expected": expected,
                        "property": property
                    })
                    .to_string(),
                );
            }

            match app.find_native(&query, Some(100)) {
                Ok(el) => {
                    let actual = read_element_property(&el, &property);
                    let passed = actual == expected;
                    ToolCallResult::ok(
                        json!({
                            "passed":   passed,
                            "actual":   actual,
                            "expected": expected,
                            "property": property
                        })
                        .to_string(),
                    )
                }
                Err(_) => {
                    // Element not found — assert fails with empty actual.
                    ToolCallResult::ok(
                        json!({
                            "passed":   false,
                            "actual":   "",
                            "expected": expected,
                            "property": property
                        })
                        .to_string(),
                    )
                }
            }
        })
        .unwrap_or_else(ToolCallResult::error)
}

// ---------------------------------------------------------------------------
// Private helpers — element tree
// ---------------------------------------------------------------------------

/// Build a JSON tree node from an `AXUIElementRef`, recursing up to `depth`.
///
/// Emits one `notifications/progress` notification per depth layer via
/// `reporter`, firing exactly once the first time each layer is entered.
fn build_element_tree<W: Write>(
    element: crate::accessibility::AXUIElementRef,
    depth: usize,
    reporter: &mut Option<ProgressReporter<'_, W>>,
) -> serde_json::Value {
    let mut emitted = vec![false; depth];
    build_tree_node(element, depth, 0, reporter, &mut emitted)
}

/// Same as `build_element_tree` but starts from the application root element.
fn build_app_root_tree<W: Write>(
    root: crate::accessibility::AXUIElementRef,
    depth: usize,
    reporter: &mut Option<ProgressReporter<'_, W>>,
) -> serde_json::Value {
    let mut emitted = vec![false; depth];
    build_tree_node(root, depth, 0, reporter, &mut emitted)
}

/// Recursive tree builder — one node per element.
///
/// `emitted[d]` tracks whether a progress notification has already been sent
/// for depth `d`.  This guarantees exactly one notification per layer
/// regardless of how many sibling nodes exist at that depth.
fn build_tree_node<W: Write>(
    element: crate::accessibility::AXUIElementRef,
    max_depth: usize,
    current_depth: usize,
    reporter: &mut Option<ProgressReporter<'_, W>>,
    emitted: &mut Vec<bool>,
) -> serde_json::Value {
    let role = crate::accessibility::get_string_attribute_value(element, attributes::AX_ROLE);
    let title = crate::accessibility::get_string_attribute_value(element, attributes::AX_TITLE);
    let value = crate::accessibility::get_string_attribute_value(element, attributes::AX_VALUE);
    let enabled = crate::accessibility::get_bool_attribute_value(element, attributes::AX_ENABLED);

    if current_depth >= max_depth {
        return json!({ "role": role, "title": title, "value": value, "enabled": enabled });
    }

    // Emit one progress notification the first time we enter each depth layer.
    if let Some(d) = emitted.get_mut(current_depth) {
        if !*d {
            *d = true;
            if let Some(ref mut rep) = reporter {
                let layer = current_depth + 1;
                let msg = format!("Scanning layer {layer}/{max_depth}…");
                // Best-effort: silently ignore I/O errors in progress notifications.
                let _ = rep.step(&msg);
            }
        }
    }

    let children: Vec<serde_json::Value> = crate::accessibility::get_children(element)
        .unwrap_or_default()
        .into_iter()
        .map(|child| {
            let node = build_tree_node(child, max_depth, current_depth + 1, reporter, emitted);
            crate::accessibility::release_cf(child as core_foundation::base::CFTypeRef);
            node
        })
        .collect();

    json!({
        "role":     role,
        "title":    title,
        "value":    value,
        "enabled":  enabled,
        "children": children
    })
}

// ---------------------------------------------------------------------------
// Private helpers — compact tree
// ---------------------------------------------------------------------------

/// Roles considered interactive / actionable for compact mode.
const ACTIONABLE_ROLES: &[&str] = &[
    crate::accessibility::roles::AX_BUTTON,
    "AXLink",
    crate::accessibility::roles::AX_TEXT_FIELD,
    crate::accessibility::roles::AX_MENU_ITEM,
    crate::accessibility::roles::AX_CHECKBOX,
    crate::accessibility::roles::AX_RADIO_BUTTON,
    "AXPopUpButton",
    crate::accessibility::roles::AX_SLIDER,
    crate::accessibility::roles::AX_TEXT_AREA,
    "AXComboBox",
];

/// Returns `true` when `role` is one of the actionable roles for compact mode.
fn is_actionable_role(role: &Option<String>) -> bool {
    role.as_deref()
        .map(|r| ACTIONABLE_ROLES.contains(&r))
        .unwrap_or(false)
}

/// Mutable counters threaded through the compact tree traversal.
#[derive(Default)]
struct CompactStats {
    /// Elements whose role matched an actionable role.
    matched: usize,
    /// Total elements visited (including non-interactive).
    scanned: usize,
}

/// Recursively collect actionable elements up to `max_depth`.
///
/// Returns a flat `Vec` of leaf-style JSON nodes; each matched node is
/// emitted without a `children` key (the list is already flat by role).
/// Traversal still recurses into non-matching nodes to find nested
/// interactive elements, as long as `current_depth < max_depth`.
fn collect_compact_nodes(
    element: crate::accessibility::AXUIElementRef,
    max_depth: usize,
    current_depth: usize,
    stats: &mut CompactStats,
) -> Vec<serde_json::Value> {
    stats.scanned += 1;

    let role = crate::accessibility::get_string_attribute_value(
        element,
        crate::accessibility::attributes::AX_ROLE,
    );
    let title = crate::accessibility::get_string_attribute_value(
        element,
        crate::accessibility::attributes::AX_TITLE,
    );
    let value = crate::accessibility::get_string_attribute_value(
        element,
        crate::accessibility::attributes::AX_VALUE,
    );
    let enabled = crate::accessibility::get_bool_attribute_value(
        element,
        crate::accessibility::attributes::AX_ENABLED,
    );

    let mut nodes: Vec<serde_json::Value> = Vec::new();

    if is_actionable_role(&role) {
        stats.matched += 1;
        nodes.push(json!({
            "role":    role,
            "title":   title,
            "value":   value,
            "enabled": enabled,
        }));
    }

    // Recurse into children unless we have hit the depth limit.
    if current_depth < max_depth {
        let children = crate::accessibility::get_children(element).unwrap_or_default();
        for child in children {
            let child_nodes = collect_compact_nodes(child, max_depth, current_depth + 1, stats);
            nodes.extend(child_nodes);
            crate::accessibility::release_cf(child as core_foundation::base::CFTypeRef);
        }
    }

    nodes
}

// ---------------------------------------------------------------------------
// Private helpers — element geometry
// ---------------------------------------------------------------------------

/// Compute the screen-space centre of an element from its AX bounds.
fn element_center(el: &crate::element::AXElement) -> Option<(f64, f64)> {
    el.bounds().map(|(x, y, w, h)| (x + w / 2.0, y + h / 2.0))
}

// CGEvent helpers, key-code table, property reader, and arg-extraction live
// in a sibling module (tools_gui_events) to keep this file under 800 LOC.
//
// `pub(crate) use` re-exports items so callers can address them as
// `crate::mcp::tools_gui::*`; it also brings them into local scope.
// `key_name_to_code` and `list_running_apps` are re-exported for use by the
// test module in `tools_extended`; they are not used locally in this file.
#[allow(unused_imports)]
pub(crate) use crate::mcp::tools_gui_events::{
    extract_app_query, key_name_to_code, list_running_apps, read_element_property, scroll_deltas,
};
// Private helpers used only within this file.
use crate::mcp::tools_gui_events::{parse_and_post_key_event, post_drag_event, post_scroll_event};

// Tests live in tools_extended_tests, which has access to both the public API
// (extended_tools / call_tool_extended) and the private helpers via pub(crate).

// ---------------------------------------------------------------------------
// Tests — compact mode pure-logic helpers
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // is_actionable_role
    // -----------------------------------------------------------------------

    #[test]
    fn is_actionable_role_matches_all_interactive_roles() {
        // GIVEN: the full set of roles compact mode should keep
        let interactive = [
            "AXButton",
            "AXLink",
            "AXTextField",
            "AXMenuItem",
            "AXCheckBox",
            "AXRadioButton",
            "AXPopUpButton",
            "AXSlider",
            "AXTextArea",
            "AXComboBox",
        ];
        // WHEN / THEN: every role is recognised as actionable
        for role in interactive {
            assert!(
                is_actionable_role(&Some(role.to_string())),
                "{role} should be actionable"
            );
        }
    }

    #[test]
    fn is_actionable_role_rejects_non_interactive_roles() {
        // GIVEN: structural / static roles that compact mode should drop
        let non_interactive = [
            "AXGroup",
            "AXStaticText",
            "AXWindow",
            "AXApplication",
            "AXScrollArea",
            "AXTable",
            "AXList",
        ];
        // WHEN / THEN: none are considered actionable
        for role in non_interactive {
            assert!(
                !is_actionable_role(&Some(role.to_string())),
                "{role} should NOT be actionable"
            );
        }
    }

    #[test]
    fn is_actionable_role_returns_false_for_none() {
        // GIVEN: element with no role attribute
        // WHEN / THEN: treated as non-interactive
        assert!(!is_actionable_role(&None));
    }

    // -----------------------------------------------------------------------
    // compact_stats default
    // -----------------------------------------------------------------------

    #[test]
    fn compact_stats_default_starts_at_zero() {
        let s = CompactStats::default();
        assert_eq!(s.matched, 0);
        assert_eq!(s.scanned, 0);
    }

    // -----------------------------------------------------------------------
    // tool_ax_get_tree schema
    // -----------------------------------------------------------------------

    #[test]
    fn tool_ax_get_tree_compact_param_defaults_to_false() {
        let tool = tool_ax_get_tree();
        let compact = &tool.input_schema["properties"]["compact"];
        assert_eq!(compact["default"].as_bool(), Some(false));
    }

    #[test]
    fn tool_ax_get_tree_max_depth_param_defaults_to_five() {
        let tool = tool_ax_get_tree();
        let max_depth = &tool.input_schema["properties"]["max_depth"];
        assert_eq!(max_depth["default"].as_u64(), Some(5));
        assert_eq!(max_depth["minimum"].as_u64(), Some(1));
        assert_eq!(max_depth["maximum"].as_u64(), Some(10));
    }
}
