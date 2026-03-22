//! Camera MCP tools (requires `camera` feature).
//!
//! | Tool | Purpose |
//! |------|---------|
//! | `ax_camera_capture`  | Capture a single JPEG frame |
//! | `ax_gesture_detect`  | Capture frame + detect hand/face gestures |
//! | `ax_gesture_listen`  | Poll until gesture or timeout (≤60 s) |
//!
//! All functions are gated behind `#[cfg(feature = "camera")]`.
//! Uses AVFoundation (capture) and Vision framework (detection) — on-device.

#[cfg(feature = "camera")]
use serde_json::{json, Value};

#[cfg(feature = "camera")]
use crate::mcp::annotations;
#[cfg(feature = "camera")]
use crate::mcp::protocol::{Tool, ToolCallResult};

// ---------------------------------------------------------------------------
// Tool declarations
// ---------------------------------------------------------------------------

/// All camera-related MCP tools.  Requires the `camera` feature.
///
/// Registers three tools:
/// - `ax_camera_capture` — single-frame JPEG capture
/// - `ax_gesture_detect` — capture frame + detect gestures
/// - `ax_gesture_listen` — poll until gesture or timeout
#[cfg(feature = "camera")]
#[must_use]
pub fn camera_tools() -> Vec<Tool> {
    vec![
        tool_ax_camera_capture(),
        tool_ax_gesture_detect(),
        tool_ax_gesture_listen(),
    ]
}

/// Declare the `ax_camera_capture` tool.
#[cfg(feature = "camera")]
fn tool_ax_camera_capture() -> Tool {
    Tool {
        name: "ax_camera_capture",
        title: "Capture a single camera frame",
        description: "Capture one JPEG frame from the specified camera (default: front-facing \
            FaceTime camera) and return it base64-encoded.\n\
            The AVCaptureSession is started, one frame is grabbed at 1280x720, and the \
            session is immediately stopped and released (no persistent camera access).\n\
            The hardware camera indicator light will be ON during capture (macOS-enforced).\n\
            Requires TCC camera permission. Returns error code `camera_denied` when denied.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "device_id": {
                    "type": "string",
                    "description": "Camera device unique ID from ax_camera_devices. \
                        Omit to use the default front-facing camera."
                }
            },
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "width":        { "type": "integer" },
                "height":       { "type": "integer" },
                "image_base64": { "type": "string"  }
            },
            "required": ["width", "height", "image_base64"]
        }),
        annotations: annotations::READ_ONLY,
    }
}

/// Declare the `ax_gesture_detect` tool.
#[cfg(feature = "camera")]
fn tool_ax_gesture_detect() -> Tool {
    Tool {
        name: "ax_gesture_detect",
        title: "Capture frame and detect hand / face gestures",
        description: "Capture one camera frame then run Vision framework gesture detection \
            on it. Returns all detected gestures with confidence scores.\n\
            Hand gestures use VNDetectHumanHandPoseRequest (macOS 11+).\n\
            Face gestures (nod, shake) use VNDetectFaceLandmarksRequest (macOS 12+).\n\
            All processing is on-device.\n\
            Supported: thumbs_up, thumbs_down, wave, stop, point, nod, shake.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "device_id": {
                    "type": "string",
                    "description": "Camera device unique ID. Omit for default camera."
                }
            },
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "gestures": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "type":       { "type": "string" },
                            "confidence": { "type": "number" },
                            "hand":       { "type": "string" }
                        },
                        "required": ["type", "confidence", "hand"]
                    }
                }
            },
            "required": ["gestures"]
        }),
        annotations: annotations::READ_ONLY,
    }
}

