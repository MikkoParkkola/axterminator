//! Audio device enumeration and TCC microphone permission check.

use std::ffi::c_void;

use objc::runtime::Object;
#[allow(unused_imports)]
use objc::{msg_send, sel, sel_impl};
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::ffi::{
    cf_string_to_string, ns_string_from_str, objc_class, AudioObjectGetPropertyData,
    AudioObjectGetPropertyDataSize, AudioObjectPropertyAddress,
    K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN, K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
    K_AUDIO_OBJECT_PROPERTY_SCOPE_INPUT, K_AUDIO_OBJECT_PROPERTY_SCOPE_OUTPUT,
    K_AUDIO_OBJECT_PROPERTY_SELECTOR_DEFAULT_INPUT,
    K_AUDIO_OBJECT_PROPERTY_SELECTOR_DEFAULT_OUTPUT, K_AUDIO_OBJECT_PROPERTY_SELECTOR_DEVICES,
    K_AUDIO_OBJECT_PROPERTY_SELECTOR_NAME, K_AUDIO_OBJECT_PROPERTY_SELECTOR_NOMINAL_SAMPLE_RATE,
    K_AUDIO_OBJECT_PROPERTY_SELECTOR_STREAMS, K_AUDIO_OBJECT_SYSTEM_OBJECT,
};
use super::AudioError;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// An audio device descriptor as returned by [`list_audio_devices`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioDevice {
    /// Human-readable device name (e.g. "Built-in Microphone").
    pub name: String,
    /// CoreAudio `AudioDeviceID` as a decimal string.
    pub id: String,
    /// `true` if this device has input channels (can capture audio).
    pub is_input: bool,
    /// `true` if this device has output channels (can play audio).
    pub is_output: bool,
    /// Default sample rate reported by the device (Hz).
    pub sample_rate: f64,
    /// `true` if this is the system default input device.
    pub is_default_input: bool,
    /// `true` if this is the system default output device.
    pub is_default_output: bool,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Check TCC microphone authorisation status via AVFoundation.
///
/// Returns `Ok(())` when access is authorised, `Err(AudioError::PermissionDenied)`
/// when denied or restricted, and `Ok(())` optimistically when status is
/// "not determined" (first-run; the capture call triggers the dialog).
///
/// # Errors
///
/// Returns [`AudioError::PermissionDenied`] when the user has explicitly
/// denied microphone access (TCC status = `AVAuthorizationStatusDenied` = 2
/// or `AVAuthorizationStatusRestricted` = 1).
///
/// # Examples
///
/// ```ignore
/// use axterminator::audio::check_microphone_permission;
/// let result = check_microphone_permission();
/// assert!(result.is_ok() || matches!(result, Err(axterminator::audio::AudioError::PermissionDenied)));
/// ```
pub fn check_microphone_permission() -> Result<(), AudioError> {
    // AVAuthorizationStatus: 0=NotDetermined, 1=Restricted, 2=Denied, 3=Authorized
    let status = query_av_authorization_status();
    match status {
        3 => Ok(()),
        0 => {
            debug!("Microphone permission not determined, will prompt on first capture");
            Ok(())
        }
        _ => Err(AudioError::PermissionDenied),
    }
}

/// Enumerate all CoreAudio audio devices on the system.
///
/// Returns an empty `Vec` when the CoreAudio system object is unavailable
/// (unlikely on macOS but handled gracefully for tests on CI).
///
/// # Examples
///
/// ```ignore
/// use axterminator::audio::list_audio_devices;
/// let devices = list_audio_devices();
/// assert!(!devices.is_empty());
/// ```
#[must_use]
pub fn list_audio_devices() -> Vec<AudioDevice> {
    let device_ids = query_device_ids();
    if device_ids.is_empty() {
        return vec![];
    }

    let default_input = query_default_device(K_AUDIO_OBJECT_PROPERTY_SELECTOR_DEFAULT_INPUT);
    let default_output = query_default_device(K_AUDIO_OBJECT_PROPERTY_SELECTOR_DEFAULT_OUTPUT);

    device_ids
        .iter()
        .filter_map(|&id| build_audio_device(id, default_input, default_output))
        .collect()
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Query `AVCaptureDevice.authorizationStatus(for: .audio)` via ObjC.
///
/// Returns the raw `AVAuthorizationStatus` integer (0–3).
fn query_av_authorization_status() -> i64 {
    let media_type = ns_string_from_str("soun");
    let cls = objc_class("AVCaptureDevice");
    if cls.is_null() || media_type.is_null() {
        return 3; // Assume authorized when AVFoundation is unavailable (tests)
    }
    // SAFETY: AVCaptureDevice is a valid ObjC class; media_type is a valid NSString.
    unsafe { msg_send![cls, authorizationStatusForMediaType: media_type] }
}

/// Query all `AudioDeviceID`s from the system audio object.
fn query_device_ids() -> Vec<u32> {
    let addr = AudioObjectPropertyAddress {
        selector: K_AUDIO_OBJECT_PROPERTY_SELECTOR_DEVICES,
        scope: K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
        element: K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
    };

    let mut size: u32 = 0;
    let status = unsafe {
        AudioObjectGetPropertyDataSize(
            K_AUDIO_OBJECT_SYSTEM_OBJECT,
            &addr,
            0,
            std::ptr::null(),
            &mut size,
        )
    };
    if status != 0 || size == 0 {
        return vec![];
    }

    let count = size as usize / std::mem::size_of::<u32>();
    let mut ids = vec![0u32; count];
    let mut actual = size;
    let status = unsafe {
        AudioObjectGetPropertyData(
            K_AUDIO_OBJECT_SYSTEM_OBJECT,
            &addr,
            0,
            std::ptr::null(),
            &mut actual,
            ids.as_mut_ptr().cast::<c_void>(),
        )
    };
    if status != 0 {
        return vec![];
    }

    ids
}

/// Query the default input or output device ID. Returns 0 on failure.
fn query_default_device(selector: u32) -> u32 {
    let addr = AudioObjectPropertyAddress {
        selector,
        scope: K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
        element: K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
    };
    let mut device_id: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            K_AUDIO_OBJECT_SYSTEM_OBJECT,
            &addr,
            0,
            std::ptr::null(),
            &mut size,
            (&mut device_id as *mut u32).cast::<c_void>(),
        )
    };
    if status == 0 {
        device_id
    } else {
        0
    }
}

