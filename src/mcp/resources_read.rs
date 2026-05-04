//! Read handlers for MCP resources.
//!
//! This module contains every `read_*` function invoked by [`super::resources::read_resource`]
//! together with the helper routines they depend on.  It is intentionally
//! `pub(super)` so that only [`resources`](super::resources) can call in;
//! external callers go through the public `read_resource` dispatcher.

use std::sync::Arc;

use base64::Engine as _;
use serde_json::{Value, json};

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

/// Read the `axterminator://clipboard` resource.
///
/// Invokes `osascript -e 'the clipboard'` to retrieve the current pasteboard text.
/// Returns an empty string for the `text` field when the clipboard contains
/// non-text content or when the AppleScript invocation fails.
pub(super) fn read_clipboard(uri: &str) -> Result<ResourceReadResult, ResourceError> {
    let text = read_clipboard_text();
    let payload = serde_json::json!({ "text": text });
    Ok(ResourceReadResult {
        contents: vec![ResourceContents::text(
            uri,
            "application/json",
            payload.to_string(),
        )],
    })
}

/// Retrieve the current pasteboard text via `osascript`.
///
/// Returns an empty string on any failure so callers never see an error for
/// clipboard operations — the clipboard may simply contain non-text data.
fn read_clipboard_text() -> String {
    std::process::Command::new("osascript")
        .arg("-e")
        .arg("the clipboard")
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        .unwrap_or_default()
}

/// Read the `axterminator://workflows` resource.
///
/// Locks the global `WORKFLOW_TRACKER` and returns aggregate stats plus every
/// detected cross-app workflow pattern (min frequency = 2).  An empty `workflows`
/// array is valid when fewer than two transitions have been recorded.
pub(super) fn read_workflows(uri: &str) -> Result<ResourceReadResult, ResourceError> {
    let payload = crate::mcp::tools_innovation::workflow_tracking_data();
    let body = serde_json::to_string(&payload)
        .map_err(|e| ResourceError::operation_failed(format!("Serialization failed: {e}")))?;
    Ok(ResourceReadResult {
        contents: vec![ResourceContents::text(uri, "application/json", body)],
    })
}

/// Read the `axterminator://profiles` resource.
///
/// Instantiates a [`ProfileRegistry`](crate::electron_profiles::ProfileRegistry)
/// with all built-in profiles and serialises each one to JSON, including
/// capabilities, selectors, shortcuts, and CDP port.
pub(super) fn read_profiles(uri: &str) -> Result<ResourceReadResult, ResourceError> {
    let profiles: Vec<Value> = crate::electron_profiles::builtin_profiles()
        .iter()
        .map(profile_to_json)
        .collect();

    let payload = json!({
        "profile_count": profiles.len(),
        "profiles":      profiles,
    });

    let body = serde_json::to_string(&payload)
        .map_err(|e| ResourceError::operation_failed(format!("Serialization failed: {e}")))?;

    Ok(ResourceReadResult {
        contents: vec![ResourceContents::text(uri, "application/json", body)],
    })
}

