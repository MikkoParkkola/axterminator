//! MCP Phase 2 resource handlers.
//!
//! Resources expose read-only, URI-addressable views of application state.
//! They complement tools by providing ambient context that agents read
//! repeatedly rather than computed results from one-shot operations.
//!
//! ## Static resources (concrete URIs)
//!
//! | URI | Content |
//! |-----|---------|
//! | `axterminator://system/status` | Accessibility enabled, version, connected apps |
//! | `axterminator://system/displays` | Connected display geometry, scale factors |
//! | `axterminator://apps` | Running apps with PID, bundle ID, accessibility status |
//! | `axterminator://spaces` | Virtual desktop layout (requires `spaces` feature) |
//!
//! ## Dynamic resource templates (RFC 6570)
//!
//! | Template | Content |
//! |----------|---------|
//! | `axterminator://app/{name}/tree` | Element hierarchy (depth ≤ 3) |
//! | `axterminator://app/{name}/screenshot` | Base64 PNG screenshot |
//! | `axterminator://app/{name}/state` | Focused element, windows, visible text |
//!
//! ## URI parsing
//!
//! [`parse_app_name`] extracts the `{name}` segment from a dynamic URI.
//! Invalid URIs produce a descriptive [`ResourceError`] that is surfaced to
//! the MCP client as a JSON-RPC error response.
//!
//! ## Implementation layout
//!
//! Read handler implementations live in [`super::resources_read`] to keep
//! file size tractable.  Only the public API (`static_resources`,
//! `resource_templates`, `read_resource`) and the shared [`ResourceError`]
//! type are defined here.

use std::sync::Arc;

use tracing::debug;

use crate::mcp::protocol::{
    Resource, ResourceListResult, ResourceReadResult, ResourceTemplate, ResourceTemplateListResult,
};
use crate::mcp::tools::AppRegistry;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// A resource read error that maps to a JSON-RPC error response.
#[derive(Debug)]
pub struct ResourceError {
    /// Short machine-readable tag (e.g. `"not_connected"`).
    pub code: &'static str,
    /// Human-readable detail forwarded to the MCP client.
    pub message: String,
}

impl ResourceError {
    pub(crate) fn not_connected(app: &str) -> Self {
        Self {
            code: "not_connected",
            message: format!("App '{app}' not connected — call ax_connect first"),
        }
    }

    pub(crate) fn invalid_uri(uri: &str) -> Self {
        Self {
            code: "invalid_uri",
            message: format!("Cannot parse app name from URI: {uri}"),
        }
    }

    pub(crate) fn operation_failed(detail: impl Into<String>) -> Self {
        Self {
            code: "operation_failed",
            message: detail.into(),
        }
    }
}

impl std::fmt::Display for ResourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

// ---------------------------------------------------------------------------
// Static resource descriptors
// ---------------------------------------------------------------------------