/// Build an [`AudioDevice`] for a CoreAudio device ID. Returns `None` when
/// the device name cannot be retrieved.
fn build_audio_device(id: u32, default_input: u32, default_output: u32) -> Option<AudioDevice> {
    let name = query_device_name(id)?;
    let is_input = device_has_streams(id, K_AUDIO_OBJECT_PROPERTY_SCOPE_INPUT);
    let is_output = device_has_streams(id, K_AUDIO_OBJECT_PROPERTY_SCOPE_OUTPUT);
    let sample_rate = query_nominal_sample_rate(id);

    Some(AudioDevice {
        name,
        id: id.to_string(),
        is_input,
        is_output,
        sample_rate,
        is_default_input: id == default_input,
        is_default_output: id == default_output,
    })
}

/// Query the human-readable name of a device via `kAudioObjectPropertyName`.
fn query_device_name(device_id: u32) -> Option<String> {
    let addr = AudioObjectPropertyAddress {
        selector: K_AUDIO_OBJECT_PROPERTY_SELECTOR_NAME,
        scope: K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
        element: K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
    };

    // The property returns a CFStringRef (pointer-sized).
    let mut cf_str: *mut Object = std::ptr::null_mut();
    let mut size = std::mem::size_of::<*mut Object>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            device_id,
            &addr,
            0,
            std::ptr::null(),
            &mut size,
            (&mut cf_str as *mut *mut Object).cast::<c_void>(),
        )
    };
    if status != 0 || cf_str.is_null() {
        return None;
    }

    // `kAudioObjectPropertyName` returns a +1 CFStringRef (Create Rule).
    // `cf_string_to_string` uses `wrap_under_create_rule` — no manual CFRelease needed.
    Some(cf_string_to_string(cf_str as *const c_void))
}

/// Return `true` if the device has at least one stream in the given scope.
fn device_has_streams(device_id: u32, scope: u32) -> bool {
    let addr = AudioObjectPropertyAddress {
        selector: K_AUDIO_OBJECT_PROPERTY_SELECTOR_STREAMS,
        scope,
        element: K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
    };
    let mut size: u32 = 0;
    let status =
        unsafe { AudioObjectGetPropertyDataSize(device_id, &addr, 0, std::ptr::null(), &mut size) };
    status == 0 && size > 0
}

/// Query the nominal sample rate of a device. Returns `0.0` when unavailable.
fn query_nominal_sample_rate(device_id: u32) -> f64 {
    let addr = AudioObjectPropertyAddress {
        selector: K_AUDIO_OBJECT_PROPERTY_SELECTOR_NOMINAL_SAMPLE_RATE,
        scope: K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
        element: K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
    };
    let mut rate: f64 = 0.0;
    let mut size = std::mem::size_of::<f64>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            device_id,
            &addr,
            0,
            std::ptr::null(),
            &mut size,
            (&mut rate as *mut f64).cast::<c_void>(),
        )
    };
    if status == 0 {
        rate
    } else {
        0.0
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_microphone_permission_returns_result() {
        // GIVEN: any system state
        // WHEN: permission is checked
        // THEN: returns either Ok or PermissionDenied — never panics
        let result = check_microphone_permission();
        match result {
            Ok(()) => {}
            Err(AudioError::PermissionDenied) => {}
            Err(e) => panic!("Unexpected error type: {e}"),
        }
    }

    #[test]
    fn list_audio_devices_returns_vec() {
        // GIVEN: a running macOS system
        // WHEN: devices are enumerated
        // THEN: Vec returned; every device has non-empty name and id
        let devices = list_audio_devices();
        for d in &devices {
            assert!(!d.name.is_empty(), "device name must not be empty");
            assert!(!d.id.is_empty(), "device id must not be empty");
        }
    }

    #[test]
    fn list_audio_devices_serializes_to_json() {
        let devices = list_audio_devices();
        let json = serde_json::to_string(&devices).unwrap();
        assert!(json.starts_with('['));
    }
}
