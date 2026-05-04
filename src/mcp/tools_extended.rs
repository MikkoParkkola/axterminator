//! Phase 3 MCP tool declarations and dispatcher.
//!
//! This module re-exports tool registrations from focused sub-modules and
//! provides the two public entry points consumed by the server:
//!
//! - [`extended_tools`] — returns all Phase 3 `Tool` descriptors.
//! - [`call_tool_extended`] — dispatches a tool call by name.
//!
//! ## Sub-modules
//!
//! | Module | Contents |
//! |--------|---------|
//! | [`tools_gui`]    | scroll, key_press, get_attributes, get_tree, list_apps, drag, assert |
//! | [`tools_spaces`] | ax_list_spaces, ax_create_space, ax_move_to_space, ax_switch_space, ax_destroy_space (feature `spaces`) |
//! | [`tools_audio`]  | ax_listen, ax_speak, ax_audio_voices, ax_audio_devices (feature `audio`) |
//! | [`tools_camera`] | ax_camera_capture, ax_gesture_detect, ax_gesture_listen (feature `camera`) |
//!
//! # Progress notifications
//!
//! `ax_get_tree` emits depth-layer progress when the requested depth is ≥ 2.
//! Both use [`crate::mcp::progress::ProgressReporter`] to keep token generation
//! central and collision-free.

use std::io::Write;
use std::sync::Arc;

use serde_json::Value;

use crate::mcp::protocol::{Tool, ToolCallResult};
use crate::mcp::tools::AppRegistry;

// Re-export public camera helpers used by resources.rs.
#[cfg(feature = "camera")]
pub use crate::mcp::tools_camera::camera_devices_payload;
#[cfg(feature = "camera")]
pub use crate::mcp::tools_camera::camera_tools;

// Re-export public spaces helpers used by external callers.
#[cfg(feature = "spaces")]
pub use crate::mcp::tools_spaces::spaces_tools;

// Re-export watch tools and state.
#[cfg(feature = "watch")]
pub use crate::mcp::tools_watch::{WatchState, watch_tools};

// Re-export docker browser tools.
#[cfg(feature = "docker")]
pub use crate::mcp::tools_docker::docker_tools;

// ---------------------------------------------------------------------------
// Tool registry
// ---------------------------------------------------------------------------

/// All Phase 3 tools in registration order.
#[must_use]
pub fn extended_tools() -> Vec<Tool> {
    // `mut` required when any feature-gated tool sets (spaces, audio, camera, watch) are enabled.
    #[allow(unused_mut)]
    let mut tools = vec![
        crate::mcp::tools_gui::tool_ax_scroll(),
        crate::mcp::tools_gui::tool_ax_key_press(),
        crate::mcp::tools_gui::tool_ax_get_attributes(),
        crate::mcp::tools_gui::tool_ax_get_tree(),
        crate::mcp::tools_gui::tool_ax_list_apps(),
        crate::mcp::tools_gui::tool_ax_drag(),
        crate::mcp::tools_gui::tool_ax_assert(),
    ];
    #[cfg(feature = "spaces")]
    tools.extend(spaces_tools());
    #[cfg(feature = "audio")]
    tools.extend(crate::mcp::tools_audio::audio_tools());
    #[cfg(feature = "audio")]
    tools.extend(crate::mcp::tools_capture::capture_tools());
    #[cfg(feature = "camera")]
    tools.extend(camera_tools());
    #[cfg(feature = "watch")]
    tools.extend(watch_tools());
    #[cfg(feature = "docker")]
    tools.extend(docker_tools());
    tools.extend(crate::mcp::tools_context::context_tools());
    tools.extend(crate::mcp::tools_innovation::innovation_tools());
    tools
}

// ---------------------------------------------------------------------------
// Dispatcher
// ---------------------------------------------------------------------------

