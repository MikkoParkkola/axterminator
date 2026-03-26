//! MCP tools for the live background capture system (requires `audio` feature).
//!
//! | Tool | Purpose |
//! |------|---------|
//! | `ax_start_capture`     | Start a background audio + screen capture session |
//! | `ax_stop_capture`      | Stop a running capture session |
//! | `ax_get_transcription` | Snapshot transcription from the ring buffer |
//! | `ax_capture_status`    | Query session health and buffer fill levels |
//!
//! ## Design
//!
//! MCP does not support streaming.  A [`CaptureSession`] runs on a background
//! OS thread that continuously accumulates audio samples and transcription
//! segments.  MCP handlers read a snapshot of the shared state on demand.
//!
//! At most one global session is held.  A second call to `ax_start_capture`
//! stops the previous session before starting a new one.
//!
//! ## Thread safety
//!
//! The global session is guarded by a `Mutex<Option<CaptureSession>>`.
//! Handlers lock it for the minimum time needed to read state, then release.

#[cfg(feature = "audio")]
use std::sync::{Mutex, OnceLock};

#[cfg(feature = "audio")]
use serde_json::{json, Value};

#[cfg(feature = "audio")]
use crate::capture::{CaptureConfig, CaptureSession};
#[cfg(feature = "audio")]
use crate::mcp::annotations;
#[cfg(feature = "audio")]
use crate::mcp::protocol::{Tool, ToolCallResult};

// ---------------------------------------------------------------------------
// Tool names
// ---------------------------------------------------------------------------

#[cfg(feature = "audio")]
pub(crate) const TOOL_AX_START_CAPTURE: &str = "ax_start_capture";
#[cfg(feature = "audio")]
pub(crate) const TOOL_AX_STOP_CAPTURE: &str = "ax_stop_capture";
#[cfg(feature = "audio")]
pub(crate) const TOOL_AX_GET_TRANSCRIPTION: &str = "ax_get_transcription";
#[cfg(feature = "audio")]
pub(crate) const TOOL_AX_CAPTURE_STATUS: &str = "ax_capture_status";

// ---------------------------------------------------------------------------
// Global session store
// ---------------------------------------------------------------------------

/// Process-lifetime container for the active capture session.
///
/// `OnceLock` initialises on first access; the inner `Mutex<Option<…>>`
/// lets handlers atomically swap sessions.
///
/// Exposed as `pub(crate)` so that resource read handlers in
/// [`super::resources_read`] can query session state without duplicating
/// the global-store logic.
#[cfg(feature = "audio")]
pub(crate) fn global_session() -> &'static Mutex<Option<CaptureSession>> {
    static STORE: OnceLock<Mutex<Option<CaptureSession>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(None))
}

// ---------------------------------------------------------------------------
// Tool declarations
// ---------------------------------------------------------------------------

/// All capture tools registered when the `audio` feature is active.
#[cfg(feature = "audio")]
#[must_use]
pub(crate) fn capture_tools() -> Vec<Tool> {
    vec![
        tool_ax_start_capture(),
        tool_ax_stop_capture(),
        tool_ax_get_transcription(),
        tool_ax_capture_status(),
    ]
}

