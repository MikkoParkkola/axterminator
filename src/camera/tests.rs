//! Unit tests for the camera module.
//!
//! Tests are grouped by the sub-system they exercise:
//! - Gesture name round-trips
//! - Duration validation
//! - Gesture name list validation
//! - Serialization (serde)
//! - ImageData utilities
//! - CameraError display messages
//! - CameraDevice / CameraPosition serialization

use std::time::Duration;

use base64::Engine as _;

use super::{
    validate_duration, validate_gesture_names, CameraDevice, CameraError, CameraPosition,
    Gesture, GestureDetection, Hand, ImageData,
};

// -----------------------------------------------------------------------
// Gesture::from_name round-trips
// -----------------------------------------------------------------------

#[test]
fn gesture_from_name_thumbs_up_round_trips() {
    // GIVEN: canonical name
    // WHEN: parsed
    let g = Gesture::from_name("thumbs_up").unwrap();
    // THEN: as_name returns the same string
    assert_eq!(g, Gesture::ThumbsUp);
    assert_eq!(g.as_name(), "thumbs_up");
}

#[test]
fn gesture_from_name_thumbs_down_round_trips() {
    let g = Gesture::from_name("thumbs_down").unwrap();
    assert_eq!(g, Gesture::ThumbsDown);
    assert_eq!(g.as_name(), "thumbs_down");
}

#[test]
fn gesture_from_name_wave_round_trips() {
    let g = Gesture::from_name("wave").unwrap();
    assert_eq!(g, Gesture::Wave);
    assert_eq!(g.as_name(), "wave");
}

#[test]
fn gesture_from_name_stop_round_trips() {
    let g = Gesture::from_name("stop").unwrap();
    assert_eq!(g, Gesture::Stop);
    assert_eq!(g.as_name(), "stop");
}

#[test]
fn gesture_from_name_point_round_trips() {
    let g = Gesture::from_name("point").unwrap();
    assert_eq!(g, Gesture::Point);
    assert_eq!(g.as_name(), "point");
}

#[test]
fn gesture_from_name_nod_round_trips() {
    let g = Gesture::from_name("nod").unwrap();
    assert_eq!(g, Gesture::Nod);
    assert_eq!(g.as_name(), "nod");
}

#[test]
fn gesture_from_name_shake_round_trips() {
    let g = Gesture::from_name("shake").unwrap();
    assert_eq!(g, Gesture::Shake);
    assert_eq!(g.as_name(), "shake");
}

#[test]
fn gesture_from_name_unknown_returns_error() {
    // GIVEN: an unrecognised name
    // WHEN: parsed
    let err = Gesture::from_name("air_guitar").unwrap_err();
    // THEN: UnknownGesture error with the name embedded
    assert!(matches!(err, CameraError::UnknownGesture(ref n) if n == "air_guitar"));
}

#[test]
fn gesture_all_names_covers_all_variants() {
    // GIVEN: all canonical names
    // WHEN: parsed
    // THEN: every name successfully round-trips
    for name in Gesture::all_names() {
        let g = Gesture::from_name(name).expect("all_names must be valid");
        assert_eq!(g.as_name(), *name);
    }
}

// -----------------------------------------------------------------------
// Duration validation
// -----------------------------------------------------------------------

#[test]
fn validate_duration_accepts_zero() {
    assert!(validate_duration(0.0).is_ok());
}

#[test]
fn validate_duration_accepts_sixty_seconds() {
    assert!(validate_duration(60.0).is_ok());
}

#[test]
fn validate_duration_accepts_fractional_below_cap() {
    assert!(validate_duration(0.1).is_ok());
}

#[test]
fn validate_duration_rejects_sixty_one_seconds() {
    // GIVEN: duration just over the cap
    // WHEN: validated
    let err = validate_duration(61.0).unwrap_err();
    // THEN: DurationExceeded with the value
    assert!(matches!(err, CameraError::DurationExceeded(d) if (d - 61.0).abs() < 0.01));
}

#[test]
fn validate_duration_rejects_large_value() {
    assert!(validate_duration(3600.0).is_err());
}

#[test]
fn validate_duration_sixty_returns_correct_duration() {
    let d = validate_duration(60.0).unwrap();
    assert_eq!(d, Duration::from_secs(60));
}

// -----------------------------------------------------------------------
// Gesture name validation
// -----------------------------------------------------------------------

#[test]
fn validate_gesture_names_accepts_all_valid() {
    let all = Gesture::all_names().to_vec();
    let result = validate_gesture_names(&all).unwrap();
    assert_eq!(result.len(), all.len());
}

