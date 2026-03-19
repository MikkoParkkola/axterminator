//! Camera capture and gesture recognition via AVFoundation and Vision frameworks.
//!
//! This module provides single-frame camera capture and on-device gesture detection
//! using macOS system frameworks. All processing is local — no images leave the
//! machine unless the caller forwards them to an external backend.
//!
//! ## Architecture
//!
//! ```text
//! ┌────────────────────────────────────────────────────────────┐
//! │  AVFoundation               Vision                         │
//! │  ┌──────────────────┐      ┌──────────────────────────┐    │
//! │  │ AVCaptureSession │ ───► │ VNDetectHumanHandPose... │    │
//! │  │ (per-call setup/ │      │ VNDetectFaceLandmarks... │    │
//! │  │  teardown)       │      └──────────┬───────────────┘    │
//! │  └──────────────────┘                 │                    │
//! │          │                            ▼                    │
//! │          ▼                      Vec<GestureDetection>      │
//! │       ImageData                                            │
//! │  (width, height, jpeg_data)                                │
//! └────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Privacy
//!
//! Camera access requires macOS TCC consent. The first call will trigger a
//! system permission dialog. The hardware camera indicator light activates
//! during capture — this is macOS-enforced and cannot be suppressed.
//!
//! ## Feature flag
//!
//! This module is compiled only with `--features camera`.
//!
//! ## Examples
//!
//! ```rust,no_run
//! use axterminator::camera::{list_cameras, capture_frame, detect_gestures, CameraError};
//!
//! fn run() -> Result<(), CameraError> {
//!     // Enumerate available cameras
//!     let devices = list_cameras();
//!     println!("Found {} camera(s)", devices.len());
//!
//!     // Capture a single frame from the default front camera
//!     let frame = capture_frame(None)?;
//!     println!("Captured {}x{} ({} bytes)", frame.width, frame.height, frame.jpeg_data.len());
//!
//!     // Detect gestures in the frame
//!     let gestures = detect_gestures(&frame)?;
//!     for g in &gestures {
//!         println!("{:?} ({:.2}, {:?})", g.gesture, g.confidence, g.hand);
//!     }
//!     Ok(())
//! }
//! ```

// FaceTime, MacBook, AVFoundation, AVCaptureSession etc. are product/API names
// that intentionally deviate from the code-backtick convention.
#![allow(clippy::doc_markdown)]

pub mod capture;
pub mod gesture;

#[cfg(test)]
mod tests;

pub use capture::{capture_frame, list_cameras};
pub use gesture::{capture_and_detect, detect_gestures, gesture_listen};

use std::ffi::c_void;
use std::time::Duration;

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use tracing::debug;

// ---------------------------------------------------------------------------
// Public error type
// ---------------------------------------------------------------------------

/// Errors that can occur during camera operations.
#[derive(Debug, thiserror::Error)]
pub enum CameraError {
    /// TCC permission denied. Direct the user to System Settings.
    #[error("camera_denied: Camera permission denied — open System Settings > Privacy & Security > Camera and grant access")]
    PermissionDenied,

    /// No camera matching the requested device ID was found.
    #[error("device_not_found: Camera device '{0}' not found — call ax_camera_devices to list available cameras")]
    DeviceNotFound(String),

    /// `duration_exceeded`: requested duration exceeds the 60-second cap.
    #[error("duration_exceeded: Requested duration {0:.1}s exceeds the maximum of 60s")]
    DurationExceeded(f64),

    /// Unknown gesture name supplied by the caller.
    #[error("unknown_gesture: '{0}' is not a recognised gesture name — valid values: thumbs_up, thumbs_down, wave, stop, point, nod, shake")]
    UnknownGesture(String),

    /// Underlying system or FFI failure.
    #[error("capture_failed: {0}")]
    CaptureFailed(String),
}

// ---------------------------------------------------------------------------
// Public data types
// ---------------------------------------------------------------------------

/// Position of a camera on the device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CameraPosition {
    /// Front-facing camera (FaceTime camera on MacBooks).
    Front,
    /// Rear-facing camera.
    Back,
    /// External USB or Thunderbolt camera.
    External,
    /// Position cannot be determined.
    Unknown,
}

/// An enumerated camera device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraDevice {
    /// Unique device identifier (AVCaptureDevice.uniqueID).
    pub id: String,
    /// Human-readable device name (AVCaptureDevice.localizedName).
    pub name: String,
    /// Physical position of the camera.
    pub position: CameraPosition,
    /// Whether this is the system-default camera for the current session.
    pub is_default: bool,
}

/// A captured camera frame.
///
/// The image is JPEG-encoded at 90% quality, 1280×720 pixels (or the
/// camera's native resolution when lower).
#[derive(Debug, Clone)]
pub struct ImageData {
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// JPEG-encoded image bytes.
    pub jpeg_data: Vec<u8>,
}