#[cfg(feature = "audio")]
fn tool_ax_start_capture() -> Tool {
    Tool {
        name: TOOL_AX_START_CAPTURE,
        title: "Start background screen + audio capture",
        description: "Begin a continuous background capture session that records system audio \
            into a ring buffer and optionally transcribes it on-device via SFSpeechRecognizer. \
            Screen snapshots can also be captured at a configurable interval.\n\
            \n\
            Audio uses ScreenCaptureKit (macOS 14+, no Screen Recording permission needed). \
            Transcription is on-device only — no cloud, no network.\n\
            \n\
            At most one session is active at a time.  Calling ax_start_capture while a \
            session is running stops the previous session first.\n\
            \n\
            Use ax_get_transcription to retrieve accumulated text and ax_capture_status to \
            check buffer fill.  Call ax_stop_capture when done.\n\
            \n\
            Example: `{\"audio\": true, \"transcribe\": true, \"buffer_seconds\": 60}`",
        input_schema: json!({
            "type": "object",
            "properties": {
                "audio": {
                    "type": "boolean",
                    "description": "Enable continuous system audio capture (default true)",
                    "default": true
                },
                "screen": {
                    "type": "boolean",
                    "description": "Enable periodic screen snapshots every ~3 seconds (default false)",
                    "default": false
                },
                "transcribe": {
                    "type": "boolean",
                    "description": "Transcribe audio windows on-device via SFSpeechRecognizer (default true)",
                    "default": true
                },
                "buffer_seconds": {
                    "type": "integer",
                    "description": "Audio ring buffer depth in seconds (default 60, min 5, max 300)",
                    "default": 60,
                    "minimum": 5,
                    "maximum": 300
                },
                "screen_diff_threshold": {
                    "type": "number",
                    "description": "Minimum perceptual diff score [0.0, 1.0] to store a new frame. \
                        Default 0.05 skips frames where fewer than 5% of 16x16 luminance cells changed. \
                        Use 0.0 to store every frame.",
                    "default": 0.05,
                    "minimum": 0.0,
                    "maximum": 1.0
                }
            },
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "started":    { "type": "boolean" },
                "session_id": { "type": "string" }
            },
            "required": ["started", "session_id"]
        }),
        annotations: annotations::ACTION,
    }
}

#[cfg(feature = "audio")]
fn tool_ax_stop_capture() -> Tool {
    Tool {
        name: TOOL_AX_STOP_CAPTURE,
        title: "Stop a running capture session",
        description: "Stop the active capture session and release all resources. \
            The audio ring buffer and transcription segments are discarded.\n\
            \n\
            Pass `{\"session_id\": \"…\"}` to stop a specific session, or `{}` to stop \
            whatever is currently running.\n\
            \n\
            Returns `{\"stopped\": false}` when no session is active.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "Session ID from ax_start_capture (optional — omit to stop current)"
                }
            },
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "stopped":     { "type": "boolean" },
                "duration_ms": { "type": "integer" }
            },
            "required": ["stopped"]
        }),
        annotations: annotations::ACTION,
    }
}

#[cfg(feature = "audio")]
fn tool_ax_get_transcription() -> Tool {
    Tool {
        name: TOOL_AX_GET_TRANSCRIPTION,
        title: "Get recent transcription from the capture buffer",
        description: "Return transcription segments accumulated by the background capture \
            session in the last `since_seconds` seconds.\n\
            \n\
            Requires an active capture session started with `ax_start_capture` and \
            `transcribe: true`.\n\
            \n\
            Returns both a structured `segments` array and a `text` field with all \
            segments joined in order.\n\
            \n\
            Example: `{\"since_seconds\": 30}`",
        input_schema: json!({
            "type": "object",
            "properties": {
                "since_seconds": {
                    "type": "integer",
                    "description": "How far back to look in the transcript buffer (default 30, max 300)",
                    "default": 30,
                    "minimum": 1,
                    "maximum": 300
                }
            },
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "segments": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "text":     { "type": "string" },
                            "start_ms": { "type": "integer" },
                            "end_ms":   { "type": "integer" },
                            "speaker":  { "type": "string" }
                        },
                        "required": ["text", "start_ms", "end_ms"]
                    }
                },
                "text":        { "type": "string", "description": "All segments joined" },
                "duration_ms": { "type": "integer" }
            },
            "required": ["segments", "text", "duration_ms"]
        }),
        annotations: annotations::READ_ONLY,
    }
}

