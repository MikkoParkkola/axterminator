//! Handler functions for Phase 1 MCP tools.
//!
//! Each `handle_*` function corresponds to exactly one Phase 1 tool declared in
//! [`crate::mcp::tools`].  They are pure functions of their arguments and the
//! shared [`crate::mcp::tools::AppRegistry`] — no global mutable state.
//!
//! Argument-extraction helpers (`extract_*`, `parse_json_*`, `format_bounds`,
//! `extract_app_query`, …) live in [`crate::mcp::args`] so they are shared
//! across all handler modules without drifting.  Internal helpers that exist
//! solely to support these handlers (`parse_app_identifier`,
//! `parse_action_mode`, `click_at_coordinates`, `scan_scene_or_error`) remain
//! here.

use std::sync::Arc;

use serde_json::{json, Value};

use crate::app::AXApp;
use crate::mcp::action_safety::{is_element_destructive, require_destructive_confirmation};
use crate::mcp::args::{
    extract_app_query, extract_bool_field_or, extract_optional_string_field, extract_or_return,
    extract_required_i64_field, extract_required_string_field, extract_string_field_or,
    extract_u64_field_or, format_bounds,
};
use crate::mcp::protocol::ToolCallResult;
use crate::mcp::tools::AppRegistry;
use crate::mcp::tools_response::ok_found_value;

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

pub(crate) fn handle_is_accessible() -> ToolCallResult {
    let enabled = crate::accessibility::check_accessibility_enabled();
    let result = if enabled {
        json!({ "enabled": true })
    } else {
        json!({
            "enabled": false,
            "suggestion": "Open System Settings > Privacy & Security > Accessibility and enable the terminal app."
        })
    };
    if enabled {
        ToolCallResult::ok_json(result)
    } else {
        ToolCallResult {
            content: vec![crate::mcp::protocol::ContentItem::text(result.to_string())],
            is_error: true,
        }
    }
}

pub(crate) fn handle_connect(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    let app_id = extract_or_return!(extract_required_string_field(args, "app"));
    let alias = extract_optional_string_field(args, "alias").unwrap_or_else(|| app_id.clone());

    let (name, bundle_id, pid) = parse_app_identifier(&app_id);
    match AXApp::connect_native(name.as_deref(), bundle_id.as_deref(), pid) {
        Ok(app) => {
            #[allow(clippy::cast_sign_loss)]
            let connected_pid = app.pid as u32;
            let bundle = app.bundle_id.clone();
            let app_type = crate::router::detect_app_type(bundle.as_deref().unwrap_or(""), app.pid)
                .name()
                .to_string();
            registry.insert(alias.clone(), app);
            ToolCallResult::ok_json(json!({
                "connected": true,
                "alias": alias,
                "pid": connected_pid,
                "bundle_id": bundle,
                "app_type": app_type
            }))
        }
        Err(e) => ToolCallResult::error(format!("Failed to connect to '{app_id}': {e}")),
    }
}

pub(crate) fn handle_find(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    let (app_name, query) = extract_or_return!(extract_app_query(args));
    let timeout_ms = extract_u64_field_or(args, "timeout_ms", 5000);

    registry
        .with_app(&app_name, |app| {
            match app.find_native(&query, Some(timeout_ms)) {
                Ok(el) => {
                    let bounds_tuple = el.bounds();
                    let bounds_arr = format_bounds(bounds_tuple);
                    let locator = build_locator(el.role(), el.title(), bounds_tuple);
                    ToolCallResult::ok_json(json!({
                        "found": true,
                        "role": el.role(),
                        "title": el.title(),
                        "value": el.value(),
                        "enabled": el.enabled(),
                        "bounds": bounds_arr,
                        "locator": locator
                    }))
                }
                // Semantic fallback: try fuzzy matching on the scene graph.
                Err(_) => semantic_find_fallback(app, &query),
            }
        })
        .unwrap_or_else(ToolCallResult::error)
}