/// Declare the `ax_gesture_listen` tool.
#[cfg(feature = "camera")]
fn tool_ax_gesture_listen() -> Tool {
    Tool {
        name: "ax_gesture_listen",
        title: "Monitor camera for gestures (up to 60 s)",
        description: "Poll the camera repeatedly until one of the specified gestures is \
            detected or the duration elapses. Returns the first matching gesture or an \
            empty result on timeout. Duration must be <=60 seconds.\n\
            Supported: thumbs_up, thumbs_down, wave, stop, point, nod, shake.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "duration_seconds": {
                    "type": "number",
                    "description": "Maximum monitoring duration in seconds (0.0-60.0)",
                    "minimum": 0.0,
                    "maximum": 60.0,
                    "default": 10.0
                },
                "gestures": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Gesture names to watch for. Omit or [] to match any.",
                    "default": []
                },
                "device_id": {
                    "type": "string",
                    "description": "Camera device unique ID. Omit for default camera."
                }
            },
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "detected":        { "type": "boolean" },
                "gesture":         { "type": "string"  },
                "confidence":      { "type": "number"  },
                "hand":            { "type": "string"  },
                "elapsed_seconds": { "type": "number"  }
            },
            "required": ["detected", "elapsed_seconds"]
        }),
        annotations: annotations::READ_ONLY,
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// Handle `ax_camera_capture`.
#[cfg(feature = "camera")]
pub(crate) fn handle_ax_camera_capture(args: &Value) -> ToolCallResult {
    let device_id = args["device_id"].as_str();
    match crate::camera::capture_frame(device_id) {
        Ok(frame) => ToolCallResult::ok(
            json!({
                "width":        frame.width,
                "height":       frame.height,
                "image_base64": frame.base64_jpeg()
            })
            .to_string(),
        ),
        Err(e) => ToolCallResult::error(e.to_string()),
    }
}

/// Handle `ax_gesture_detect`.
#[cfg(feature = "camera")]
pub(crate) fn handle_ax_gesture_detect(args: &Value) -> ToolCallResult {
    let device_id = args["device_id"].as_str();
    match crate::camera::capture_and_detect(device_id) {
        Ok((frame, detections)) => {
            let gestures: Vec<_> = detections.iter().map(gesture_to_json).collect();
            ToolCallResult::ok(
                json!({ "gestures": gestures, "frame_base64": frame.base64_jpeg() }).to_string(),
            )
        }
        Err(e) => ToolCallResult::error(e.to_string()),
    }
}

/// Serialise a [`crate::camera::GestureDetection`] to a JSON value.
#[cfg(feature = "camera")]
pub(crate) fn gesture_to_json(d: &crate::camera::GestureDetection) -> serde_json::Value {
    json!({
        "type":       d.gesture.as_name(),
        "confidence": d.confidence,
        "hand":       serde_json::to_value(&d.hand)
                          .unwrap_or(serde_json::Value::String("unknown".into()))
    })
}

/// Handle `ax_gesture_listen`.
#[cfg(feature = "camera")]
pub(crate) fn handle_ax_gesture_listen(args: &Value) -> ToolCallResult {
    let duration_secs = args["duration_seconds"].as_f64().unwrap_or(10.0);
    if let Err(e) = crate::camera::validate_duration(duration_secs) {
        return ToolCallResult::error(e.to_string());
    }
    let gesture_names: Vec<&str> = match args["gestures"].as_array() {
        Some(arr) => arr.iter().filter_map(|v| v.as_str()).collect(),
        None => vec![],
    };
    if !gesture_names.is_empty() {
        if let Err(e) = crate::camera::validate_gesture_names(&gesture_names) {
            return ToolCallResult::error(e.to_string());
        }
    }
    let start = std::time::Instant::now();
    let all_names: Vec<&str>;
    let effective_names: &[&str] = if gesture_names.is_empty() {
        all_names = crate::camera::Gesture::all_names().to_vec();
        &all_names
    } else {
        &gesture_names
    };
    let result = listen_with_device(args["device_id"].as_str(), duration_secs, effective_names);
    let elapsed = start.elapsed().as_secs_f64();
    build_listen_result(result, elapsed)
}

