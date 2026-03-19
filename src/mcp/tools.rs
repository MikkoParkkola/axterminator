//! MCP tool registration and dispatch.
//!
//! Every tool:
//!   - Is declared as a `Tool` constant (name, description, schemas, annotations).
//!   - Has a matching dispatch arm in `call_tool`.
//!   - Returns `ToolCallResult` — never panics.
//!
//! The session state (`AppRegistry`) is passed by reference so tools remain pure
//! functions of their inputs + session state.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use serde_json::{json, Value};

use crate::app::AXApp;
use crate::mcp::annotations;
use crate::mcp::protocol::{Tool, ToolCallResult};

// ---------------------------------------------------------------------------
// App registry — shared session state
// ---------------------------------------------------------------------------

/// Connected application registry, thread-safe for concurrent tool calls.
#[derive(Default)]
pub struct AppRegistry {
    apps: RwLock<HashMap<String, AXApp>>,
}

impl AppRegistry {
    /// Insert or replace a connection.
    ///
    /// # Panics
    ///
    /// Panics if the internal `RwLock` is poisoned, which can only happen if
    /// a previous writer panicked while holding the lock.
    pub fn insert(&self, key: String, app: AXApp) {
        let mut guard = self.apps.write().expect("lock poisoned");
        guard.insert(key, app);
    }

    /// Execute a closure with shared access to a named app.
    ///
    /// Returns `Err` with a human-readable message if the app is not connected.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` when no app with the given `name` has been registered
    /// via [`AppRegistry::insert`].
    ///
    /// # Panics
    ///
    /// Panics if the internal `RwLock` is poisoned.
    pub fn with_app<F, T>(&self, name: &str, f: F) -> Result<T, String>
    where
        F: FnOnce(&AXApp) -> T,
    {
        let guard = self.apps.read().expect("lock poisoned");
        guard
            .get(name)
            .map(f)
            .ok_or_else(|| format!("App '{name}' not connected — call ax_connect first"))
    }

    /// Return the names of all connected apps.
    ///
    /// # Panics
    ///
    /// Panics if the internal `RwLock` is poisoned.
    pub fn connected_names(&self) -> Vec<String> {
        self.apps
            .read()
            .expect("lock poisoned")
            .keys()
            .cloned()
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Tool declarations
// ---------------------------------------------------------------------------

/// All Phase 1 tools in registration order.
#[must_use]
pub fn all_tools() -> Vec<Tool> {
    vec![
        tool_ax_is_accessible(),
        tool_ax_connect(),
        tool_ax_find(),
        tool_ax_click(),
        tool_ax_type(),
        tool_ax_set_value(),
        tool_ax_get_value(),
        tool_ax_list_windows(),
        tool_ax_screenshot(),
        tool_ax_click_at(),
        tool_ax_find_visual(),
        tool_ax_wait_idle(),
    ]
}

fn tool_ax_is_accessible() -> Tool {
    Tool {
        name: "ax_is_accessible",
        title: "Check accessibility permissions",
        description: "Check if macOS accessibility permissions are enabled for this process. \
            Must return enabled=true before any other tool will work. \
            If false, guide the user to System Settings > Privacy & Security > Accessibility.",
        input_schema: json!({ "type": "object", "additionalProperties": false }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "enabled": { "type": "boolean" },
                "suggestion": { "type": "string" }
            },
            "required": ["enabled"]
        }),
        annotations: annotations::READ_ONLY,
    }
}

fn tool_ax_connect() -> Tool {
    Tool {
        name: "ax_connect",
        title: "Connect to a macOS application",
        description: "Connect to a running macOS application by name, bundle ID \
            (e.g. com.apple.Safari), or PID. \
            The app must be running; accessibility must be enabled.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "app": {
                    "type": "string",
                    "description": "App name, bundle ID (com.apple.Safari), or PID"
                },
                "alias": {
                    "type": "string",
                    "description": "Optional alias for referencing this app in subsequent calls"
                }
            },
            "required": ["app"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "connected": { "type": "boolean" },
                "alias": { "type": "string" },
                "pid": { "type": "integer" }
            },
            "required": ["connected", "alias"]
        }),
        annotations: annotations::CONNECT,
    }
}