/// All static (concrete URI) resources advertised in `resources/list`.
///
/// # Examples
///
/// ```
/// let resources = axterminator::mcp::resources::static_resources();
/// assert!(resources.resources.iter().any(|r| r.uri == "axterminator://system/status"));
/// assert!(resources.resources.iter().any(|r| r.uri == "axterminator://system/displays"));
/// ```
#[must_use]
pub fn static_resources() -> ResourceListResult {
    // `mut` is required when the `spaces` feature is enabled (push below).
    #[allow(unused_mut)]
    let mut resources = vec![
        Resource {
            uri: "axterminator://system/status",
            name: "system-status",
            title: "System Accessibility Status",
            description: "Accessibility permissions, connected apps count, server version.",
            mime_type: "application/json",
        },
        Resource {
            uri: "axterminator://system/displays",
            name: "system-displays",
            title: "Connected Displays",
            description:
                "All connected displays with id, bounds (global logical-point coordinates), \
                 scale factor, and is_primary flag. Bounds origin may be negative for secondary \
                 monitors placed left of or above the primary display.",
            mime_type: "application/json",
        },
        Resource {
            uri: "axterminator://apps",
            name: "running-apps",
            title: "Running Applications",
            description:
                "All running macOS applications with PIDs, bundle IDs, and accessibility info.",
            mime_type: "application/json",
        },
    ];

    #[cfg(feature = "spaces")]
    resources.push(Resource {
        uri: "axterminator://spaces",
        name: "virtual-desktops",
        title: "Virtual Desktops (Spaces)",
        description: "All macOS virtual desktops (Spaces) with IDs, types, active flag, and \
             which windows are assigned to each space.",
        mime_type: "application/json",
    });

    #[cfg(feature = "audio")]
    resources.push(Resource {
        uri: "axterminator://audio/devices",
        name: "audio-devices",
        title: "Audio Devices",
        description: "All CoreAudio input/output devices with name, ID, sample rate, \
            and default-device status. Requires the `audio` cargo feature.",
        mime_type: "application/json",
    });

    #[cfg(feature = "camera")]
    resources.push(Resource {
        uri: "axterminator://camera/devices",
        name: "camera-devices",
        title: "Available Camera Devices",
        description: "All video capture devices with device_id, name, position \
            (front/back/external), and is_default flag. No permission required to list.",
        mime_type: "application/json",
    });

    resources.push(Resource {
        uri: "axterminator://workflows",
        name: "detected-workflows",
        title: "Detected Cross-App Workflows",
        description: "Aggregate stats and detected cross-app workflow patterns observed \
            via ax_track_workflow. Patterns repeat at least twice to appear here. \
            Useful for discovering automation candidates across apps.",
        mime_type: "application/json",
    });

    resources.push(Resource {
        uri: "axterminator://profiles",
        name: "electron-app-profiles",
        title: "Electron App Profiles",
        description: "All built-in Electron app profiles with capabilities, CSS selectors, \
            keyboard shortcuts, and CDP debug ports. Covers VS Code, Slack, Chrome, \
            Terminal, and Finder. Use selectors with CDP and shortcuts with ax_shortcut.",
        mime_type: "application/json",
    });

    ResourceListResult { resources }
}

/// All dynamic (RFC 6570 template) resources advertised in
/// `resources/templates/list`.
///
/// # Examples
///
/// ```
/// let templates = axterminator::mcp::resources::resource_templates();
/// let uris: Vec<_> = templates.resource_templates.iter()
///     .map(|t| t.uri_template)
///     .collect();
/// assert!(uris.contains(&"axterminator://app/{name}/tree"));
/// ```
#[must_use]
pub fn resource_templates() -> ResourceTemplateListResult {
    ResourceTemplateListResult {
        resource_templates: vec![
            ResourceTemplate {
                uri_template: "axterminator://app/{name}/tree",
                name: "app-element-tree",
                title: "Application Element Tree",
                description:
                    "Accessibility element hierarchy for a connected app (depth ≤ 3 by default).",
                mime_type: "application/json",
            },
            ResourceTemplate {
                uri_template: "axterminator://app/{name}/screenshot",
                name: "app-screenshot",
                title: "Application Screenshot",
                description: "Current screenshot of a connected app as a base64-encoded PNG.",
                mime_type: "image/png",
            },
            ResourceTemplate {
                uri_template: "axterminator://app/{name}/state",
                name: "app-ui-state",
                title: "Application UI State",
                description:
                    "Current UI state: window titles, focused element, visible text summary.",
                mime_type: "application/json",
            },
            ResourceTemplate {
                uri_template: "axterminator://app/{name}/query/{question}",
                name: "app-scene-query",
                title: "Natural-Language Scene Query",
                description: "Query the live accessibility scene graph of a connected app \
                    with a natural-language question. Returns confidence, a scene description, \
                    and matching elements with roles, labels, and bounds. \
                    Example: axterminator://app/Safari/query/is%20there%20a%20search%20field",
                mime_type: "application/json",
            },
        ],
    }
}

// ---------------------------------------------------------------------------
// Resource read dispatcher
// ---------------------------------------------------------------------------

