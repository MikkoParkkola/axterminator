//! MCP tools for the continuous watch system (requires `watch` feature).
//!
//! | Tool | Purpose |
//! |------|---------|
//! | `ax_watch_start`  | Start continuous monitoring (audio, camera, or both) |
//! | `ax_watch_stop`   | Stop all active watchers |
//! | `ax_watch_status` | Show whether watchers are running |
//!
//! Events emitted by the watchers are delivered as `notifications/claude/channel`
//! JSON-RPC notifications.  The server loop drives this delivery; these tools
//! only manage watcher lifecycle.

use serde_json::{json, Value};

use crate::mcp::annotations;
use crate::mcp::args::{extract_or_return, reject_unknown_fields};
use crate::mcp::protocol::{Tool, ToolCallResult};

// ---------------------------------------------------------------------------
// Tool declarations
// ---------------------------------------------------------------------------

/// All watch tools registered when the `watch` feature is active.
#[cfg(feature = "watch")]
#[must_use]
pub fn watch_tools() -> Vec<Tool> {
    vec![
        tool_ax_watch_start(),
        tool_ax_watch_stop(),
        tool_ax_watch_status(),
    ]
}

#[cfg(feature = "watch")]
fn tool_ax_watch_start() -> Tool {
    Tool {
        name: "ax_watch_start",
        title: "Start continuous audio/camera monitoring",
        description: "Begin background monitoring that pushes events to Claude Code via \
            notifications/claude/channel.\n\
            \n\
            Audio monitoring: captures 5-second windows, applies voice activity detection \
            (−40 dBFS threshold), transcribes speech on-device via SFSpeechRecognizer \
            (macOS, no cloud), and emits [speech detected] notifications.\n\
            \n\
            Camera monitoring: captures one frame every 2 seconds, detects motion via JPEG \
            size heuristic, runs Vision gesture detection when motion is found, and emits \
            [gesture detected] notifications.  The camera indicator light is ON during captures.\n\
            \n\
            Memory: at most ~2 MB of binary data in RAM at any time (one audio window + two \
            camera frames).  Events are small text strings (bounded channel of 100).\n\
            \n\
            Call ax_watch_stop to halt monitoring.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "audio": {
                    "type": "boolean",
                    "description": "Enable audio capture and speech transcription (default false)",
                    "default": false
                },
                "camera": {
                    "type": "boolean",
                    "description": "Enable camera capture and gesture detection (default false)",
                    "default": false
                },
                "vad_threshold_db": {
                    "type": "number",
                    "description": "Voice activity threshold in dBFS. Audio below this is ignored. Default −40.0.",
                    "default": -40.0,
                    "minimum": -96.0,
                    "maximum": 0.0
                },
                "camera_interval_ms": {
                    "type": "integer",
                    "description": "Milliseconds between camera frame captures. Default 2000.",
                    "default": 2000,
                    "minimum": 500,
                    "maximum": 30000
                }
            },
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "started":         { "type": "boolean" },
                "audio_enabled":   { "type": "boolean" },
                "camera_enabled":  { "type": "boolean" },
                "message":         { "type": "string" }
            },
            "required": ["started", "audio_enabled", "camera_enabled"]
        }),
        annotations: annotations::ACTION,
    }
}

#[cfg(feature = "watch")]
fn tool_ax_watch_stop() -> Tool {
    Tool {
        name: "ax_watch_stop",
        title: "Stop all active watchers",
        description: "Stop the background audio and camera watchers started by ax_watch_start. \
            All in-flight captures complete before the tasks exit — no data is lost mid-window.\n\
            Safe to call even if no watchers are running (no-op).",
        input_schema: json!({
            "type": "object",
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "stopped": { "type": "boolean" },
                "message": { "type": "string" }
            },
            "required": ["stopped"]
        }),
        annotations: annotations::ACTION,
    }
}