fn tool_ax_find() -> Tool {
    Tool {
        name: "ax_find",
        title: "Find a UI element",
        description: "Find a UI element in a connected app using text, role, or attribute queries.\n\
            Query syntax:\n\
            - Simple text: \"Save\" (matches title/label/identifier)\n\
            - By role: \"role:AXButton\"\n\
            - Combined: \"role:AXButton title:Save\"\n\
            - XPath-like: \"//AXButton[@AXTitle='OK']\"\n\
            Uses 7-strategy self-healing locators.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "app":        { "type": "string", "description": "App alias from ax_connect" },
                "query":      { "type": "string", "description": "Element query" },
                "timeout_ms": { "type": "integer", "default": 5000 }
            },
            "required": ["app", "query"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "found":    { "type": "boolean" },
                "role":     { "type": "string" },
                "title":    { "type": "string" },
                "value":    { "type": "string" },
                "enabled":  { "type": "boolean" },
                "bounds":   {
                    "type": "array",
                    "items": { "type": "number" },
                    "description": "[x, y, width, height]"
                }
            },
            "required": ["found"]
        }),
        annotations: annotations::READ_ONLY,
    }
}

fn tool_ax_click() -> Tool {
    Tool {
        name: "ax_click",
        title: "Click a UI element",
        description: "Click a UI element in background mode (no focus stealing).\n\
            Use mode=focus only when the element requires keyboard focus (e.g. text input).",
        input_schema: json!({
            "type": "object",
            "properties": {
                "app":        { "type": "string" },
                "query":      { "type": "string" },
                "mode":       {
                    "type": "string",
                    "enum": ["background", "focus"],
                    "default": "background"
                },
                "click_type": {
                    "type": "string",
                    "enum": ["single", "double", "right"],
                    "default": "single"
                }
            },
            "required": ["app", "query"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "clicked": { "type": "boolean" },
                "query":   { "type": "string" }
            },
            "required": ["clicked"]
        }),
        annotations: annotations::ACTION,
    }
}

fn tool_ax_type() -> Tool {
    Tool {
        name: "ax_type",
        title: "Type text into an element",
        description: "Type text into a UI element. \
            Text input typically requires focus mode. \
            For setting values without simulating keystrokes, use ax_set_value instead.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "app":   { "type": "string" },
                "query": { "type": "string" },
                "text":  { "type": "string" },
                "mode":  {
                    "type": "string",
                    "enum": ["background", "focus"],
                    "default": "focus"
                }
            },
            "required": ["app", "query", "text"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "typed":      { "type": "boolean" },
                "char_count": { "type": "integer" }
            },
            "required": ["typed"]
        }),
        annotations: annotations::DESTRUCTIVE,
    }
}

fn tool_ax_set_value() -> Tool {
    Tool {
        name: "ax_set_value",
        title: "Set an element value directly",
        description: "Set the AXValue of an element directly without keystroke simulation. \
            Faster than ax_type and works in background mode. \
            Use for text fields, sliders, and other value-bearing elements.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "app":   { "type": "string" },
                "query": { "type": "string" },
                "value": { "type": "string" }
            },
            "required": ["app", "query", "value"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "set":   { "type": "boolean" },
                "value": { "type": "string" }
            },
            "required": ["set"]
        }),
        annotations: annotations::DESTRUCTIVE,
    }
}

fn tool_ax_get_value() -> Tool {
    Tool {
        name: "ax_get_value",
        title: "Get the current value of an element",
        description: "Read the AXValue attribute of an element. \
            Works for text fields, labels, checkboxes, sliders, and similar elements.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "app":   { "type": "string" },
                "query": { "type": "string" }
            },
            "required": ["app", "query"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "found": { "type": "boolean" },
                "value": { "type": "string" }
            },
            "required": ["found"]
        }),
        annotations: annotations::READ_ONLY,
    }
}

fn tool_ax_list_windows() -> Tool {
    Tool {
        name: "ax_list_windows",
        title: "List application windows",
        description: "List all windows of a connected app with titles, positions, and sizes.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "app": { "type": "string" }
            },
            "required": ["app"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "windows": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "title":  { "type": "string" },
                            "bounds": {
                                "type": "array",
                                "items": { "type": "number" }
                            }
                        }
                    }
                }
            },
            "required": ["windows"]
        }),
        annotations: annotations::READ_ONLY,
    }
}