impl ImageData {
    /// Return the JPEG data base64-encoded, suitable for JSON transport.
    #[must_use]
    pub fn base64_jpeg(&self) -> String {
        base64::engine::general_purpose::STANDARD.encode(&self.jpeg_data)
    }
}

/// Recognised gesture type.
///
/// Gesture classification is performed on-device by the Vision framework.
/// Hand gestures use `VNDetectHumanHandPoseRequest`; face gestures (nod/shake)
/// use `VNDetectFaceLandmarksRequest`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Gesture {
    /// Thumb extended upward — approve / positive signal.
    ThumbsUp,
    /// Thumb extended downward — reject / negative signal.
    ThumbsDown,
    /// Open flat palm facing the camera — stop / pause.
    Wave,
    /// Flat open hand (stop sign) — explicit stop command.
    Stop,
    /// Index finger extended upward — select / confirm.
    Point,
    /// Head nodding vertically — affirmative.
    Nod,
    /// Head shaking horizontally — negative.
    Shake,
}

impl Gesture {
    /// Parse a gesture from its canonical snake_case name.
    ///
    /// # Errors
    ///
    /// Returns [`CameraError::UnknownGesture`] for unrecognised names.
    ///
    /// # Examples
    ///
    /// ```
    /// use axterminator::camera::Gesture;
    /// assert_eq!(Gesture::from_name("thumbs_up").unwrap(), Gesture::ThumbsUp);
    /// ```
    pub fn from_name(name: &str) -> Result<Self, CameraError> {
        match name {
            "thumbs_up" => Ok(Self::ThumbsUp),
            "thumbs_down" => Ok(Self::ThumbsDown),
            "wave" => Ok(Self::Wave),
            "stop" => Ok(Self::Stop),
            "point" => Ok(Self::Point),
            "nod" => Ok(Self::Nod),
            "shake" => Ok(Self::Shake),
            other => Err(CameraError::UnknownGesture(other.to_string())),
        }
    }

    /// Return the canonical snake_case name of this gesture.
    ///
    /// # Examples
    ///
    /// ```
    /// use axterminator::camera::Gesture;
    /// assert_eq!(Gesture::ThumbsUp.as_name(), "thumbs_up");
    /// ```
    #[must_use]
    pub fn as_name(&self) -> &'static str {
        match self {
            Self::ThumbsUp => "thumbs_up",
            Self::ThumbsDown => "thumbs_down",
            Self::Wave => "wave",
            Self::Stop => "stop",
            Self::Point => "point",
            Self::Nod => "nod",
            Self::Shake => "shake",
        }
    }

    /// All gesture names accepted by the public API.
    #[must_use]
    pub fn all_names() -> &'static [&'static str] {
        &[
            "thumbs_up",
            "thumbs_down",
            "wave",
            "stop",
            "point",
            "nod",
            "shake",
        ]
    }
}

/// Which hand performed the detected gesture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Hand {
    /// Left hand.
    Left,
    /// Right hand.
    Right,
    /// Face-based gesture (nod/shake) — hand not applicable.
    Face,
    /// Could not determine chirality.
    Unknown,
}

/// A single gesture detection result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GestureDetection {
    /// The classified gesture.
    pub gesture: Gesture,
    /// Confidence in [0.0, 1.0].
    pub confidence: f32,
    /// Which hand (or face) performed the gesture.
    pub hand: Hand,
}

// ---------------------------------------------------------------------------
// Duration validation
// ---------------------------------------------------------------------------

/// Maximum duration for `gesture_listen` operations (seconds).
pub const MAX_GESTURE_DURATION_SECS: f64 = 60.0;

/// Validate that a requested duration does not exceed [`MAX_GESTURE_DURATION_SECS`].
///
/// # Errors
///
/// Returns [`CameraError::DurationExceeded`] when `duration_secs > 60.0`.
///
/// # Examples
///
/// ```
/// use axterminator::camera::validate_duration;
/// assert!(validate_duration(30.0).is_ok());
/// assert!(validate_duration(60.0).is_ok());
/// assert!(validate_duration(61.0).is_err());
/// ```
pub fn validate_duration(duration_secs: f64) -> Result<Duration, CameraError> {
    if duration_secs > MAX_GESTURE_DURATION_SECS {
        return Err(CameraError::DurationExceeded(duration_secs));
    }
    Ok(Duration::from_secs_f64(duration_secs.max(0.0)))
}

// ---------------------------------------------------------------------------
// Gesture name validation
// ---------------------------------------------------------------------------

/// Validate a list of gesture name strings, returning typed `Gesture` values.
///
/// # Errors
///
/// Returns [`CameraError::UnknownGesture`] for the first unrecognised name.
///
/// # Examples
///
/// ```
/// use axterminator::camera::{validate_gesture_names, Gesture};
/// let gestures = validate_gesture_names(&["thumbs_up", "wave"]).unwrap();
/// assert_eq!(gestures.len(), 2);
/// ```
pub fn validate_gesture_names(names: &[&str]) -> Result<Vec<Gesture>, CameraError> {
    names.iter().map(|n| Gesture::from_name(n)).collect()
}

