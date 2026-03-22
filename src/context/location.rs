//! Geolocation via `CLLocationManager`.
//!
//! Requires the `context` cargo feature and Location Services TCC permission.
//! Uses a one-shot location request that resolves within a few seconds.
//!
//! CoreLocation works in CLI tools without Info.plist on macOS — the system
//! prompts for Location Services access via the Terminal/parent app.

use std::time::Duration;

use objc::runtime::Object;
#[allow(unused_imports)]
use objc::{msg_send, sel, sel_impl};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

fn objc_class(name: &str) -> *const objc::runtime::Class {
    use std::ffi::CString;
    let c = CString::new(name).unwrap_or_default();
    unsafe { objc::runtime::objc_getClass(c.as_ptr()) }
}

fn ns_string_to_rust(ns: *mut Object) -> String {
    if ns.is_null() {
        return String::new();
    }
    let utf8: *const u8 = unsafe { msg_send![ns, UTF8String] };
    if utf8.is_null() {
        return String::new();
    }
    unsafe {
        std::ffi::CStr::from_ptr(utf8 as *const std::ffi::c_char)
            .to_string_lossy()
            .into_owned()
    }
}

/// Geolocation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    /// Latitude in degrees (WGS-84).
    pub latitude: f64,
    /// Longitude in degrees (WGS-84).
    pub longitude: f64,
    /// Horizontal accuracy in meters (lower is better).
    pub accuracy_m: f64,
    /// Altitude in meters above sea level (if available).
    pub altitude: Option<f64>,
    /// Timestamp of the location fix (ISO 8601).
    pub timestamp: String,
}

/// Geolocation error.
#[derive(Debug, thiserror::Error)]
pub enum LocationError {
    #[error("Location Services disabled. Enable in System Settings > Privacy & Security > Location Services.")]
    Disabled,
    #[error("Location permission denied for this app. Grant access in System Settings > Privacy & Security > Location Services.")]
    PermissionDenied,
    #[error("Location request timed out after {0}s — try again or check GPS signal.")]
    Timeout(u64),
    #[error("CoreLocation error: {0}")]
    Framework(String),
}

impl LocationError {
    /// Machine-readable error code for MCP responses.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Disabled => "location_disabled",
            Self::PermissionDenied => "location_denied",
            Self::Timeout(_) => "location_timeout",
            Self::Framework(_) => "location_error",
        }
    }
}