fn tool_ax_screenshot() -> Tool {
    Tool {
        name: "ax_screenshot",
        title: "Take a screenshot",
        description: "Capture a screenshot of an app or a specific element without stealing focus. \
            Returns base64-encoded PNG data.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "app":   { "type": "string" },
                "query": {
                    "type": "string",
                    "description": "Optional element to crop to. Captures whole app if omitted."
                }
            },
            "required": ["app"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "captured":      { "type": "boolean" },
                "base64_png":    { "type": "string" },
                "size_bytes":    { "type": "integer" }
            },
            "required": ["captured"]
        }),
        annotations: annotations::READ_ONLY,
    }
}

fn tool_ax_click_at() -> Tool {
    Tool {
        name: "ax_click_at",
        title: "Click at screen coordinates",
        description: "Click at absolute screen coordinates. \
            Use when VLM visual detection found an element by position \
            but the accessibility tree could not locate it.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "x":          { "type": "integer", "description": "X coordinate (pixels from left)" },
                "y":          { "type": "integer", "description": "Y coordinate (pixels from top)" },
                "click_type": {
                    "type": "string",
                    "enum": ["single", "double", "right"],
                    "default": "single"
                }
            },
            "required": ["x", "y"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "clicked": { "type": "boolean" },
                "x":       { "type": "integer" },
                "y":       { "type": "integer" }
            },
            "required": ["clicked"]
        }),
        annotations: annotations::ACTION,
    }
}

fn tool_ax_find_visual() -> Tool {
    Tool {
        name: "ax_find_visual",
        title: "Find element via visual AI detection",
        description: "Find a UI element using VLM (vision AI) when the accessibility tree fails. \
            Takes a screenshot and uses AI to locate the element by natural-language description. \
            Requires ANTHROPIC_API_KEY or OPENAI_API_KEY environment variable.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "app":         { "type": "string" },
                "description": {
                    "type": "string",
                    "description": "Natural language description, e.g. 'Load unpacked button'"
                }
            },
            "required": ["app", "description"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "found": { "type": "boolean" },
                "x":     { "type": "integer" },
                "y":     { "type": "integer" }
            },
            "required": ["found"]
        }),
        annotations: annotations::READ_ONLY,
    }
}

fn tool_ax_wait_idle() -> Tool {
    Tool {
        name: "ax_wait_idle",
        title: "Wait for app to become idle",
        description: "Block until the app has no pending UI updates or until the timeout expires. \
            Useful before asserting state or taking screenshots.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "app":        { "type": "string" },
                "timeout_ms": { "type": "integer", "default": 5000 }
            },
            "required": ["app"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "idle":       { "type": "boolean" },
                "elapsed_ms": { "type": "integer" }
            },
            "required": ["idle"]
        }),
        annotations: annotations::READ_ONLY,
    }
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

/// Dispatch a `tools/call` invocation.
///
/// The registry is `Arc`-wrapped so the server can share it across async tasks.
/// Every branch must return `ToolCallResult` — never panic.
pub fn call_tool(name: &str, args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    match name {
        "ax_is_accessible" => handle_is_accessible(),
        "ax_connect" => handle_connect(args, registry),
        "ax_find" => handle_find(args, registry),
        "ax_click" => handle_click(args, registry),
        "ax_type" => handle_type(args, registry),
        "ax_set_value" => handle_set_value(args, registry),
        "ax_get_value" => handle_get_value(args, registry),
        "ax_list_windows" => handle_list_windows(args, registry),
        "ax_screenshot" => handle_screenshot(args, registry),
        "ax_click_at" => handle_click_at(args),
        "ax_find_visual" => handle_find_visual(args, registry),
        "ax_wait_idle" => handle_wait_idle(args, registry),
        _ => ToolCallResult::error(format!("Unknown tool: {name}")),
    }
}

// ---------------------------------------------------------------------------
// Individual handlers — each ≤30 lines
// ---------------------------------------------------------------------------

