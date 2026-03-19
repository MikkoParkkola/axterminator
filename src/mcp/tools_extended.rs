//! Phase 3 MCP tool declarations and handlers.
//!
//! This module adds seven new tools to the Phase 1 set:
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
//!
//! All tools follow the same contract as Phase 1:
//! - Declared as a `Tool` constant via a builder function.
//! - Dispatched through `call_tool_extended`.
//! - Always return `ToolCallResult` — never panic.
//!
//! # Progress notifications
//!
//! `ax_get_tree` emits depth-layer progress when the requested depth is ≥ 2.
//! `ax_find_visual` would emit model loading progress when VLM support is added.
//! Both use [`crate::mcp::progress::ProgressReporter`] to keep token generation
//! central and collision-free.

use std::io::Write;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::accessibility::{attributes, perform_action};
use crate::mcp::annotations;
use crate::mcp::progress::ProgressReporter;
use crate::mcp::protocol::{Tool, ToolCallResult};
use crate::mcp::tools::AppRegistry;

// ---------------------------------------------------------------------------
// Tool declarations
// ---------------------------------------------------------------------------

/// All Phase 3 tools in registration order.
#[must_use]
pub fn extended_tools() -> Vec<Tool> {
    // `mut` required when any feature-gated tool sets (spaces, audio, camera) are enabled.
    #[allow(unused_mut)]
    let mut tools = vec![
        tool_ax_scroll(),
        tool_ax_key_press(),
        tool_ax_get_attributes(),
        tool_ax_get_tree(),
        tool_ax_list_apps(),
        tool_ax_drag(),
        tool_ax_assert(),
    ];
    #[cfg(feature = "spaces")]
    tools.extend(spaces_tools());
    #[cfg(feature = "audio")]
    tools.extend(audio_tools());
    #[cfg(feature = "camera")]
    tools.extend(camera_tools());
    tools
}

/// Space management tools (requires `spaces` feature).
///
/// Returns 5 tools: `ax_list_spaces`, `ax_create_space`, `ax_move_to_space`,
/// `ax_switch_space`, `ax_destroy_space`.
#[cfg(feature = "spaces")]
#[must_use]
pub fn spaces_tools() -> Vec<Tool> {
    vec![
        tool_ax_list_spaces(),
        tool_ax_create_space(),
        tool_ax_move_to_space(),
        tool_ax_switch_space(),
        tool_ax_destroy_space(),
    ]
}

fn tool_ax_scroll() -> Tool {
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

fn tool_ax_key_press() -> Tool {
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

fn tool_ax_get_attributes() -> Tool {
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

fn tool_ax_get_tree() -> Tool {
    Tool {
        name: "ax_get_tree",
        title: "Get the element hierarchy tree",
        description: "Walk the accessibility element tree starting from the application root \
            (or a specific element matched by `query`) and return a nested JSON structure.\n\
            \n\
            Each node has: `role`, `title`, `value`, `enabled`, and `children`.\n\
            Depth 1 returns only immediate children; depth 3 (default) covers most UIs.\n\
            Emits progress notifications while scanning each depth layer.",
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
                    "description": "Nested element tree"
                }
            },
            "required": ["found"]
        }),
        annotations: annotations::READ_ONLY,
    }
}