#[cfg(feature = "audio")]
fn tool_ax_capture_status() -> Tool {
    Tool {
        name: TOOL_AX_CAPTURE_STATUS,
        title: "Query capture session status",
        description: "Return health and fill-level information for the active capture \
            session.\n\
            \n\
            `audio_buffer_seconds` is the number of seconds of audio currently in the \
            ring buffer (≤ `buffer_seconds` from ax_start_capture).\n\
            `transcript_segments` is the total number of recognised speech segments \
            accumulated since the session started.\n\
            `frames_captured` is the count of screen frames stored (passed diff threshold).\n\
            `frames_skipped` is the count of screen frames dropped (below diff threshold).\n\
            \n\
            Example: `{}`",
        input_schema: json!({ "type": "object", "additionalProperties": false }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "running":               { "type": "boolean" },
                "session_id":            { "type": "string" },
                "duration_ms":           { "type": "integer" },
                "audio_buffer_seconds":  { "type": "number" },
                "transcript_segments":   { "type": "integer" },
                "frames_captured":       { "type": "integer" },
                "frames_skipped":        { "type": "integer" }
            },
            "required": ["running"]
        }),
        annotations: annotations::READ_ONLY,
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// Handle `ax_start_capture` — start (or restart) the background capture session.
///
/// The previous session (if any) is extracted from the mutex and the lock is
/// released *before* dropping it.  `CaptureSession::drop` joins the background
/// capture thread, which may block for up to one audio-window period (~5 s).
/// Holding the global mutex during that join would deadlock any concurrent MCP
/// call that tries to read session state.
#[cfg(feature = "audio")]
pub(crate) fn handle_ax_start_capture(args: &Value) -> ToolCallResult {
    let cfg = parse_capture_config(args);
    let session = CaptureSession::start(cfg);
    let session_id = session.session_id.clone();

    // Extract the previous session while holding the lock, then immediately
    // release the lock before the potentially-blocking drop/join.
    let prev_session = match global_session().lock() {
        Ok(mut guard) => {
            let prev = guard.take();
            *guard = Some(session);
            prev
        }
        Err(e) => {
            return ToolCallResult::error(format!("session store lock poisoned: {e}"));
        }
    };

    // Drop (and join) the old session AFTER releasing the mutex.
    drop(prev_session);

    ToolCallResult::ok(json!({ "started": true, "session_id": session_id }).to_string())
}

/// Handle `ax_stop_capture` — stop the active capture session.
///
/// When `session_id` is provided it must match the active session's ID.
/// A mismatch returns an error rather than silently stopping an unrelated
/// session — this is the honest API contract the schema advertises.
///
/// As with [`handle_ax_start_capture`], the session is extracted from the
/// mutex and the lock is released *before* the blocking drop/join.
#[cfg(feature = "audio")]
pub(crate) fn handle_ax_stop_capture(args: &Value) -> ToolCallResult {
    let requested_id = args["session_id"].as_str();

    // Extract (and optionally validate) the session while holding the lock,
    // then release before the potentially-blocking drop/join.
    let session_to_drop = match global_session().lock() {
        Ok(mut guard) => {
            match guard.as_ref() {
                None => return ToolCallResult::ok(json!({ "stopped": false }).to_string()),
                Some(active) => {
                    if let Some(rid) = requested_id {
                        if rid != active.session_id {
                            return ToolCallResult::error(format!(
                                "session_id mismatch: requested \"{rid}\" but active session \
                                 is \"{}\". Pass the correct session_id or omit it to stop \
                                 whatever is running.",
                                active.session_id
                            ));
                        }
                    }
                }
            }
            guard.take()
        }
        Err(e) => return ToolCallResult::error(format!("session store lock poisoned: {e}")),
    };

    // Drop (and join) the session AFTER releasing the mutex.
    let duration_ms = session_to_drop
        .as_ref()
        .map(|s| s.duration_ms())
        .unwrap_or(0);
    drop(session_to_drop);

    ToolCallResult::ok(json!({ "stopped": true, "duration_ms": duration_ms }).to_string())
}

