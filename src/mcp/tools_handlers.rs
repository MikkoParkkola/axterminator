//! Handler functions for Phase 1 MCP tools.
//!
//! Each `handle_*` function corresponds to exactly one Phase 1 tool declared in
//! [`crate::mcp::tools`].  They are pure functions of their arguments and the
//! shared [`crate::mcp::tools::AppRegistry`] — no global mutable state.
//!
//! Internal helpers (`parse_app_identifier`, `extract_app_query`,
//! `parse_action_mode`, `click_at_coordinates`) are also defined here because
//! they exist solely to support these handlers.

use std::sync::Arc;

use serde_json::{Value, json};

use crate::app::AXApp;
use crate::mcp::elicitation::is_destructive_element;
use crate::mcp::protocol::ToolCallResult;
use crate::mcp::tools::AppRegistry;

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
    ToolCallResult {
        content: vec![crate::mcp::protocol::ContentItem::text(result.to_string())],
        is_error: !enabled,
    }
}

pub(crate) fn handle_connect(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    let Some(app_id) = args["app"].as_str() else {
        return ToolCallResult::error("Missing required field: app");
    };
    let alias = args["alias"].as_str().unwrap_or(app_id);

    let (name, bundle_id, pid) = parse_app_identifier(app_id);
    match AXApp::connect_native(name.as_deref(), bundle_id.as_deref(), pid) {
        Ok(app) => {
            #[allow(clippy::cast_sign_loss)]
            let connected_pid = app.pid as u32;
            let bundle = app.bundle_id.clone();
            let app_type = crate::router::detect_app_type(bundle.as_deref().unwrap_or(""), app.pid)
                .name()
                .to_string();
            registry.insert(alias.to_string(), app);
            ToolCallResult::ok(
                json!({
                    "connected": true,
                    "alias": alias,
                    "pid": connected_pid,
                    "bundle_id": bundle,
                    "app_type": app_type
                })
                .to_string(),
            )
        }
        Err(e) => ToolCallResult::error(format!("Failed to connect to '{app_id}': {e}")),
    }
}

pub(crate) fn handle_find(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    let (app_name, query) = match extract_app_query(args) {
        Ok(v) => v,
        Err(e) => return ToolCallResult::error(e),
    };
    let timeout_ms = args["timeout_ms"].as_u64().unwrap_or(5000);

    registry
        .with_app(&app_name, |app| {
            match app.find_native(&query, Some(timeout_ms)) {
                Ok(el) => {
                    let bounds_tuple = el.bounds();
                    let bounds_arr = bounds_tuple.map(|(x, y, w, h)| {
                        serde_json::Value::Array(vec![
                            serde_json::json!(x),
                            serde_json::json!(y),
                            serde_json::json!(w),
                            serde_json::json!(h),
                        ])
                    });
                    let locator = build_locator(el.role(), el.title(), bounds_tuple);
                    ToolCallResult::ok(
                        json!({
                            "found": true,
                            "role": el.role(),
                            "title": el.title(),
                            "value": el.value(),
                            "enabled": el.enabled(),
                            "bounds": bounds_arr,
                            "locator": locator
                        })
                        .to_string(),
                    )
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
            ));
        }
    };

    let finder = SemanticFinder;
    let fq = FindQuery::new(query);
    let result = finder.find(&scene, &fq);

    if let Some(top) = result.matches.first()
        && top.score >= 0.3
    {
        return ToolCallResult::ok(
            json!({
                "found": true,
                "semantic_match": true,
                "confidence": top.score,
                "role": top.role,
                "label": top.label,
                "bounds": top.bounds.map(|(x, y, w, h)| [x, y, w, h]),
                "reasoning": top.reasoning
            })
            .to_string(),
        );
    }

    ToolCallResult::error(format!("Element not found: '{query}'"))
}

pub(crate) fn handle_click(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    let (app_name, query) = match extract_app_query(args) {
        Ok(v) => v,
        Err(e) => return ToolCallResult::error(e),
    };
    let mode_str = args["mode"].as_str().unwrap_or("background");
    let click_type = args["click_type"].as_str().unwrap_or("single");
    let confirmed = args["confirm"].as_bool().unwrap_or(false);
    let mode = parse_action_mode(mode_str);

    registry
        .with_app(&app_name, |app| match app.find_native(&query, Some(100)) {
            Ok(el) => {
                let destructive = is_element_destructive(&el);
                if destructive && !confirmed {
                    return destructive_gate_error(&query);
                }
                perform_click(&el, click_type, mode, &query, destructive)
            }
            Err(_) => ToolCallResult::error(format!("Element not found: '{query}'")),
        })
        .unwrap_or_else(ToolCallResult::error)
}