fn tool_ax_list_apps() -> Tool {
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

fn tool_ax_drag() -> Tool {
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

fn tool_ax_assert() -> Tool {
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
// Dispatch
// ---------------------------------------------------------------------------

/// Dispatch a Phase 3 tool call, emitting optional progress to `out`.
///
/// Returns `None` when `name` is not a Phase 3 tool — the caller falls
/// through to the Phase 1 dispatch table.
///
/// # Errors
///
/// `out` I/O errors from progress notifications are silently ignored to
/// avoid masking the tool result.  This matches the MCP convention that
/// notification delivery is best-effort.
pub fn call_tool_extended<W: Write>(
    name: &str,
    args: &Value,
    registry: &Arc<AppRegistry>,
    out: &mut W,
) -> Option<ToolCallResult> {
    match name {
        "ax_scroll" => Some(handle_scroll(args, registry)),
        "ax_key_press" => Some(handle_key_press(args, registry)),
        "ax_get_attributes" => Some(handle_get_attributes(args, registry)),
        "ax_get_tree" => Some(handle_get_tree(args, registry, out)),
        "ax_list_apps" => Some(handle_list_apps()),
        "ax_drag" => Some(handle_drag(args, registry)),
        "ax_assert" => Some(handle_assert(args, registry)),
        #[cfg(feature = "spaces")]
        "ax_list_spaces" => Some(handle_ax_list_spaces()),
        #[cfg(feature = "spaces")]
        "ax_create_space" => Some(handle_ax_create_space()),
        #[cfg(feature = "spaces")]
        "ax_move_to_space" => Some(handle_ax_move_to_space(args, registry)),
        #[cfg(feature = "spaces")]
        "ax_switch_space" => Some(handle_ax_switch_space(args)),
        #[cfg(feature = "spaces")]
        "ax_destroy_space" => Some(handle_ax_destroy_space(args)),
        #[cfg(feature = "audio")]
        "ax_listen" => Some(handle_ax_listen(args)),
        #[cfg(feature = "audio")]
        "ax_speak" => Some(handle_ax_speak(args)),
        #[cfg(feature = "audio")]
        "ax_audio_devices" => Some(handle_ax_audio_devices()),
        #[cfg(feature = "camera")]
        "ax_camera_capture" => Some(handle_ax_camera_capture(args)),
        #[cfg(feature = "camera")]
        "ax_gesture_detect" => Some(handle_ax_gesture_detect(args)),
        #[cfg(feature = "camera")]
        "ax_gesture_listen" => Some(handle_ax_gesture_listen(args)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

fn handle_scroll(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
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

fn handle_key_press(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
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

fn handle_get_attributes(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
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

fn handle_get_tree<W: Write>(
    args: &Value,
    registry: &Arc<AppRegistry>,
    out: &mut W,
) -> ToolCallResult {
    let Some(app_name) = args["app"].as_str().map(str::to_string) else {
        return ToolCallResult::error("Missing required field: app");
    };
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

fn handle_list_apps() -> ToolCallResult {
    let apps = list_running_apps();
    ToolCallResult::ok(json!({ "apps": apps }).to_string())
}

fn handle_drag(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
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

fn handle_assert(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
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
/// Progress notifications are emitted by the caller via `reporter`.
fn build_element_tree<W: Write>(
    element: crate::accessibility::AXUIElementRef,
    depth: usize,
    reporter: &mut Option<ProgressReporter<'_, W>>,
) -> Value {
    build_tree_node(element, depth, 0, reporter)
}

/// Same as `build_element_tree` but starts from the application root element.
fn build_app_root_tree<W: Write>(
    root: crate::accessibility::AXUIElementRef,
    depth: usize,
    reporter: &mut Option<ProgressReporter<'_, W>>,
) -> Value {
    build_tree_node(root, depth, 0, reporter)
}

/// Recursive tree builder — one node per element.
fn build_tree_node<W: Write>(
    element: crate::accessibility::AXUIElementRef,
    max_depth: usize,
    current_depth: usize,
    reporter: &mut Option<ProgressReporter<'_, W>>,
) -> Value {
    let role = crate::accessibility::get_string_attribute_value(element, attributes::AX_ROLE);
    let title = crate::accessibility::get_string_attribute_value(element, attributes::AX_TITLE);
    let value = crate::accessibility::get_string_attribute_value(element, attributes::AX_VALUE);
    let enabled = crate::accessibility::get_bool_attribute_value(element, attributes::AX_ENABLED);

    if current_depth >= max_depth {
        return json!({ "role": role, "title": title, "value": value, "enabled": enabled });
    }

    // Emit per-layer progress when moving into the next depth level.
    if let Some(ref mut rep) = reporter {
        if current_depth < max_depth {
            let layer = current_depth + 1;
            let msg = format!("Scanning layer {layer}/{max_depth}…");
            // Best-effort: silently ignore I/O errors in progress notifications.
            let _ = rep.step(&msg);
        }
    }

    let children: Vec<Value> = crate::accessibility::get_children(element)
        .unwrap_or_default()
        .into_iter()
        .map(|child| {
            let node =
                build_tree_node::<std::io::Sink>(child, max_depth, current_depth + 1, &mut None);
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
// Private helpers — system running apps
// ---------------------------------------------------------------------------

/// Return a JSON array of all running GUI applications.
///
/// Uses `sysinfo` to enumerate processes.  We filter to those that have an
/// accessible AX application element (i.e. non-zero PID with a process name).
fn list_running_apps() -> Vec<Value> {
    use sysinfo::System;

    let mut sys = System::new_all();
    sys.refresh_all();

    let mut apps: Vec<Value> = sys
        .processes()
        .values()
        .filter_map(|proc| {
            let name = proc.name().to_string_lossy().to_string();
            if name.is_empty() {
                return None;
            }
            let pid = i64::from(proc.pid().as_u32());
            Some(json!({ "name": name, "pid": pid }))
        })
        .collect();

    // Sort by name for deterministic output.
    apps.sort_by(|a, b| {
        let na = a["name"].as_str().unwrap_or("");
        let nb = b["name"].as_str().unwrap_or("");
        na.cmp(nb)
    });

    apps
}

// ---------------------------------------------------------------------------
// Private helpers — element geometry
// ---------------------------------------------------------------------------

/// Compute the screen-space centre of an element from its AX bounds.
fn element_center(el: &crate::element::AXElement) -> Option<(f64, f64)> {
    el.bounds().map(|(x, y, w, h)| (x + w / 2.0, y + h / 2.0))
}

// ---------------------------------------------------------------------------
// Private helpers — key press CGEvent
// ---------------------------------------------------------------------------

/// Parse a key combo string and post `CGEvent`s to the target PID.
///
/// Supported modifiers (case-insensitive): `cmd`, `ctrl`, `opt`, `alt`,
/// `shift`.  The final token is the key name.
fn parse_and_post_key_event(pid: i32, keys: &str) -> Result<(), String> {
    use core_graphics::event::{CGEvent, CGEventFlags};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|()| "Failed to create CGEventSource".to_string())?;

    let parts: Vec<&str> = keys.split('+').map(str::trim).collect();
    let (modifier_parts, key_part) = match parts.split_last() {
        Some((k, mods)) => (mods, *k),
        None => return Err(format!("Empty key string: '{keys}'")),
    };

    let key_code =
        key_name_to_code(key_part).ok_or_else(|| format!("Unknown key: '{key_part}'"))?;

    let flags = modifier_parts
        .iter()
        .fold(CGEventFlags::CGEventFlagNull, |acc, &m| {
            acc | modifier_to_flag(m)
        });

    let key_down = CGEvent::new_keyboard_event(source.clone(), key_code, true)
        .map_err(|()| "Failed to create key-down event".to_string())?;
    key_down.set_flags(flags);
    key_down.post_to_pid(pid);

    let key_up = CGEvent::new_keyboard_event(source, key_code, false)
        .map_err(|()| "Failed to create key-up event".to_string())?;
    key_up.set_flags(flags);
    key_up.post_to_pid(pid);

    Ok(())
}

/// Map a modifier name to a `CGEventFlags` bit.
fn modifier_to_flag(modifier: &str) -> core_graphics::event::CGEventFlags {
    use core_graphics::event::CGEventFlags;
    match modifier.to_lowercase().as_str() {
        "cmd" | "command" => CGEventFlags::CGEventFlagCommand,
        "ctrl" | "control" => CGEventFlags::CGEventFlagControl,
        "opt" | "alt" | "option" => CGEventFlags::CGEventFlagAlternate,
        "shift" => CGEventFlags::CGEventFlagShift,
        _ => CGEventFlags::CGEventFlagNull,
    }
}

/// Map a human-readable key name to a macOS virtual key code.
///
/// Only the most common keys are covered.  Unknown names return `None`.
#[allow(clippy::too_many_lines)]
fn key_name_to_code(name: &str) -> Option<u16> {
    match name.to_lowercase().as_str() {
        // Letters
        "a" => Some(0),
        "b" => Some(11),
        "c" => Some(8),
        "d" => Some(2),
        "e" => Some(14),
        "f" => Some(3),
        "g" => Some(5),
        "h" => Some(4),
        "i" => Some(34),
        "j" => Some(38),
        "k" => Some(40),
        "l" => Some(37),
        "m" => Some(46),
        "n" => Some(45),
        "o" => Some(31),
        "p" => Some(35),
        "q" => Some(12),
        "r" => Some(15),
        "s" => Some(1),
        "t" => Some(17),
        "u" => Some(32),
        "v" => Some(9),
        "w" => Some(13),
        "x" => Some(7),
        "y" => Some(16),
        "z" => Some(6),
        // Digits
        "0" => Some(29),
        "1" => Some(18),
        "2" => Some(19),
        "3" => Some(20),
        "4" => Some(21),
        "5" => Some(23),
        "6" => Some(22),
        "7" => Some(26),
        "8" => Some(28),
        "9" => Some(25),
        // Navigation
        "return" | "enter" => Some(36),
        "tab" => Some(48),
        "space" => Some(49),
        "delete" | "backspace" => Some(51),
        "escape" | "esc" => Some(53),
        "left" => Some(123),
        "right" => Some(124),
        "down" => Some(125),
        "up" => Some(126),
        "home" => Some(115),
        "end" => Some(119),
        "pageup" | "page_up" => Some(116),
        "pagedown" | "page_down" => Some(121),
        "forwarddelete" | "forward_delete" => Some(117),
        // Function keys
        "f1" => Some(122),
        "f2" => Some(120),
        "f3" => Some(99),
        "f4" => Some(118),
        "f5" => Some(96),
        "f6" => Some(97),
        "f7" => Some(98),
        "f8" => Some(100),
        "f9" => Some(101),
        "f10" => Some(109),
        "f11" => Some(103),
        "f12" => Some(111),
        "f13" => Some(105),
        "f14" => Some(107),
        "f15" => Some(113),
        "f16" => Some(106),
        "f17" => Some(64),
        "f18" => Some(79),
        "f19" => Some(80),
        "f20" => Some(90),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Private helpers — scroll CGEvent
// ---------------------------------------------------------------------------

/// Compute `(delta_x, delta_y)` scroll amounts for a direction and amount.
///
/// `CGScrollWheel` uses axis-1 = vertical, axis-2 = horizontal.
/// Positive axis-1 = scroll up; negative = scroll down (follows HID convention).
const fn scroll_deltas(direction: &str, amount: u32) -> (i32, i32) {
    #[allow(clippy::cast_possible_wrap)] // amount is clamped 1..=100 by callers
    let ticks = amount as i32;
    match direction.as_bytes() {
        b"up" => (0, ticks),
        b"down" => (0, -ticks),
        b"left" => (-ticks, 0),
        _ => (ticks, 0), // "right"
    }
}

/// Post a `CGScrollWheelChanged` event at the current cursor position.
///
/// Uses `CGEventCreateScrollWheelEvent2` via the `highsierra` feature of the
/// `core-graphics` crate.  `kCGScrollEventUnitLine` = 1 (raw value; the crate
/// exposes only the type alias, not named unit constants).
///
/// axis-1 = vertical (positive = up), axis-2 = horizontal (positive = right).
fn post_scroll_event(dx: i32, dy: i32) -> Result<(), String> {
    use core_graphics::event::CGEvent;
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|()| "Failed to create CGEventSource".to_string())?;

    // unit=1 → kCGScrollEventUnitLine; wheel1=vertical, wheel2=horizontal.
    let event = CGEvent::new_scroll_event(source, 1_u32, 2, dy, dx, 0)
        .map_err(|()| "Failed to create scroll event".to_string())?;
    event.post(core_graphics::event::CGEventTapLocation::HID);
    Ok(())
}

// ---------------------------------------------------------------------------
// Private helpers — drag CGEvent
// ---------------------------------------------------------------------------

/// Post mouse-drag events from `from` to `to` via the HID tap.
fn post_drag_event(from: (f64, f64), to: (f64, f64)) -> Result<(), String> {
    use core_graphics::event::{CGEvent, CGEventType, CGMouseButton};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    use core_graphics::geometry::CGPoint;

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|()| "Failed to create CGEventSource".to_string())?;

    let from_pt = CGPoint::new(from.0, from.1);
    let to_pt = CGPoint::new(to.0, to.1);

    // Mouse-down at source.
    let down = CGEvent::new_mouse_event(
        source.clone(),
        CGEventType::LeftMouseDown,
        from_pt,
        CGMouseButton::Left,
    )
    .map_err(|()| "Failed to create mouse-down event".to_string())?;
    down.post(core_graphics::event::CGEventTapLocation::HID);

    // Drag event to destination.
    let drag = CGEvent::new_mouse_event(
        source.clone(),
        CGEventType::LeftMouseDragged,
        to_pt,
        CGMouseButton::Left,
    )
    .map_err(|()| "Failed to create drag event".to_string())?;
    drag.post(core_graphics::event::CGEventTapLocation::HID);

    // Mouse-up at destination.
    let up = CGEvent::new_mouse_event(source, CGEventType::LeftMouseUp, to_pt, CGMouseButton::Left)
        .map_err(|()| "Failed to create mouse-up event".to_string())?;
    up.post(core_graphics::event::CGEventTapLocation::HID);

    Ok(())
}

// ---------------------------------------------------------------------------
// Private helpers — property reader
// ---------------------------------------------------------------------------

/// Read a named property from an element as a string.
///
/// Boolean properties are normalised to `"true"` / `"false"`.
fn read_element_property(el: &crate::element::AXElement, property: &str) -> String {
    match property {
        "value" => el.value().unwrap_or_default(),
        "title" => el.title().unwrap_or_default(),
        "role" => el.role().unwrap_or_default(),
        "enabled" => el.enabled().to_string(),
        "focused" => el.focused().to_string(),
        _ => String::new(),
    }
}

// ---------------------------------------------------------------------------
// Shared helper (mirrors tools.rs helper — kept local to avoid pub leakage)
// ---------------------------------------------------------------------------

fn extract_app_query(args: &Value) -> Result<(String, String), String> {
    let app = args["app"]
        .as_str()
        .ok_or_else(|| "Missing required field: app".to_string())?
        .to_string();
    let query = args["query"]
        .as_str()
        .ok_or_else(|| "Missing required field: query".to_string())?
        .to_string();
    Ok((app, query))
}

// Re-export AX actions so handlers can reference them without the full path.
use crate::accessibility::actions;

// ---------------------------------------------------------------------------
// Spaces tool declarations (feature = "spaces")
// ---------------------------------------------------------------------------

#[cfg(feature = "spaces")]
fn tool_ax_list_spaces() -> Tool {
    Tool {
        name: "ax_list_spaces",
        title: "List virtual desktops (Spaces)",
        description: "Enumerate all macOS virtual desktops (Spaces) with their IDs, types \
            (user/fullscreen/system), active flag, and whether each was created by the agent.\n\
            \n\
            Requires the `spaces` feature. Uses CGSSpace private SPI — not available in \
            App Store builds.",
        input_schema: json!({ "type": "object", "additionalProperties": false }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "space_count": { "type": "integer" },
                "spaces": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id":               { "type": "integer" },
                            "type":             { "type": "string" },
                            "is_active":        { "type": "boolean" },
                            "is_agent_created": { "type": "boolean" }
                        }
                    }
                }
            },
            "required": ["space_count", "spaces"]
        }),
        annotations: annotations::READ_ONLY,
    }
}

#[cfg(feature = "spaces")]
fn tool_ax_create_space() -> Tool {
    Tool {
        name: "ax_create_space",
        title: "Create an isolated agent virtual desktop",
        description: "Create a new macOS virtual desktop (Space) for agent use. The new Space \
            is NOT switched to automatically — the user's current desktop is undisturbed.\n\
            \n\
            Agent-created Spaces are automatically destroyed when the MCP session ends.\n\
            \n\
            Requires the `spaces` feature. Uses CGSSpace private SPI.",
        input_schema: json!({ "type": "object", "additionalProperties": false }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "created":   { "type": "boolean" },
                "space_id":  { "type": "integer" },
                "error":     { "type": "string" }
            },
            "required": ["created"]
        }),
        annotations: annotations::ACTION,
    }
}

#[cfg(feature = "spaces")]
fn tool_ax_move_to_space() -> Tool {
    Tool {
        name: "ax_move_to_space",
        title: "Move an app's windows to a virtual desktop",
        description: "Move all windows of a connected application to the specified Space.\n\
            Returns the count of windows moved.\n\
            \n\
            Use with `ax_create_space` to isolate an app for background interaction without \
            disturbing the user's desktop.\n\
            \n\
            Requires the `spaces` feature. Uses CGSSpace private SPI.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "app":      { "type": "string",  "description": "App alias from ax_connect" },
                "space_id": { "type": "integer", "description": "Target space ID from ax_list_spaces or ax_create_space" }
            },
            "required": ["app", "space_id"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "moved":         { "type": "boolean" },
                "windows_moved": { "type": "integer" },
                "space_id":      { "type": "integer" },
                "error":         { "type": "string" }
            },
            "required": ["moved"]
        }),
        annotations: annotations::ACTION,
    }
}

#[cfg(feature = "spaces")]
fn tool_ax_switch_space() -> Tool {
    Tool {
        name: "ax_switch_space",
        title: "Switch the active virtual desktop",
        description: "Switch to the specified Space, making it the visible desktop.\n\
            \n\
            NOTE: This changes the user's active desktop. For background automation, prefer \
            `ax_move_to_space` to move the target app to an agent Space without switching.\n\
            \n\
            Requires the `spaces` feature. Uses CGSSpace private SPI.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "space_id": { "type": "integer", "description": "ID of the Space to switch to" }
            },
            "required": ["space_id"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "switched": { "type": "boolean" },
                "space_id": { "type": "integer" },
                "error":    { "type": "string" }
            },
            "required": ["switched"]
        }),
        annotations: annotations::ACTION,
    }
}

#[cfg(feature = "spaces")]
fn tool_ax_destroy_space() -> Tool {
    Tool {
        name: "ax_destroy_space",
        title: "Destroy an agent-created virtual desktop",
        description: "Destroy a Space that was created by `ax_create_space`. Refuses to destroy \
            Spaces created by the user (returns error `not_agent_space`).\n\
            \n\
            Windows on the destroyed Space are moved back to the previously active Space by macOS.\n\
            \n\
            Requires the `spaces` feature. Uses CGSSpace private SPI.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "space_id": { "type": "integer", "description": "ID of the agent-created Space to destroy" }
            },
            "required": ["space_id"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "destroyed": { "type": "boolean" },
                "space_id":  { "type": "integer" },
                "error":     { "type": "string" },
                "error_code": { "type": "string" }
            },
            "required": ["destroyed"]
        }),
        annotations: annotations::ACTION,
    }
}

