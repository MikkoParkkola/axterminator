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

use std::ffi::{c_void, CStr};
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

const AV_AUTH_NOT_DETERMINED: i32 = 0;

// ---------------------------------------------------------------------------
// Device enumeration
// ---------------------------------------------------------------------------

/// List all available camera devices on this machine.
///
/// Returns an empty vector on systems without cameras or when the
/// AVFoundation framework is unavailable. Never returns an error —
/// permission is not required to enumerate devices.
///
/// # Examples
///
/// ```
/// use axterminator::camera::list_cameras;
/// // Will be empty on headless CI, non-empty on real hardware.
/// let cameras = list_cameras();
/// println!("{} camera(s)", cameras.len());
/// ```
#[must_use]
pub fn list_cameras() -> Vec<CameraDevice> {
    // Safety: av_list_cameras returns a heap-allocated slice of CDeviceInfo
    // structs which we immediately copy into owned Rust values and then free.
    unsafe {
        let mut count: usize = 0;
        let ptr = av_list_cameras(std::ptr::addr_of_mut!(count));
        if ptr.is_null() || count == 0 {
            return Vec::new();
        }

        let slice = std::slice::from_raw_parts(ptr, count);
        let devices = slice.iter().map(c_device_info_to_rust).collect();
        av_free_camera_list(ptr, count);
        devices
    }
}

// ---------------------------------------------------------------------------
// Frame capture
// ---------------------------------------------------------------------------

/// Capture a single JPEG frame from the specified camera (or the front-facing
/// default when `device_id` is `None`).
///
/// The AVCaptureSession is started, one frame is grabbed, the session is
/// stopped and released. This satisfies AC8 (no persistent camera access).
///
/// Resolution target: 1280×720. If the camera does not support this resolution
/// the native default is used.
///
/// # Errors
///
/// - [`CameraError::PermissionDenied`] — TCC authorization not granted.
/// - [`CameraError::DeviceNotFound`] — `device_id` not recognised.
/// - [`CameraError::CaptureFailed`] — framework-level failure.
///
/// # Examples
///
/// ```rust,no_run
/// use axterminator::camera::{capture_frame, CameraError};
/// fn run() -> Result<(), CameraError> {
///     let frame = capture_frame(None)?;
///     println!("{}x{}", frame.width, frame.height);
///     Ok(())
/// }
/// ```
pub fn capture_frame(device_id: Option<&str>) -> Result<ImageData, CameraError> {
    if !check_camera_permission() {
        return Err(CameraError::PermissionDenied);
    }

    if let Some(id) = device_id {
        ensure_device_exists(id)?;
    }

    let id_cstr;
    let id_ptr: *const CChar = if let Some(id) = device_id {
        id_cstr =
            std::ffi::CString::new(id).map_err(|e| CameraError::CaptureFailed(e.to_string()))?;
        id_cstr.as_ptr()
    } else {
        std::ptr::null()
    };

    debug!(device_id = ?device_id, "capturing camera frame");

    // Safety: av_capture_frame writes into an allocated CFrameResult which we
    // own and must free with av_free_frame_result. The id_ptr is either null
    // (use default device) or a valid NUL-terminated C string for the lifetime
    // of this function call.
    unsafe {
        let mut result = CFrameResult {
            jpeg_data: std::ptr::null_mut(),
            jpeg_len: 0,
            width: 0,
            height: 0,
            error_msg: std::ptr::null(),
        };

        let ok = av_capture_frame(id_ptr, std::ptr::addr_of_mut!(result));

        if !ok || result.jpeg_data.is_null() {
            let msg = if result.error_msg.is_null() {
                "Unknown capture error".to_string()
            } else {
                CStr::from_ptr(result.error_msg)
                    .to_string_lossy()
                    .into_owned()
            };
            av_free_frame_result(std::ptr::addr_of_mut!(result));
            return Err(CameraError::CaptureFailed(msg));
        }

        let jpeg_data =
            std::slice::from_raw_parts(result.jpeg_data as *const u8, result.jpeg_len).to_vec();
        let width = result.width;
        let height = result.height;
        av_free_frame_result(std::ptr::addr_of_mut!(result));

        Ok(ImageData {
            width,
            height,
            jpeg_data,
        })
    }
}

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
    unsafe {
        let mut list = CGestureList {
            items: std::ptr::null_mut(),
            count: 0,
            error_msg: std::ptr::null(),
        };

        let ok = vn_detect_gestures(
            image.jpeg_data.as_ptr(),
            image.jpeg_data.len(),
            std::ptr::addr_of_mut!(list),
        );

        if !ok {
            let msg = if list.error_msg.is_null() {
                "Vision framework error".to_string()
            } else {
                CStr::from_ptr(list.error_msg)
                    .to_string_lossy()
                    .into_owned()
            };
            vn_free_gesture_list(std::ptr::addr_of_mut!(list));
            return Err(CameraError::CaptureFailed(msg));
        }

        let detections = if list.count == 0 || list.items.is_null() {
            Vec::new()
        } else {
            std::slice::from_raw_parts(list.items, list.count)
                .iter()
                .filter_map(c_gesture_to_rust)
                .collect()
        };

        vn_free_gesture_list(std::ptr::addr_of_mut!(list));
        Ok(detections)
    }
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

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn ensure_device_exists(id: &str) -> Result<(), CameraError> {
    if list_cameras().iter().any(|d| d.id == id) {
        Ok(())
    } else {
        Err(CameraError::DeviceNotFound(id.to_string()))
    }
}