/// Semantic fallback when exact element search fails.
///
/// Builds a [`crate::intent::SceneGraph`] from the app's live AX tree and
/// uses bigram-based fuzzy matching via [`crate::semantic_find::SemanticFinder`]
/// to find the closest element.  Returns a result with `"semantic_match": true`
/// when confidence ≥ 0.3, or an error when nothing is close enough.
fn semantic_find_fallback(app: &crate::app::AXApp, query: &str) -> ToolCallResult {
    use crate::semantic_find::{FindQuery, SemanticFinder};

    // Build scene graph from the live AX tree.
    let scene = match crate::intent::scan_scene(app.element) {
        Ok(s) => s,
        Err(_) => {
            return ToolCallResult::error(format!(
                "Element not found: '{query}' (semantic fallback also failed)"
            ))
        }
    };

    let finder = SemanticFinder;
    let fq = FindQuery::new(query);
    let result = finder.find(&scene, &fq);

    if let Some(top) = result.matches.first() {
        if top.score >= 0.3 {
            return ToolCallResult::ok_json(json!({
                "found": true,
                "semantic_match": true,
                "confidence": top.score,
                "role": top.role,
                "label": top.label,
                "bounds": format_bounds(top.bounds),
                "reasoning": top.reasoning
            }));
        }
    }

    ToolCallResult::error(format!("Element not found: '{query}'"))
}

pub(crate) fn handle_click(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    let (app_name, query) = extract_or_return!(extract_app_query(args));
    let mode_str = extract_string_field_or(args, "mode", "background");
    let click_type = extract_string_field_or(args, "click_type", "single");
    let confirmed = extract_bool_field_or(args, "confirm", false);
    let mode = parse_action_mode(mode_str);

    registry
        .with_app(&app_name, |app| match app.find_native(&query, Some(100)) {
            Ok(el) => {
                let destructive = is_element_destructive(&el);
                if let Err(error) = require_destructive_confirmation(
                    &query,
                    destructive,
                    confirmed,
                    "ax_click",
                    "clicking",
                ) {
                    return error;
                }
                perform_click(&el, click_type, mode, &query, destructive)
            }
            Err(_) => ToolCallResult::error(format!("Element not found: '{query}'")),
        })
        .unwrap_or_else(ToolCallResult::error)
}

/// Execute the click and build the success response.
fn perform_click(
    el: &crate::AXElement,
    click_type: &str,
    mode: crate::ActionMode,
    query: &str,
    destructive: bool,
) -> ToolCallResult {
    let click_result = match click_type {
        "double" => el.double_click_native(mode),
        "right" => el.right_click_native(mode),
        _ => el.click_native(mode),
    };
    match click_result {
        Ok(()) => {
            let bounds = el.bounds();
            let mut resp = json!({
                "clicked": true,
                "query": query,
                "bounds": format_bounds(bounds)
            });
            if destructive {
                resp["destructive"] = json!(true);
            }
            ToolCallResult::ok_json(resp)
        }
        Err(e) => ToolCallResult::error(format!("Click failed: {e}")),
    }
}

pub(crate) fn handle_type(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    let (app_name, query) = extract_or_return!(extract_app_query(args));
    let text = extract_or_return!(extract_required_string_field(args, "text"));
    let mode_str = extract_string_field_or(args, "mode", "focus");
    let mode = parse_action_mode(mode_str);

    let char_count = text.chars().count();
    registry
        .with_app(&app_name, |app| match app.find_native(&query, Some(100)) {
            Ok(el) => match el.type_text_native(&text, mode) {
                Ok(()) => ToolCallResult::ok_json(json!({"typed": true, "char_count": char_count})),
                Err(e) => ToolCallResult::error(format!("Type failed: {e}")),
            },
            Err(_) => ToolCallResult::error(format!("Element not found: '{query}'")),
        })
        .unwrap_or_else(ToolCallResult::error)
}

pub(crate) fn handle_set_value(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    let (app_name, query) = extract_or_return!(extract_app_query(args));
    let value = extract_or_return!(extract_required_string_field(args, "value"));

    registry
        .with_app(&app_name, |app| match app.find_native(&query, Some(100)) {
            Ok(el) => match el.set_value_native(&value) {
                Ok(()) => ToolCallResult::ok_json(json!({"set": true, "value": value})),
                Err(e) => ToolCallResult::error(format!("set_value failed: {e}")),
            },
            Err(_) => ToolCallResult::error(format!("Element not found: '{query}'")),
        })
        .unwrap_or_else(ToolCallResult::error)
}

pub(crate) fn handle_get_value(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    let (app_name, query) = extract_or_return!(extract_app_query(args));

    registry
        .with_app(&app_name, |app| match app.find_native(&query, Some(100)) {
            Ok(el) => ok_found_value(el.value()),
            Err(_) => ToolCallResult::error(format!("Element not found: '{query}'")),
        })
        .unwrap_or_else(ToolCallResult::error)
}