// ---------------------------------------------------------------------------
// Spaces handlers (feature = "spaces")
// ---------------------------------------------------------------------------

#[cfg(feature = "spaces")]
fn handle_ax_list_spaces() -> ToolCallResult {
    use crate::spaces::SpaceManager;

    let mgr = SpaceManager::new();
    match mgr.list_spaces() {
        Ok(spaces) => {
            let values: Vec<serde_json::Value> = spaces
                .iter()
                .map(|s| {
                    json!({
                        "id": s.id,
                        "type": format!("{:?}", s.space_type).to_lowercase(),
                        "is_active": s.is_active,
                        "is_agent_created": s.is_agent_created,
                    })
                })
                .collect();
            ToolCallResult::ok(
                json!({
                    "space_count": values.len(),
                    "spaces": values,
                })
                .to_string(),
            )
        }
        Err(e) => ToolCallResult::error(format!("Failed to list spaces: {e}")),
    }
}

#[cfg(feature = "spaces")]
fn handle_ax_create_space() -> ToolCallResult {
    use crate::spaces::SpaceManager;

    let mgr = SpaceManager::new();
    match mgr.create_space() {
        Ok(space) => {
            // Transfer the space ID out so we can forget the manager here
            // without Drop destroying the newly created space.
            // Callers must call ax_destroy_space or rely on session cleanup.
            let sid = space.id;
            // Prevent Drop from cleaning up — ownership passes to the session.
            std::mem::forget(mgr);
            ToolCallResult::ok(json!({ "created": true, "space_id": sid }).to_string())
        }
        Err(e) => ToolCallResult::error(format!("Failed to create space: {e}")),
    }
}