/// Request the current location (one-shot).
///
/// Blocks the calling thread for up to `timeout` seconds waiting for a fix.
/// Uses `CLLocationManager.requestLocation()` (macOS 10.14+).
///
/// # Errors
///
/// - [`LocationError::Disabled`] when Location Services are off system-wide.
/// - [`LocationError::PermissionDenied`] when the app lacks location permission.
/// - [`LocationError::Timeout`] when no fix arrives within the timeout.
pub fn request_location(timeout: Duration) -> Result<Location, LocationError> {
    // Check if Location Services are enabled.
    let cls = objc_class("CLLocationManager");
    if cls.is_null() {
        return Err(LocationError::Framework(
            "CLLocationManager class not available".to_string(),
        ));
    }

    let enabled: bool = unsafe { msg_send![cls, locationServicesEnabled] };
    if !enabled {
        return Err(LocationError::Disabled);
    }

    // Check authorization status.
    // CLAuthorizationStatus: 0=notDetermined, 1=restricted, 2=denied,
    //   3=authorizedAlways, 4=authorizedWhenInUse
    let status: i32 = unsafe { msg_send![cls, authorizationStatus] };
    debug!(status, "CLLocationManager authorization status");

    match status {
        1 | 2 => return Err(LocationError::PermissionDenied),
        0 => {
            info!("Location permission not determined — requesting authorization");
        }
        _ => {} // 3 or 4 = authorized
    }

    // Create CLLocationManager and request location.
    // Note: CLLocationManager requires the thread to have a RunLoop.
    // We pump the RunLoop while waiting for the delegate callback.
    let manager: *mut Object = unsafe {
        let obj: *mut Object = msg_send![cls, alloc];
        msg_send![obj, init]
    };
    if manager.is_null() {
        return Err(LocationError::Framework(
            "Failed to create CLLocationManager".to_string(),
        ));
    }

    // Set desired accuracy to best.
    unsafe {
        let _: () = msg_send![manager, setDesiredAccuracy: -1.0f64]; // kCLLocationAccuracyBest = -1
    }

    // Request authorization if not yet determined. On macOS, CLI tools can
    // request "when in use" authorization. The system dialog may appear
    // via the parent terminal app.
    if status == 0 {
        unsafe {
            let _: () = msg_send![manager, requestWhenInUseAuthorization];
        }
        // Pump RunLoop briefly to allow the auth dialog to appear.
        extern "C" {
            fn CFRunLoopRunInMode(mode: *const Object, seconds: f64, ret: bool) -> i32;
            static kCFRunLoopDefaultMode: *const Object;
        }
        for _ in 0..20 {
            unsafe {
                CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.1, false);
            }
            // Re-check auth status.
            let new_status: i32 = unsafe { msg_send![cls, authorizationStatus] };
            if new_status == 3 || new_status == 4 {
                debug!(new_status, "Location authorization granted");
                break;
            }
            if new_status == 1 || new_status == 2 {
                return Err(LocationError::PermissionDenied);
            }
        }
    }

    // Start updating location and poll the `location` property.
    unsafe {
        let _: () = msg_send![manager, startUpdatingLocation];
    }

    // Pump RunLoop and poll for location.
    extern "C" {
        fn CFRunLoopRunInMode(mode: *const Object, seconds: f64, ret: bool) -> i32;
        static kCFRunLoopDefaultMode: *const Object;
    }

    let deadline = std::time::Instant::now() + timeout;
    loop {
        // Check if location is available.
        let loc: *mut Object = unsafe { msg_send![manager, location] };
        if !loc.is_null() {
            let coord: (f64, f64) = unsafe { msg_send![loc, coordinate] };
            let h_acc: f64 = unsafe { msg_send![loc, horizontalAccuracy] };
            let alt: f64 = unsafe { msg_send![loc, altitude] };
            let v_acc: f64 = unsafe { msg_send![loc, verticalAccuracy] };

            // Negative accuracy means invalid.
            if h_acc >= 0.0 {
                unsafe {
                    let _: () = msg_send![manager, stopUpdatingLocation];
                }

                // Get timestamp.
                let ts: *mut Object = unsafe { msg_send![loc, timestamp] };
                let desc: *mut Object = if ts.is_null() {
                    std::ptr::null_mut()
                } else {
                    unsafe { msg_send![ts, description] }
                };
                let timestamp = ns_string_to_rust(desc);

                return Ok(Location {
                    latitude: coord.0,
                    longitude: coord.1,
                    accuracy_m: h_acc,
                    altitude: if v_acc >= 0.0 { Some(alt) } else { None },
                    timestamp,
                });
            }
        }

        if std::time::Instant::now() >= deadline {
            unsafe {
                let _: () = msg_send![manager, stopUpdatingLocation];
            }
            warn!("Location request timed out");
            return Err(LocationError::Timeout(timeout.as_secs()));
        }

        // Pump RunLoop for 200ms.
        unsafe {
            CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.2, false);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn location_error_codes() {
        assert_eq!(LocationError::Disabled.code(), "location_disabled");
        assert_eq!(LocationError::PermissionDenied.code(), "location_denied");
        assert_eq!(LocationError::Timeout(10).code(), "location_timeout");
        assert_eq!(
            LocationError::Framework("test".to_string()).code(),
            "location_error"
        );
    }

    #[test]
    fn location_serializes() {
        let loc = Location {
            latitude: 60.1699,
            longitude: 24.9384,
            accuracy_m: 10.0,
            altitude: Some(15.0),
            timestamp: "2026-03-22T12:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&loc).unwrap();
        assert!(json.contains("60.1699"));
        assert!(json.contains("24.9384"));
    }
}