#[cfg(feature = "watch")]
fn tool_ax_watch_status() -> Tool {
    Tool {
        name: "ax_watch_status",
        title: "Check watch monitoring status",
        description: "Return whether the audio and camera watchers are currently running.",
        input_schema: json!({
            "type": "object",
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "audio_running":  { "type": "boolean" },
                "camera_running": { "type": "boolean" }
            },
            "required": ["audio_running", "camera_running"]
        }),
        annotations: annotations::READ_ONLY,
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// Handle `ax_watch_start` — start watchers, return immediately.
///
/// The watcher state is stored in the [`WatchState`] extracted from the
/// server's shared state.  The caller is responsible for routing events
/// from the channel to MCP notifications.
#[cfg(feature = "watch")]
pub(crate) fn handle_ax_watch_start(args: &Value, state: &WatchState) -> ToolCallResult {
    extract_or_return!(reject_unknown_fields(
        args,
        &["audio", "camera", "vad_threshold_db", "camera_interval_ms"]
    ));
    let audio = extract_or_return!(parse_optional_bool_field(args, "audio", false));
    let camera = extract_or_return!(parse_optional_bool_field(args, "camera", false));

    if !audio && !camera {
        return ToolCallResult::error("At least one of 'audio' or 'camera' must be true");
    }

    let vad_threshold_db = extract_or_return!(parse_optional_f64_in_range(
        args,
        "vad_threshold_db",
        -40.0,
        -96.0,
        0.0
    )) as f32;
    let camera_interval_ms = extract_or_return!(parse_optional_u64_in_range(
        args,
        "camera_interval_ms",
        2000,
        500,
        30000
    ));

    let config = crate::watch::WatchConfig {
        audio_enabled: audio,
        camera_enabled: camera,
        audio_vad_threshold_db: vad_threshold_db,
        camera_poll_interval_ms: camera_interval_ms,
        ..crate::watch::WatchConfig::default()
    };

    let _rx = state.start(config);
    // The event receiver is stored in WatchState::pending_rx and will be
    // retrieved by the server's stdio loop via take_pending_receiver().

    ToolCallResult::ok(
        json!({
            "started":        true,
            "audio_enabled":  audio,
            "camera_enabled": camera,
            "message": format!(
                "Watch started: audio={audio}, camera={camera}, \
                 vad={vad_threshold_db:.1} dB, camera_interval={camera_interval_ms} ms"
            )
        })
        .to_string(),
    )
}

/// Handle `ax_watch_stop`.
#[cfg(feature = "watch")]
pub(crate) fn handle_ax_watch_stop(args: &Value, state: &WatchState) -> ToolCallResult {
    extract_or_return!(reject_unknown_fields(args, &[]));
    state.stop();
    ToolCallResult::ok(json!({ "stopped": true, "message": "All watchers stopped" }).to_string())
}

/// Handle `ax_watch_status`.
#[cfg(feature = "watch")]
pub(crate) fn handle_ax_watch_status(args: &Value, state: &WatchState) -> ToolCallResult {
    extract_or_return!(reject_unknown_fields(args, &[]));
    let status = state.status();
    ToolCallResult::ok(
        json!({
            "audio_running":  status.audio_running,
            "camera_running": status.camera_running,
        })
        .to_string(),
    )
}

#[cfg(feature = "watch")]
fn parse_optional_bool_field(args: &Value, field: &str, default: bool) -> Result<bool, String> {
    match args.get(field) {
        None => Ok(default),
        Some(value) => value
            .as_bool()
            .ok_or_else(|| format!("Field '{field}' must be a boolean")),
    }
}

#[cfg(feature = "watch")]
fn parse_optional_f64_in_range(
    args: &Value,
    field: &str,
    default: f64,
    min: f64,
    max: f64,
) -> Result<f64, String> {
    let value = match args.get(field) {
        None => return Ok(default),
        Some(value) => value
            .as_f64()
            .ok_or_else(|| format!("Field '{field}' must be a number"))?,
    };

    if !(min..=max).contains(&value) {
        return Err(format!("Field '{field}' must be between {min} and {max}"));
    }

    Ok(value)
}

#[cfg(feature = "watch")]
fn parse_optional_u64_in_range(
    args: &Value,
    field: &str,
    default: u64,
    min: u64,
    max: u64,
) -> Result<u64, String> {
    let value = match args.get(field) {
        None => return Ok(default),
        Some(value) => value
            .as_u64()
            .ok_or_else(|| format!("Field '{field}' must be an integer"))?,
    };

    if !(min..=max).contains(&value) {
        return Err(format!("Field '{field}' must be between {min} and {max}"));
    }

    Ok(value)
}

// ---------------------------------------------------------------------------
// WatchState — shared server-side coordinator handle
// ---------------------------------------------------------------------------

/// Thread-safe container for the optional active [`crate::watch::WatchCoordinator`].
///
/// The MCP server holds one `WatchState` instance.  Tool handlers call
/// `start`, `stop`, and `status` without knowing about the underlying
/// task topology.
#[cfg(feature = "watch")]
pub struct WatchState {
    inner: std::sync::Mutex<WatchStateInner>,
}

#[cfg(feature = "watch")]
struct WatchStateInner {
    coordinator: Option<crate::watch::WatchCoordinator>,
    /// Pending event receiver created by the most recent `start()` call.
    /// Taken once by the server's stdio loop via `take_pending_receiver()`.
    pending_rx: Option<tokio::sync::mpsc::Receiver<crate::watch::WatchEvent>>,
}

#[cfg(feature = "watch")]
impl WatchState {
    /// Create a dormant (no watchers running) state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: std::sync::Mutex::new(WatchStateInner {
                coordinator: None,
                pending_rx: None,
            }),
        }
    }

    /// Start watchers with the given config, stopping any previously running ones.
    ///
    /// Returns the new event receiver.  The caller forwards events to the MCP
    /// notification emit loop.
    pub fn start(
        &self,
        config: crate::watch::WatchConfig,
    ) -> tokio::sync::mpsc::Receiver<crate::watch::WatchEvent> {
        let mut guard = self.inner.lock().expect("watch state lock poisoned");
        // Stop previous coordinator synchronously by dropping it.
        // The cancellation token fires when the coordinator is dropped.
        if let Some(old) = guard.coordinator.take() {
            // Signal cancellation; handles are abandoned (will complete independently).
            // We do not await them here to keep this call synchronous.
            old.cancel.cancel();
        }
        let (coordinator, event_rx) = crate::watch::WatchCoordinator::start(config);
        guard.coordinator = Some(coordinator);
        guard.pending_rx = Some(event_rx);
        // Return a clone of the sender side isn't possible; the receiver is
        // stored and vended via take_pending_receiver() by the server loop.
        // We return a fresh dummy channel here — callers should use
        // take_pending_receiver() instead.
        let (_, dummy_rx) = tokio::sync::mpsc::channel(1);
        dummy_rx
    }

    /// Take the event receiver created by the most recent `start()` call.
    ///
    /// Returns `Some` exactly once after each `start()`.  The server stdio
    /// loop calls this to wire the receiver into its notification drain loop.
    pub fn take_pending_receiver(
        &self,
    ) -> Option<tokio::sync::mpsc::Receiver<crate::watch::WatchEvent>> {
        self.inner
            .lock()
            .expect("watch state lock poisoned")
            .pending_rx
            .take()
    }

    /// Signal all watchers to stop (non-blocking).
    pub fn stop(&self) {
        let mut guard = self.inner.lock().expect("watch state lock poisoned");
        if let Some(coord) = guard.coordinator.take() {
            coord.cancel.cancel();
        }
    }

    /// Return a status snapshot.
    #[must_use]
    pub fn status(&self) -> crate::watch::WatchStatus {
        let guard = self.inner.lock().expect("watch state lock poisoned");
        guard.coordinator.as_ref().map_or(
            crate::watch::WatchStatus {
                audio_running: false,
                camera_running: false,
            },
            |c| c.status(),
        )
    }
}