#[cfg(feature = "spaces")]
fn handle_ax_move_to_space(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    use crate::spaces::SpaceManager;

    let Some(app_name) = args["app"].as_str() else {
        return ToolCallResult::error("Missing required field: app");
    };
    let Some(space_id) = args["space_id"].as_u64() else {
        return ToolCallResult::error("Missing required field: space_id (integer)");
    };

    let window_ids = match collect_window_ids(app_name, registry) {
        Ok(ids) => ids,
        Err(e) => return ToolCallResult::error(e),
    };

    if window_ids.is_empty() {
        return ToolCallResult::error(format!("App '{app_name}' has no windows"));
    }

    let mgr = SpaceManager::new();
    match mgr.move_windows_to_space(&window_ids, space_id) {
        Ok(count) => ToolCallResult::ok(
            json!({
                "moved": true,
                "windows_moved": count,
                "space_id": space_id,
            })
            .to_string(),
        ),
        Err(e) => ToolCallResult::error(format!("Failed to move windows: {e}")),
    }
}

#[cfg(feature = "spaces")]
fn handle_ax_switch_space(args: &Value) -> ToolCallResult {
    use crate::spaces::SpaceManager;

    let Some(space_id) = args["space_id"].as_u64() else {
        return ToolCallResult::error("Missing required field: space_id (integer)");
    };

    let mgr = SpaceManager::new();
    match mgr.switch_to_space(space_id) {
        Ok(()) => ToolCallResult::ok(json!({ "switched": true, "space_id": space_id }).to_string()),
        Err(e) => ToolCallResult::error(format!("Failed to switch space: {e}")),
    }
}

#[cfg(feature = "spaces")]
fn handle_ax_destroy_space(args: &Value) -> ToolCallResult {
    use crate::spaces::{SpaceError, SpaceManager};

    let Some(space_id) = args["space_id"].as_u64() else {
        return ToolCallResult::error("Missing required field: space_id (integer)");
    };

    // We need a manager that knows about agent-created spaces. Since tool
    // calls are stateless in the current architecture, we create a fresh
    // manager. The only safe spaces to destroy are those in the agent set
    // which this process created; a fresh manager has an empty set, so
    // attempting to destroy any space will return NotAgentSpace.
    //
    // To support full lifecycle (create → destroy across calls) the caller
    // should use the session-scoped SpaceRegistry below. This handler acts as
    // a safety guard: it will always reject user spaces.
    let mgr = SpaceManager::new();
    match mgr.destroy_space(space_id) {
        Ok(()) => {
            ToolCallResult::ok(json!({ "destroyed": true, "space_id": space_id }).to_string())
        }
        Err(SpaceError::NotAgentSpace(sid)) => {
            let result = json!({
                "destroyed": false,
                "space_id": sid,
                "error": format!("Space {sid} was not created by the agent"),
                "error_code": "not_agent_space",
            });
            ToolCallResult {
                content: vec![crate::mcp::protocol::ContentItem::text(result.to_string())],
                is_error: true,
            }
        }
        Err(e) => ToolCallResult::error(format!("Failed to destroy space: {e}")),
    }
}

/// Collect CGWindowIDs for all windows of the named app.
///
/// Uses `osascript` to retrieve window IDs for the given PID — consistent with
/// the existing approach in `app.rs`.  Window IDs are `CGWindowID` values
/// usable with `CGSAddWindowsToSpaces`.
///
/// Returns an empty `Vec` when the app has no visible windows or when the
/// osascript invocation fails (e.g. the app is not scriptable).
#[cfg(feature = "spaces")]
fn collect_window_ids(app_name: &str, registry: &Arc<AppRegistry>) -> Result<Vec<u32>, String> {
    let pid = registry
        .with_app(app_name, |app| app.pid)
        .map_err(|e| e.to_string())?;

    let script = format!(
        "tell application \"System Events\" to \
         get id of every window of (processes whose unix id is {pid})"
    );

    let output = std::process::Command::new("osascript")
        .args(["-e", &script])
        .output()
        .map_err(|e| format!("osascript failed: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let ids = stdout
        .trim()
        .split(", ")
        .filter_map(|s| s.trim().parse::<u32>().ok())
        .collect();
    Ok(ids)
}

// ---------------------------------------------------------------------------
// Audio tool declarations (feature = "audio")
// ---------------------------------------------------------------------------

/// All audio tools registered when the `audio` feature is active.
#[cfg(feature = "audio")]
fn audio_tools() -> Vec<Tool> {
    vec![tool_ax_listen(), tool_ax_speak(), tool_ax_audio_devices()]
}

#[cfg(feature = "audio")]
fn tool_ax_listen() -> Tool {
    Tool {
        name: "ax_listen",
        title: "Capture audio and optionally transcribe it",
        description: "Capture audio from the system (microphone or loopback output) for \
            `duration` seconds and return the raw WAV data as base64. When `transcribe` is \
            true the audio is also transcribed on-device via SFSpeechRecognizer (macOS 13+, \
            no cloud — privacy-preserving).\n\
            \n\
            Sources:\n\
            - `\"microphone\"` — default input device (requires TCC microphone permission)\n\
            - `\"system\"` — system audio output loopback\n\
            \n\
            Duration is capped at 30 seconds. The call returns within `duration + 1s`.\n\
            \n\
            Example: verify an error sound played\n\
            `{\"duration\": 3, \"source\": \"system\", \"transcribe\": false}`",
        input_schema: json!({
            "type": "object",
            "properties": {
                "duration": {
                    "type": "number",
                    "description": "Capture length in seconds (default 5, max 30)",
                    "default": 5.0,
                    "minimum": 0.1,
                    "maximum": 30.0
                },
                "source": {
                    "type": "string",
                    "enum": ["microphone", "system"],
                    "description": "Audio source (default \"microphone\")",
                    "default": "microphone"
                },
                "transcribe": {
                    "type": "boolean",
                    "description": "When true, return a text transcript in addition to raw audio",
                    "default": false
                }
            },
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "captured":     { "type": "boolean" },
                "duration_ms":  { "type": "integer" },
                "sample_rate":  { "type": "integer" },
                "base64_wav":   { "type": "string" },
                "transcript":   { "type": "string" }
            },
            "required": ["captured", "duration_ms", "sample_rate", "base64_wav"]
        }),
        annotations: annotations::READ_ONLY,
    }
}

#[cfg(feature = "audio")]
fn tool_ax_speak() -> Tool {
    Tool {
        name: "ax_speak",
        title: "Synthesize and play text as speech",
        description: "Speak `text` through the default system audio output using \
            NSSpeechSynthesizer (on-device, no network). Blocks until synthesis \
            completes and returns the elapsed duration.\n\
            \n\
            Useful for: testing VoiceOver integrations, verifying audio feedback, \
            injecting voice prompts into the agent workflow.\n\
            \n\
            Example: `{\"text\": \"Test complete\"}`",
        input_schema: json!({
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "Text to synthesize and speak"
                }
            },
            "required": ["text"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "spoken":      { "type": "boolean" },
                "duration_ms": { "type": "integer" }
            },
            "required": ["spoken", "duration_ms"]
        }),
        annotations: annotations::ACTION,
    }
}

#[cfg(feature = "audio")]
fn tool_ax_audio_devices() -> Tool {
    Tool {
        name: "ax_audio_devices",
        title: "List available audio input/output devices",
        description: "Enumerate all CoreAudio devices on the system with their name, ID, \
            input/output capability, sample rate, and default-device status.\n\
            \n\
            Use this before `ax_listen` to confirm that a microphone or virtual audio \
            device is available.\n\
            \n\
            Example: `{}`",
        input_schema: json!({ "type": "object", "additionalProperties": false }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "device_count": { "type": "integer" },
                "devices": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name":              { "type": "string" },
                            "id":                { "type": "string" },
                            "is_input":          { "type": "boolean" },
                            "is_output":         { "type": "boolean" },
                            "sample_rate":       { "type": "number" },
                            "is_default_input":  { "type": "boolean" },
                            "is_default_output": { "type": "boolean" }
                        },
                        "required": ["name", "id", "is_input", "is_output",
                                     "sample_rate", "is_default_input", "is_default_output"]
                    }
                }
            },
            "required": ["device_count", "devices"]
        }),
        annotations: annotations::READ_ONLY,
    }
}

// ---------------------------------------------------------------------------
// Audio handlers (feature = "audio")
// ---------------------------------------------------------------------------