/// Dispatch a `resources/read` request to the appropriate handler.
///
/// Returns `Ok(ResourceReadResult)` on success, or `Err(ResourceError)`
/// when the URI is unknown/unparseable or the underlying operation fails.
///
/// # Errors
///
/// - [`ResourceError::invalid_uri`] when the URI scheme or path cannot be parsed.
/// - [`ResourceError::not_connected`] when a dynamic URI names an app that has
///   not been registered via `ax_connect`.
/// - [`ResourceError::operation_failed`] when the accessibility API returns an error.
pub fn read_resource(
    uri: &str,
    registry: &Arc<AppRegistry>,
) -> Result<ResourceReadResult, ResourceError> {
    use super::resources_read as read;

    debug!(uri, "reading resource");

    match uri {
        "axterminator://system/status" => read::read_system_status(uri, registry),
        "axterminator://system/displays" => read::read_system_displays(uri),
        "axterminator://apps" => read::read_running_apps(uri, registry),
        "axterminator://workflows" => read::read_workflows(uri),
        "axterminator://profiles" => read::read_profiles(uri),
        #[cfg(feature = "spaces")]
        "axterminator://spaces" => read::read_spaces(uri),
        #[cfg(feature = "audio")]
        "axterminator://audio/devices" => read::read_audio_devices(uri),
        #[cfg(feature = "camera")]
        "axterminator://camera/devices" => read::read_camera_devices(uri),
        other => read::read_dynamic(other, registry),
    }
}

// ---------------------------------------------------------------------------
// URI parsing
// ---------------------------------------------------------------------------

