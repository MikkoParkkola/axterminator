//! Read handlers for MCP resources.
//!
//! This module contains every `read_*` function invoked by [`super::resources::read_resource`]
//! together with the helper routines they depend on.  It is intentionally
//! `pub(super)` so that only [`resources`](super::resources) can call in;
//! external callers go through the public `read_resource` dispatcher.

use std::sync::Arc;

use base64::Engine as _;
use serde_json::{json, Value};

use crate::display;
use crate::mcp::protocol::{ResourceContents, ResourceReadResult};
use crate::mcp::tools::AppRegistry;

use super::resources::ResourceError;

// ---------------------------------------------------------------------------
// Static resource handlers
// ---------------------------------------------------------------------------

pub(super) fn read_system_status(
    uri: &str,
    registry: &Arc<AppRegistry>,
) -> Result<ResourceReadResult, ResourceError> {
    let accessibility_enabled = crate::accessibility::check_accessibility_enabled();
    let connected = registry.connected_names();
    let payload = json!({
        "accessibility_enabled": accessibility_enabled,
        "server_version": env!("CARGO_PKG_VERSION"),
        "protocol_version": "2025-11-05",
        "connected_apps": connected,
        "connected_count": connected.len(),
    });

    Ok(ResourceReadResult {
        contents: vec![ResourceContents::text(
            uri,
            "application/json",
            payload.to_string(),
        )],
    })
}

pub(super) fn read_running_apps(
    uri: &str,
    _registry: &Arc<AppRegistry>,
) -> Result<ResourceReadResult, ResourceError> {
    let apps = list_running_apps();
    let payload = json!({ "apps": apps });

    Ok(ResourceReadResult {
        contents: vec![ResourceContents::text(
            uri,
            "application/json",
            payload.to_string(),
        )],
    })
}

/// Read the `axterminator://system/displays` resource.
///
/// Returns the complete list of connected displays with id, bounds
/// (in global logical-point coordinates — may have negative origin for
/// secondary monitors), scale factor, and is_primary flag.
pub(super) fn read_system_displays(uri: &str) -> Result<ResourceReadResult, ResourceError> {
    let displays = display::list_displays()
        .map_err(|e| ResourceError::operation_failed(format!("Display enumeration failed: {e}")))?;

    let display_values: Vec<Value> = displays
        .iter()
        .map(|d| {
            json!({
                "id": d.id,
                "bounds": {
                    "x": d.bounds.x,
                    "y": d.bounds.y,
                    "width": d.bounds.width,
                    "height": d.bounds.height,
                },
                "scale_factor": d.scale_factor,
                "is_primary": d.is_primary,
            })
        })
        .collect();

    let payload = json!({
        "display_count": display_values.len(),
        "displays": display_values,
    });

    Ok(ResourceReadResult {
        contents: vec![ResourceContents::text(
            uri,
            "application/json",
            payload.to_string(),
        )],
    })
}

/// Read the `axterminator://spaces` virtual desktop resource.
///
/// Lists all Spaces with id, type, active flag, and agent-created status.
/// Requires the `spaces` feature flag.
#[cfg(feature = "spaces")]
pub(super) fn read_spaces(uri: &str) -> Result<ResourceReadResult, ResourceError> {
    use crate::spaces::SpaceManager;

    let mgr = SpaceManager::new();
    let spaces = mgr
        .list_spaces()
        .map_err(|e| ResourceError::operation_failed(format!("Space enumeration failed: {e}")))?;

    let space_values: Vec<Value> = spaces
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

    let payload = json!({
        "space_count": space_values.len(),
        "spaces": space_values,
    });

    Ok(ResourceReadResult {
        contents: vec![ResourceContents::text(
            uri,
            "application/json",
            payload.to_string(),
        )],
    })
}

/// Read the `axterminator://audio/devices` resource.
///
/// Returns all CoreAudio input/output devices with name, ID, sample rate,
/// and default-device flags. Requires the `audio` cargo feature.
///
/// # Errors
///
/// Returns [`ResourceError::operation_failed`] when serialization fails
/// (should never occur in practice).
#[cfg(feature = "audio")]
pub(super) fn read_audio_devices(uri: &str) -> Result<ResourceReadResult, ResourceError> {
    let devices = crate::audio::list_audio_devices();
    let payload = json!({
        "device_count": devices.len(),
        "devices": devices,
    });
    let body = serde_json::to_string(&payload)
        .map_err(|e| ResourceError::operation_failed(format!("Serialization failed: {e}")))?;
    Ok(ResourceReadResult {
        contents: vec![ResourceContents::text(uri, "application/json", body)],
    })
}

/// Read `axterminator://camera/devices`.
///
/// Enumerates available video capture devices via AVFoundation. No TCC permission
/// is required for device enumeration — only capture operations need it.
#[cfg(feature = "camera")]
pub(super) fn read_camera_devices(uri: &str) -> Result<ResourceReadResult, ResourceError> {
    let payload = crate::mcp::tools_extended::camera_devices_payload();
    Ok(ResourceReadResult {
        contents: vec![ResourceContents::text(uri, "application/json", payload)],
    })
}

// ---------------------------------------------------------------------------
// Dynamic resource handlers
// ---------------------------------------------------------------------------

pub(super) fn read_dynamic(
    uri: &str,
    registry: &Arc<AppRegistry>,
) -> Result<ResourceReadResult, ResourceError> {
    // Expected pattern: axterminator://app/{name}/{resource}
    let name = super::resources::parse_app_name(uri)?;

    if uri.ends_with("/tree") {
        read_app_tree(uri, name, registry)
    } else if uri.ends_with("/screenshot") {
        read_app_screenshot(uri, name, registry)
    } else if uri.ends_with("/state") {
        read_app_state(uri, name, registry)
    } else {
        Err(ResourceError::invalid_uri(uri))
    }
}

