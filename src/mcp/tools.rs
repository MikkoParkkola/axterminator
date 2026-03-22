//! MCP tool registration and dispatch.
//!
//! Every tool:
//!   - Is declared as a `Tool` constant (name, description, schemas, annotations).
//!   - Has a matching dispatch arm in `call_tool`.
//!   - Returns `ToolCallResult` — never panics.
//!
//! The session state (`AppRegistry`) is passed by reference so tools remain pure
//! functions of their inputs + session state.
//!
//! Handler implementations live in [`crate::mcp::tools_handlers`].

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use serde_json::{json, Value};

use crate::app::AXApp;
use crate::mcp::annotations;
use crate::mcp::protocol::{Tool, ToolCallResult};
use crate::mcp::tools_handlers::{
    handle_click, handle_click_at, handle_connect, handle_find, handle_find_visual,
    handle_get_value, handle_is_accessible, handle_list_windows, handle_screenshot,
    handle_set_value, handle_type, handle_wait_idle,
};

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

/// All tools (Phase 1 + Phase 3) in registration order.
///
/// Phase 1 tools are listed first so existing clients that index by position
/// are unaffected. Phase 3 tools are appended via [`crate::mcp::tools_extended::extended_tools`].
///
/// # Examples
///
/// ```
/// // Phase 1 (12) + Phase 3 GUI (7) + innovation (7) = 26 total base
/// let tools = axterminator::mcp::tools::all_tools();
/// assert!(tools.len() >= 26);
/// ```
#[must_use]
pub fn all_tools() -> Vec<Tool> {
    let mut tools = vec![
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
    ];
    tools.extend(crate::mcp::tools_extended::extended_tools());
    tools
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
        description:
            "Find a UI element in a connected app using text, role, or attribute queries.\n\
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
            Use mode=focus only when the element requires keyboard focus (e.g. text input).\n\
            \n\
            SAFETY: When the target element contains a destructive keyword (delete, remove,\n\
            erase, reset, clear, wipe, destroy, terminate, uninstall, revoke, format, quit,\n\
            close), the tool returns an error instead of clicking. Re-call with confirm=true\n\
            to proceed after verifying the action is intentional.",
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
                },
                "confirm": {
                    "type": "boolean",
                    "description": "Set to true to confirm a destructive action and bypass \
                        the safety gate. Only use after explicitly verifying the action is \
                        intentional.",
                    "default": false
                }
            },
            "required": ["app", "query"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "clicked":     { "type": "boolean" },
                "query":       { "type": "string" },
                "destructive": {
                    "type": "boolean",
                    "description": "Present and true when the clicked element was identified \
                        as potentially destructive."
                }
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
        description:
            "Capture a screenshot of an app or a specific element without stealing focus. \
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
        annotations: annotations::OPEN_WORLD,
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
/// Phase 1 tools are matched directly. Phase 3 tools are dispatched via
/// [`crate::mcp::tools_extended::call_tool_extended`], which returns `None`
/// when the name is unrecognised so this function can fall through to the
/// unknown-tool error.
///
/// Progress notifications from Phase 3 tools are written to `stdout` (the
/// MCP transport channel).  The server holds `stdout_lock` for the lifetime
/// of each request, so passing a mutable reference here is safe.
///
/// The registry is `Arc`-wrapped so the server can share it across async tasks.
/// Every branch must return `ToolCallResult` — never panic.
pub fn call_tool<W: std::io::Write>(
    name: &str,
    args: &Value,
    registry: &Arc<AppRegistry>,
    out: &mut W,
) -> ToolCallResult {
    match name {
        "ax_is_accessible" => handle_is_accessible(),
        "ax_connect" => handle_connect(args, registry),
        "ax_find" => {
            // Emit a start notification: the semantic fallback scans the full
            // AX scene graph which can be slow for complex UIs.  The complete
            // notification fires unconditionally so clients always see a pair.
            let token = crate::mcp::progress::next_progress_token();
            let _ = crate::mcp::progress::emit_progress(out, &token, 0, 1, "Searching…");
            let result = handle_find(args, registry);
            let _ = crate::mcp::progress::emit_progress(out, &token, 1, 1, "");
            result
        }
        "ax_click" => handle_click(args, registry),
        "ax_type" => handle_type(args, registry),
        "ax_set_value" => handle_set_value(args, registry),
        "ax_get_value" => handle_get_value(args, registry),
        "ax_list_windows" => handle_list_windows(args, registry),
        "ax_screenshot" => handle_screenshot(args, registry),
        "ax_click_at" => handle_click_at(args),
        "ax_find_visual" => handle_find_visual(args, registry),
        "ax_wait_idle" => handle_wait_idle(args, registry),
        other => {
            if let Some(result) =
                crate::mcp::tools_extended::call_tool_extended(other, args, registry, out)
            {
                return result;
            }
            ToolCallResult::error(format!("Unknown tool: {other}"))
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn all_tools_count_matches_feature_set() {
        // GIVEN: Phase 1 (12) + Phase 3 GUI (7) + innovation (7) = 26 base
        //        +3 camera = 29; +5 spaces = 31/34; +3 audio = 29/32/34/37
        //        +3 watch (watch implies audio+camera, so net +3 over camera+audio)
        // WHEN: requesting all tools
        let tools = all_tools();
        // THEN: count is a deterministic function of active features
        let base = 28usize; // Phase 1 (12) + Phase 3 GUI (7) + innovation (8, incl. ax_record) + ax_analyze (1)
        let extra_spaces: usize = if cfg!(feature = "spaces") { 5 } else { 0 };
        // `watch` implies `audio` and `camera`, so these are additive
        let extra_audio: usize = if cfg!(feature = "audio") { 3 } else { 0 };
        let extra_camera: usize = if cfg!(feature = "camera") { 3 } else { 0 };
        let extra_watch: usize = if cfg!(feature = "watch") { 3 } else { 0 };
        let extra_docker: usize = if cfg!(feature = "docker") { 2 } else { 0 };
        assert_eq!(
            tools.len(),
            base + extra_spaces + extra_audio + extra_camera + extra_watch + extra_docker
        );
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
            assert!(
                !tool.description.is_empty(),
                "empty description on {}",
                tool.name
            );
        }
    }

    #[test]
    fn call_tool_unknown_name_returns_error() {
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        let result = call_tool("ax_nonexistent", &json!({}), &registry, &mut out);
        assert!(result.is_error);
        assert!(result.content[0].text.contains("Unknown tool"));
    }

    #[test]
    fn call_tool_is_accessible_returns_result() {
        // ax_is_accessible never requires a connected app
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        let result = call_tool("ax_is_accessible", &json!({}), &registry, &mut out);
        // Result content is valid JSON
        let parsed: Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert!(parsed.get("enabled").is_some());
    }

    #[test]
    fn call_tool_connect_missing_app_field_is_error() {
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        let result = call_tool("ax_connect", &json!({}), &registry, &mut out);
        assert!(result.is_error);
        assert!(result.content[0].text.contains("Missing"));
    }

    #[test]
    fn call_tool_find_requires_app_not_connected() {
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        let result = call_tool(
            "ax_find",
            &json!({"app": "NotConnected", "query": "Save"}),
            &registry,
            &mut out,
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