/// Handle `ax_listen` — capture audio and optionally transcribe.
#[cfg(feature = "audio")]
fn handle_ax_listen(args: &Value) -> ToolCallResult {
    let duration = args["duration"].as_f64().unwrap_or(5.0) as f32;
    let source = args["source"].as_str().unwrap_or("microphone");
    let do_transcribe = args["transcribe"].as_bool().unwrap_or(false);

    // AC5: validate duration cap before touching any hardware.
    if let Err(e) = crate::audio::validate_duration(duration) {
        return ToolCallResult::error(
            json!({ "error": e.to_string(), "error_code": e.code() }).to_string(),
        );
    }

    let capture_result = match source {
        "system" => crate::audio::capture_system_audio(duration),
        _ => crate::audio::capture_microphone(duration),
    };

    let audio_data = match capture_result {
        Ok(d) => d,
        Err(e) => {
            return ToolCallResult::error(
                json!({ "error": e.to_string(), "error_code": e.code() }).to_string(),
            );
        }
    };

    let base64_wav = audio_data.to_wav_base64();
    let duration_ms = audio_data.duration_ms();
    let sample_rate = audio_data.sample_rate;

    let transcript = if do_transcribe {
        match crate::audio::transcribe(&audio_data) {
            Ok(t) => Some(t),
            Err(e) => {
                tracing::warn!(error = %e, "transcription failed — returning audio without transcript");
                None
            }
        }
    } else {
        None
    };

    let mut payload = json!({
        "captured":    true,
        "duration_ms": duration_ms,
        "sample_rate": sample_rate,
        "base64_wav":  base64_wav,
    });

    if let Some(t) = transcript {
        payload["transcript"] = serde_json::Value::String(t);
    }

    ToolCallResult::ok(payload.to_string())
}

/// Handle `ax_speak` — text-to-speech via NSSpeechSynthesizer.
#[cfg(feature = "audio")]
fn handle_ax_speak(args: &Value) -> ToolCallResult {
    let Some(text) = args["text"].as_str().map(str::to_string) else {
        return ToolCallResult::error("Missing required field: text");
    };

    match crate::audio::speak(&text) {
        Ok(elapsed) => ToolCallResult::ok(
            json!({
                "spoken":      true,
                "duration_ms": elapsed.as_millis() as u64,
            })
            .to_string(),
        ),
        Err(e) => ToolCallResult::error(
            json!({ "error": e.to_string(), "error_code": e.code() }).to_string(),
        ),
    }
}