fn handle_is_accessible() -> ToolCallResult {
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

fn handle_connect(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
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
            registry.insert(alias.to_string(), app);
            let result = json!({
                "connected": true,
                "alias": alias,
                "pid": connected_pid,
                "bundle_id": bundle
            });
            ToolCallResult::ok(result.to_string())
        }
        Err(e) => ToolCallResult::error(format!("Failed to connect to '{app_id}': {e}")),
    }
}

fn handle_find(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    let (app_name, query) = match extract_app_query(args) {
        Ok(v) => v,
        Err(e) => return ToolCallResult::error(e),
    };
    let timeout_ms = args["timeout_ms"].as_u64().unwrap_or(5000);

    registry
        .with_app(&app_name, |app| {
            match app.find_native(&query, Some(timeout_ms)) {
                Ok(el) => {
                    let bounds_arr = el.bounds().map(|(x, y, w, h)| {
                        serde_json::Value::Array(vec![
                            serde_json::json!(x),
                            serde_json::json!(y),
                            serde_json::json!(w),
                            serde_json::json!(h),
                        ])
                    });
                    let result = json!({
                        "found": true,
                        "role": el.role(),
                        "title": el.title(),
                        "value": el.value(),
                        "enabled": el.enabled(),
                        "bounds": bounds_arr
                    });
                    ToolCallResult::ok(result.to_string())
                }
                Err(_) => ToolCallResult::error(format!(
                    "Element not found: '{query}' (timeout: {timeout_ms}ms)"
                )),
            }
        })
        .unwrap_or_else(ToolCallResult::error)
}

fn handle_click(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    let (app_name, query) = match extract_app_query(args) {
        Ok(v) => v,
        Err(e) => return ToolCallResult::error(e),
    };
    let mode_str = args["mode"].as_str().unwrap_or("background");
    let click_type = args["click_type"].as_str().unwrap_or("single");
    let mode = parse_action_mode(mode_str);

    registry
        .with_app(&app_name, |app| {
            match app.find_native(&query, Some(100)) {
                Ok(el) => {
                    let click_result = match click_type {
                        "double" => el.double_click_native(mode),
                        "right" => el.right_click_native(mode),
                        _ => el.click_native(mode),
                    };
                    match click_result {
                        Ok(()) => ToolCallResult::ok(
                            json!({"clicked": true, "query": query}).to_string(),
                        ),
                        Err(e) => ToolCallResult::error(format!("Click failed: {e}")),
                    }
                }
                Err(_) => ToolCallResult::error(format!("Element not found: '{query}'")),
            }
        })
        .unwrap_or_else(ToolCallResult::error)
}

fn handle_type(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    let (app_name, query) = match extract_app_query(args) {
        Ok(v) => v,
        Err(e) => return ToolCallResult::error(e),
    };
    let text = match args["text"].as_str() {
        Some(t) => t.to_string(),
        None => return ToolCallResult::error("Missing required field: text"),
    };
    let mode_str = args["mode"].as_str().unwrap_or("focus");
    let mode = parse_action_mode(mode_str);

    let char_count = text.chars().count();
    registry
        .with_app(&app_name, |app| match app.find_native(&query, Some(100)) {
            Ok(el) => match el.type_text_native(&text, mode) {
                Ok(()) => ToolCallResult::ok(
                    json!({"typed": true, "char_count": char_count}).to_string(),
                ),
                Err(e) => ToolCallResult::error(format!("Type failed: {e}")),
            },
            Err(_) => ToolCallResult::error(format!("Element not found: '{query}'")),
        })
        .unwrap_or_else(ToolCallResult::error)
}