/// Handle `ax_get_transcription` — snapshot transcript segments from the buffer.
#[cfg(feature = "audio")]
pub(crate) fn handle_ax_get_transcription(args: &Value) -> ToolCallResult {
    let since_seconds = args["since_seconds"].as_u64().unwrap_or(30).min(300);

    match global_session().lock() {
        Ok(guard) => match guard.as_ref() {
            None => {
                ToolCallResult::error("No active capture session. Call ax_start_capture first.")
            }
            Some(session) => {
                let segments = session.read_transcription(since_seconds);
                let duration_ms = session.duration_ms();
                let text = segments
                    .iter()
                    .map(|s| s.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                let segments_json: Vec<Value> = segments
                    .iter()
                    .map(|s| {
                        let mut obj = json!({
                            "text":     s.text,
                            "start_ms": s.start_ms,
                            "end_ms":   s.end_ms,
                        });
                        if let Some(ref spk) = s.speaker {
                            obj["speaker"] = json!(spk);
                        }
                        obj
                    })
                    .collect();
                ToolCallResult::ok(
                    json!({
                        "segments":    segments_json,
                        "text":        text,
                        "duration_ms": duration_ms,
                    })
                    .to_string(),
                )
            }
        },
        Err(e) => ToolCallResult::error(format!("session store lock poisoned: {e}")),
    }
}

/// Handle `ax_capture_status` — return session health and buffer fill levels.
#[cfg(feature = "audio")]
pub(crate) fn handle_ax_capture_status() -> ToolCallResult {
    match global_session().lock() {
        Ok(guard) => match guard.as_ref() {
            None => ToolCallResult::ok(json!({ "running": false }).to_string()),
            Some(session) => ToolCallResult::ok(
                json!({
                    "running":              session.is_running(),
                    "session_id":           session.session_id,
                    "duration_ms":          session.duration_ms(),
                    "audio_buffer_seconds": session.audio_buffer_seconds(),
                    "transcript_segments":  session.transcript_segment_count(),
                    "frames_captured":      session.frames_captured(),
                    "frames_skipped":       session.frames_skipped(),
                })
                .to_string(),
            ),
        },
        Err(e) => ToolCallResult::error(format!("session store lock poisoned: {e}")),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse `CaptureConfig` from tool arguments, applying defaults.
#[cfg(feature = "audio")]
fn parse_capture_config(args: &Value) -> CaptureConfig {
    CaptureConfig {
        audio: args["audio"].as_bool().unwrap_or(true),
        screen: args["screen"].as_bool().unwrap_or(false),
        transcribe: args["transcribe"].as_bool().unwrap_or(true),
        buffer_seconds: args["buffer_seconds"]
            .as_u64()
            .map_or(60, |v| v.clamp(5, 300) as u32),
        #[allow(clippy::cast_possible_truncation)]
        screen_diff_threshold: args["screen_diff_threshold"]
            .as_f64()
            .map_or(0.05_f32, |v| v.clamp(0.0, 1.0) as f32),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(all(test, feature = "audio"))]
mod tests {
    use super::*;
    use serde_json::json;

    // -----------------------------------------------------------------------
    // Tool declarations
    // -----------------------------------------------------------------------

    #[test]
    fn capture_tools_returns_four_tools() {
        // GIVEN: audio feature enabled
        // WHEN: capture_tools() is called
        // THEN: exactly four tools returned
        let tools = capture_tools();
        assert_eq!(tools.len(), 4);
        let names: Vec<&str> = tools.iter().map(|t| t.name).collect();
        assert!(names.contains(&"ax_start_capture"));
        assert!(names.contains(&"ax_stop_capture"));
        assert!(names.contains(&"ax_get_transcription"));
        assert!(names.contains(&"ax_capture_status"));
    }

    #[test]
    fn capture_tool_names_are_unique() {
        let tools = capture_tools();
        let names: std::collections::HashSet<&str> = tools.iter().map(|t| t.name).collect();
        assert_eq!(names.len(), tools.len());
    }

    #[test]
    fn all_capture_tools_have_descriptions() {
        for tool in capture_tools() {
            assert!(
                !tool.description.is_empty(),
                "empty description: {}",
                tool.name
            );
        }
    }

    #[test]
    fn ax_start_capture_schema_has_buffer_seconds_bounds() {
        let tool = tool_ax_start_capture();
        let props = &tool.input_schema["properties"];
        assert_eq!(props["buffer_seconds"]["minimum"], 5);
        assert_eq!(props["buffer_seconds"]["maximum"], 300);
    }

    #[test]
    fn ax_get_transcription_schema_since_seconds_bounded() {
        let tool = tool_ax_get_transcription();
        let props = &tool.input_schema["properties"];
        assert_eq!(props["since_seconds"]["minimum"], 1);
        assert_eq!(props["since_seconds"]["maximum"], 300);
    }

    #[test]
    fn ax_capture_status_input_schema_accepts_no_properties() {
        let tool = tool_ax_capture_status();
        assert!(
            tool.input_schema.get("properties").is_none()
                || tool.input_schema["properties"].is_null()
        );
    }

    // -----------------------------------------------------------------------
    // parse_capture_config
    // -----------------------------------------------------------------------

    #[test]
    fn parse_capture_config_uses_defaults_on_empty_args() {
        // GIVEN: empty args
        let cfg = parse_capture_config(&json!({}));
        // THEN: defaults applied
        assert!(cfg.audio);
        assert!(!cfg.screen);
        assert!(cfg.transcribe);
        assert_eq!(cfg.buffer_seconds, 60);
        assert!((cfg.screen_diff_threshold - 0.05).abs() < f32::EPSILON);
    }

    #[test]
    fn parse_capture_config_respects_explicit_values() {
        let cfg = parse_capture_config(&json!({
            "audio": false,
            "screen": true,
            "transcribe": false,
            "buffer_seconds": 120,
            "screen_diff_threshold": 0.10
        }));
        assert!(!cfg.audio);
        assert!(cfg.screen);
        assert!(!cfg.transcribe);
        assert_eq!(cfg.buffer_seconds, 120);
        assert!((cfg.screen_diff_threshold - 0.10).abs() < f32::EPSILON);
    }

    #[test]
    fn parse_capture_config_clamps_buffer_seconds_min() {
        let cfg = parse_capture_config(&json!({ "buffer_seconds": 1 }));
        assert_eq!(cfg.buffer_seconds, 5);
    }

    #[test]
    fn parse_capture_config_clamps_buffer_seconds_max() {
        let cfg = parse_capture_config(&json!({ "buffer_seconds": 9999 }));
        assert_eq!(cfg.buffer_seconds, 300);
    }

    #[test]
    fn parse_capture_config_clamps_screen_diff_threshold_below_zero() {
        // GIVEN: threshold below 0.0 clamped to 0.0
        let cfg = parse_capture_config(&json!({ "screen_diff_threshold": -0.5 }));
        assert_eq!(cfg.screen_diff_threshold, 0.0);
    }

    #[test]
    fn parse_capture_config_clamps_screen_diff_threshold_above_one() {
        // GIVEN: threshold above 1.0 clamped to 1.0
        let cfg = parse_capture_config(&json!({ "screen_diff_threshold": 99.0 }));
        assert_eq!(cfg.screen_diff_threshold, 1.0);
    }

    #[test]
    fn parse_capture_config_screen_diff_threshold_zero_stored() {
        let cfg = parse_capture_config(&json!({ "screen_diff_threshold": 0.0 }));
        assert_eq!(cfg.screen_diff_threshold, 0.0);
    }

    #[test]
    fn ax_capture_status_output_schema_includes_diff_stat_fields() {
        // GIVEN: tool declaration
        let tool = tool_ax_capture_status();
        let props = &tool.output_schema["properties"];
        // THEN: diff stat fields are present
        assert!(
            props.get("frames_captured").is_some(),
            "frames_captured missing from output schema"
        );
        assert!(
            props.get("frames_skipped").is_some(),
            "frames_skipped missing from output schema"
        );
    }

    #[test]
    fn ax_start_capture_schema_has_screen_diff_threshold_field() {
        // GIVEN: tool declaration
        let tool = tool_ax_start_capture();
        let props = &tool.input_schema["properties"];
        let thr = &props["screen_diff_threshold"];
        // THEN: bounds are correct
        assert_eq!(thr["minimum"], 0.0);
        assert_eq!(thr["maximum"], 1.0);
        assert!((thr["default"].as_f64().unwrap_or(99.0) - 0.05).abs() < 1e-6);
    }

    #[test]
    fn handle_ax_capture_status_returns_diff_counters_when_session_active() {
        let _guard = crate::test_sync::capture_session_lock()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        // GIVEN: active session (no audio, no screen — counters start at 0)
        let _ = handle_ax_stop_capture(&json!({}));
        let start = handle_ax_start_capture(&json!({
            "audio": false,
            "transcribe": false,
            "screen": false
        }));
        assert!(!start.is_error, "{}", start.content[0].text);

        // WHEN: status queried
        let status = handle_ax_capture_status();
        assert!(!status.is_error);
        let v: Value = serde_json::from_str(&status.content[0].text).unwrap();

        // THEN: diff counters are present and zero (no screen capture occurred)
        assert_eq!(v["frames_captured"], 0, "frames_captured should be 0");
        assert_eq!(v["frames_skipped"], 0, "frames_skipped should be 0");

        let _ = handle_ax_stop_capture(&json!({}));
    }

    // -----------------------------------------------------------------------
    // Handlers — no active session
    // -----------------------------------------------------------------------

    #[test]
    fn handle_ax_capture_status_no_session_returns_running_false() {
        let _guard = crate::test_sync::capture_session_lock()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        // Ensure no session is running (stop any lingering global state).
        let _ = handle_ax_stop_capture(&json!({}));

        let result = handle_ax_capture_status();
        assert!(
            !result.is_error,
            "unexpected error: {}",
            result.content[0].text
        );
        let v: Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["running"], false);
    }

    #[test]
    fn handle_ax_get_transcription_no_session_returns_error() {
        let _guard = crate::test_sync::capture_session_lock()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        // Ensure no session is running.
        let _ = handle_ax_stop_capture(&json!({}));

        let result = handle_ax_get_transcription(&json!({}));
        assert!(result.is_error);
        assert!(result.content[0].text.contains("No active capture session"));
    }

    #[test]
    fn handle_ax_stop_capture_no_session_returns_stopped_false() {
        let _guard = crate::test_sync::capture_session_lock()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        // Ensure no session is running.
        let _ = handle_ax_stop_capture(&json!({}));

        let result = handle_ax_stop_capture(&json!({}));
        assert!(!result.is_error);
        let v: Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["stopped"], false);
    }

    #[test]
    fn handle_ax_stop_capture_wrong_session_id_returns_error() {
        let _guard = crate::test_sync::capture_session_lock()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        // GIVEN: an active session
        let _ = handle_ax_stop_capture(&json!({}));
        let start = handle_ax_start_capture(&json!({
            "audio": false, "transcribe": false, "screen": false
        }));
        assert!(!start.is_error);

        // WHEN: stop is called with a wrong session_id
        let result = handle_ax_stop_capture(&json!({ "session_id": "deadbeef00000000" }));

        // THEN: error returned, session still running
        assert!(result.is_error, "expected error for mismatched session_id");
        assert!(
            result.content[0].text.contains("session_id mismatch"),
            "unexpected error text: {}",
            result.content[0].text
        );

        // Cleanup
        let _ = handle_ax_stop_capture(&json!({}));
    }

    #[test]
    fn handle_ax_stop_capture_correct_session_id_stops_session() {
        let _guard = crate::test_sync::capture_session_lock()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        // GIVEN: an active session
        let _ = handle_ax_stop_capture(&json!({}));
        let start = handle_ax_start_capture(&json!({
            "audio": false, "transcribe": false, "screen": false
        }));
        assert!(!start.is_error);
        let sv: Value = serde_json::from_str(&start.content[0].text).unwrap();
        let sid = sv["session_id"].as_str().unwrap().to_string();

        // WHEN: stop is called with the correct session_id
        let result = handle_ax_stop_capture(&json!({ "session_id": sid }));

        // THEN: success
        assert!(
            !result.is_error,
            "unexpected error: {}",
            result.content[0].text
        );
        let v: Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["stopped"], true);
    }

    // -----------------------------------------------------------------------
    // Handlers — full lifecycle (no audio hardware)
    // -----------------------------------------------------------------------

    #[test]
    fn start_stop_lifecycle_produces_valid_json() {
        let _guard = crate::test_sync::capture_session_lock()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        // GIVEN: start a session with no audio (avoids hardware dependency)
        let start_result = handle_ax_start_capture(&json!({
            "audio": false,
            "transcribe": false,
            "screen": false,
            "buffer_seconds": 5
        }));
        assert!(!start_result.is_error, "{}", start_result.content[0].text);
        let start_v: Value = serde_json::from_str(&start_result.content[0].text).unwrap();
        assert_eq!(start_v["started"], true);
        assert!(start_v["session_id"].is_string());
        let _session_id = start_v["session_id"].as_str().unwrap().to_string();

        // THEN: status shows running
        let status_result = handle_ax_capture_status();
        assert!(!status_result.is_error);
        let sv: Value = serde_json::from_str(&status_result.content[0].text).unwrap();
        assert_eq!(sv["running"], true);

        // AND: transcription returns empty segments (no audio captured)
        let tx_result = handle_ax_get_transcription(&json!({ "since_seconds": 30 }));
        assert!(!tx_result.is_error);
        let tv: Value = serde_json::from_str(&tx_result.content[0].text).unwrap();
        assert!(tv["segments"].is_array());
        assert_eq!(tv["text"], "");

        // WHEN: stopped
        let stop_result = handle_ax_stop_capture(&json!({}));
        assert!(!stop_result.is_error);
        let stop_v: Value = serde_json::from_str(&stop_result.content[0].text).unwrap();
        assert_eq!(stop_v["stopped"], true);
        assert!(stop_v["duration_ms"].is_number());
    }

    #[test]
    fn start_capture_twice_replaces_old_session() {
        let _guard = crate::test_sync::capture_session_lock()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        // GIVEN: start first session
        let r1 = handle_ax_start_capture(&json!({
            "audio": false, "transcribe": false, "screen": false
        }));
        let id1 = serde_json::from_str::<Value>(&r1.content[0].text).unwrap()["session_id"]
            .as_str()
            .unwrap()
            .to_string();

        // WHEN: start second session (replaces first)
        let r2 = handle_ax_start_capture(&json!({
            "audio": false, "transcribe": false, "screen": false
        }));
        let id2 = serde_json::from_str::<Value>(&r2.content[0].text).unwrap()["session_id"]
            .as_str()
            .unwrap()
            .to_string();

        // THEN: IDs differ
        assert_ne!(id1, id2);

        // Cleanup
        let _ = handle_ax_stop_capture(&json!({}));
    }

    #[test]
    fn extended_tools_includes_capture_tools_when_audio_feature_enabled() {
        let tools = crate::mcp::tools_extended::extended_tools();
        let names: Vec<&str> = tools.iter().map(|t| t.name).collect();
        assert!(
            names.contains(&"ax_start_capture"),
            "ax_start_capture missing"
        );
        assert!(
            names.contains(&"ax_stop_capture"),
            "ax_stop_capture missing"
        );
        assert!(
            names.contains(&"ax_get_transcription"),
            "ax_get_transcription missing"
        );
        assert!(
            names.contains(&"ax_capture_status"),
            "ax_capture_status missing"
        );
    }

    #[test]
    fn call_tool_extended_ax_capture_status_dispatches() {
        let _guard = crate::test_sync::capture_session_lock()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        use crate::mcp::tools::AppRegistry;
        use std::sync::Arc;

        let _ = handle_ax_stop_capture(&json!({}));

        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        let result = crate::mcp::tools_extended::call_tool_extended(
            "ax_capture_status",
            &json!({}),
            &registry,
            &mut out,
        );
        assert!(result.is_some(), "ax_capture_status should dispatch");
        let r = result.unwrap();
        assert!(!r.is_error, "unexpected error: {}", r.content[0].text);
    }

    #[test]
    fn call_tool_extended_ax_get_transcription_no_session_returns_error() {
        let _guard = crate::test_sync::capture_session_lock()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        use crate::mcp::tools::AppRegistry;
        use std::sync::Arc;

        let _ = handle_ax_stop_capture(&json!({}));

        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        let result = crate::mcp::tools_extended::call_tool_extended(
            "ax_get_transcription",
            &json!({ "since_seconds": 10 }),
            &registry,
            &mut out,
        );
        assert!(result.is_some());
        assert!(result.unwrap().is_error);
    }
}
