//! Gesture detection and monitoring via the Vision framework.
//!
//! This module provides both one-shot gesture detection on a supplied
//! [`ImageData`] frame and a polling loop that monitors the camera until a
//! target gesture is observed or a timeout elapses.

use std::ffi::CStr;
use std::time::Duration;

use tracing::debug;

use super::capture::capture_frame;
use super::{
    CGestureItem, CGestureList, CameraError, Gesture, GestureDetection, Hand, ImageData,
    check_camera_permission, validate_duration, validate_gesture_names, vn_detect_gestures,
    vn_free_gesture_list,
};

// ---------------------------------------------------------------------------
// Gesture detection
// ---------------------------------------------------------------------------

/// Detect gestures in a captured frame using the Vision framework.
///
/// Runs `VNDetectHumanHandPoseRequest` for hand gestures and
/// `VNDetectFaceLandmarksRequest` for nod/shake. Returns all detected
/// gestures with confidence scores.
///
/// On-device processing only — no images leave the machine.
///
/// # Errors
///
/// - [`CameraError::CaptureFailed`] — Vision framework returned an error.
///
/// # Examples
///
/// ```rust,no_run
/// use axterminator::camera::{capture_frame, detect_gestures, CameraError};
/// fn run() -> Result<(), CameraError> {
///     let frame = capture_frame(None)?;
///     let _detections = detect_gestures(&frame)?;
///     Ok(())
/// }
/// ```
pub fn detect_gestures(image: &ImageData) -> Result<Vec<GestureDetection>, CameraError> {
    debug!(
        width = image.width,
        height = image.height,
        "detecting gestures"
    );

    // Safety: vn_detect_gestures reads the jpeg bytes and writes into an
    // allocated CGestureList which we free after copying into Rust types.
    unsafe { invoke_detect_gestures(image) }
}

/// Capture a frame and detect gestures in a single operation.
///
/// This is the implementation behind `ax_gesture_detect`. Equivalent to
/// calling `capture_frame` then `detect_gestures` but avoids a clone.
///
/// # Errors
///
/// See [`capture_frame`] and [`detect_gestures`].
pub fn capture_and_detect(
    device_id: Option<&str>,
) -> Result<(ImageData, Vec<GestureDetection>), CameraError> {
    let frame = capture_frame(device_id)?;
    let gestures = detect_gestures(&frame)?;
    Ok((frame, gestures))
}

/// Monitor the camera for a specified set of gestures until `duration_secs`
/// elapses or a matching gesture is detected (whichever comes first).
///
/// Returns the first matching [`GestureDetection`] or `None` when the
/// duration expires without a match.
///
/// # Errors
///
/// - [`CameraError::DurationExceeded`] when `duration_secs > 60.0`.
/// - [`CameraError::PermissionDenied`] when camera access is denied.
/// - [`CameraError::UnknownGesture`] for invalid names in `gesture_names`.
///
/// # Examples
///
/// ```rust,no_run
/// use axterminator::camera::{gesture_listen, CameraError};
/// fn run() -> Result<(), CameraError> {
///     // Block for up to 5 seconds waiting for a thumbs-up
///     let _detection = gesture_listen(5.0, &["thumbs_up"])?;
///     Ok(())
/// }
/// ```
pub fn gesture_listen(
    duration_secs: f64,
    gesture_names: &[&str],
) -> Result<Option<GestureDetection>, CameraError> {
    let timeout = validate_duration(duration_secs)?;
    let wanted: Vec<Gesture> = validate_gesture_names(gesture_names)?;

    if !check_camera_permission() {
        return Err(CameraError::PermissionDenied);
    }

    poll_until_gesture(timeout, &wanted)
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// # Safety
///
/// `image.jpeg_data` must be a valid slice for the duration of the call.
/// `vn_detect_gestures` fills a `CGestureList` that we own and free.
unsafe fn invoke_detect_gestures(image: &ImageData) -> Result<Vec<GestureDetection>, CameraError> {
    let mut list = CGestureList {
        items: std::ptr::null_mut(),
        count: 0,
        error_msg: std::ptr::null(),
    };

    let ok = unsafe {
        vn_detect_gestures(
            image.jpeg_data.as_ptr(),
            image.jpeg_data.len(),
            std::ptr::addr_of_mut!(list),
        )
    };

    if !ok {
        let msg = if list.error_msg.is_null() {
            "Vision framework error".to_string()
        } else {
            unsafe { CStr::from_ptr(list.error_msg) }
                .to_string_lossy()
                .into_owned()
        };
        unsafe { vn_free_gesture_list(std::ptr::addr_of_mut!(list)) };
        return Err(CameraError::CaptureFailed(msg));
    }

    let detections = unsafe { collect_gesture_detections(&list) };
    unsafe { vn_free_gesture_list(std::ptr::addr_of_mut!(list)) };
    Ok(detections)
}

/// # Safety
///
/// `list.items` must be either null or a valid slice of `list.count` items.
unsafe fn collect_gesture_detections(list: &CGestureList) -> Vec<GestureDetection> {
    if list.count == 0 || list.items.is_null() {
        return Vec::new();
    }
    unsafe { std::slice::from_raw_parts(list.items, list.count) }
        .iter()
        .filter_map(c_gesture_to_rust)
        .collect()
}

fn poll_until_gesture(
    timeout: Duration,
    wanted: &[Gesture],
) -> Result<Option<GestureDetection>, CameraError> {
    let deadline = std::time::Instant::now() + timeout;
    let poll_interval = Duration::from_millis(200);

    while std::time::Instant::now() < deadline {
        let frame = capture_frame(None)?;
        let detections = detect_gestures(&frame)?;

        if let Some(hit) = detections.into_iter().find(|d| wanted.contains(&d.gesture)) {
            return Ok(Some(hit));
        }

        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        std::thread::sleep(poll_interval.min(remaining));
    }

    Ok(None)
}

fn c_gesture_to_rust(item: &CGestureItem) -> Option<GestureDetection> {
    // Safety: gesture_name is a valid NUL-terminated C string from our own
    // Objective-C code. hand_code is a bounded discriminant (0-3).
    let name = unsafe { CStr::from_ptr(item.gesture_name).to_string_lossy() };
    let gesture = Gesture::from_name(name.as_ref()).ok()?;
    let hand = match item.hand_code {
        0 => Hand::Left,
        1 => Hand::Right,
        2 => Hand::Face,
        _ => Hand::Unknown,
    };
    Some(GestureDetection {
        gesture,
        confidence: item.confidence,
        hand,
    })
}
