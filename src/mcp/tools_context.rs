//! Context MCP tools: system state, clipboard, and geolocation.
//!
//! | Tool | Purpose | Permission |
//! |------|---------|------------|
//! | `ax_system_context` | Full environmental snapshot | None |
//! | `ax_clipboard`      | Read/write clipboard       | None |
//! | `ax_location`       | GPS coordinates            | Location Services (feature `context`) |

use serde_json::{json, Value};

use crate::mcp::annotations;
use crate::mcp::protocol::{Tool, ToolCallResult};

// ---------------------------------------------------------------------------
// Tool declarations
// ---------------------------------------------------------------------------

/// Context tools. `ax_system_context` is always available; `ax_location`
/// requires the `context` feature flag and Location Services permission.
///
/// Note: `ax_clipboard` is already provided by the innovation tools module.
pub(crate) fn context_tools() -> Vec<Tool> {
    #[allow(unused_mut)]
    let mut tools = vec![tool_ax_system_context()];
    #[cfg(feature = "context")]
    tools.push(tool_ax_location());
    tools
}

fn tool_ax_system_context() -> Tool {
    Tool {
        name: "ax_system_context",
        title: "Get full system context snapshot",
        description: "Returns a comprehensive environmental snapshot for AI agent situational \
            awareness. Includes: battery level & power source, dark mode, screen dimensions, \
            system volume, locale/language/timezone, macOS version, hostname, WiFi status/SSID, \
            network interfaces, keyboard layout, frontmost app, memory, uptime.\n\
            \n\
            No permissions required. All queries are local and instant.\n\
            \n\
            Example: `{}`",
        input_schema: json!({ "type": "object", "additionalProperties": false }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "battery_level":       { "type": ["number", "null"] },
                "battery_charging":    { "type": ["boolean", "null"] },
                "power_source":        { "type": ["string", "null"] },
                "dark_mode":           { "type": "boolean" },
                "screen_width":        { "type": "number" },
                "screen_height":       { "type": "number" },
                "screen_scale":        { "type": "number" },
                "system_volume":       { "type": ["number", "null"] },
                "output_muted":        { "type": ["boolean", "null"] },
                "locale":              { "type": "string" },
                "language":            { "type": "string" },
                "timezone":            { "type": "string" },
                "timezone_offset_secs":{ "type": "integer" },
                "macos_version":       { "type": "string" },
                "hostname":            { "type": "string" },
                "username":            { "type": "string" },
                "uptime_secs":         { "type": "number" },
                "physical_memory_gb":  { "type": "number" },
                "wifi_enabled":        { "type": ["boolean", "null"] },
                "wifi_ssid":           { "type": ["string", "null"] },
                "active_interfaces":   { "type": "array" },
                "keyboard_layout":     { "type": ["string", "null"] },
                "frontmost_app":       { "type": ["string", "null"] }
            }
        }),
        annotations: annotations::READ_ONLY,
    }
}

#[allow(dead_code)]
fn tool_ax_clipboard() -> Tool {
    Tool {
        name: "ax_clipboard",
        title: "Read or write the system clipboard",
        description: "Read or write the macOS system clipboard (NSPasteboard).\n\
            \n\
            **Read mode** (default): Returns clipboard text, available types, item count, \
            and change count.\n\
            \n\
            **Write mode**: Set `text` to write to the clipboard, replacing existing contents.\n\
            \n\
            No special permissions required.\n\
            \n\
            Example read: `{}`\n\
            Example write: `{\"text\": \"Hello from AXTerminator\"}`",
        input_schema: json!({
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "When provided, writes this text to the clipboard. \
                        When omitted, reads the current clipboard contents."
                }
            },
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "text":         { "type": ["string", "null"] },
                "types":        { "type": "array", "items": { "type": "string" } },
                "item_count":   { "type": "integer" },
                "change_count": { "type": "integer" },
                "written":      { "type": "boolean" }
            }
        }),
        annotations: annotations::READ_ONLY, // Read is read-only; write is action.
    }
}