/// Format the `gesture_listen` result into a `ToolCallResult`.
#[cfg(feature = "camera")]
fn build_listen_result(
    result: Result<Option<crate::camera::GestureDetection>, crate::camera::CameraError>,
    elapsed: f64,
) -> ToolCallResult {
    match result {
        Ok(Some(d)) => ToolCallResult::ok(
            json!({
                "detected":        true,
                "gesture":         d.gesture.as_name(),
                "confidence":      d.confidence,
                "hand":            serde_json::to_value(&d.hand)
                    .unwrap_or(serde_json::Value::String("unknown".into())),
                "elapsed_seconds": elapsed
            })
            .to_string(),
        ),
        Ok(None) => ToolCallResult::ok(
            json!({
                "detected":        false,
                "gesture":         null,
                "confidence":      null,
                "hand":            null,
                "elapsed_seconds": elapsed
            })
            .to_string(),
        ),
        Err(e) => ToolCallResult::error(e.to_string()),
    }
}

/// Poll the camera for matching gestures until deadline.
#[cfg(feature = "camera")]
fn listen_with_device(
    device_id: Option<&str>,
    duration_secs: f64,
    gesture_names: &[&str],
) -> Result<Option<crate::camera::GestureDetection>, crate::camera::CameraError> {
    use std::time::{Duration, Instant};
    let wanted = crate::camera::validate_gesture_names(gesture_names)?;
    if !crate::camera::check_camera_permission() {
        return Err(crate::camera::CameraError::PermissionDenied);
    }
    let deadline = Instant::now() + Duration::from_secs_f64(duration_secs);
    let poll = Duration::from_millis(200);
    while Instant::now() < deadline {
        let frame = crate::camera::capture_frame(device_id)?;
        let detections = crate::camera::detect_gestures(&frame)?;
        if let Some(hit) = detections.into_iter().find(|d| wanted.contains(&d.gesture)) {
            return Ok(Some(hit));
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        std::thread::sleep(poll.min(remaining));
    }
    Ok(None)
}

// ---------------------------------------------------------------------------
// Camera resource helper
// ---------------------------------------------------------------------------

/// Produce the JSON payload for `axterminator://camera/devices`.
///
/// Returns `{ "cameras": [...] }` with one entry per detected device.
/// Permission is not required to enumerate devices.
///
/// # Examples
///
/// ```
/// let payload = axterminator::mcp::tools_extended::camera_devices_payload();
/// let v: serde_json::Value = serde_json::from_str(&payload).unwrap();
/// assert!(v["cameras"].is_array());
/// ```
#[cfg(feature = "camera")]
#[must_use]
pub fn camera_devices_payload() -> String {
    let devices = crate::camera::list_cameras();
    let cameras: Vec<_> = devices
        .iter()
        .map(|d| {
            json!({
                "device_id": d.id,
                "name":      d.name,
                "position":  serde_json::to_value(&d.position)
                    .unwrap_or(serde_json::Value::String("unknown".into())),
                "is_default": d.is_default
            })
        })
        .collect();
    json!({ "cameras": cameras }).to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(all(test, feature = "camera"))]
mod tests {
    use super::*;
    use crate::camera::{CameraError, Gesture};

    #[test]
    fn camera_tools_returns_three_tools() {
        // GIVEN: camera feature enabled
        // WHEN: camera_tools() is called
        let tools = camera_tools();
        // THEN: exactly 3 tools
        assert_eq!(tools.len(), 3);
    }

    #[test]
    fn camera_tools_names_are_unique() {
        let tools = camera_tools();
        let names: std::collections::HashSet<_> = tools.iter().map(|t| t.name).collect();
        assert_eq!(names.len(), tools.len(), "tool names must be unique");
    }

    #[test]
    fn camera_tools_all_have_object_schemas() {
        for tool in camera_tools() {
            assert!(
                tool.input_schema.is_object(),
                "tool {} must have object input_schema",
                tool.name
            );
            assert!(
                tool.output_schema.is_object(),
                "tool {} must have object output_schema",
                tool.name
            );
        }
    }

    #[test]
    fn ax_camera_capture_has_read_only_annotation() {
        // GIVEN/WHEN: tool declaration
        let tool = tool_ax_camera_capture();
        // THEN: read_only=true, destructive=false
        assert!(tool.annotations.read_only);
        assert!(!tool.annotations.destructive);
    }

    #[test]
    fn ax_gesture_detect_has_read_only_annotation() {
        let tool = tool_ax_gesture_detect();
        assert!(tool.annotations.read_only);
        assert!(!tool.annotations.destructive);
    }

    #[test]
    fn ax_gesture_listen_has_read_only_annotation() {
        let tool = tool_ax_gesture_listen();
        assert!(tool.annotations.read_only);
        assert!(!tool.annotations.destructive);
    }

    #[test]
    fn gesture_listen_handler_rejects_duration_above_60() {
        // GIVEN: duration = 90s
        let args = json!({"duration_seconds": 90.0, "gestures": ["thumbs_up"]});
        // WHEN: dispatched
        let result = handle_ax_gesture_listen(&args);
        // THEN: error with "duration_exceeded"
        assert!(result.is_error);
        assert!(
            result.content[0].text.contains("duration_exceeded"),
            "got: {}",
            result.content[0].text
        );
    }

    #[test]
    fn gesture_listen_handler_rejects_unknown_gesture() {
        // GIVEN: invalid gesture name
        let args = json!({"duration_seconds": 5.0, "gestures": ["robot_dance"]});
        // WHEN: dispatched (validation before camera access)
        let result = handle_ax_gesture_listen(&args);
        // THEN: error with "unknown_gesture"
        assert!(result.is_error);
        assert!(
            result.content[0].text.contains("unknown_gesture"),
            "got: {}",
            result.content[0].text
        );
    }

    #[test]
    fn gesture_listen_handler_zero_duration_validates_cleanly() {
        // Zero duration is valid (<=60). Actual capture will fail on CI.
        let args = json!({"duration_seconds": 0.0, "gestures": ["thumbs_up"]});
        let result = handle_ax_gesture_listen(&args);
        if result.is_error {
            let msg = &result.content[0].text;
            assert!(!msg.contains("duration_exceeded"), "got: {msg}");
            assert!(!msg.contains("unknown_gesture"), "got: {msg}");
        }
    }

    #[test]
    fn camera_devices_payload_is_valid_json() {
        // GIVEN: any machine
        // WHEN: payload generated
        let payload = camera_devices_payload();
        // THEN: valid JSON with cameras array
        let v: serde_json::Value = serde_json::from_str(&payload).expect("must be valid JSON");
        assert!(v["cameras"].is_array());
    }

    #[test]
    fn camera_devices_payload_entries_have_required_fields() {
        let payload = camera_devices_payload();
        let v: serde_json::Value = serde_json::from_str(&payload).unwrap();
        for device in v["cameras"].as_array().unwrap() {
            assert!(device["device_id"].is_string());
            assert!(device["name"].is_string());
            assert!(device["position"].is_string());
            assert!(device["is_default"].is_boolean());
        }
    }

    #[test]
    fn all_gesture_names_pass_validation() {
        let all = Gesture::all_names().to_vec();
        let result = crate::camera::validate_gesture_names(&all);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), all.len());
    }

    #[test]
    fn camera_error_permission_denied_starts_with_code() {
        let e = CameraError::PermissionDenied;
        assert!(e.to_string().starts_with("camera_denied:"));
    }

    #[test]
    fn camera_error_duration_exceeded_starts_with_code() {
        let e = CameraError::DurationExceeded(90.0);
        assert!(e.to_string().starts_with("duration_exceeded:"));
    }

    #[test]
    fn camera_error_unknown_gesture_starts_with_code() {
        let e = CameraError::UnknownGesture("laser_fingers".into());
        assert!(e.to_string().starts_with("unknown_gesture:"));
    }

    #[test]
    fn gesture_to_json_produces_correct_fields() {
        use crate::camera::{GestureDetection, Hand};
        let d = GestureDetection {
            gesture: Gesture::ThumbsUp,
            confidence: 0.9,
            hand: Hand::Right,
        };
        let v = gesture_to_json(&d);
        assert_eq!(v["type"], "thumbs_up");
        assert_eq!(v["hand"], "right");
        assert!(v["confidence"].as_f64().unwrap() > 0.8);
    }
}