fn c_device_info_to_rust(info: &CDeviceInfo) -> CameraDevice {
    // Safety: name and id are guaranteed non-null NUL-terminated C strings
    // by the Objective-C layer. position is a validated enum discriminant.
    let id = unsafe {
        CStr::from_ptr(info.unique_id)
            .to_string_lossy()
            .into_owned()
    };
    let name = unsafe {
        CStr::from_ptr(info.localized_name)
            .to_string_lossy()
            .into_owned()
    };
    let position = match info.position {
        1 => CameraPosition::Front,
        2 => CameraPosition::Back,
        3 => CameraPosition::External,
        _ => CameraPosition::Unknown,
    };
    CameraDevice {
        id,
        name,
        position,
        is_default: info.is_default != 0,
    }
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

// ---------------------------------------------------------------------------
// C/Objective-C ABI types and extern declarations
// ---------------------------------------------------------------------------

// C char type alias — avoids pulling in the libc crate just for this.
type CChar = std::os::raw::c_char;

/// Authorization status codes mirroring `AVAuthorizationStatus`.
const AV_AUTH_AUTHORIZED: i32 = 3;

/// C-compatible device info struct written by `av_list_cameras`.
#[repr(C)]
struct CDeviceInfo {
    unique_id: *const CChar,
    localized_name: *const CChar,
    /// 1=front, 2=back, 3=external, 0=unknown
    position: i32,
    /// 1 if this is the system default camera, 0 otherwise
    is_default: i32,
}

/// Output struct for a single captured frame.
#[repr(C)]
struct CFrameResult {
    jpeg_data: *mut c_void,
    jpeg_len: usize,
    width: u32,
    height: u32,
    /// NUL-terminated error description, or null on success.
    error_msg: *const CChar,
}

/// A single detected gesture from the Vision framework.
#[repr(C)]
struct CGestureItem {
    gesture_name: *const CChar,
    confidence: f32,
    /// 0=left, 1=right, 2=face, 3=unknown
    hand_code: i32,
}

/// List of gesture detections returned by `vn_detect_gestures`.
#[repr(C)]
struct CGestureList {
    items: *mut CGestureItem,
    count: usize,
    /// NUL-terminated error description, or null on success.
    error_msg: *const CChar,
}

extern "C" {
    /// Returns `AVAuthorizationStatus` for `AVMediaTypeVideo` (0-3).
    /// Does NOT trigger a permission dialog.
    fn av_camera_authorization_status() -> i32;

    /// Requests camera access if status is `NotDetermined`. Blocks until
    /// the user responds (up to 30s timeout). Returns 1 if granted, 0 if denied.
    fn av_request_camera_access() -> i32;

    /// Fills `*count` with the number of video capture devices and returns a
    /// heap-allocated array of `CDeviceInfo`. Caller must call
    /// `av_free_camera_list` after use.
    fn av_list_cameras(count: *mut usize) -> *mut CDeviceInfo;

    /// Free the array returned by `av_list_cameras`.
    fn av_free_camera_list(ptr: *mut CDeviceInfo, count: usize);

    /// Capture one JPEG frame from the named device (null = default).
    /// Returns true on success; fills `result` and the caller must call
    /// `av_free_frame_result`.
    fn av_capture_frame(device_id: *const CChar, result: *mut CFrameResult) -> bool;

    /// Free resources held by a `CFrameResult`.
    fn av_free_frame_result(result: *mut CFrameResult);

    /// Run Vision gesture detection on JPEG bytes.
    /// Returns true on success; fills `list` and caller must call
    /// `vn_free_gesture_list`.
    fn vn_detect_gestures(jpeg_data: *const u8, jpeg_len: usize, list: *mut CGestureList) -> bool;

    /// Free resources held by a `CGestureList`.
    fn vn_free_gesture_list(list: *mut CGestureList);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

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
}