/// Serialise a single [`AppProfile`](crate::electron_profiles::AppProfile) to JSON.
fn profile_to_json(profile: &crate::electron_profiles::AppProfile) -> Value {
    use crate::electron_profiles::AppCapability;

    let capabilities: Vec<&str> = profile
        .capabilities
        .iter()
        .map(|cap| match cap {
            AppCapability::Chat => "chat",
            AppCapability::Email => "email",
            AppCapability::Calendar => "calendar",
            AppCapability::CodeEditor => "code_editor",
            AppCapability::Browser => "browser",
            AppCapability::Terminal => "terminal",
            AppCapability::FileManager => "file_manager",
            AppCapability::Custom(_) => "custom",
        })
        .collect();

    let selectors: Value = profile.selectors.iter().fold(json!({}), |mut acc, (k, v)| {
        acc[k] = json!(v);
        acc
    });

    let shortcuts: Value = profile.shortcuts.iter().fold(json!({}), |mut acc, (k, v)| {
        acc[k] = json!(v);
        acc
    });

    json!({
        "name":         profile.name,
        "app_id":       profile.app_id,
        "cdp_port":     profile.cdp_port,
        "capabilities": capabilities,
        "selectors":    selectors,
        "shortcuts":    shortcuts,
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
// Capture resources (feature = "audio")
// ---------------------------------------------------------------------------

/// Read `axterminator://capture/transcription`.
///
/// Returns all transcription segments accumulated by the active capture session
/// together with a joined `text` field.  When no session is running the resource
/// returns an empty payload (no error) so subscribers can distinguish "session not
/// started" from a true failure.
#[cfg(feature = "audio")]
pub(super) fn read_capture_transcription(uri: &str) -> Result<ResourceReadResult, ResourceError> {
    use crate::mcp::tools_capture::global_session;

    let guard = global_session()
        .lock()
        .map_err(|e| ResourceError::operation_failed(format!("session lock poisoned: {e}")))?;

    let payload = match guard.as_ref() {
        None => json!({ "running": false, "segments": [], "text": "" }),
        Some(session) => build_transcription_payload(session),
    };

    Ok(ResourceReadResult {
        contents: vec![ResourceContents::text(
            uri,
            "application/json",
            payload.to_string(),
        )],
    })
}

/// Build the transcription payload JSON from a live session.
#[cfg(feature = "audio")]
fn build_transcription_payload(session: &crate::capture::CaptureSession) -> Value {
    // u64::MAX / 1000 is effectively "all segments" — larger than any realistic
    // session duration in seconds.
    let segments = session.read_transcription(u64::MAX / 1_000);
    let text: String = segments
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let segments_json: Vec<Value> = segments
        .iter()
        .map(|s| {
            json!({
                "text":     s.text,
                "start_ms": s.start_ms,
                "end_ms":   s.end_ms,
                "speaker":  s.speaker,
            })
        })
        .collect();
    json!({
        "running":     true,
        "session_id":  session.session_id,
        "duration_ms": session.duration_ms(),
        "segments":    segments_json,
        "text":        text,
    })
}

/// Read `axterminator://capture/screen`.
///
/// Returns the most recently captured screen frame as a base64-encoded PNG blob.
/// When no frame is available (no session, screen capture disabled, or first frame
/// not yet taken), returns a text/JSON payload describing the current status
/// so the client is not left with an opaque error.
#[cfg(feature = "audio")]
pub(super) fn read_capture_screen(uri: &str) -> Result<ResourceReadResult, ResourceError> {
    use crate::mcp::tools_capture::global_session;

    let guard = global_session()
        .lock()
        .map_err(|e| ResourceError::operation_failed(format!("session lock poisoned: {e}")))?;

    // Cloning the Option<ScreenFrame> lets us release the guard before building
    // the response, keeping lock hold time minimal.
    let (frame, is_running) = match guard.as_ref() {
        Some(session) => (session.latest_frame(), session.is_running()),
        None => (None, false),
    };

    if let Some(f) = frame {
        return Ok(ResourceReadResult {
            contents: vec![ResourceContents::blob(uri, "image/png", f.png_base64)],
        });
    }
    // No frame yet — describe why in JSON so the agent can react.
    let payload = json!({
        "running":         is_running,
        "frame_available": false,
    });
    Ok(ResourceReadResult {
        contents: vec![ResourceContents::text(
            uri,
            "application/json",
            payload.to_string(),
        )],
    })
}

/// Read `axterminator://capture/status`.
///
/// Returns health and fill-level information for the active capture session.
/// When no session is running, returns `{"running": false}`.
#[cfg(feature = "audio")]
pub(super) fn read_capture_status(uri: &str) -> Result<ResourceReadResult, ResourceError> {
    use crate::mcp::tools_capture::global_session;

    let guard = global_session()
        .lock()
        .map_err(|e| ResourceError::operation_failed(format!("session lock poisoned: {e}")))?;

    let payload = match guard.as_ref() {
        None => json!({ "running": false }),
        Some(session) => json!({
            "running":              session.is_running(),
            "session_id":           session.session_id,
            "duration_ms":          session.duration_ms(),
            "audio_buffer_seconds": session.audio_buffer_seconds(),
            "transcript_segments":  session.transcript_segment_count(),
        }),
    };

    Ok(ResourceReadResult {
        contents: vec![ResourceContents::text(
            uri,
            "application/json",
            payload.to_string(),
        )],
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
    } else if let Some(question) = parse_query_question(uri, name) {
        read_app_query(uri, name, question, registry)
    } else {
        Err(ResourceError::invalid_uri(uri))
    }
}

/// Extract the `{question}` segment from a `query` template URI.
///
/// Expected form: `axterminator://app/{name}/query/{question}`.
/// Returns `None` when the URI does not match this pattern or the question
/// segment is empty.
fn parse_query_question<'a>(uri: &'a str, app_name: &str) -> Option<&'a str> {
    let prefix = format!("axterminator://app/{app_name}/query/");
    let question = uri.strip_prefix(prefix.as_str())?;
    if question.is_empty() {
        None
    } else {
        Some(question)
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

/// Read `axterminator://app/{name}/query/{question}`.
///
/// Builds a live [`SceneGraph`](crate::scene::SceneGraph) from the app's
/// accessibility tree and answers a percent-encoded natural-language question.
/// The `{question}` segment should be percent-encoded (spaces as `%20`).
///
/// # Errors
///
/// - [`ResourceError::not_connected`] when the app has not been registered.
/// - [`ResourceError::operation_failed`] when the accessibility scan fails.
fn read_app_query(
    uri: &str,
    app_name: &str,
    question: &str,
    registry: &Arc<AppRegistry>,
) -> Result<ResourceReadResult, ResourceError> {
    let decoded = percent_decode(question);

    registry
        .with_app(app_name, |app| {
            let scene = crate::intent::scan_scene(app.element)
                .map_err(|e| ResourceError::operation_failed(format!("scan_scene failed: {e}")))?;

            let result = crate::scene::SceneEngine::new().query(&decoded, &scene);

            let matches_json: Vec<Value> = result
                .matches
                .iter()
                .map(|m| {
                    json!({
                        "role":         m.element_role,
                        "label":        m.element_label,
                        "path":         m.element_path,
                        "match_score":  m.match_score,
                        "match_reason": m.match_reason,
                        "bounds": m.bounds.map(|(x, y, w, h)| json!([x, y, w, h])),
                    })
                })
                .collect();

            let payload = json!({
                "app":               app_name,
                "question":          decoded,
                "confidence":        result.confidence,
                "scene_description": result.scene_description,
                "matches":           matches_json,
            });

            Ok(ResourceReadResult {
                contents: vec![ResourceContents::text(
                    uri,
                    "application/json",
                    payload.to_string(),
                )],
            })
        })
        .map_err(|_| ResourceError::not_connected(app_name))?
}

/// Decode percent-encoded characters in a URI path segment.
///
/// Only replaces `%XX` sequences; non-ASCII pass through unchanged.
/// Invalid sequences are left as-is rather than returning an error.
fn percent_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        let hi = chars.next();
        let lo = chars.next();
        match (hi, lo) {
            (Some(h), Some(l)) => {
                let hex = format!("{h}{l}");
                if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                    out.push(byte as char);
                } else {
                    out.push('%');
                    out.push(h);
                    out.push(l);
                }
            }
            (Some(h), None) => {
                out.push('%');
                out.push(h);
            }
            _ => out.push('%'),
        }
    }
    out
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
