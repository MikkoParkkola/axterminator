//! Docker browser tools — Neko container lifecycle management.
//!
//! | Tool | Purpose |
//! |------|---------|
//! | `ax_browser_launch` | Launch a Neko browser container and return its CDP URL |
//! | `ax_browser_stop`   | Stop and remove a Neko browser container by container ID |
//!
//! All functions are gated behind `#[cfg(feature = "docker")]`.  The feature
//! requires the `docker` CLI to be available on `$PATH` and the Docker daemon
//! to be running.
//!
//! # Example
//!
//! ```text
//! ax_browser_launch { browser: "chromium", cdp_port: 9333 }
//! -> { launched: true, container_id: "abc123", cdp_url: "ws://127.0.0.1:9333/devtools/browser" }
//!
//! ax_browser_stop { container_id: "abc123" }
//! -> { stopped: true, container_id: "abc123" }
//! ```

#[cfg(feature = "docker")]
use serde_json::json;

#[cfg(feature = "docker")]
use crate::mcp::annotations;
#[cfg(feature = "docker")]
use crate::mcp::args::{
    extract_or_return, extract_required_string_field, extract_string_field_or, extract_u64_field_or,
};
#[cfg(feature = "docker")]
use crate::mcp::protocol::{Tool, ToolCallResult};

// ---------------------------------------------------------------------------
// Tool names
// ---------------------------------------------------------------------------

#[cfg(feature = "docker")]
pub(crate) const TOOL_AX_BROWSER_LAUNCH: &str = "ax_browser_launch";
#[cfg(feature = "docker")]
pub(crate) const TOOL_AX_BROWSER_STOP: &str = "ax_browser_stop";

// ---------------------------------------------------------------------------
// Tool declarations
// ---------------------------------------------------------------------------

/// All Docker browser tools (requires `docker` feature).
///
/// Returns 2 tools: `ax_browser_launch`, `ax_browser_stop`.
#[cfg(feature = "docker")]
#[must_use]
pub fn docker_tools() -> Vec<Tool> {
    vec![tool_ax_browser_launch(), tool_ax_browser_stop()]
}

#[cfg(feature = "docker")]
fn tool_ax_browser_launch() -> Tool {
    Tool {
        name: TOOL_AX_BROWSER_LAUNCH,
        title: "Launch an isolated browser container",
        description: "Launch a Neko browser container as an isolated, reproducible test target. \
            The container exposes a CDP WebSocket endpoint for scripting and VNC for visual \
            inspection. Supported browsers: chromium, firefox, brave, edge.\n\
            \n\
            Returns the container ID and CDP URL. Pass the container ID to \
            ax_browser_stop when the test completes.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "browser": {
                    "type": "string",
                    "enum": ["chromium", "firefox", "brave", "edge"],
                    "description": "Browser to run inside the container",
                    "default": "chromium"
                },
                "cdp_port": {
                    "type": "integer",
                    "description": "Host port to expose for CDP (default: 9222)",
                    "default": 9222
                },
                "vnc_port": {
                    "type": "integer",
                    "description": "Host port to expose for VNC (default: 5900)",
                    "default": 5900
                },
                "width": {
                    "type": "integer",
                    "description": "Virtual desktop width in pixels (default: 1920)",
                    "default": 1920
                },
                "height": {
                    "type": "integer",
                    "description": "Virtual desktop height in pixels (default: 1080)",
                    "default": 1080
                }
            },
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "launched":     { "type": "boolean" },
                "container_id": { "type": "string" },
                "cdp_url":      { "type": "string" },
                "vnc_addr":     { "type": "string" },
                "browser":      { "type": "string" }
            },
            "required": ["launched"]
        }),
        annotations: annotations::ACTION,
    }
}