pub(crate) fn handle_list_windows(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    let app_name = extract_or_return!(extract_required_string_field(args, "app"));

    registry
        .with_app(&app_name, |app| match app.windows_native() {
            Ok(windows) => {
                let items: Vec<Value> = windows
                    .iter()
                    .map(|w: &crate::AXElement| {
                        let bounds_val = format_bounds(w.bounds());
                        json!({ "title": w.title(), "bounds": bounds_val })
                    })
                    .collect();
                ToolCallResult::ok_json(json!({"windows": items}))
            }
            Err(e) => ToolCallResult::error(format!("Failed to list windows: {e}")),
        })
        .unwrap_or_else(ToolCallResult::error)
}

pub(crate) fn handle_screenshot(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    let app_name = extract_or_return!(extract_required_string_field(args, "app"));
    let query = args["query"].as_str().map(str::to_string);

    registry
        .with_app(&app_name, |app| {
            let data_result: Result<Vec<u8>, String> = if let Some(ref q) = query {
                app.find_native(q, Some(100))
                    .map_err(|e| e.to_string())
                    .and_then(|el| el.screenshot_native().map_err(|e| e.to_string()))
            } else {
                app.screenshot_native().map_err(|e| e.to_string())
            };

            match data_result {
                Ok(bytes) => {
                    use base64::Engine as _;
                    let b64 = base64::engine::general_purpose::STANDARD.encode::<&[u8]>(&bytes);
                    let size = bytes.len();
                    ToolCallResult::ok_json(json!({
                        "captured": true,
                        "base64_png": b64,
                        "size_bytes": size
                    }))
                }
                Err(e) => ToolCallResult::error(format!("Screenshot failed: {e}")),
            }
        })
        .unwrap_or_else(ToolCallResult::error)
}

pub(crate) fn handle_click_at(args: &Value) -> ToolCallResult {
    let x_raw = extract_or_return!(extract_required_i64_field(args, "x"));
    let y_raw = extract_or_return!(extract_required_i64_field(args, "y"));
    #[allow(clippy::cast_possible_truncation)]
    let x = x_raw as i32;
    #[allow(clippy::cast_possible_truncation)]
    let y = y_raw as i32;
    let click_type = extract_string_field_or(args, "click_type", "single");

    match click_at_coordinates(x, y, click_type) {
        Ok(()) => ToolCallResult::ok_json(json!({"clicked": true, "x": x, "y": y})),
        Err(e) => ToolCallResult::error(format!("click_at ({x},{y}) failed: {e}")),
    }
}

pub(crate) fn handle_find_visual(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    handle_find_visual_with_sampling(
        args,
        registry,
        crate::mcp::sampling::SamplingContext::unavailable(),
    )
}

/// `ax_find_visual` handler with explicit sampling capability context.
///
/// When the connected client advertises `sampling` support, the response
/// includes a base64-encoded PNG screenshot and a `sampling_available: true`
/// flag. This lets the client perform the VLM inference itself by:
///
/// 1. Receiving the screenshot in the tool result.
/// 2. Sending a `sampling/createMessage` to its own LLM with the image.
/// 3. Parsing the LLM's coordinate response.
/// 4. Calling `ax_click_at` with the identified coordinates.
///
/// When the client does not support sampling, the response is a clear error
/// message guiding the caller toward the manual screenshot + external VLM path.
///
/// # Synchronous stdio constraint
///
/// True mid-call sampling (write request, read response, continue) requires
/// the handler to hold references to stdin/stdout, which the synchronous
/// dispatch loop does not thread through to tool handlers. The pragmatic
/// resolution is to surface the screenshot and capability flag in the tool
/// result so the client drives the sampling loop itself.
pub(crate) fn handle_find_visual_with_sampling(
    args: &Value,
    registry: &Arc<AppRegistry>,
    sampling_ctx: crate::mcp::sampling::SamplingContext,
) -> ToolCallResult {
    let app_name = extract_or_return!(extract_required_string_field(args, "app"));
    let description = extract_or_return!(extract_required_string_field(args, "description"));

    registry
        .with_app(&app_name, |app| {
            build_find_visual_response(app, &description, sampling_ctx)
        })
        .unwrap_or_else(ToolCallResult::error)
}