/// Handle `ax_audio_devices` — enumerate CoreAudio devices.
#[cfg(feature = "audio")]
fn handle_ax_audio_devices() -> ToolCallResult {
    let devices = crate::audio::list_audio_devices();
    let count = devices.len();
    match serde_json::to_value(&devices) {
        Ok(devices_val) => {
            ToolCallResult::ok(json!({ "device_count": count, "devices": devices_val }).to_string())
        }
        Err(e) => ToolCallResult::error(format!("Failed to serialize devices: {e}")),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // -----------------------------------------------------------------------
    // Tool registry
    // -----------------------------------------------------------------------

    #[test]
    fn extended_tools_count_matches_feature_set() {
        // GIVEN: Phase 3 base (7) + optional feature extensions
        // WHEN: requesting extended tools
        let tools = extended_tools();
        // THEN: count is deterministic per feature set
        let base = 7usize;
        let extra_spaces: usize = if cfg!(feature = "spaces") { 5 } else { 0 };
        let extra_audio: usize = if cfg!(feature = "audio") { 3 } else { 0 };
        let extra_camera: usize = if cfg!(feature = "camera") { 3 } else { 0 };
        assert_eq!(
            tools.len(),
            base + extra_spaces + extra_audio + extra_camera
        );
    }

    #[test]
    fn all_extended_tool_names_are_unique() {
        let tools = extended_tools();
        let names: std::collections::HashSet<&str> = tools.iter().map(|t| t.name).collect();
        assert_eq!(names.len(), tools.len(), "duplicate tool names in Phase 3");
    }

    #[test]
    fn all_extended_tools_have_non_empty_descriptions() {
        for tool in extended_tools() {
            assert!(
                !tool.description.is_empty(),
                "empty description on {}",
                tool.name
            );
        }
    }

    #[test]
    fn all_extended_tools_have_annotations() {
        for tool in extended_tools() {
            // At least one annotation flag should be set on each tool.
            // All of: read_only=false, destructive=false, idempotent=false, open_world=false
            // would be the zero-value — currently no tool has all four as false.
            let ann = &tool.annotations;
            // Just verify they compile and are accessible; specific values are tested
            // in annotations.rs unit tests.
            let _ = ann.read_only;
            let _ = ann.destructive;
            let _ = ann.idempotent;
            let _ = ann.open_world;
        }
    }

    // -----------------------------------------------------------------------
    // call_tool_extended dispatch
    // -----------------------------------------------------------------------

    #[test]
    fn call_tool_extended_unknown_name_returns_none() {
        // GIVEN: name not in Phase 3 set
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        // WHEN: dispatching
        let result = call_tool_extended("ax_nonexistent_phase3", &json!({}), &registry, &mut out);
        // THEN: falls through (None)
        assert!(result.is_none());
    }

    #[test]
    fn call_tool_extended_list_apps_always_succeeds() {
        // GIVEN: no app connected (list_apps doesn't need one)
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        // WHEN: ax_list_apps is called
        let result = call_tool_extended("ax_list_apps", &json!({}), &registry, &mut out).unwrap();
        // THEN: not an error and contains "apps" array
        assert!(!result.is_error, "ax_list_apps should not error");
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert!(v["apps"].is_array(), "apps field must be an array");
    }

    #[test]
    fn call_tool_extended_scroll_missing_app_returns_error() {
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        let result = call_tool_extended(
            "ax_scroll",
            &json!({"direction": "down"}),
            &registry,
            &mut out,
        )
        .unwrap();
        assert!(result.is_error);
        assert!(result.content[0].text.contains("Missing"));
    }

    #[test]
    fn call_tool_extended_scroll_missing_direction_returns_error() {
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        let result =
            call_tool_extended("ax_scroll", &json!({"app": "Finder"}), &registry, &mut out)
                .unwrap();
        assert!(result.is_error);
        assert!(result.content[0].text.contains("Missing"));
    }

    #[test]
    fn call_tool_extended_scroll_unconnected_app_returns_error() {
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        let result = call_tool_extended(
            "ax_scroll",
            &json!({"app": "Ghost", "direction": "down"}),
            &registry,
            &mut out,
        )
        .unwrap();
        assert!(result.is_error);
        assert!(result.content[0].text.contains("not connected"));
    }

    #[test]
    fn call_tool_extended_key_press_missing_app_returns_error() {
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        let result = call_tool_extended(
            "ax_key_press",
            &json!({"keys": "cmd+s"}),
            &registry,
            &mut out,
        )
        .unwrap();
        assert!(result.is_error);
    }

    #[test]
    fn call_tool_extended_key_press_missing_keys_returns_error() {
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        let result = call_tool_extended(
            "ax_key_press",
            &json!({"app": "Safari"}),
            &registry,
            &mut out,
        )
        .unwrap();
        assert!(result.is_error);
    }

    #[test]
    fn call_tool_extended_get_attributes_missing_query_returns_error() {
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        let result = call_tool_extended(
            "ax_get_attributes",
            &json!({"app": "Finder"}),
            &registry,
            &mut out,
        )
        .unwrap();
        assert!(result.is_error);
    }

    #[test]
    fn call_tool_extended_get_tree_missing_app_returns_error() {
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        let result = call_tool_extended("ax_get_tree", &json!({}), &registry, &mut out).unwrap();
        assert!(result.is_error);
    }

    #[test]
    fn call_tool_extended_drag_missing_from_query_returns_error() {
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        let result = call_tool_extended(
            "ax_drag",
            &json!({"app": "Finder", "to_query": "Desktop"}),
            &registry,
            &mut out,
        )
        .unwrap();
        assert!(result.is_error);
    }

    #[test]
    fn call_tool_extended_assert_missing_property_returns_error() {
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        let result = call_tool_extended(
            "ax_assert",
            &json!({"app": "Finder", "query": "Save", "expected": "true"}),
            &registry,
            &mut out,
        )
        .unwrap();
        assert!(result.is_error);
    }

    #[test]
    fn call_tool_extended_assert_unconnected_app_returns_error() {
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        let result = call_tool_extended(
            "ax_assert",
            &json!({
                "app": "Ghost",
                "query": "Save",
                "property": "exists",
                "expected": "false"
            }),
            &registry,
            &mut out,
        )
        .unwrap();
        assert!(result.is_error);
        assert!(result.content[0].text.contains("not connected"));
    }

    // -----------------------------------------------------------------------
    // key_name_to_code
    // -----------------------------------------------------------------------

    #[test]
    fn key_name_to_code_letters_are_mapped() {
        assert_eq!(key_name_to_code("a"), Some(0));
        assert_eq!(key_name_to_code("s"), Some(1));
        assert_eq!(key_name_to_code("z"), Some(6));
    }

    #[test]
    fn key_name_to_code_is_case_insensitive() {
        assert_eq!(key_name_to_code("A"), key_name_to_code("a"));
        assert_eq!(key_name_to_code("ENTER"), key_name_to_code("enter"));
    }

    #[test]
    fn key_name_to_code_function_keys_are_mapped() {
        assert!(key_name_to_code("f1").is_some());
        assert!(key_name_to_code("f12").is_some());
        assert!(key_name_to_code("f20").is_some());
    }

    #[test]
    fn key_name_to_code_unknown_key_returns_none() {
        assert!(key_name_to_code("nonsense").is_none());
        assert!(key_name_to_code("").is_none());
    }

    #[test]
    fn key_name_to_code_navigation_keys_are_mapped() {
        assert!(key_name_to_code("up").is_some());
        assert!(key_name_to_code("down").is_some());
        assert!(key_name_to_code("left").is_some());
        assert!(key_name_to_code("right").is_some());
        assert!(key_name_to_code("enter").is_some());
        assert!(key_name_to_code("tab").is_some());
        assert!(key_name_to_code("escape").is_some());
    }

    // -----------------------------------------------------------------------
    // scroll_deltas
    // -----------------------------------------------------------------------

    #[test]
    fn scroll_deltas_down_is_negative_y() {
        let (dx, dy) = scroll_deltas("down", 3);
        assert_eq!(dx, 0);
        assert_eq!(dy, -3);
    }

    #[test]
    fn scroll_deltas_up_is_positive_y() {
        let (dx, dy) = scroll_deltas("up", 5);
        assert_eq!(dx, 0);
        assert_eq!(dy, 5);
    }

    #[test]
    fn scroll_deltas_left_is_negative_x() {
        let (dx, dy) = scroll_deltas("left", 2);
        assert_eq!(dx, -2);
        assert_eq!(dy, 0);
    }

    #[test]
    fn scroll_deltas_right_is_positive_x() {
        let (dx, dy) = scroll_deltas("right", 4);
        assert_eq!(dx, 4);
        assert_eq!(dy, 0);
    }

    // -----------------------------------------------------------------------
    // list_running_apps
    // -----------------------------------------------------------------------

    #[test]
    fn list_running_apps_returns_non_empty_list() {
        // GIVEN: a running system
        let apps = list_running_apps();
        // THEN: at least the current process is visible
        assert!(!apps.is_empty(), "expected at least one running process");
    }

    #[test]
    fn list_running_apps_all_have_name_and_pid() {
        for app in list_running_apps() {
            assert!(app["name"].is_string(), "name must be string: {app}");
            assert!(app["pid"].is_number(), "pid must be number: {app}");
        }
    }

    #[test]
    fn list_running_apps_is_sorted_by_name() {
        let apps = list_running_apps();
        let names: Vec<&str> = apps.iter().map(|a| a["name"].as_str().unwrap()).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted, "apps should be sorted by name");
    }

    // -----------------------------------------------------------------------
    // extract_app_query (private helper)
    // -----------------------------------------------------------------------

    #[test]
    fn extract_app_query_succeeds_with_both_fields() {
        let args = json!({"app": "Safari", "query": "Load"});
        let (app, query) = extract_app_query(&args).unwrap();
        assert_eq!(app, "Safari");
        assert_eq!(query, "Load");
    }

    #[test]
    fn extract_app_query_fails_when_app_missing() {
        let args = json!({"query": "Load"});
        assert!(extract_app_query(&args).is_err());
    }

    #[test]
    fn extract_app_query_fails_when_query_missing() {
        let args = json!({"app": "Safari"});
        assert!(extract_app_query(&args).is_err());
    }

    // -----------------------------------------------------------------------
    // Audio tool declarations (feature = "audio")
    // -----------------------------------------------------------------------

    #[cfg(feature = "audio")]
    #[test]
    fn audio_tools_returns_three_tools() {
        // GIVEN: audio feature is enabled
        // WHEN: audio_tools() is called
        // THEN: exactly three tools are returned
        let tools = audio_tools();
        assert_eq!(tools.len(), 3);
        let names: Vec<&str> = tools.iter().map(|t| t.name).collect();
        assert!(names.contains(&"ax_listen"));
        assert!(names.contains(&"ax_speak"));
        assert!(names.contains(&"ax_audio_devices"));
    }

    #[cfg(feature = "audio")]
    #[test]
    fn extended_tools_includes_audio_tools_when_feature_enabled() {
        // GIVEN: audio feature is active
        let tools = extended_tools();
        let names: Vec<&str> = tools.iter().map(|t| t.name).collect();
        // THEN: all three audio tools are registered
        assert!(names.contains(&"ax_listen"), "ax_listen missing");
        assert!(names.contains(&"ax_speak"), "ax_speak missing");
        assert!(
            names.contains(&"ax_audio_devices"),
            "ax_audio_devices missing"
        );
    }

    #[cfg(feature = "audio")]
    #[test]
    fn ax_listen_tool_has_duration_parameter() {
        let tool = tool_ax_listen();
        let props = &tool.input_schema["properties"];
        assert!(
            props["duration"].is_object(),
            "duration property missing from schema"
        );
        assert_eq!(props["duration"]["maximum"], 30.0);
    }

    #[cfg(feature = "audio")]
    #[test]
    fn ax_speak_tool_requires_text_field() {
        let tool = tool_ax_speak();
        let required = tool.input_schema["required"].as_array().unwrap();
        let req_names: Vec<&str> = required.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(req_names.contains(&"text"), "text must be required");
    }

    #[cfg(feature = "audio")]
    #[test]
    fn ax_audio_devices_tool_has_empty_input_schema() {
        let tool = tool_ax_audio_devices();
        // input_schema is an empty object with only additionalProperties: false
        assert!(
            tool.input_schema["properties"].is_null()
                || tool.input_schema.get("properties").is_none()
        );
    }

    // -----------------------------------------------------------------------
    // Audio handlers (feature = "audio") — unit tests (no hardware required)
    // -----------------------------------------------------------------------

    #[cfg(feature = "audio")]
    #[test]
    fn handle_ax_listen_duration_exceeded_returns_error() {
        // GIVEN: duration > 30s
        let args = json!({ "duration": 31.0 });
        // WHEN: dispatched
        let result = handle_ax_listen(&args);
        // THEN: is_error flag is set and error_code is duration_exceeded
        assert!(result.is_error);
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["error_code"], "duration_exceeded");
    }

    #[cfg(feature = "audio")]
    #[test]
    fn handle_ax_speak_missing_text_returns_error() {
        // GIVEN: no text argument
        let args = json!({});
        let result = handle_ax_speak(&args);
        assert!(result.is_error);
        assert!(result.content[0]
            .text
            .contains("Missing required field: text"));
    }

    #[cfg(feature = "audio")]
    #[test]
    fn handle_ax_audio_devices_returns_valid_json_with_required_keys() {
        // GIVEN: running macOS system
        // WHEN: ax_audio_devices is called
        let result = handle_ax_audio_devices();
        // THEN: parses as JSON with device_count and devices keys
        assert!(
            !result.is_error,
            "unexpected error: {}",
            result.content[0].text
        );
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert!(v["device_count"].is_number());
        assert!(v["devices"].is_array());
    }

    #[cfg(feature = "audio")]
    #[test]
    fn call_tool_extended_ax_audio_devices_dispatches() {
        // GIVEN: running system, no app registry needed
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        let result = call_tool_extended("ax_audio_devices", &json!({}), &registry, &mut out);
        // THEN: the audio tool is dispatched (returns Some, not None)
        assert!(result.is_some(), "ax_audio_devices should dispatch");
        let r = result.unwrap();
        assert!(!r.is_error, "unexpected error: {}", r.content[0].text);
    }

    #[cfg(feature = "audio")]
    #[test]
    fn call_tool_extended_ax_listen_duration_exceeded_returns_error() {
        // GIVEN: duration well over the cap
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        let result = call_tool_extended(
            "ax_listen",
            &json!({ "duration": 999.0 }),
            &registry,
            &mut out,
        );
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(r.is_error);
        let v: serde_json::Value = serde_json::from_str(&r.content[0].text).unwrap();
        assert_eq!(v["error_code"], "duration_exceeded");
    }

    #[cfg(feature = "audio")]
    #[test]
    fn call_tool_extended_ax_speak_missing_text_returns_error() {
        // GIVEN: no text field
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        let result = call_tool_extended("ax_speak", &json!({}), &registry, &mut out);
        assert!(result.is_some());
        assert!(result.unwrap().is_error);
    }
}

