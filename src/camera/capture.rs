//! Camera device enumeration and single-frame capture.
//!
//! All functions in this module interact with AVFoundation via the C FFI layer
//! declared in `mod.rs`. No persistent camera session is held between calls.

use std::ffi::CStr;

use tracing::debug;

use super::{
    av_capture_frame, av_free_camera_list, av_free_frame_result, av_list_cameras,
    check_camera_permission, CChar, CDeviceInfo, CFrameResult, CameraDevice, CameraError,
    CameraPosition, ImageData,
};

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

    let id_ptr = build_id_ptr(device_id)?;
    debug!(device_id = ?device_id, "capturing camera frame");

    // Safety: av_capture_frame writes into an allocated CFrameResult which we
    // own and must free with av_free_frame_result. The id_ptr is either null
    // (use default device) or a valid NUL-terminated C string for the lifetime
    // of this function call.
    unsafe { invoke_capture_frame(id_ptr.as_ref()) }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Build a `(Option<CString>, *const CChar)` pair for the optional device ID.
///
/// Returns the `CString` so it lives long enough for the FFI call, along with
/// the raw pointer (null when `device_id` is `None`).
fn build_id_ptr(device_id: Option<&str>) -> Result<Option<std::ffi::CString>, CameraError> {
    device_id
        .map(|id| std::ffi::CString::new(id).map_err(|e| CameraError::CaptureFailed(e.to_string())))
        .transpose()
}

/// # Safety
///
/// `id_cstr` must be either `None` (passes null) or a valid NUL-terminated
/// `CString` whose data outlives this call. `av_capture_frame` writes into
/// `result` which is stack-allocated here and freed before return.
unsafe fn invoke_capture_frame(
    id_cstr: Option<&std::ffi::CString>,
) -> Result<ImageData, CameraError> {
    let id_ptr: *const CChar = id_cstr.map_or(std::ptr::null(), |c| c.as_ptr());

    let mut result = CFrameResult {
        jpeg_data: std::ptr::null_mut(),
        jpeg_len: 0,
        width: 0,
        height: 0,
        error_msg: std::ptr::null(),
    };

    let ok = av_capture_frame(id_ptr, std::ptr::addr_of_mut!(result));

    if !ok || result.jpeg_data.is_null() {
        let msg = error_msg_from_ptr(result.error_msg)
            .unwrap_or_else(|| "Unknown capture error".to_string());
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

/// Convert a nullable C error string pointer to an `Option<String>`.
///
/// # Safety
///
/// `ptr` must be either null or a valid NUL-terminated C string.
unsafe fn error_msg_from_ptr(ptr: *const CChar) -> Option<String> {
    if ptr.is_null() {
        None
    } else {
        Some(CStr::from_ptr(ptr).to_string_lossy().into_owned())
    }
}

pub(crate) fn ensure_device_exists(id: &str) -> Result<(), CameraError> {
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