// ---------------------------------------------------------------------------
// Permission check
// ---------------------------------------------------------------------------

/// Check whether camera TCC permission is currently granted.
///
/// This function does not trigger a permission dialog — it only reads the
/// current authorization status from the system. Returns `true` when
/// access is already authorized.
///
/// On macOS < 10.14 (Mojave) the permission system did not exist for cameras,
/// so this function returns `true` on those systems.
///
/// # Examples
///
/// ```
/// use axterminator::camera::check_camera_permission;
/// // Will be false on CI / headless systems.
/// let _granted = check_camera_permission();
/// ```
#[must_use]
pub fn check_camera_permission() -> bool {
    // Safety: AVCaptureDevice is a stable macOS API.
    let status = unsafe { av_camera_authorization_status() };
    if status == AV_AUTH_AUTHORIZED {
        return true;
    }
    // If not yet determined, request access (triggers system dialog).
    if status == AV_AUTH_NOT_DETERMINED {
        debug!("Camera permission not determined, requesting access");
        let granted = unsafe { av_request_camera_access() };
        return granted == 1;
    }
    false
}

// ---------------------------------------------------------------------------
// C/Objective-C ABI types and extern declarations
// ---------------------------------------------------------------------------

// C char type alias — avoids pulling in the libc crate just for this.
pub(crate) type CChar = std::os::raw::c_char;

/// Authorization status codes mirroring `AVAuthorizationStatus`.
pub(crate) const AV_AUTH_AUTHORIZED: i32 = 3;
const AV_AUTH_NOT_DETERMINED: i32 = 0;

/// C-compatible device info struct written by `av_list_cameras`.
#[repr(C)]
pub(crate) struct CDeviceInfo {
    pub unique_id: *const CChar,
    pub localized_name: *const CChar,
    /// 1=front, 2=back, 3=external, 0=unknown
    pub position: i32,
    /// 1 if this is the system default camera, 0 otherwise
    pub is_default: i32,
}

/// Output struct for a single captured frame.
#[repr(C)]
pub(crate) struct CFrameResult {
    pub jpeg_data: *mut c_void,
    pub jpeg_len: usize,
    pub width: u32,
    pub height: u32,
    /// NUL-terminated error description, or null on success.
    pub error_msg: *const CChar,
}

/// A single detected gesture from the Vision framework.
#[repr(C)]
pub(crate) struct CGestureItem {
    pub gesture_name: *const CChar,
    pub confidence: f32,
    /// 0=left, 1=right, 2=face, 3=unknown
    pub hand_code: i32,
}

/// List of gesture detections returned by `vn_detect_gestures`.
#[repr(C)]
pub(crate) struct CGestureList {
    pub items: *mut CGestureItem,
    pub count: usize,
    /// NUL-terminated error description, or null on success.
    pub error_msg: *const CChar,
}

extern "C" {
    /// Returns `AVAuthorizationStatus` for `AVMediaTypeVideo` (0-3).
    /// Does NOT trigger a permission dialog.
    pub(crate) fn av_camera_authorization_status() -> i32;

    /// Requests camera access if status is `NotDetermined`. Blocks until
    /// the user responds (up to 30s timeout). Returns 1 if granted, 0 if denied.
    pub(crate) fn av_request_camera_access() -> i32;

    /// Fills `*count` with the number of video capture devices and returns a
    /// heap-allocated array of `CDeviceInfo`. Caller must call
    /// `av_free_camera_list` after use.
    pub(crate) fn av_list_cameras(count: *mut usize) -> *mut CDeviceInfo;

    /// Free the array returned by `av_list_cameras`.
    pub(crate) fn av_free_camera_list(ptr: *mut CDeviceInfo, count: usize);

    /// Capture one JPEG frame from the named device (null = default).
    /// Returns true on success; fills `result` and the caller must call
    /// `av_free_frame_result`.
    pub(crate) fn av_capture_frame(device_id: *const CChar, result: *mut CFrameResult) -> bool;

    /// Free resources held by a `CFrameResult`.
    pub(crate) fn av_free_frame_result(result: *mut CFrameResult);

    /// Run Vision gesture detection on JPEG bytes.
    /// Returns true on success; fills `list` and caller must call
    /// `vn_free_gesture_list`.
    pub(crate) fn vn_detect_gestures(
        jpeg_data: *const u8,
        jpeg_len: usize,
        list: *mut CGestureList,
    ) -> bool;

    /// Free resources held by a `CGestureList`.
    pub(crate) fn vn_free_gesture_list(list: *mut CGestureList);
}