#[test]
fn validate_gesture_names_rejects_first_invalid() {
    // GIVEN: mix of valid and invalid names
    // WHEN: validated
    let err = validate_gesture_names(&["thumbs_up", "robot_dance"]).unwrap_err();
    // THEN: UnknownGesture for the invalid name
    assert!(matches!(err, CameraError::UnknownGesture(ref n) if n == "robot_dance"));
}

#[test]
fn validate_gesture_names_empty_list_is_ok() {
    let result = validate_gesture_names(&[]).unwrap();
    assert!(result.is_empty());
}

// -----------------------------------------------------------------------
// Serialization
// -----------------------------------------------------------------------

#[test]
fn gesture_serialises_to_snake_case() {
    let json = serde_json::to_string(&Gesture::ThumbsUp).unwrap();
    assert_eq!(json, "\"thumbs_up\"");
}

#[test]
fn gesture_deserialises_from_snake_case() {
    let g: Gesture = serde_json::from_str("\"thumbs_down\"").unwrap();
    assert_eq!(g, Gesture::ThumbsDown);
}

#[test]
fn camera_position_serialises_correctly() {
    assert_eq!(
        serde_json::to_string(&CameraPosition::Front).unwrap(),
        "\"front\""
    );
    assert_eq!(
        serde_json::to_string(&CameraPosition::External).unwrap(),
        "\"external\""
    );
}

#[test]
fn hand_serialises_correctly() {
    assert_eq!(serde_json::to_string(&Hand::Left).unwrap(), "\"left\"");
    assert_eq!(serde_json::to_string(&Hand::Face).unwrap(), "\"face\"");
}

#[test]
fn gesture_detection_serialises_fully() {
    // GIVEN: a GestureDetection with all fields populated
    let d = GestureDetection {
        gesture: Gesture::Wave,
        confidence: 0.95,
        hand: Hand::Right,
    };
    // WHEN: serialised
    let json = serde_json::to_string(&d).unwrap();
    // THEN: all fields present in snake_case
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["gesture"], "wave");
    assert_eq!(v["hand"], "right");
    assert!(v["confidence"].as_f64().unwrap() > 0.9);
}

// -----------------------------------------------------------------------
// ImageData
// -----------------------------------------------------------------------

#[test]
fn image_data_base64_encodes_deterministically() {
    // GIVEN: fixed JPEG bytes
    let image = ImageData {
        width: 2,
        height: 2,
        jpeg_data: vec![0xFF, 0xD8, 0xFF, 0xE0],
    };
    // WHEN: base64 encoded twice
    let a = image.base64_jpeg();
    let b = image.base64_jpeg();
    // THEN: same result
    assert_eq!(a, b);
    assert!(!a.is_empty());
}

#[test]
fn image_data_base64_decodes_back_to_original() {
    let original = vec![1u8, 2, 3, 4, 5];
    let image = ImageData {
        width: 1,
        height: 1,
        jpeg_data: original.clone(),
    };
    let encoded = image.base64_jpeg();
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&encoded)
        .unwrap();
    assert_eq!(decoded, original);
}

// -----------------------------------------------------------------------
// CameraError display
// -----------------------------------------------------------------------

#[test]
fn camera_error_permission_denied_contains_guidance() {
    let msg = CameraError::PermissionDenied.to_string();
    assert!(msg.contains("System Settings"));
    assert!(msg.contains("camera_denied"));
}

#[test]
fn camera_error_duration_exceeded_contains_value() {
    let msg = CameraError::DurationExceeded(90.0).to_string();
    assert!(msg.contains("90"));
    assert!(msg.contains("duration_exceeded"));
}

#[test]
fn camera_error_unknown_gesture_contains_name() {
    let msg = CameraError::UnknownGesture("flying_kick".into()).to_string();
    assert!(msg.contains("flying_kick"));
    assert!(msg.contains("unknown_gesture"));
}

#[test]
fn camera_error_device_not_found_contains_id() {
    let msg = CameraError::DeviceNotFound("com.acme.cam1".into()).to_string();
    assert!(msg.contains("com.acme.cam1"));
    assert!(msg.contains("device_not_found"));
}

#[test]
fn camera_error_capture_failed_contains_detail() {
    let msg = CameraError::CaptureFailed("timeout".into()).to_string();
    assert!(msg.contains("timeout"));
    assert!(msg.contains("capture_failed"));
}

// -----------------------------------------------------------------------
// CameraPosition / CameraDevice
// -----------------------------------------------------------------------

#[test]
fn camera_device_serialises_with_all_fields() {
    let device = CameraDevice {
        id: "AVFoundation:0".into(),
        name: "FaceTime HD Camera".into(),
        position: CameraPosition::Front,
        is_default: true,
    };
    let v: serde_json::Value = serde_json::to_value(&device).unwrap();
    assert_eq!(v["position"], "front");
    assert_eq!(v["is_default"], true);
    assert_eq!(v["name"], "FaceTime HD Camera");
}