fn handle_set_value(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
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

fn handle_get_value(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    let (app_name, query) = match extract_app_query(args) {
        Ok(v) => v,
        Err(e) => return ToolCallResult::error(e),
    };

    registry
        .with_app(&app_name, |app| match app.find_native(&query, Some(100)) {
            Ok(el) => ToolCallResult::ok(
                json!({"found": true, "value": el.value()}).to_string(),
            ),
            Err(_) => ToolCallResult::error(format!("Element not found: '{query}'")),
        })
        .unwrap_or_else(ToolCallResult::error)
}

fn handle_list_windows(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
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

fn handle_screenshot(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
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

fn handle_click_at(args: &Value) -> ToolCallResult {
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

fn handle_find_visual(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
    let Some(app_name) = args["app"].as_str().map(str::to_string) else {
        return ToolCallResult::error("Missing required field: app");
    };
    let Some(description) = args["description"].as_str().map(str::to_string) else {
        return ToolCallResult::error("Missing required field: description");
    };

    // VLM detection is a future capability. We take a screenshot and explain
    // the situation clearly so the MCP client can decide what to do next.
    registry
        .with_app(&app_name, |_app| {
            ToolCallResult::error(format!(
                "VLM visual detection for '{description}' is not yet available in the Rust server. \
                 Configure ANTHROPIC_API_KEY or OPENAI_API_KEY and use ax_screenshot to \
                 capture the app, then call your VLM to locate the element."
            ))
        })
        .unwrap_or_else(ToolCallResult::error)
}

fn handle_wait_idle(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
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

/// Extract the mandatory `app` and `query` string fields from an argument object.
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

/// Parse an app identifier string into (name, `bundle_id`, pid) for `AXApp::connect`.
///
/// Heuristics:
/// - All digits → PID
/// - Contains two or more dots → bundle ID
/// - Otherwise → display name
fn parse_app_identifier(id: &str) -> (Option<String>, Option<String>, Option<u32>) {
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
fn parse_action_mode(mode: &str) -> crate::ActionMode {
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
fn click_at_coordinates(x: i32, y: i32, click_type: &str) -> Result<(), String> {
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
    fn all_tools_returns_twelve_tools() {
        // GIVEN: Phase 1 tool set
        // WHEN: requesting all tools
        let tools = all_tools();
        // THEN: exactly 12 tools
        assert_eq!(tools.len(), 12);
    }

    #[test]
    fn all_tool_names_are_unique() {
        let tools = all_tools();
        let names: std::collections::HashSet<&str> = tools.iter().map(|t| t.name).collect();
        assert_eq!(names.len(), tools.len(), "duplicate tool names");
    }

    #[test]
    fn all_tools_have_non_empty_descriptions() {
        for tool in all_tools() {
            assert!(!tool.description.is_empty(), "empty description on {}", tool.name);
        }
    }

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
        assert_eq!(parse_action_mode("background"), crate::ActionMode::Background);
        assert_eq!(parse_action_mode("unknown"), crate::ActionMode::Background);
    }

    #[test]
    fn parse_action_mode_focus_recognised() {
        assert_eq!(parse_action_mode("focus"), crate::ActionMode::Focus);
    }

    #[test]
    fn call_tool_unknown_name_returns_error() {
        let registry = Arc::new(AppRegistry::default());
        let result = call_tool("ax_nonexistent", &json!({}), &registry);
        assert!(result.is_error);
        assert!(result.content[0].text.contains("Unknown tool"));
    }

    #[test]
    fn call_tool_is_accessible_returns_result() {
        // ax_is_accessible never requires a connected app
        let registry = Arc::new(AppRegistry::default());
        let result = call_tool("ax_is_accessible", &json!({}), &registry);
        // Result content is valid JSON
        let parsed: Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert!(parsed.get("enabled").is_some());
    }

    #[test]
    fn call_tool_connect_missing_app_field_is_error() {
        let registry = Arc::new(AppRegistry::default());
        let result = call_tool("ax_connect", &json!({}), &registry);
        assert!(result.is_error);
        assert!(result.content[0].text.contains("Missing"));
    }

    #[test]
    fn call_tool_find_requires_app_not_connected() {
        let registry = Arc::new(AppRegistry::default());
        let result = call_tool(
            "ax_find",
            &json!({"app": "NotConnected", "query": "Save"}),
            &registry,
        );
        assert!(result.is_error);
        assert!(result.content[0].text.contains("not connected"));
    }

    #[test]
    fn app_registry_connected_names_empty_initially() {
        let reg = AppRegistry::default();
        assert!(reg.connected_names().is_empty());
    }

    #[test]
    fn app_registry_with_app_returns_err_for_unknown() {
        let reg = AppRegistry::default();
        let result = reg.with_app("ghost", |_| ());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("ghost"));
    }
}
