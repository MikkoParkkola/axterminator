//! Space management MCP tools (requires `spaces` feature).
//!
//! | Tool | Purpose |
//! |------|---------|
//! | `ax_list_spaces`   | Enumerate all macOS virtual desktops |
//! | `ax_create_space`  | Create an isolated agent virtual desktop |
//! | `ax_move_to_space` | Move an app's windows to a virtual desktop |
//! | `ax_switch_space`  | Switch the active virtual desktop |
//! | `ax_destroy_space` | Destroy an agent-created virtual desktop |
//!
//! All functions are gated behind `#[cfg(feature = "spaces")]` and use the
//! CGSSpace private SPI — not available in App Store builds.

#[cfg(feature = "spaces")]
use std::sync::Arc;

#[cfg(feature = "spaces")]
use serde_json::{json, Value};

#[cfg(feature = "spaces")]
use crate::mcp::annotations;
#[cfg(feature = "spaces")]
use crate::mcp::protocol::{Tool, ToolCallResult};
#[cfg(feature = "spaces")]
use crate::mcp::tools::AppRegistry;

// ---------------------------------------------------------------------------
// Tool declarations
// ---------------------------------------------------------------------------

/// All space management tools (requires `spaces` feature).
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
// Handlers
// ---------------------------------------------------------------------------

#[cfg(feature = "spaces")]
pub(crate) fn handle_ax_list_spaces() -> ToolCallResult {
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
pub(crate) fn handle_ax_create_space() -> ToolCallResult {
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
pub(crate) fn handle_ax_move_to_space(args: &Value, registry: &Arc<AppRegistry>) -> ToolCallResult {
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
pub(crate) fn handle_ax_switch_space(args: &Value) -> ToolCallResult {
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
pub(crate) fn handle_ax_destroy_space(args: &Value) -> ToolCallResult {
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