/// Build the `ax_find_visual` response body based on sampling availability.
fn build_find_visual_response(
    app: &crate::app::AXApp,
    description: &str,
    sampling_ctx: crate::mcp::sampling::SamplingContext,
) -> ToolCallResult {
    if !sampling_ctx.is_available() {
        return ToolCallResult::error(format!(
            "Visual element detection for '{description}' requires a client that supports \
             MCP sampling (sampling/createMessage). The connected client does not advertise \
             this capability. Alternative: call ax_screenshot to capture the app, then use \
             an external VLM with the screenshot to locate the element."
        ));
    }

    // Client supports sampling — take a screenshot and include it so the
    // client can send it to its LLM via sampling/createMessage.
    match app.screenshot_native() {
        Ok(png_bytes) => {
            use base64::Engine as _;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
            let (messages, system_prompt) =
                crate::mcp::sampling::locate_element_messages(description, &png_bytes);

            ToolCallResult::ok_json(serde_json::json!({
                "sampling_available": true,
                "description": description,
                "screenshot_b64": b64,
                "screenshot_mime": "image/png",
                "sampling_request": {
                    "method": "sampling/createMessage",
                    "params": {
                        "messages": messages,
                        "maxTokens": 512,
                        "systemPrompt": system_prompt
                    }
                },
                "hint": "Send sampling_request to the LLM via sampling/createMessage, \
                         then parse the JSON response for {found, x, y} coordinates."
            }))
        }
        Err(e) => ToolCallResult::error(format!(
            "ax_find_visual: screenshot failed for visual sampling: {e}. \
             Use ax_screenshot separately and call an external VLM."
        )),
    }
}