/// Return `true` when the element's title or description contains a destructive keyword.
fn is_element_destructive(el: &crate::AXElement) -> bool {
    let title_hit = el
        .title()
        .as_deref()
        .map(is_destructive_element)
        .unwrap_or(false);
    let desc_hit = el
        .description()
        .as_deref()
        .map(is_destructive_element)
        .unwrap_or(false);
    title_hit || desc_hit
}

/// Build the error returned when a destructive click is blocked.
fn destructive_gate_error(query: &str) -> ToolCallResult {
    ToolCallResult::error(format!(
        "Destructive action detected: clicking '{query}'. \
         Re-call ax_click with confirm=true to proceed."
    ))
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
                "bounds": bounds.map(|(x, y, w, h)| [x, y, w, h])
            });
            if destructive {
                resp["destructive"] = json!(true);
            }
            ToolCallResult::ok(resp.to_string())
        }
        Err(e) => ToolCallResult::error(format!("Click failed: {e}")),
    }
}

pub(crate) fn handle_type(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    let (app_name, query) = match extract_app_query(args) {
        Ok(v) => v,
        Err(e) => return ToolCallResult::error(e),
    };
    let text = match args["text"].as_str() {
        Some(t) => t.to_string(),
        None => return ToolCallResult::error("Missing required field: text"),
    };
    let mode_str = args["mode"].as_str().unwrap_or("background");
    let mode = parse_action_mode(mode_str);

    let char_count = text.chars().count();
    registry
        .with_app(&app_name, |app| match app.find_native(&query, Some(100)) {
            Ok(el) => match el.type_text_native(&text, mode) {
                Ok(()) => {
                    ToolCallResult::ok(json!({"typed": true, "char_count": char_count}).to_string())
                }
                Err(e) => ToolCallResult::error(format!("Type failed: {e}")),
            },
            Err(_) => ToolCallResult::error(format!("Element not found: '{query}'")),
        })
        .unwrap_or_else(ToolCallResult::error)
}

pub(crate) fn handle_set_value(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    let (app_name, query) = match extract_app_query(args) {
        Ok(v) => v,
        Err(e) => return ToolCallResult::error(e),
    };
    let value = match args["value"].as_str() {
        Some(v) => v.to_string(),
        None => return ToolCallResult::error("Missing required field: value"),
    };

    registry
        .with_app(&app_name, |app| match app.find_native(&query, Some(100)) {
            Ok(el) => match el.set_value_native(&value) {
                Ok(()) => ToolCallResult::ok(json!({"set": true, "value": value}).to_string()),
                Err(e) => ToolCallResult::error(format!("set_value failed: {e}")),
            },
            Err(_) => ToolCallResult::error(format!("Element not found: '{query}'")),
        })
        .unwrap_or_else(ToolCallResult::error)
}

pub(crate) fn handle_get_value(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    let (app_name, query) = match extract_app_query(args) {
        Ok(v) => v,
        Err(e) => return ToolCallResult::error(e),
    };

    registry
        .with_app(&app_name, |app| match app.find_native(&query, Some(100)) {
            Ok(el) => ToolCallResult::ok(json!({"found": true, "value": el.value()}).to_string()),
            Err(_) => ToolCallResult::error(format!("Element not found: '{query}'")),
        })
        .unwrap_or_else(ToolCallResult::error)
}

pub(crate) fn handle_list_windows(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    let app_name = match args["app"].as_str() {
        Some(s) => s.to_string(),
        None => return ToolCallResult::error("Missing required field: app"),
    };

    registry
        .with_app(&app_name, |app| match app.windows_native() {
            Ok(windows) => {
                let items: Vec<Value> = windows
                    .iter()
                    .map(|w: &crate::AXElement| {
                        let bounds_val = w.bounds().map(|(x, y, bw, bh)| {
                            serde_json::Value::Array(vec![
                                serde_json::json!(x),
                                serde_json::json!(y),
                                serde_json::json!(bw),
                                serde_json::json!(bh),
                            ])
                        });
                        json!({ "title": w.title(), "bounds": bounds_val })
                    })
                    .collect();
                ToolCallResult::ok(json!({"windows": items}).to_string())
            }
            Err(e) => ToolCallResult::error(format!("Failed to list windows: {e}")),
        })
        .unwrap_or_else(ToolCallResult::error)
}

pub(crate) fn handle_screenshot(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    let app_name = match args["app"].as_str() {
        Some(s) => s.to_string(),
        None => return ToolCallResult::error("Missing required field: app"),
    };
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
                    ToolCallResult::ok(
                        json!({
                            "captured": true,
                            "base64_png": b64,
                            "size_bytes": size
                        })
                        .to_string(),
                    )
                }
                Err(e) => ToolCallResult::error(format!("Screenshot failed: {e}")),
            }
        })
        .unwrap_or_else(ToolCallResult::error)
}