/// Dispatch a Phase 3 tool call, emitting optional progress to `out`.
///
/// Returns `None` when `name` is not a Phase 3 tool — the caller falls
/// through to the Phase 1 dispatch table.
///
/// # Errors
///
/// `out` I/O errors from progress notifications are silently ignored to
/// avoid masking the tool result.  This matches the MCP convention that
/// notification delivery is best-effort.
pub fn call_tool_extended<W: Write>(
    name: &str,
    args: &Value,
    registry: &Arc<AppRegistry>,
    out: &mut W,
) -> Option<ToolCallResult> {
    match name {
        "ax_scroll" => Some(crate::mcp::tools_gui::handle_scroll(args, registry)),
        "ax_key_press" => Some(crate::mcp::tools_gui::handle_key_press(args, registry)),
        "ax_get_attributes" => Some(crate::mcp::tools_gui::handle_get_attributes(args, registry)),
        "ax_get_tree" => Some(crate::mcp::tools_gui::handle_get_tree(args, registry, out)),
        "ax_list_apps" => Some(crate::mcp::tools_gui::handle_list_apps()),
        "ax_drag" => Some(crate::mcp::tools_gui::handle_drag(args, registry)),
        "ax_assert" => Some(crate::mcp::tools_gui::handle_assert(args, registry)),
        #[cfg(feature = "spaces")]
        "ax_list_spaces" => Some(crate::mcp::tools_spaces::handle_ax_list_spaces()),
        #[cfg(feature = "spaces")]
        "ax_create_space" => Some(crate::mcp::tools_spaces::handle_ax_create_space()),
        #[cfg(feature = "spaces")]
        "ax_move_to_space" => Some(crate::mcp::tools_spaces::handle_ax_move_to_space(
            args, registry,
        )),
        #[cfg(feature = "spaces")]
        "ax_switch_space" => Some(crate::mcp::tools_spaces::handle_ax_switch_space(args)),
        #[cfg(feature = "spaces")]
        "ax_destroy_space" => Some(crate::mcp::tools_spaces::handle_ax_destroy_space(args)),
        #[cfg(feature = "audio")]
        "ax_listen" => Some(crate::mcp::tools_audio::handle_ax_listen(args)),
        #[cfg(feature = "audio")]
        "ax_speak" => Some(crate::mcp::tools_audio::handle_ax_speak(args)),
        #[cfg(feature = "audio")]
        "ax_audio_voices" => Some(crate::mcp::tools_audio::handle_ax_audio_voices()),
        #[cfg(feature = "audio")]
        "ax_audio_devices" => Some(crate::mcp::tools_audio::handle_ax_audio_devices()),
        #[cfg(feature = "audio")]
        "ax_start_capture" => Some(crate::mcp::tools_capture::handle_ax_start_capture(args)),
        #[cfg(feature = "audio")]
        "ax_stop_capture" => Some(crate::mcp::tools_capture::handle_ax_stop_capture(args)),
        #[cfg(feature = "audio")]
        "ax_get_transcription" => {
            Some(crate::mcp::tools_capture::handle_ax_get_transcription(args))
        }
        #[cfg(feature = "audio")]
        "ax_capture_status" => Some(crate::mcp::tools_capture::handle_ax_capture_status()),
        #[cfg(feature = "camera")]
        "ax_camera_capture" => Some(crate::mcp::tools_camera::handle_ax_camera_capture(args)),
        #[cfg(feature = "camera")]
        "ax_gesture_detect" => Some(crate::mcp::tools_camera::handle_ax_gesture_detect(args)),
        #[cfg(feature = "camera")]
        "ax_gesture_listen" => Some(crate::mcp::tools_camera::handle_ax_gesture_listen(args)),
        #[cfg(feature = "docker")]
        "ax_browser_launch" => Some(crate::mcp::tools_docker::handle_ax_browser_launch(args)),
        #[cfg(feature = "docker")]
        "ax_browser_stop" => Some(crate::mcp::tools_docker::handle_ax_browser_stop(args)),
        "ax_system_context" => Some(crate::mcp::tools_context::handle_ax_system_context()),
        #[cfg(feature = "context")]
        "ax_location" => Some(crate::mcp::tools_context::handle_ax_location(args)),
        // Watch tools require a WatchState which is not available in the stateless
        // extended dispatcher.  These are dispatched by the server's handle_tools_call
        // via the Server::call_watch_tool helper instead.
        _ => {
            if let Some(result) =
                crate::mcp::tools_innovation::call_tool_innovation(name, args, registry, out)
            {
                return Some(result);
            }
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Tests — Phase 3 public API + GUI helpers
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use crate::mcp::tools::AppRegistry;
    use crate::mcp::tools_gui::{
        extract_app_query, key_name_to_code, list_running_apps, scroll_deltas,
    };

    // -----------------------------------------------------------------------
    // Tool registry
    // -----------------------------------------------------------------------

    #[test]
    fn extended_tools_count_matches_feature_set() {
        // GIVEN: Phase 3 GUI base (7) + context (2-3) + innovation (15) = 24-25 + optional feature extensions
        // WHEN: requesting extended tools
        let tools = super::extended_tools();
        // THEN: count is deterministic per feature set
        let base = 22usize; // Phase 3 GUI (7) + innovation (15)
        let context_base = 1usize; // system_context (always on); clipboard is in innovation
        let extra_context_location: usize = if cfg!(feature = "context") { 1 } else { 0 };
        let extra_spaces: usize = if cfg!(feature = "spaces") { 5 } else { 0 };
        // audio feature: ax_listen + ax_speak + ax_audio_voices + ax_audio_devices (4)
        // + capture tools (4) = 8
        let extra_audio: usize = if cfg!(feature = "audio") { 8 } else { 0 };
        let extra_camera: usize = if cfg!(feature = "camera") { 3 } else { 0 };
        let extra_watch: usize = if cfg!(feature = "watch") { 3 } else { 0 };
        let extra_docker: usize = if cfg!(feature = "docker") { 2 } else { 0 };
        assert_eq!(
            tools.len(),
            base + context_base
                + extra_context_location
                + extra_spaces
                + extra_audio
                + extra_camera
                + extra_watch
                + extra_docker
        );
    }

    #[test]
    fn all_extended_tool_names_are_unique() {
        let tools = super::extended_tools();
        let names: std::collections::HashSet<&str> = tools.iter().map(|t| t.name).collect();
        assert_eq!(names.len(), tools.len(), "duplicate tool names in Phase 3");
    }

    #[test]
    fn all_extended_tools_have_non_empty_descriptions() {
        for tool in super::extended_tools() {
            assert!(
                !tool.description.is_empty(),
                "empty description on {}",
                tool.name
            );
        }
    }

    #[test]
    fn all_extended_tools_have_annotations() {
        for tool in super::extended_tools() {
            let ann = &tool.annotations;
            let _ = ann.read_only;
            let _ = ann.destructive;
            let _ = ann.idempotent;
            let _ = ann.open_world;
        }
    }

    // -----------------------------------------------------------------------
    // call_tool_extended dispatch
    // -----------------------------------------------------------------------

    #[test]
    fn call_tool_extended_unknown_name_returns_none() {
        // GIVEN: name not in Phase 3 set
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        // WHEN: dispatching
        let result =
            super::call_tool_extended("ax_nonexistent_phase3", &json!({}), &registry, &mut out);
        // THEN: falls through (None)
        assert!(result.is_none());
    }

    #[test]
    fn call_tool_extended_list_apps_always_succeeds() {
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        let result =
            super::call_tool_extended("ax_list_apps", &json!({}), &registry, &mut out).unwrap();
        assert!(!result.is_error, "ax_list_apps should not error");
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert!(v["apps"].is_array(), "apps field must be an array");
    }

    #[test]
    fn call_tool_extended_scroll_missing_app_returns_error() {
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        let result = super::call_tool_extended(
            "ax_scroll",
            &json!({"direction": "down"}),
            &registry,
            &mut out,
        )
        .unwrap();
        assert!(result.is_error);
        assert!(result.content[0].text.contains("Missing"));
    }

    #[test]
    fn call_tool_extended_scroll_missing_direction_returns_error() {
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        let result =
            super::call_tool_extended("ax_scroll", &json!({"app": "Finder"}), &registry, &mut out)
                .unwrap();
        assert!(result.is_error);
        assert!(result.content[0].text.contains("Missing"));
    }

    #[test]
    fn call_tool_extended_scroll_unconnected_app_returns_error() {
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        let result = super::call_tool_extended(
            "ax_scroll",
            &json!({"app": "Ghost", "direction": "down"}),
            &registry,
            &mut out,
        )
        .unwrap();
        assert!(result.is_error);
        assert!(result.content[0].text.contains("not connected"));
    }

    #[test]
    fn call_tool_extended_key_press_missing_app_returns_error() {
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        let result = super::call_tool_extended(
            "ax_key_press",
            &json!({"keys": "cmd+s"}),
            &registry,
            &mut out,
        )
        .unwrap();
        assert!(result.is_error);
    }

    #[test]
    fn call_tool_extended_key_press_missing_keys_returns_error() {
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        let result = super::call_tool_extended(
            "ax_key_press",
            &json!({"app": "Safari"}),
            &registry,
            &mut out,
        )
        .unwrap();
        assert!(result.is_error);
    }

    #[test]
    fn call_tool_extended_get_attributes_missing_query_returns_error() {
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        let result = super::call_tool_extended(
            "ax_get_attributes",
            &json!({"app": "Finder"}),
            &registry,
            &mut out,
        )
        .unwrap();
        assert!(result.is_error);
    }

    #[test]
    fn call_tool_extended_get_tree_missing_app_returns_error() {
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        let result =
            super::call_tool_extended("ax_get_tree", &json!({}), &registry, &mut out).unwrap();
        assert!(result.is_error);
    }

    // -----------------------------------------------------------------------
    // ax_get_tree compact mode
    // -----------------------------------------------------------------------

    #[test]
    fn get_tree_compact_unconnected_app_returns_error() {
        // GIVEN: no app connected under alias "Ghost"
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        // WHEN: compact mode requested for an unconnected app
        let result = super::call_tool_extended(
            "ax_get_tree",
            &json!({"app": "Ghost", "compact": true}),
            &registry,
            &mut out,
        )
        .unwrap();
        // THEN: error returned (not connected)
        assert!(result.is_error, "should error for unconnected app");
        assert!(result.content[0].text.contains("not connected"));
    }

    #[test]
    fn get_tree_compact_false_behaves_like_default() {
        // GIVEN: compact explicitly set to false, app not connected
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        // WHEN: dispatching with compact: false
        let result = super::call_tool_extended(
            "ax_get_tree",
            &json!({"app": "Ghost", "compact": false}),
            &registry,
            &mut out,
        )
        .unwrap();
        // THEN: falls through to normal path → same "not connected" error
        assert!(result.is_error);
        assert!(result.content[0].text.contains("not connected"));
    }

    #[test]
    fn get_tree_compact_schema_includes_compact_and_max_depth_params() {
        // GIVEN: the tool declaration
        let tools = super::extended_tools();
        let tree_tool = tools.iter().find(|t| t.name == "ax_get_tree").unwrap();
        // WHEN: inspecting the input schema
        let props = &tree_tool.input_schema["properties"];
        // THEN: compact and max_depth are present
        assert!(
            props["compact"].is_object(),
            "compact property must be in schema"
        );
        assert_eq!(
            props["compact"]["type"].as_str(),
            Some("boolean"),
            "compact must be boolean"
        );
        assert!(
            props["max_depth"].is_object(),
            "max_depth property must be in schema"
        );
        assert_eq!(
            props["max_depth"]["type"].as_str(),
            Some("integer"),
            "max_depth must be integer"
        );
    }

    #[test]
    fn get_tree_compact_output_schema_includes_metadata_fields() {
        // GIVEN: the tool declaration
        let tools = super::extended_tools();
        let tree_tool = tools.iter().find(|t| t.name == "ax_get_tree").unwrap();
        // WHEN: inspecting the output schema
        let out_props = &tree_tool.output_schema["properties"];
        // THEN: element_count and total_scanned are declared
        assert!(
            out_props["element_count"].is_object(),
            "output schema must include element_count"
        );
        assert!(
            out_props["total_scanned"].is_object(),
            "output schema must include total_scanned"
        );
    }

    #[test]
    fn call_tool_extended_drag_missing_from_query_returns_error() {
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        let result = super::call_tool_extended(
            "ax_drag",
            &json!({"app": "Finder", "to_query": "Desktop"}),
            &registry,
            &mut out,
        )
        .unwrap();
        assert!(result.is_error);
    }

    #[test]
    fn call_tool_extended_assert_missing_property_returns_error() {
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        let result = super::call_tool_extended(
            "ax_assert",
            &json!({"app": "Finder", "query": "Save", "expected": "true"}),
            &registry,
            &mut out,
        )
        .unwrap();
        assert!(result.is_error);
    }

    #[test]
    fn call_tool_extended_assert_unconnected_app_returns_error() {
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        let result = super::call_tool_extended(
            "ax_assert",
            &json!({"app": "Ghost", "query": "Save", "property": "exists", "expected": "false"}),
            &registry,
            &mut out,
        )
        .unwrap();
        assert!(result.is_error);
        assert!(result.content[0].text.contains("not connected"));
    }

    // -----------------------------------------------------------------------
    // key_name_to_code
    // -----------------------------------------------------------------------

    #[test]
    fn key_name_to_code_letters_are_mapped() {
        assert_eq!(key_name_to_code("a"), Some(0));
        assert_eq!(key_name_to_code("s"), Some(1));
        assert_eq!(key_name_to_code("z"), Some(6));
    }

    #[test]
    fn key_name_to_code_is_case_insensitive() {
        assert_eq!(key_name_to_code("A"), key_name_to_code("a"));
        assert_eq!(key_name_to_code("ENTER"), key_name_to_code("enter"));
    }

    #[test]
    fn key_name_to_code_function_keys_are_mapped() {
        assert!(key_name_to_code("f1").is_some());
        assert!(key_name_to_code("f12").is_some());
        assert!(key_name_to_code("f20").is_some());
    }

    #[test]
    fn key_name_to_code_unknown_key_returns_none() {
        assert!(key_name_to_code("nonsense").is_none());
        assert!(key_name_to_code("").is_none());
    }

    #[test]
    fn key_name_to_code_navigation_keys_are_mapped() {
        assert!(key_name_to_code("up").is_some());
        assert!(key_name_to_code("down").is_some());
        assert!(key_name_to_code("left").is_some());
        assert!(key_name_to_code("right").is_some());
        assert!(key_name_to_code("enter").is_some());
        assert!(key_name_to_code("tab").is_some());
        assert!(key_name_to_code("escape").is_some());
    }

    // -----------------------------------------------------------------------
    // scroll_deltas
    // -----------------------------------------------------------------------

    #[test]
    fn scroll_deltas_down_is_negative_y() {
        let (dx, dy) = scroll_deltas("down", 3);
        assert_eq!(dx, 0);
        assert_eq!(dy, -3);
    }

    #[test]
    fn scroll_deltas_up_is_positive_y() {
        let (dx, dy) = scroll_deltas("up", 5);
        assert_eq!(dx, 0);
        assert_eq!(dy, 5);
    }

    #[test]
    fn scroll_deltas_left_is_negative_x() {
        let (dx, dy) = scroll_deltas("left", 2);
        assert_eq!(dx, -2);
        assert_eq!(dy, 0);
    }

    #[test]
    fn scroll_deltas_right_is_positive_x() {
        let (dx, dy) = scroll_deltas("right", 4);
        assert_eq!(dx, 4);
        assert_eq!(dy, 0);
    }

    // -----------------------------------------------------------------------
    // list_running_apps
    // -----------------------------------------------------------------------

    #[test]
    fn list_running_apps_returns_non_empty_list() {
        let apps = list_running_apps();
        assert!(!apps.is_empty(), "expected at least one running process");
    }

    #[test]
    fn list_running_apps_all_have_name_and_pid() {
        for app in list_running_apps() {
            assert!(app["name"].is_string(), "name must be string: {app}");
            assert!(app["pid"].is_number(), "pid must be number: {app}");
        }
    }

    #[test]
    fn list_running_apps_is_sorted_by_name() {
        let apps = list_running_apps();
        let names: Vec<&str> = apps.iter().map(|a| a["name"].as_str().unwrap()).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted, "apps should be sorted by name");
    }

    // -----------------------------------------------------------------------
    // extract_app_query
    // -----------------------------------------------------------------------

    #[test]
    fn extract_app_query_succeeds_with_both_fields() {
        let args = json!({"app": "Safari", "query": "Load"});
        let (app, query) = extract_app_query(&args).unwrap();
        assert_eq!(app, "Safari");
        assert_eq!(query, "Load");
    }

    #[test]
    fn extract_app_query_fails_when_app_missing() {
        let args = json!({"query": "Load"});
        assert!(extract_app_query(&args).is_err());
    }

    #[test]
    fn extract_app_query_fails_when_query_missing() {
        let args = json!({"app": "Safari"});
        assert!(extract_app_query(&args).is_err());
    }
}