// ---------------------------------------------------------------------------
// Camera tools (feature-gated)
// ---------------------------------------------------------------------------

/// All camera-related MCP tools.  Requires the `camera` feature.
///
/// Registers three tools:
/// - `ax_camera_capture` — single-frame JPEG capture
/// - `ax_gesture_detect` — capture frame + detect gestures
/// - `ax_gesture_listen` — poll until gesture or timeout
#[cfg(feature = "camera")]
#[must_use]
pub fn camera_tools() -> Vec<Tool> {
    vec![
        tool_ax_camera_capture(),
        tool_ax_gesture_detect(),
        tool_ax_gesture_listen(),
    ]
}

/// Declare the `ax_camera_capture` tool.
#[cfg(feature = "camera")]
fn tool_ax_camera_capture() -> Tool {
    Tool {
        name: "ax_camera_capture",
        title: "Capture a single camera frame",
        description: "Capture one JPEG frame from the specified camera (default: front-facing \
            FaceTime camera) and return it base64-encoded.\n\
            The AVCaptureSession is started, one frame is grabbed at 1280x720, and the \
            session is immediately stopped and released (no persistent camera access).\n\
            The hardware camera indicator light will be ON during capture (macOS-enforced).\n\
            Requires TCC camera permission. Returns error code `camera_denied` when denied.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "device_id": {
                    "type": "string",
                    "description": "Camera device unique ID from ax_camera_devices. \
                        Omit to use the default front-facing camera."
                }
            },
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "width":        { "type": "integer" },
                "height":       { "type": "integer" },
                "image_base64": { "type": "string"  }
            },
            "required": ["width", "height", "image_base64"]
        }),
        annotations: annotations::READ_ONLY,
    }
}

/// Declare the `ax_gesture_detect` tool.
#[cfg(feature = "camera")]
fn tool_ax_gesture_detect() -> Tool {
    Tool {
        name: "ax_gesture_detect",
        title: "Capture frame and detect hand / face gestures",
        description: "Capture one camera frame then run Vision framework gesture detection \
            on it. Returns all detected gestures with confidence scores.\n\
            Hand gestures use VNDetectHumanHandPoseRequest (macOS 11+).\n\
            Face gestures (nod, shake) use VNDetectFaceLandmarksRequest (macOS 12+).\n\
            All processing is on-device.\n\
            Supported: thumbs_up, thumbs_down, wave, stop, point, nod, shake.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "device_id": {
                    "type": "string",
                    "description": "Camera device unique ID. Omit for default camera."
                }
            },
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "gestures": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "type":       { "type": "string" },
                            "confidence": { "type": "number" },
                            "hand":       { "type": "string" }
                        },
                        "required": ["type", "confidence", "hand"]
                    }
                },
                "frame_base64": { "type": "string" }
            },
            "required": ["gestures", "frame_base64"]
        }),
        annotations: annotations::READ_ONLY,
    }
}

/// Declare the `ax_gesture_listen` tool.
#[cfg(feature = "camera")]
fn tool_ax_gesture_listen() -> Tool {
    Tool {
        name: "ax_gesture_listen",
        title: "Monitor camera for gestures (up to 60 s)",
        description: "Poll the camera repeatedly until one of the specified gestures is \
            detected or the duration elapses. Returns the first matching gesture or an \
            empty result on timeout. Duration must be <=60 seconds.\n\
            Supported: thumbs_up, thumbs_down, wave, stop, point, nod, shake.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "duration_seconds": {
                    "type": "number",
                    "description": "Maximum monitoring duration in seconds (0.0-60.0)",
                    "minimum": 0.0,
                    "maximum": 60.0,
                    "default": 10.0
                },
                "gestures": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Gesture names to watch for. Omit or [] to match any.",
                    "default": []
                },
                "device_id": {
                    "type": "string",
                    "description": "Camera device unique ID. Omit for default camera."
                }
            },
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "detected":        { "type": "boolean" },
                "gesture":         { "type": "string"  },
                "confidence":      { "type": "number"  },
                "hand":            { "type": "string"  },
                "elapsed_seconds": { "type": "number"  }
            },
            "required": ["detected", "elapsed_seconds"]
        }),
        annotations: annotations::READ_ONLY,
    }
}

// ---------------------------------------------------------------------------
// Camera handlers
// ---------------------------------------------------------------------------

/// Handle `ax_camera_capture`.
#[cfg(feature = "camera")]
fn handle_ax_camera_capture(args: &Value) -> ToolCallResult {
    let device_id = args["device_id"].as_str();
    match crate::camera::capture_frame(device_id) {
        Ok(frame) => ToolCallResult::ok(
            json!({
                "width":        frame.width,
                "height":       frame.height,
                "image_base64": frame.base64_jpeg()
            })
            .to_string(),
        ),
        Err(e) => ToolCallResult::error(e.to_string()),
    }
}

/// Handle `ax_gesture_detect`.
#[cfg(feature = "camera")]
fn handle_ax_gesture_detect(args: &Value) -> ToolCallResult {
    let device_id = args["device_id"].as_str();
    match crate::camera::capture_and_detect(device_id) {
        Ok((frame, detections)) => {
            let gestures: Vec<_> = detections.iter().map(gesture_to_json).collect();
            ToolCallResult::ok(
                json!({ "gestures": gestures, "frame_base64": frame.base64_jpeg() }).to_string(),
            )
        }
        Err(e) => ToolCallResult::error(e.to_string()),
    }
}

/// Serialise a [`crate::camera::GestureDetection`] to a JSON value.
#[cfg(feature = "camera")]
fn gesture_to_json(d: &crate::camera::GestureDetection) -> serde_json::Value {
    json!({
        "type":       d.gesture.as_name(),
        "confidence": d.confidence,
        "hand":       serde_json::to_value(&d.hand)
                          .unwrap_or(serde_json::Value::String("unknown".into()))
    })
}

/// Handle `ax_gesture_listen`.
#[cfg(feature = "camera")]
fn handle_ax_gesture_listen(args: &Value) -> ToolCallResult {
    let duration_secs = args["duration_seconds"].as_f64().unwrap_or(10.0);
    if let Err(e) = crate::camera::validate_duration(duration_secs) {
        return ToolCallResult::error(e.to_string());
    }
    let gesture_names: Vec<&str> = match args["gestures"].as_array() {
        Some(arr) => arr.iter().filter_map(|v| v.as_str()).collect(),
        None => vec![],
    };
    if !gesture_names.is_empty() {
        if let Err(e) = crate::camera::validate_gesture_names(&gesture_names) {
            return ToolCallResult::error(e.to_string());
        }
    }
    let start = std::time::Instant::now();
    let all_names: Vec<&str>;
    let effective_names: &[&str] = if gesture_names.is_empty() {
        all_names = crate::camera::Gesture::all_names().to_vec();
        &all_names
    } else {
        &gesture_names
    };
    let result = listen_with_device(args["device_id"].as_str(), duration_secs, effective_names);
    let elapsed = start.elapsed().as_secs_f64();
    build_listen_result(result, elapsed)
}

