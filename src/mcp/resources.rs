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
//! | `axterminator://apps` | Running apps with PID, bundle ID, accessibility status |
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

use std::sync::Arc;

use base64::Engine as _;
use serde_json::{json, Value};
use tracing::debug;

use crate::mcp::protocol::{
    Resource, ResourceContents, ResourceListResult, ResourceReadResult, ResourceTemplate,
    ResourceTemplateListResult,
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
    fn not_connected(app: &str) -> Self {
        Self {
            code: "not_connected",
            message: format!("App '{app}' not connected — call ax_connect first"),
        }
    }

    fn invalid_uri(uri: &str) -> Self {
        Self {
            code: "invalid_uri",
            message: format!("Cannot parse app name from URI: {uri}"),
        }
    }

    fn operation_failed(detail: impl Into<String>) -> Self {
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
/// ```
#[must_use]
pub fn static_resources() -> ResourceListResult {
    ResourceListResult {
        resources: vec![
            Resource {
                uri: "axterminator://system/status",
                name: "system-status",
                title: "System Accessibility Status",
                description: "Accessibility permissions, connected apps count, server version.",
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
        ],
    }
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
    debug!(uri, "reading resource");

    match uri {
        "axterminator://system/status" => read_system_status(uri, registry),
        "axterminator://apps" => read_running_apps(uri, registry),
        other => read_dynamic(other, registry),
    }
}

// ---------------------------------------------------------------------------
// Static resource handlers
// ---------------------------------------------------------------------------

fn read_system_status(
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

fn read_running_apps(
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

// ---------------------------------------------------------------------------
// Dynamic resource handlers
// ---------------------------------------------------------------------------

fn read_dynamic(
    uri: &str,
    registry: &Arc<AppRegistry>,
) -> Result<ResourceReadResult, ResourceError> {
    // Expected pattern: axterminator://app/{name}/{resource}
    let name = parse_app_name(uri)?;

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
    registry
        .with_app(app_name, |app| {
            let windows = app
                .windows_native()
                .unwrap_or_default()
                .iter()
                .map(|w| {
                    json!({
                        "title": w.title(),
                        "role": w.role(),
                    })
                })
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
}