/// Extract the `{name}` segment from a dynamic resource URI.
///
/// Expected form: `axterminator://app/{name}/...`
///
/// Returns `Err(ResourceError::invalid_uri)` when the URI does not match
/// the expected pattern or the name segment is empty.
///
/// # Examples
///
/// ```
/// use axterminator::mcp::resources::parse_app_name;
///
/// assert_eq!(parse_app_name("axterminator://app/Safari/tree").unwrap(), "Safari");
/// assert_eq!(parse_app_name("axterminator://app/Finder/state").unwrap(), "Finder");
/// assert!(parse_app_name("axterminator://system/status").is_err());
/// ```
pub fn parse_app_name(uri: &str) -> Result<&str, ResourceError> {
    let path = uri
        .strip_prefix("axterminator://app/")
        .ok_or_else(|| ResourceError::invalid_uri(uri))?;

    let name = path
        .split('/')
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ResourceError::invalid_uri(uri))?;

    Ok(name)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // parse_app_name
    // -----------------------------------------------------------------------

    #[test]
    fn parse_app_name_extracts_safari() {
        // GIVEN: a well-formed dynamic resource URI
        // WHEN: parsed
        let name = parse_app_name("axterminator://app/Safari/tree").unwrap();
        // THEN: returns the app segment
        assert_eq!(name, "Safari");
    }

    #[test]
    fn parse_app_name_extracts_name_with_spaces_encoded() {
        // GIVEN: a URI whose name contains a hyphen (valid URL chars)
        let name = parse_app_name("axterminator://app/Google-Chrome/state").unwrap();
        assert_eq!(name, "Google-Chrome");
    }

    #[test]
    fn parse_app_name_rejects_static_uri() {
        // GIVEN: a static resource URI that has no {name} segment
        let result = parse_app_name("axterminator://system/status");
        // THEN: returns an error
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, "invalid_uri");
    }

    #[test]
    fn parse_app_name_rejects_empty_name() {
        // GIVEN: a URI with an empty name segment
        let result = parse_app_name("axterminator://app//tree");
        assert!(result.is_err());
    }

    #[test]
    fn parse_app_name_rejects_wrong_scheme() {
        let result = parse_app_name("https://example.com/app/Safari/tree");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, "invalid_uri");
    }

    // -----------------------------------------------------------------------
    // Static resource descriptors
    // -----------------------------------------------------------------------

    #[test]
    fn static_resources_contains_system_status() {
        let list = static_resources();
        let has_status = list
            .resources
            .iter()
            .any(|r| r.uri == "axterminator://system/status");
        assert!(has_status);
    }

    #[test]
    fn static_resources_contains_apps() {
        let list = static_resources();
        let has_apps = list
            .resources
            .iter()
            .any(|r| r.uri == "axterminator://apps");
        assert!(has_apps);
    }

    #[test]
    fn static_resources_serialise_without_panic() {
        let list = static_resources();
        let json = serde_json::to_string(&list).unwrap();
        assert!(json.contains("system-status"));
    }

    // -----------------------------------------------------------------------
    // Resource templates
    // -----------------------------------------------------------------------

    #[test]
    fn resource_templates_has_tree_template() {
        let list = resource_templates();
        let has_tree = list
            .resource_templates
            .iter()
            .any(|t| t.uri_template == "axterminator://app/{name}/tree");
        assert!(has_tree);
    }

    #[test]
    fn resource_templates_has_screenshot_template() {
        let list = resource_templates();
        let has_ss = list
            .resource_templates
            .iter()
            .any(|t| t.uri_template == "axterminator://app/{name}/screenshot");
        assert!(has_ss);
    }

    #[test]
    fn resource_templates_has_state_template() {
        let list = resource_templates();
        let has_state = list
            .resource_templates
            .iter()
            .any(|t| t.uri_template == "axterminator://app/{name}/state");
        assert!(has_state);
    }

    #[test]
    fn resource_templates_serialise_without_panic() {
        let list = resource_templates();
        let json = serde_json::to_string(&list).unwrap();
        assert!(json.contains("uriTemplate"));
    }

    // -----------------------------------------------------------------------
    // read_resource dispatch
    // -----------------------------------------------------------------------

    #[test]
    fn read_system_status_returns_json_content() {
        // GIVEN: an empty registry
        let registry = Arc::new(AppRegistry::default());
        // WHEN: reading the system status resource
        let result = read_resource("axterminator://system/status", &registry).unwrap();
        // THEN: exactly one text content item is returned
        assert_eq!(result.contents.len(), 1);
        let text = result.contents[0].text.as_ref().unwrap();
        let v: serde_json::Value = serde_json::from_str(text).unwrap();
        assert!(v["accessibility_enabled"].is_boolean());
        assert_eq!(v["connected_count"], 0);
    }

    #[test]
    fn read_running_apps_returns_json_content() {
        let registry = Arc::new(AppRegistry::default());
        let result = read_resource("axterminator://apps", &registry).unwrap();
        assert_eq!(result.contents.len(), 1);
        assert_eq!(result.contents[0].mime_type, "application/json");
        let text = result.contents[0].text.as_ref().unwrap();
        let v: serde_json::Value = serde_json::from_str(text).unwrap();
        assert!(v["apps"].is_array());
    }

    #[test]
    fn read_unknown_uri_returns_invalid_uri_error() {
        let registry = Arc::new(AppRegistry::default());
        let err = read_resource("axterminator://unknown/path", &registry).unwrap_err();
        assert_eq!(err.code, "invalid_uri");
    }

    #[test]
    fn read_tree_for_unconnected_app_returns_not_connected() {
        let registry = Arc::new(AppRegistry::default());
        let err = read_resource("axterminator://app/NonExistent/tree", &registry).unwrap_err();
        assert_eq!(err.code, "not_connected");
    }

    #[test]
    fn read_screenshot_for_unconnected_app_returns_not_connected() {
        let registry = Arc::new(AppRegistry::default());
        let err =
            read_resource("axterminator://app/NonExistent/screenshot", &registry).unwrap_err();
        assert_eq!(err.code, "not_connected");
    }

    #[test]
    fn read_state_for_unconnected_app_returns_not_connected() {
        let registry = Arc::new(AppRegistry::default());
        let err = read_resource("axterminator://app/NonExistent/state", &registry).unwrap_err();
        assert_eq!(err.code, "not_connected");
    }

    #[test]
    fn resource_error_display_includes_code_and_message() {
        let e = ResourceError::not_connected("Safari");
        let s = e.to_string();
        assert!(s.contains("not_connected"));
        assert!(s.contains("Safari"));
    }

    #[test]
    fn system_status_includes_server_version() {
        let registry = Arc::new(AppRegistry::default());
        let result = read_resource("axterminator://system/status", &registry).unwrap();
        let text = result.contents[0].text.as_ref().unwrap();
        let v: serde_json::Value = serde_json::from_str(text).unwrap();
        assert!(v["server_version"].as_str().is_some());
    }

    // -----------------------------------------------------------------------
    // system/displays resource
    // -----------------------------------------------------------------------

    #[test]
    fn static_resources_contains_system_displays() {
        // GIVEN: static resource list
        let list = static_resources();
        // THEN: system/displays is advertised
        let has_displays = list
            .resources
            .iter()
            .any(|r| r.uri == "axterminator://system/displays");
        assert!(
            has_displays,
            "system/displays must be in static resource list"
        );
    }

    #[test]
    fn read_system_displays_returns_valid_json() {
        // GIVEN: an empty registry
        let registry = Arc::new(AppRegistry::default());
        // WHEN: reading the displays resource
        let result = read_resource("axterminator://system/displays", &registry)
            .expect("system/displays must succeed");
        // THEN: one JSON content item with display_count and displays array
        assert_eq!(result.contents.len(), 1);
        assert_eq!(result.contents[0].mime_type, "application/json");
        let text = result.contents[0].text.as_ref().unwrap();
        let v: serde_json::Value = serde_json::from_str(text).unwrap();
        assert!(v["display_count"].as_u64().unwrap_or(0) >= 1);
        assert!(v["displays"].is_array());
    }

    #[test]
    fn read_system_displays_each_entry_has_required_fields() {
        let registry = Arc::new(AppRegistry::default());
        let result = read_resource("axterminator://system/displays", &registry).unwrap();
        let text = result.contents[0].text.as_ref().unwrap();
        let v: serde_json::Value = serde_json::from_str(text).unwrap();
        for display in v["displays"].as_array().unwrap() {
            assert!(display["id"].is_number(), "id must be present");
            assert!(display["bounds"].is_object(), "bounds must be object");
            assert!(display["bounds"]["width"].as_f64().unwrap_or(0.0) > 0.0);
            assert!(display["bounds"]["height"].as_f64().unwrap_or(0.0) > 0.0);
            assert!(display["scale_factor"].as_f64().unwrap_or(0.0) >= 1.0);
            assert!(display["is_primary"].is_boolean());
        }
    }

    #[test]
    fn read_system_displays_exactly_one_primary() {
        let registry = Arc::new(AppRegistry::default());
        let result = read_resource("axterminator://system/displays", &registry).unwrap();
        let text = result.contents[0].text.as_ref().unwrap();
        let v: serde_json::Value = serde_json::from_str(text).unwrap();
        let primary_count = v["displays"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|d| d["is_primary"].as_bool().unwrap_or(false))
            .count();
        assert_eq!(primary_count, 1, "exactly one primary display");
    }

    #[test]
    fn read_system_displays_primary_has_non_negative_x() {
        // Primary display is always at (0,0) in macOS coordinate space.
        let registry = Arc::new(AppRegistry::default());
        let result = read_resource("axterminator://system/displays", &registry).unwrap();
        let text = result.contents[0].text.as_ref().unwrap();
        let v: serde_json::Value = serde_json::from_str(text).unwrap();
        let primary = v["displays"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["is_primary"].as_bool().unwrap_or(false))
            .unwrap();
        assert_eq!(primary["bounds"]["x"].as_f64().unwrap_or(-1.0), 0.0);
        assert_eq!(primary["bounds"]["y"].as_f64().unwrap_or(-1.0), 0.0);
    }

    // -----------------------------------------------------------------------
    // spaces resource (feature = "spaces")
    // -----------------------------------------------------------------------

    #[cfg(feature = "spaces")]
    #[test]
    fn static_resources_contains_spaces_when_feature_enabled() {
        let list = static_resources();
        let has_spaces = list
            .resources
            .iter()
            .any(|r| r.uri == "axterminator://spaces");
        assert!(
            has_spaces,
            "spaces resource must be in list with spaces feature"
        );
    }

    #[cfg(feature = "spaces")]
    #[test]
    fn read_spaces_returns_valid_json_with_at_least_one_space() {
        let registry = Arc::new(AppRegistry::default());
        let result = read_resource("axterminator://spaces", &registry)
            .expect("spaces resource must succeed");
        assert_eq!(result.contents.len(), 1);
        let text = result.contents[0].text.as_ref().unwrap();
        let v: serde_json::Value = serde_json::from_str(text).unwrap();
        assert!(v["space_count"].as_u64().unwrap_or(0) >= 1);
        assert!(v["spaces"].is_array());
    }

    #[cfg(feature = "spaces")]
    #[test]
    fn read_spaces_each_entry_has_required_fields() {
        let registry = Arc::new(AppRegistry::default());
        let result = read_resource("axterminator://spaces", &registry).unwrap();
        let text = result.contents[0].text.as_ref().unwrap();
        let v: serde_json::Value = serde_json::from_str(text).unwrap();
        for space in v["spaces"].as_array().unwrap() {
            assert!(space["id"].is_number());
            assert!(space["type"].is_string());
            assert!(space["is_active"].is_boolean());
            assert!(space["is_agent_created"].is_boolean());
        }
    }

    // -----------------------------------------------------------------------
    // audio/devices resource (feature = "audio")
    // -----------------------------------------------------------------------

    #[cfg(feature = "audio")]
    #[test]
    fn static_resources_contains_audio_devices_when_feature_enabled() {
        // GIVEN: audio feature is active
        let list = static_resources();
        // THEN: audio/devices resource is advertised
        let has_audio = list
            .resources
            .iter()
            .any(|r| r.uri == "axterminator://audio/devices");
        assert!(
            has_audio,
            "audio/devices must be in static resource list with audio feature"
        );
    }

    #[cfg(feature = "audio")]
    #[test]
    fn read_audio_devices_returns_valid_json() {
        // GIVEN: running macOS system
        let registry = Arc::new(AppRegistry::default());
        // WHEN: reading the audio devices resource
        let result = read_resource("axterminator://audio/devices", &registry)
            .expect("audio/devices resource must succeed");
        // THEN: one JSON content item
        assert_eq!(result.contents.len(), 1);
        assert_eq!(result.contents[0].mime_type, "application/json");
        let text = result.contents[0].text.as_ref().unwrap();
        let v: serde_json::Value = serde_json::from_str(text).unwrap();
        assert!(v["device_count"].is_number());
        assert!(v["devices"].is_array());
    }

    #[cfg(feature = "audio")]
    #[test]
    fn read_audio_devices_device_count_matches_array_length() {
        let registry = Arc::new(AppRegistry::default());
        let result = read_resource("axterminator://audio/devices", &registry).unwrap();
        let text = result.contents[0].text.as_ref().unwrap();
        let v: serde_json::Value = serde_json::from_str(text).unwrap();
        let count = v["device_count"].as_u64().unwrap();
        let arr_len = v["devices"].as_array().unwrap().len() as u64;
        assert_eq!(
            count, arr_len,
            "device_count must match devices array length"
        );
    }

    #[cfg(feature = "audio")]
    #[test]
    fn read_audio_devices_mime_type_is_application_json() {
        let registry = Arc::new(AppRegistry::default());
        let result = read_resource("axterminator://audio/devices", &registry).unwrap();
        assert_eq!(result.contents[0].mime_type, "application/json");
    }

    // -----------------------------------------------------------------------
    // Camera resource tests (feature-gated)
    // -----------------------------------------------------------------------

    #[cfg(feature = "camera")]
    #[test]
    fn static_resources_contains_camera_devices_when_feature_enabled() {
        // GIVEN: camera feature enabled
        // WHEN: static_resources() is called
        let list = static_resources();
        // THEN: camera/devices resource is included
        let has_camera = list
            .resources
            .iter()
            .any(|r| r.uri == "axterminator://camera/devices");
        assert!(has_camera, "camera/devices must be advertised");
    }

    #[cfg(feature = "camera")]
    #[test]
    fn read_camera_devices_returns_valid_json() {
        // GIVEN: an empty registry (device listing needs no connected apps)
        let registry = Arc::new(AppRegistry::default());
        // WHEN: reading the camera devices resource
        let result = read_resource("axterminator://camera/devices", &registry).unwrap();
        // THEN: one JSON item with a "cameras" array
        assert_eq!(result.contents.len(), 1);
        let text = result.contents[0].text.as_ref().unwrap();
        let v: serde_json::Value = serde_json::from_str(text).unwrap();
        assert!(v["cameras"].is_array());
    }

    #[cfg(feature = "camera")]
    #[test]
    fn read_camera_devices_mime_type_is_json() {
        let registry = Arc::new(AppRegistry::default());
        let result = read_resource("axterminator://camera/devices", &registry).unwrap();
        assert_eq!(result.contents[0].mime_type, "application/json");
    }
}
