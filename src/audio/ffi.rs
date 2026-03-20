//! Private ObjC / CoreAudio FFI helpers shared across the audio sub-modules.
//!
//! None of the symbols here are part of the public API; they are all
//! `pub(super)` so that sibling modules can share them without leaking
//! implementation details to crate consumers.

use std::ffi::c_void;

use objc::runtime::{Class, Object};
#[allow(unused_imports)]
use objc::{msg_send, sel, sel_impl};

// ---------------------------------------------------------------------------
// CoreAudio data structures
// ---------------------------------------------------------------------------

/// Layout mirrors `AudioStreamBasicDescription` from CoreAudio/AudioToolbox.
///
/// Reserved for the direct-AudioQueue capture path (not yet wired to `AVAudioEngine`).
#[repr(C)]
#[allow(dead_code)]
pub(super) struct AudioStreamBasicDescription {
    pub(super) sample_rate: f64,
    pub(super) format_id: u32,
    pub(super) format_flags: u32,
    pub(super) bytes_per_packet: u32,
    pub(super) frames_per_packet: u32,
    pub(super) bytes_per_frame: u32,
    pub(super) channels_per_frame: u32,
    pub(super) bits_per_channel: u32,
    pub(super) reserved: u32,
}

/// `AudioObjectPropertyAddress` (CoreAudio).
#[repr(C)]
pub(super) struct AudioObjectPropertyAddress {
    pub(super) selector: u32,
    pub(super) scope: u32,
    pub(super) element: u32,
}

// ---------------------------------------------------------------------------
// CoreAudio constants
// ---------------------------------------------------------------------------

// PCM format constants — kept for the future AudioQueue path.
#[allow(dead_code)]
pub(super) const K_AUDIO_FORMAT_LINEAR_PCM: u32 = 0x6C70_636D; // 'lpcm'
#[allow(dead_code)]
pub(super) const K_AUDIO_FORMAT_FLAG_SIGNED_INTEGER: u32 = 0x0004;
#[allow(dead_code)]
pub(super) const K_AUDIO_FORMAT_FLAG_PACKED: u32 = 0x0008;

pub(super) const K_AUDIO_OBJECT_PROPERTY_SELECTOR_DEVICES: u32 = 0x6465_7623; // 'dev#'
pub(super) const K_AUDIO_OBJECT_PROPERTY_SELECTOR_DEFAULT_INPUT: u32 = 0x6471_6966;
pub(super) const K_AUDIO_OBJECT_PROPERTY_SELECTOR_DEFAULT_OUTPUT: u32 = 0x6471_6F66;
pub(super) const K_AUDIO_OBJECT_PROPERTY_SELECTOR_NAME: u32 = 0x6C6E_616D; // 'lnam'
pub(super) const K_AUDIO_OBJECT_PROPERTY_SELECTOR_STREAMS: u32 = 0x7374_726D; // 'strm'
pub(super) const K_AUDIO_OBJECT_PROPERTY_SELECTOR_NOMINAL_SAMPLE_RATE: u32 = 0x6E73_7274; // 'nsrt'

pub(super) const K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL: u32 = 0x676C_6F62; // 'glob'
pub(super) const K_AUDIO_OBJECT_PROPERTY_SCOPE_INPUT: u32 = 0x696E_7074; // 'inpt'
pub(super) const K_AUDIO_OBJECT_PROPERTY_SCOPE_OUTPUT: u32 = 0x6F757470; // 'outp'
pub(super) const K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN: u32 = 0;

pub(super) const K_AUDIO_OBJECT_SYSTEM_OBJECT: u32 = 1;

// ---------------------------------------------------------------------------
// CoreAudio extern "C" bindings
// ---------------------------------------------------------------------------

// CoreAudio.framework is linked via build.rs when the `audio` feature is enabled.
extern "C" {
    pub(super) fn AudioObjectGetPropertyDataSize(
        object_id: u32,
        address: *const AudioObjectPropertyAddress,
        qualifier_data_size: u32,
        qualifier_data: *const c_void,
        out_data_size: *mut u32,
    ) -> i32;

    pub(super) fn AudioObjectGetPropertyData(
        object_id: u32,
        address: *const AudioObjectPropertyAddress,
        qualifier_data_size: u32,
        qualifier_data: *const c_void,
        io_data_size: *mut u32,
        out_data: *mut c_void,
    ) -> i32;
}

// ---------------------------------------------------------------------------
// ObjC helpers
// ---------------------------------------------------------------------------

/// Retrieve an ObjC class by name; returns null when unavailable.
pub(super) fn objc_class(name: &str) -> *const Class {
    use std::ffi::CString;
    let c = CString::new(name).unwrap_or_default();
    unsafe { objc::runtime::objc_getClass(c.as_ptr()) as *const Class }
}

/// Create an `NSString` from a Rust `&str`.
///
/// The returned pointer is autoreleased. Callers in non-autorelease contexts
/// must retain/release manually.
pub(super) fn ns_string_from_str(s: &str) -> *mut Object {
    let cls = objc_class("NSString");
    if cls.is_null() {
        return std::ptr::null_mut();
    }
    unsafe {
        let obj: *mut Object = msg_send![cls, alloc];
        msg_send![obj, initWithBytes: s.as_ptr() as *const c_void
                              length: s.len()
                            encoding: 4u64] // NSUTF8StringEncoding = 4
    }
}

/// Convert a `CFStringRef` (or compatible `NSString *`) to a Rust `String`.
pub(super) fn cf_string_to_string(cf_str: *const c_void) -> String {
    if cf_str.is_null() {
        return String::new();
    }
    use core_foundation::base::TCFType;
    use core_foundation::string::CFStringRef;
    let cf =
        unsafe { core_foundation::string::CFString::wrap_under_create_rule(cf_str as CFStringRef) };
    cf.to_string()
}

/// Convert an `NSString *` to a Rust `String`.
pub(super) fn ns_string_to_rust(ns: *mut Object) -> String {
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

/// Release an ObjC object (decrements retain count).
pub(super) fn release_objc_object(obj: *mut Object) {
    if !obj.is_null() {
        unsafe {
            let _: () = msg_send![obj, release];
        }
    }
}