pub(crate) fn handle_wait_idle(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    let app_name = extract_or_return!(extract_required_string_field(args, "app"));
    let timeout_ms = extract_u64_field_or(args, "timeout_ms", 5000);

    let start = std::time::Instant::now();
    registry
        .with_app(&app_name, |app| {
            let idle = app.wait_idle_native(timeout_ms);
            #[allow(clippy::cast_possible_truncation)]
            let elapsed = start.elapsed().as_millis() as u64;
            ToolCallResult::ok_json(json!({"idle": idle, "elapsed_ms": elapsed}))
        })
        .unwrap_or_else(ToolCallResult::error)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Build a healing-compatible locator from element attributes and bounds.
///
/// Agents can store this locator and pass it back in future calls to reliably
/// re-identify the element even after minor UI layout changes.
fn build_locator(
    role: Option<String>,
    title: Option<String>,
    bounds: Option<(f64, f64, f64, f64)>,
) -> Value {
    json!({
        "role": role,
        "title": title,
        "bounds": format_bounds(bounds)
    })
}

pub(crate) fn scan_scene_or_error(
    element: crate::accessibility::AXUIElementRef,
) -> Result<crate::intent::SceneGraph, String> {
    crate::intent::scan_scene(element).map_err(|e| format!("scan_scene failed: {e}"))
}

/// Parse an app identifier string into (name, `bundle_id`, pid) for `AXApp::connect`.
///
/// Heuristics:
/// - All digits → PID
/// - Contains two or more dots → bundle ID
/// - Otherwise → display name
pub(crate) fn parse_app_identifier(id: &str) -> (Option<String>, Option<String>, Option<u32>) {
    if id.chars().all(|c| c.is_ascii_digit()) {
        let pid: u32 = id.parse().unwrap_or(0);
        return (None, None, Some(pid));
    }
    if id.matches('.').count() >= 2 {
        return (None, Some(id.to_string()), None);
    }
    (Some(id.to_string()), None, None)
}

/// Map mode string to `ActionMode`.
pub(crate) fn parse_action_mode(mode: &str) -> crate::ActionMode {
    if mode == "focus" {
        crate::ActionMode::Focus
    } else {
        crate::ActionMode::Background
    }
}

/// Perform a coordinate click via `CGEvent` posted to the HID event tap.
///
/// # Safety
/// Calls macOS Core Graphics APIs. The coordinates must be within display bounds.
pub(crate) fn click_at_coordinates(x: i32, y: i32, click_type: &str) -> Result<(), String> {
    use core_graphics::event::{CGEvent, CGEventType, CGMouseButton};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    use core_graphics::geometry::CGPoint;

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|()| "Failed to create CGEventSource".to_string())?;

    let point = CGPoint::new(f64::from(x), f64::from(y));

    let post_click = |down_type: CGEventType, up_type: CGEventType, btn: CGMouseButton| {
        let down = CGEvent::new_mouse_event(source.clone(), down_type, point, btn)
            .map_err(|()| "Failed to create mouse event".to_string())?;
        down.post(core_graphics::event::CGEventTapLocation::HID);

        let up = CGEvent::new_mouse_event(source.clone(), up_type, point, btn)
            .map_err(|()| "Failed to create mouse event".to_string())?;
        up.post(core_graphics::event::CGEventTapLocation::HID);
        Ok::<(), String>(())
    };

    match click_type {
        "right" => post_click(
            CGEventType::RightMouseDown,
            CGEventType::RightMouseUp,
            CGMouseButton::Right,
        )?,
        "double" => {
            post_click(
                CGEventType::LeftMouseDown,
                CGEventType::LeftMouseUp,
                CGMouseButton::Left,
            )?;
            std::thread::sleep(std::time::Duration::from_millis(80));
            post_click(
                CGEventType::LeftMouseDown,
                CGEventType::LeftMouseUp,
                CGMouseButton::Left,
            )?;
        }
        _ => post_click(
            CGEventType::LeftMouseDown,
            CGEventType::LeftMouseUp,
            CGMouseButton::Left,
        )?,
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ------------------------------------------------------------------
    // parse_app_identifier
    // ------------------------------------------------------------------

    #[test]
    fn parse_app_identifier_digits_returns_pid() {
        // GIVEN: pure digit string
        // WHEN: parsed
        let (name, bundle, pid) = parse_app_identifier("12345");
        // THEN: pid branch
        assert_eq!(pid, Some(12345));
        assert!(name.is_none());
        assert!(bundle.is_none());
    }

    #[test]
    fn parse_app_identifier_bundle_id_detected() {
        // GIVEN: bundle ID with two dots
        let (name, bundle, pid) = parse_app_identifier("com.apple.Safari");
        // THEN: bundle branch
        assert_eq!(bundle.as_deref(), Some("com.apple.Safari"));
        assert!(name.is_none());
        assert!(pid.is_none());
    }

    #[test]
    fn parse_app_identifier_name_fallback() {
        // GIVEN: plain name
        let (name, bundle, pid) = parse_app_identifier("Safari");
        // THEN: name branch
        assert_eq!(name.as_deref(), Some("Safari"));
        assert!(bundle.is_none());
        assert!(pid.is_none());
    }

    // ------------------------------------------------------------------
    // parse_action_mode
    // ------------------------------------------------------------------

    #[test]
    fn parse_action_mode_background_is_default() {
        assert_eq!(
            parse_action_mode("background"),
            crate::ActionMode::Background
        );
        assert_eq!(parse_action_mode("unknown"), crate::ActionMode::Background);
    }

    #[test]
    fn parse_action_mode_focus_recognised() {
        assert_eq!(parse_action_mode("focus"), crate::ActionMode::Focus);
    }

    // ------------------------------------------------------------------
    // Handler error-path smoke tests (exact user-visible error strings)
    // ------------------------------------------------------------------

    #[test]
    fn handle_connect_missing_app_returns_exact_error() {
        let registry = Arc::new(AppRegistry::default());
        let result = handle_connect(&json!({}), &registry);
        assert!(result.is_error);
        assert_eq!(result.content[0].text, "Missing required field: app");
    }

    #[test]
    fn handle_type_missing_text_returns_exact_error() {
        let registry = Arc::new(AppRegistry::default());
        let result = handle_type(&json!({"app": "Safari", "query": "Search"}), &registry);
        assert!(result.is_error);
        assert_eq!(result.content[0].text, "Missing required field: text");
    }

    #[test]
    fn handle_set_value_missing_value_returns_exact_error() {
        let registry = Arc::new(AppRegistry::default());
        let result = handle_set_value(&json!({"app": "Safari", "query": "Search"}), &registry);
        assert!(result.is_error);
        assert_eq!(result.content[0].text, "Missing required field: value");
    }

    #[test]
    fn handle_list_windows_missing_app_returns_exact_error() {
        let registry = Arc::new(AppRegistry::default());
        let result = handle_list_windows(&json!({}), &registry);
        assert!(result.is_error);
        assert_eq!(result.content[0].text, "Missing required field: app");
    }

    #[test]
    fn handle_screenshot_missing_app_returns_exact_error() {
        let registry = Arc::new(AppRegistry::default());
        let result = handle_screenshot(&json!({}), &registry);
        assert!(result.is_error);
        assert_eq!(result.content[0].text, "Missing required field: app");
    }

    #[test]
    fn handle_click_at_returns_error_for_missing_x() {
        let result = handle_click_at(&json!({"y": 100}));
        assert!(result.is_error);
        let msg = &result.content[0].text;
        assert!(msg.contains("Missing required field: x"), "got: {msg}");
    }

    #[test]
    fn handle_click_at_returns_error_for_missing_y() {
        let result = handle_click_at(&json!({"x": 50}));
        assert!(result.is_error);
        let msg = &result.content[0].text;
        assert!(msg.contains("Missing required field: y"), "got: {msg}");
    }
}