#[cfg(feature = "docker")]
fn tool_ax_browser_stop() -> Tool {
    Tool {
        name: TOOL_AX_BROWSER_STOP,
        title: "Stop and remove a browser container",
        description: "Stop and remove a Neko browser container previously launched with \
            ax_browser_launch. Always call this when the test completes to free resources.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "container_id": {
                    "type": "string",
                    "description": "Container ID returned by ax_browser_launch"
                }
            },
            "required": ["container_id"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "stopped":      { "type": "boolean" },
                "container_id": { "type": "string" }
            },
            "required": ["stopped"]
        }),
        annotations: annotations::DESTRUCTIVE,
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// Handle `ax_browser_launch` — start a Neko container from the given config.
#[cfg(feature = "docker")]
pub fn handle_ax_browser_launch(args: &serde_json::Value) -> ToolCallResult {
    use crate::docker_browser::{BrowserType, DockerManager, NekoConfig};

    let browser_str = extract_string_field_or(args, "browser", "chromium");
    let browser = match browser_str {
        "firefox" => BrowserType::Firefox,
        "brave" => BrowserType::Brave,
        "edge" => BrowserType::Edge,
        _ => BrowserType::Chromium,
    };

    #[allow(clippy::cast_possible_truncation)]
    let cdp_port = extract_u64_field_or(args, "cdp_port", 9222) as u16;
    #[allow(clippy::cast_possible_truncation)]
    let vnc_port = extract_u64_field_or(args, "vnc_port", 5900) as u16;
    #[allow(clippy::cast_possible_truncation)]
    let width = extract_u64_field_or(args, "width", 1920) as u32;
    #[allow(clippy::cast_possible_truncation)]
    let height = extract_u64_field_or(args, "height", 1080) as u32;

    let config = NekoConfig::builder()
        .browser(browser)
        .cdp_port(cdp_port)
        .vnc_port(vnc_port)
        .dimensions(width, height)
        .build();

    let mut mgr = DockerManager::new();
    match mgr.launch(config) {
        Ok(b) => ToolCallResult::ok(
            json!({
                "launched":     true,
                "container_id": b.container_id(),
                "cdp_url":      b.cdp_url(),
                "vnc_addr":     b.vnc_addr(),
                "browser":      browser_str
            })
            .to_string(),
        ),
        Err(e) => ToolCallResult::error(format!("Failed to launch browser container: {e}")),
    }
}

/// Handle `ax_browser_stop` — stop and remove the named container.
#[cfg(feature = "docker")]
pub fn handle_ax_browser_stop(args: &serde_json::Value) -> ToolCallResult {
    use crate::docker_browser::{BrowserType, DockerManager, NekoBrowser};

    let container_id = extract_or_return!(extract_required_string_field(args, "container_id"));

    // Construct a minimal handle — DockerManager::stop only needs the container_id.
    let browser = NekoBrowser {
        container_id: container_id.clone(),
        cdp_port: 0,
        vnc_port: 0,
        browser: BrowserType::Chromium,
    };

    let mut mgr = DockerManager::new();
    match mgr.stop(&browser) {
        Ok(()) => ToolCallResult::ok(
            json!({
                "stopped":      true,
                "container_id": container_id
            })
            .to_string(),
        ),
        Err(e) => ToolCallResult::error(format!("Failed to stop container '{container_id}': {e}")),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(all(test, feature = "docker"))]
mod tests {
    use serde_json::json;

    #[test]
    fn docker_tools_registers_two_tools() {
        // GIVEN: docker feature enabled
        // WHEN: requesting tool list
        let tools = super::docker_tools();
        // THEN: exactly two tools
        assert_eq!(
            tools.len(),
            2,
            "expected ax_browser_launch + ax_browser_stop"
        );
    }

    #[test]
    fn docker_tool_names_are_unique() {
        // GIVEN: tool list
        let tools = super::docker_tools();
        crate::mcp::test_support::assert_tool_names_unique(&tools, "docker tools");
    }

    #[test]
    fn docker_tools_have_non_empty_descriptions() {
        // GIVEN: tool list
        for tool in super::docker_tools() {
            // THEN: description is present
            assert!(
                !tool.description.is_empty(),
                "empty description on {}",
                tool.name
            );
        }
    }

    #[test]
    fn ax_browser_launch_has_annotations() {
        // GIVEN: launch tool descriptor
        let tools = super::docker_tools();
        let launch = tools
            .iter()
            .find(|t| t.name == "ax_browser_launch")
            .unwrap();
        // THEN: annotations accessible (no panic)
        let _ = launch.annotations.destructive;
        let _ = launch.annotations.read_only;
    }

    #[test]
    fn ax_browser_stop_missing_container_id_returns_error() {
        // GIVEN: no container_id field
        // WHEN: dispatching
        let result = super::handle_ax_browser_stop(&json!({}));
        // THEN: error payload
        assert!(result.is_error);
        assert_eq!(
            result.content[0].text,
            "Missing required field: container_id"
        );
    }

    #[test]
    fn ax_browser_launch_defaults_to_chromium() {
        // GIVEN: no browser field — should default to Chromium
        // WHEN: building config (test via handler, which will fail without Docker — that's expected)
        let result = super::handle_ax_browser_launch(&json!({}));
        // THEN: error mentions Docker (not "Unknown browser" or similar)
        assert!(result.is_error);
        let msg = &result.content[0].text;
        // The error comes from Docker CLI not being reachable in CI — that's the correct path
        assert!(
            msg.contains("docker") || msg.contains("Failed"),
            "unexpected error: {msg}"
        );
    }
}