#[cfg(feature = "watch")]
impl Default for WatchState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(all(test, feature = "watch"))]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn watch_tools_returns_three_tools() {
        let tools = watch_tools();
        assert_eq!(tools.len(), 3);
        let names: Vec<&str> = tools.iter().map(|t| t.name).collect();
        assert!(names.contains(&"ax_watch_start"));
        assert!(names.contains(&"ax_watch_stop"));
        assert!(names.contains(&"ax_watch_status"));
    }

    #[test]
    fn watch_tools_all_have_non_empty_descriptions() {
        for tool in watch_tools() {
            assert!(
                !tool.description.is_empty(),
                "empty description on {}",
                tool.name
            );
        }
    }

    #[test]
    fn watch_tools_names_are_unique() {
        let tools = watch_tools();
        crate::mcp::test_support::assert_tool_names_unique(&tools, "watch tools");
    }

    #[test]
    fn ax_watch_start_requires_at_least_one_sensor() {
        // GIVEN: both audio and camera false
        let state = WatchState::new();
        let args = json!({ "audio": false, "camera": false });
        // WHEN: dispatched
        let result = handle_ax_watch_start(&args, &state);
        // THEN: error returned
        assert!(result.is_error);
        assert!(result.content[0].text.contains("audio"));
    }

    #[tokio::test]
    async fn ax_watch_stop_no_op_when_nothing_running() {
        // GIVEN: fresh state with no watchers
        let state = WatchState::new();
        // WHEN: stop called
        let result = handle_ax_watch_stop(&json!({}), &state);
        // THEN: succeeds silently
        assert!(!result.is_error);
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["stopped"], true);
    }

    #[tokio::test]
    async fn ax_watch_status_reports_nothing_running_initially() {
        let state = WatchState::new();
        let result = handle_ax_watch_status(&json!({}), &state);
        assert!(!result.is_error);
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["audio_running"], false);
        assert_eq!(v["camera_running"], false);
    }

    #[tokio::test]
    async fn watch_state_start_stop_roundtrip() {
        // GIVEN: state with audio enabled (no real mic needed — just tests lifecycle)
        let state = WatchState::new();
        let config = crate::watch::WatchConfig {
            audio_enabled: false, // off: no TCC dialogs in tests
            camera_enabled: false,
            ..crate::watch::WatchConfig::default()
        };
        let _rx = state.start(config);
        // Coordinator with no tasks should report not running
        let status = state.status();
        assert!(!status.audio_running);
        assert!(!status.camera_running);
        state.stop();
    }

    #[tokio::test]
    async fn ax_watch_start_applies_custom_vad_threshold() {
        // GIVEN: custom VAD threshold (no hardware needed — watchers won't actually
        // capture because both audio and camera features are enabled but sensors
        // are blocked until the cancellation token fires in the loop)
        let state = WatchState::new();
        let args = json!({ "audio": true, "camera": false, "vad_threshold_db": -30.0 });
        let result = handle_ax_watch_start(&args, &state);
        // THEN: started, message mentions threshold
        assert!(!result.is_error, "{}", result.content[0].text);
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["started"], true);
        assert!(v["message"].as_str().unwrap().contains("-30.0"));
        state.stop();
    }

    #[tokio::test]
    async fn ax_watch_start_applies_custom_camera_interval() {
        let state = WatchState::new();
        let args = json!({ "audio": false, "camera": true, "camera_interval_ms": 5000 });
        let result = handle_ax_watch_start(&args, &state);
        assert!(!result.is_error, "{}", result.content[0].text);
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert!(v["message"].as_str().unwrap().contains("5000"));
        state.stop();
    }

    #[test]
    fn ax_watch_start_rejects_unknown_top_level_fields() {
        let state = WatchState::new();
        let result = handle_ax_watch_start(&json!({ "audio": true, "extra": true }), &state);
        assert!(result.is_error);
        assert_eq!(result.content[0].text, "unknown field: extra");
    }

    #[test]
    fn ax_watch_start_rejects_non_boolean_audio() {
        let state = WatchState::new();
        let result = handle_ax_watch_start(&json!({ "audio": "yes" }), &state);
        assert!(result.is_error);
        assert_eq!(result.content[0].text, "Field 'audio' must be a boolean");
    }

    #[test]
    fn ax_watch_start_rejects_non_boolean_camera() {
        let state = WatchState::new();
        let result = handle_ax_watch_start(&json!({ "audio": true, "camera": "yes" }), &state);
        assert!(result.is_error);
        assert_eq!(result.content[0].text, "Field 'camera' must be a boolean");
    }

    #[test]
    fn ax_watch_start_rejects_non_numeric_vad_threshold() {
        let state = WatchState::new();
        let result = handle_ax_watch_start(
            &json!({ "audio": true, "vad_threshold_db": "loud" }),
            &state,
        );
        assert!(result.is_error);
        assert_eq!(
            result.content[0].text,
            "Field 'vad_threshold_db' must be a number"
        );
    }

    #[test]
    fn ax_watch_start_rejects_out_of_range_vad_threshold() {
        let state = WatchState::new();
        let result = handle_ax_watch_start(
            &json!({ "audio": true, "vad_threshold_db": -120.0 }),
            &state,
        );
        assert!(result.is_error);
        assert_eq!(
            result.content[0].text,
            "Field 'vad_threshold_db' must be between -96 and 0"
        );
    }

    #[test]
    fn ax_watch_start_rejects_non_integer_camera_interval() {
        let state = WatchState::new();
        let result = handle_ax_watch_start(
            &json!({ "camera": true, "camera_interval_ms": "fast" }),
            &state,
        );
        assert!(result.is_error);
        assert_eq!(
            result.content[0].text,
            "Field 'camera_interval_ms' must be an integer"
        );
    }

    #[test]
    fn ax_watch_start_rejects_out_of_range_camera_interval() {
        let state = WatchState::new();
        let result = handle_ax_watch_start(
            &json!({ "camera": true, "camera_interval_ms": 100 }),
            &state,
        );
        assert!(result.is_error);
        assert_eq!(
            result.content[0].text,
            "Field 'camera_interval_ms' must be between 500 and 30000"
        );
    }

    #[test]
    fn ax_watch_stop_rejects_unknown_top_level_fields() {
        let state = WatchState::new();
        let result = handle_ax_watch_stop(&json!({ "extra": true }), &state);
        assert!(result.is_error);
        assert_eq!(result.content[0].text, "unknown field: extra");
    }

    #[test]
    fn ax_watch_status_rejects_unknown_top_level_fields() {
        let state = WatchState::new();
        let result = handle_ax_watch_status(&json!({ "extra": true }), &state);
        assert!(result.is_error);
        assert_eq!(result.content[0].text, "unknown field: extra");
    }
}