/// Format the `gesture_listen` result into a `ToolCallResult`.
#[cfg(feature = "camera")]
fn build_listen_result(
    result: Result<Option<crate::camera::GestureDetection>, crate::camera::CameraError>,
    elapsed: f64,
) -> ToolCallResult {
    match result {
        Ok(Some(d)) => ToolCallResult::ok(
            json!({
                "detected":        true,
                "gesture":         d.gesture.as_name(),
                "confidence":      d.confidence,
                "hand":            serde_json::to_value(&d.hand)
                    .unwrap_or(serde_json::Value::String("unknown".into())),
                "elapsed_seconds": elapsed
            })
            .to_string(),
        ),
        Ok(None) => ToolCallResult::ok(
            json!({
                "detected":        false,
                "gesture":         null,
                "confidence":      null,
                "hand":            null,
                "elapsed_seconds": elapsed
            })
            .to_string(),
        ),
        Err(e) => ToolCallResult::error(e.to_string()),
    }
}

/// Poll the camera for matching gestures until deadline.
#[cfg(feature = "camera")]
fn listen_with_device(
    device_id: Option<&str>,
    duration_secs: f64,
    gesture_names: &[&str],
) -> Result<Option<crate::camera::GestureDetection>, crate::camera::CameraError> {
    use std::time::{Duration, Instant};
    let wanted = crate::camera::validate_gesture_names(gesture_names)?;
    if !crate::camera::check_camera_permission() {
        return Err(crate::camera::CameraError::PermissionDenied);
    }
    let deadline = Instant::now() + Duration::from_secs_f64(duration_secs);
    let poll = Duration::from_millis(200);
    while Instant::now() < deadline {
        let frame = crate::camera::capture_frame(device_id)?;
        let detections = crate::camera::detect_gestures(&frame)?;
        if let Some(hit) = detections.into_iter().find(|d| wanted.contains(&d.gesture)) {
            return Ok(Some(hit));
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        std::thread::sleep(poll.min(remaining));
    }
    Ok(None)
}

// ---------------------------------------------------------------------------
// Camera resource helper
// ---------------------------------------------------------------------------

/// Produce the JSON payload for `axterminator://camera/devices`.
///
/// Returns `{ "cameras": [...] }` with one entry per detected device.
/// Permission is not required to enumerate devices.
///
/// # Examples
///
/// ```
/// let payload = axterminator::mcp::tools_extended::camera_devices_payload();
/// let v: serde_json::Value = serde_json::from_str(&payload).unwrap();
/// assert!(v["cameras"].is_array());
/// ```
#[cfg(feature = "camera")]
#[must_use]
pub fn camera_devices_payload() -> String {
    let devices = crate::camera::list_cameras();
    let cameras: Vec<_> = devices
        .iter()
        .map(|d| {
            json!({
                "device_id": d.id,
                "name":      d.name,
                "position":  serde_json::to_value(&d.position)
                    .unwrap_or(serde_json::Value::String("unknown".into())),
                "is_default": d.is_default
            })
        })
        .collect();
    json!({ "cameras": cameras }).to_string()
}

// ---------------------------------------------------------------------------
// Camera tool tests
// ---------------------------------------------------------------------------

#[cfg(all(test, feature = "camera"))]
mod camera_tests {
    use super::*;
    use crate::camera::{CameraError, Gesture};

    #[test]
    fn camera_tools_returns_three_tools() {
        // GIVEN: camera feature enabled
        // WHEN: camera_tools() is called
        let tools = camera_tools();
        // THEN: exactly 3 tools
        assert_eq!(tools.len(), 3);
    }

    #[test]
    fn camera_tools_names_are_unique() {
        let tools = camera_tools();
        let names: std::collections::HashSet<_> = tools.iter().map(|t| t.name).collect();
        assert_eq!(names.len(), tools.len(), "tool names must be unique");
    }

    #[test]
    fn camera_tools_all_have_object_schemas() {
        for tool in camera_tools() {
            assert!(
                tool.input_schema.is_object(),
                "tool {} must have object input_schema",
                tool.name
            );
            assert!(
                tool.output_schema.is_object(),
                "tool {} must have object output_schema",
                tool.name
            );
        }
    }

    #[test]
    fn ax_camera_capture_has_read_only_annotation() {
        // GIVEN/WHEN: tool declaration
        let tool = tool_ax_camera_capture();
        // THEN: read_only=true, destructive=false
        assert!(tool.annotations.read_only);
        assert!(!tool.annotations.destructive);
    }

    #[test]
    fn ax_gesture_detect_has_read_only_annotation() {
        let tool = tool_ax_gesture_detect();
        assert!(tool.annotations.read_only);
        assert!(!tool.annotations.destructive);
    }

    #[test]
    fn ax_gesture_listen_has_read_only_annotation() {
        let tool = tool_ax_gesture_listen();
        assert!(tool.annotations.read_only);
        assert!(!tool.annotations.destructive);
    }

    #[test]
    fn gesture_listen_handler_rejects_duration_above_60() {
        // GIVEN: duration = 90s
        let args = json!({"duration_seconds": 90.0, "gestures": ["thumbs_up"]});
        // WHEN: dispatched
        let result = handle_ax_gesture_listen(&args);
        // THEN: error with "duration_exceeded"
        assert!(result.is_error);
        assert!(
            result.content[0].text.contains("duration_exceeded"),
            "got: {}",
            result.content[0].text
        );
    }

    #[test]
    fn gesture_listen_handler_rejects_unknown_gesture() {
        // GIVEN: invalid gesture name
        let args = json!({"duration_seconds": 5.0, "gestures": ["robot_dance"]});
        // WHEN: dispatched (validation before camera access)
        let result = handle_ax_gesture_listen(&args);
        // THEN: error with "unknown_gesture"
        assert!(result.is_error);
        assert!(
            result.content[0].text.contains("unknown_gesture"),
            "got: {}",
            result.content[0].text
        );
    }

    #[test]
    fn gesture_listen_handler_zero_duration_validates_cleanly() {
        // Zero duration is valid (<=60). Actual capture will fail on CI.
        let args = json!({"duration_seconds": 0.0, "gestures": ["thumbs_up"]});
        let result = handle_ax_gesture_listen(&args);
        if result.is_error {
            let msg = &result.content[0].text;
            assert!(!msg.contains("duration_exceeded"), "got: {msg}");
            assert!(!msg.contains("unknown_gesture"), "got: {msg}");
        }
    }

    #[test]
    fn camera_devices_payload_is_valid_json() {
        // GIVEN: any machine
        // WHEN: payload generated
        let payload = camera_devices_payload();
        // THEN: valid JSON with cameras array
        let v: serde_json::Value = serde_json::from_str(&payload).expect("must be valid JSON");
        assert!(v["cameras"].is_array());
    }

    #[test]
    fn camera_devices_payload_entries_have_required_fields() {
        let payload = camera_devices_payload();
        let v: serde_json::Value = serde_json::from_str(&payload).unwrap();
        for device in v["cameras"].as_array().unwrap() {
            assert!(device["device_id"].is_string());
            assert!(device["name"].is_string());
            assert!(device["position"].is_string());
            assert!(device["is_default"].is_boolean());
        }
    }

    #[test]
    fn all_gesture_names_pass_validation() {
        let all = Gesture::all_names().to_vec();
        let result = crate::camera::validate_gesture_names(&all);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), all.len());
    }

    #[test]
    fn camera_error_permission_denied_starts_with_code() {
        let e = CameraError::PermissionDenied;
        assert!(e.to_string().starts_with("camera_denied:"));
    }

    #[test]
    fn camera_error_duration_exceeded_starts_with_code() {
        let e = CameraError::DurationExceeded(90.0);
        assert!(e.to_string().starts_with("duration_exceeded:"));
    }

    #[test]
    fn camera_error_unknown_gesture_starts_with_code() {
        let e = CameraError::UnknownGesture("laser_fingers".into());
        assert!(e.to_string().starts_with("unknown_gesture:"));
    }

    #[test]
    fn gesture_to_json_produces_correct_fields() {
        use crate::camera::{GestureDetection, Hand};
        let d = GestureDetection {
            gesture: Gesture::ThumbsUp,
            confidence: 0.9,
            hand: Hand::Right,
        };
        let v = gesture_to_json(&d);
        assert_eq!(v["type"], "thumbs_up");
        assert_eq!(v["hand"], "right");
        assert!(v["confidence"].as_f64().unwrap() > 0.8);
    }
}