pub(crate) fn handle_click_at(args: &Value) -> ToolCallResult {
    let Some(x_raw) = args["x"].as_i64() else {
        return ToolCallResult::error("Missing required field: x");
    };
    let Some(y_raw) = args["y"].as_i64() else {
        return ToolCallResult::error("Missing required field: y");
    };
    #[allow(clippy::cast_possible_truncation)]
    let x = x_raw as i32;
    #[allow(clippy::cast_possible_truncation)]
    let y = y_raw as i32;
    let click_type = args["click_type"].as_str().unwrap_or("single");

    match click_at_coordinates(x, y, click_type) {
        Ok(()) => ToolCallResult::ok(json!({"clicked": true, "x": x, "y": y}).to_string()),
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
    let Some(app_name) = args["app"].as_str().map(str::to_string) else {
        return ToolCallResult::error("Missing required field: app");
    };
    let Some(description) = args["description"].as_str().map(str::to_string) else {
        return ToolCallResult::error("Missing required field: description");
    };

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

            ToolCallResult::ok(
                serde_json::json!({
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
                })
                .to_string(),
            )
        }
        Err(e) => ToolCallResult::error(format!(
            "ax_find_visual: screenshot failed for visual sampling: {e}. \
             Use ax_screenshot separately and call an external VLM."
        )),
    }
}

pub(crate) fn handle_wait_idle(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    let Some(app_name) = args["app"].as_str().map(str::to_string) else {
        return ToolCallResult::error("Missing required field: app");
    };
    let timeout_ms = args["timeout_ms"].as_u64().unwrap_or(5000);

    let start = std::time::Instant::now();
    registry
        .with_app(&app_name, |app| {
            let idle = app.wait_idle_native(timeout_ms);
            #[allow(clippy::cast_possible_truncation)]
            let elapsed = start.elapsed().as_millis() as u64;
            ToolCallResult::ok(json!({"idle": idle, "elapsed_ms": elapsed}).to_string())
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
        "bounds": bounds.map(|(x, y, w, h)| [x, y, w, h])
    })
}

/// Extract the mandatory `app` and `query` string fields from an argument object.
pub(crate) fn extract_app_query(args: &Value) -> Result<(String, String), String> {
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
        assert!(extract_app_query(&args).is_err());
    }

    #[test]
    fn extract_app_query_fails_without_query() {
        let args = json!({"app": "Safari"});
        assert!(extract_app_query(&args).is_err());
    }

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
    // Destructive gate helpers
    // ------------------------------------------------------------------

    #[test]
    fn destructive_gate_error_contains_query_and_confirm_hint() {
        // GIVEN: a destructive element query
        // WHEN: gate error is built
        let result = destructive_gate_error("Delete All Files");
        // THEN: error text names the query and the escape hatch
        assert!(result.is_error);
        let msg = &result.content[0].text;
        assert!(
            msg.contains("Delete All Files"),
            "query missing from: {msg}"
        );
        assert!(
            msg.contains("confirm=true"),
            "confirm hint missing from: {msg}"
        );
    }

    #[test]
    fn destructive_gate_error_is_error_result() {
        // GIVEN/WHEN: any query
        let result = destructive_gate_error("Reset");
        // THEN: is_error flag is set
        assert!(result.is_error);
    }

    #[test]
    fn confirm_arg_false_is_treated_as_unconfirmed() {
        // GIVEN: args with explicit confirm=false (same as absent)
        let args = json!({"app": "x", "query": "q", "confirm": false});
        // WHEN: confirm is extracted
        let confirmed = args["confirm"].as_bool().unwrap_or(false);
        // THEN: treated as not confirmed
        assert!(!confirmed);
    }

    #[test]
    fn confirm_arg_true_is_treated_as_confirmed() {
        // GIVEN: args with explicit confirm=true
        let args = json!({"app": "x", "query": "q", "confirm": true});
        // WHEN: confirm is extracted
        let confirmed = args["confirm"].as_bool().unwrap_or(false);
        // THEN: treated as confirmed
        assert!(confirmed);
    }

    #[test]
    fn confirm_arg_absent_defaults_to_false() {
        // GIVEN: args without a confirm field
        let args = json!({"app": "x", "query": "q"});
        // WHEN: confirm is extracted with default
        let confirmed = args["confirm"].as_bool().unwrap_or(false);
        // THEN: defaults to false (unconfirmed)
        assert!(!confirmed);
    }
}