#[cfg(feature = "context")]
fn tool_ax_location() -> Tool {
    Tool {
        name: "ax_location",
        title: "Get current GPS location",
        description: "Request the device's current geographic location via CoreLocation.\n\
            \n\
            Returns latitude, longitude, accuracy, altitude, and timestamp.\n\
            Requires Location Services permission (prompted on first use).\n\
            \n\
            Timeout defaults to 10 seconds. On Macs without GPS (most desktops), \
            location is estimated via WiFi positioning (~100m accuracy).\n\
            \n\
            Example: `{}`\n\
            Example with timeout: `{\"timeout\": 5}`",
        input_schema: json!({
            "type": "object",
            "properties": {
                "timeout": {
                    "type": "number",
                    "description": "Maximum seconds to wait for a location fix (default 10, max 30)",
                    "default": 10,
                    "minimum": 1,
                    "maximum": 30
                }
            },
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "latitude":    { "type": "number" },
                "longitude":   { "type": "number" },
                "accuracy_m":  { "type": "number" },
                "altitude":    { "type": ["number", "null"] },
                "timestamp":   { "type": "string" }
            },
            "required": ["latitude", "longitude", "accuracy_m", "timestamp"]
        }),
        annotations: annotations::READ_ONLY,
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// Handle `ax_system_context` — return full system snapshot.
pub(crate) fn handle_ax_system_context() -> ToolCallResult {
    let ctx = crate::context::system::collect_system_context();
    match serde_json::to_value(&ctx) {
        Ok(v) => ToolCallResult::ok(v.to_string()),
        Err(e) => ToolCallResult::error(format!("Serialization failed: {e}")),
    }
}

/// Handle `ax_clipboard` — read or write clipboard.
///
/// Note: this handler is available for direct use but not registered in
/// `context_tools()` because `ax_clipboard` is already in innovation tools.
#[allow(dead_code)]
pub(crate) fn handle_ax_clipboard(args: &Value) -> ToolCallResult {
    if let Some(text) = args["text"].as_str() {
        // Write mode.
        match crate::context::clipboard::write_clipboard(text) {
            Ok(count) => {
                ToolCallResult::ok(json!({ "written": true, "change_count": count }).to_string())
            }
            Err(e) => ToolCallResult::error(json!({ "error": e, "written": false }).to_string()),
        }
    } else {
        // Read mode.
        let content = crate::context::clipboard::read_clipboard();
        match serde_json::to_value(&content) {
            Ok(v) => ToolCallResult::ok(v.to_string()),
            Err(e) => ToolCallResult::error(format!("Serialization failed: {e}")),
        }
    }
}

/// Handle `ax_location` — request GPS location.
#[cfg(feature = "context")]
pub(crate) fn handle_ax_location(args: &Value) -> ToolCallResult {
    let timeout_secs = args["timeout"].as_f64().unwrap_or(10.0).clamp(1.0, 30.0);
    let timeout = std::time::Duration::from_secs_f64(timeout_secs);

    match crate::context::location::request_location(timeout) {
        Ok(loc) => match serde_json::to_value(&loc) {
            Ok(v) => ToolCallResult::ok(v.to_string()),
            Err(e) => ToolCallResult::error(format!("Serialization failed: {e}")),
        },
        Err(e) => ToolCallResult::error(
            json!({ "error": e.to_string(), "error_code": e.code() }).to_string(),
        ),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_tools_returns_expected_count() {
        let tools = context_tools();
        let base = 1; // system_context (clipboard is in innovation tools)
        let extra_location = if cfg!(feature = "context") { 1 } else { 0 };
        assert_eq!(tools.len(), base + extra_location);
    }

    #[test]
    fn all_context_tool_names_start_with_ax() {
        for tool in context_tools() {
            assert!(
                tool.name.starts_with("ax_"),
                "tool {} should start with ax_",
                tool.name
            );
        }
    }

    #[test]
    #[ignore = "touches live macOS system APIs and is flaky under parallel cargo test"]
    fn handle_system_context_returns_valid_json() {
        let result = handle_ax_system_context();
        assert!(
            !result.is_error,
            "unexpected error: {}",
            result.content[0].text
        );
        let v: Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert!(v["macos_version"].is_string());
        assert!(v["dark_mode"].is_boolean());
    }

    #[test]
    #[ignore = "touches the macOS pasteboard and is flaky under parallel cargo test"]
    fn handle_clipboard_read_returns_valid_json() {
        let result = handle_ax_clipboard(&json!({}));
        assert!(
            !result.is_error,
            "unexpected error: {}",
            result.content[0].text
        );
        let v: Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert!(v["change_count"].is_number());
    }

    #[test]
    #[ignore = "writes the macOS pasteboard and is flaky under parallel cargo test"]
    fn handle_clipboard_write_then_read() {
        let test_text = "ax_context_test_67890";
        let write_result = handle_ax_clipboard(&json!({ "text": test_text }));
        assert!(!write_result.is_error);
        let wv: Value = serde_json::from_str(&write_result.content[0].text).unwrap();
        assert_eq!(wv["written"], true);

        let read_result = handle_ax_clipboard(&json!({}));
        let rv: Value = serde_json::from_str(&read_result.content[0].text).unwrap();
        assert_eq!(rv["text"].as_str(), Some(test_text));
    }
}
