//! Context MCP tools: system state and geolocation.
//!
//! | Tool | Purpose | Permission |
//! |------|---------|------------|
//! | `ax_system_context` | Full environmental snapshot | None |
//! | `ax_location`       | GPS coordinates            | Location Services (feature `context`) |

use serde_json::{json, Value};

use crate::mcp::annotations;
use crate::mcp::args::{extract_or_return, reject_unknown_fields};
use crate::mcp::protocol::{Tool, ToolCallResult};

// ---------------------------------------------------------------------------
// Tool names
// ---------------------------------------------------------------------------

pub(crate) const TOOL_AX_SYSTEM_CONTEXT: &str = "ax_system_context";
#[cfg(feature = "context")]
pub(crate) const TOOL_AX_LOCATION: &str = "ax_location";

// ---------------------------------------------------------------------------
// Tool declarations
// ---------------------------------------------------------------------------

/// Context tools. `ax_system_context` is always available; `ax_location`
/// requires the `context` feature flag and Location Services permission.
pub(crate) fn context_tools() -> Vec<Tool> {
    #[allow(unused_mut)]
    let mut tools = vec![tool_ax_system_context()];
    #[cfg(feature = "context")]
    tools.push(tool_ax_location());
    tools
}

fn tool_ax_system_context() -> Tool {
    Tool {
        name: TOOL_AX_SYSTEM_CONTEXT,
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

#[cfg(feature = "context")]
fn tool_ax_location() -> Tool {
    Tool {
        name: TOOL_AX_LOCATION,
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
pub(crate) fn handle_ax_system_context(args: &Value) -> ToolCallResult {
    extract_or_return!(reject_unknown_fields(args, &[]));
    let ctx = crate::context::system::collect_system_context();
    match serde_json::to_value(&ctx) {
        Ok(v) => ToolCallResult::ok(v.to_string()),
        Err(e) => ToolCallResult::error(format!("Serialization failed: {e}")),
    }
}

/// Handle `ax_location` — request GPS location.
#[cfg(feature = "context")]
pub(crate) fn handle_ax_location(args: &Value) -> ToolCallResult {
    if let Err(err) = reject_unknown_fields(args, &["timeout"]) {
        return context_input_error("unknown_field", err);
    }

    let timeout_secs = match parse_location_timeout(args) {
        Ok(timeout_secs) => timeout_secs,
        Err(err) => return context_input_error("invalid_timeout", err),
    };
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

#[cfg(feature = "context")]
fn context_input_error(code: &str, message: impl Into<String>) -> ToolCallResult {
    ToolCallResult::error(json!({ "error": message.into(), "error_code": code }).to_string())
}

#[cfg(feature = "context")]
fn parse_location_timeout(args: &Value) -> Result<f64, String> {
    let timeout_secs = match args.get("timeout") {
        None => return Ok(10.0),
        Some(value) => value
            .as_f64()
            .ok_or_else(|| "Field 'timeout' must be a number".to_owned())?,
    };

    if !(1.0..=30.0).contains(&timeout_secs) {
        return Err("Field 'timeout' must be between 1 and 30".to_owned());
    }

    Ok(timeout_secs)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn appkit_test_guard() -> std::sync::MutexGuard<'static, ()> {
        crate::test_sync::appkit_lock()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn context_tools_return_expected_surface() {
        let actual: Vec<&str> = context_tools().iter().map(|tool| tool.name).collect();
        let expected: Vec<&str> = {
            let base = vec![TOOL_AX_SYSTEM_CONTEXT];
            #[cfg(feature = "context")]
            {
                base.into_iter().chain(["ax_location"]).collect()
            }
            #[cfg(not(feature = "context"))]
            {
                base
            }
        };
        assert_eq!(actual, expected);
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
    fn handle_system_context_returns_valid_json() {
        let _guard = appkit_test_guard();
        let result = handle_ax_system_context(&json!({}));
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
    fn handle_system_context_rejects_unknown_top_level_fields() {
        let result = handle_ax_system_context(&json!({ "extra": true }));
        assert!(result.is_error);
        assert_eq!(result.content[0].text, "unknown field: extra");
    }

    #[cfg(feature = "context")]
    #[test]
    fn handle_location_rejects_unknown_top_level_fields() {
        let result = handle_ax_location(&json!({ "extra": true }));
        assert!(result.is_error);
        let v: Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["error_code"], "unknown_field");
    }

    #[cfg(feature = "context")]
    #[test]
    fn handle_location_rejects_non_numeric_timeout() {
        let result = handle_ax_location(&json!({ "timeout": "slow" }));
        assert!(result.is_error);
        let v: Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["error_code"], "invalid_timeout");
    }

    #[cfg(feature = "context")]
    #[test]
    fn handle_location_rejects_out_of_range_timeout() {
        let result = handle_ax_location(&json!({ "timeout": 0.5 }));
        assert!(result.is_error);
        let v: Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["error_code"], "invalid_timeout");
    }
}