fn read_app_tree(
    uri: &str,
    app_name: &str,
    registry: &Arc<AppRegistry>,
) -> Result<ResourceReadResult, ResourceError> {
    registry
        .with_app(app_name, |app| {
            let tree = build_element_tree(app, 3);
            let payload = json!({
                "app": app_name,
                "depth_limit": 3,
                "tree": tree,
            });
            ResourceReadResult {
                contents: vec![ResourceContents::text(
                    uri,
                    "application/json",
                    payload.to_string(),
                )],
            }
        })
        .map_err(|_| ResourceError::not_connected(app_name))
}

fn read_app_screenshot(
    uri: &str,
    app_name: &str,
    registry: &Arc<AppRegistry>,
) -> Result<ResourceReadResult, ResourceError> {
    registry
        .with_app(app_name, |app| {
            app.screenshot_native()
                .map_err(|e| ResourceError::operation_failed(format!("Screenshot failed: {e}")))
                .map(|bytes| {
                    let b64 = base64::engine::general_purpose::STANDARD.encode::<&[u8]>(&bytes);
                    ResourceReadResult {
                        contents: vec![ResourceContents::blob(uri, "image/png", b64)],
                    }
                })
        })
        .map_err(|_| ResourceError::not_connected(app_name))?
}

fn read_app_state(
    uri: &str,
    app_name: &str,
    registry: &Arc<AppRegistry>,
) -> Result<ResourceReadResult, ResourceError> {
    // Enumerate displays once so we can annotate each window with its display.
    let displays = display::list_displays().unwrap_or_default();

    registry
        .with_app(app_name, |app| {
            let windows = app
                .windows_native()
                .unwrap_or_default()
                .iter()
                .map(|w| window_state_json(w, &displays))
                .collect::<Vec<_>>();

            let payload = json!({
                "app": app_name,
                "pid": app.pid,
                "windows": windows,
                "window_count": windows.len(),
            });

            ResourceReadResult {
                contents: vec![ResourceContents::text(
                    uri,
                    "application/json",
                    payload.to_string(),
                )],
            }
        })
        .map_err(|_| ResourceError::not_connected(app_name))
}

/// Build the JSON state object for a single window, annotated with display info.
fn window_state_json(w: &crate::element::AXElement, displays: &[display::Display]) -> Value {
    let bounds = w.bounds();

    let display_id =
        bounds.and_then(|(x, y, _, _)| display::display_for_point(x, y, displays).map(|d| d.id));

    // For windows spanning multiple displays, include both display IDs.
    let spanning_displays: Vec<u32> = bounds
        .map(|(x, y, w_size, h_size)| {
            let rect = display::Rect {
                x,
                y,
                width: w_size,
                height: h_size,
            };
            display::displays_for_rect(&rect, displays)
                .iter()
                .map(|d| d.id)
                .collect()
        })
        .unwrap_or_default();

    json!({
        "title": w.title(),
        "role": w.role(),
        "bounds": bounds.map(|(x, y, w_size, h_size)| json!({
            "x": x, "y": y, "width": w_size, "height": h_size
        })),
        "display_id": display_id,
        "spanning_displays": spanning_displays,
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a depth-limited element tree rooted at `app`'s accessibility element.
///
/// Returns a `Value::Null` if the tree cannot be accessed (accessibility
/// denied, app not responding, etc.). Callers should treat `null` as an
/// empty tree rather than an error, since accessibility state is inherently
/// transient.
fn build_element_tree(app: &crate::AXApp, max_depth: u32) -> Value {
    use crate::accessibility;

    let root = app.element;
    build_node(root, max_depth, 0, &accessibility::get_children)
}

/// Recursively build one tree node up to `max_depth`.
fn build_node(
    element: crate::accessibility::AXUIElementRef,
    max_depth: u32,
    current_depth: u32,
    get_children: &dyn Fn(
        crate::accessibility::AXUIElementRef,
    ) -> crate::error::AXResult<Vec<crate::accessibility::AXUIElementRef>>,
) -> Value {
    use crate::accessibility;

    let role =
        accessibility::get_string_attribute_value(element, accessibility::attributes::AX_ROLE);
    let title =
        accessibility::get_string_attribute_value(element, accessibility::attributes::AX_TITLE);
    let identifier = accessibility::get_string_attribute_value(
        element,
        accessibility::attributes::AX_IDENTIFIER,
    );

    let children = if current_depth < max_depth {
        get_children(element)
            .unwrap_or_default()
            .into_iter()
            .map(|child| build_node(child, max_depth, current_depth + 1, get_children))
            .collect::<Vec<_>>()
    } else {
        vec![]
    };

    json!({
        "role": role,
        "title": title,
        "identifier": identifier,
        "children": children,
    })
}

/// Enumerate running applications via `sysinfo`.
fn list_running_apps() -> Vec<Value> {
    use sysinfo::System;

    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    sys.processes()
        .values()
        .filter_map(|proc| {
            let name = proc.name().to_string_lossy().into_owned();
            // Filter out kernel threads and very short names
            if name.is_empty() || proc.pid().as_u32() == 0 {
                return None;
            }
            Some(json!({
                "name": name,
                "pid": proc.pid().as_u32(),
            }))
        })
        .collect()
}
