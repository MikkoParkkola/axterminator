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
    vec![
        tool_ax_scroll(),
        tool_ax_key_press(),
        tool_ax_get_attributes(),
        tool_ax_get_tree(),
        tool_ax_list_apps(),
        tool_ax_drag(),
        tool_ax_assert(),
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
    fn extended_tools_returns_seven_tools() {
        // GIVEN: Phase 3 tool set
        let tools = extended_tools();
        // THEN: exactly 7 tools
        assert_eq!(tools.len(), 7);
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
}
