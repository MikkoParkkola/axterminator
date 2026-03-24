//! Clipboard read/write via `NSPasteboard`.
//!
//! No special permissions required — `NSPasteboard` is part of the
//! standard AppKit API available to all macOS apps.

use objc::runtime::Object;
#[allow(unused_imports)]
use objc::{msg_send, sel, sel_impl};
use serde::{Deserialize, Serialize};

// Re-use the ObjC helpers from the audio module's FFI (or define locally
// when audio feature is not enabled).
fn objc_class(name: &str) -> *const objc::runtime::Class {
    use std::ffi::CString;
    let c = CString::new(name).unwrap_or_default();
    unsafe { objc::runtime::objc_getClass(c.as_ptr()) }
}

fn ns_string_from_str(s: &str) -> *mut Object {
    let cls = objc_class("NSString");
    if cls.is_null() {
        return std::ptr::null_mut();
    }
    unsafe {
        let obj: *mut Object = msg_send![cls, alloc];
        msg_send![obj, initWithBytes: s.as_ptr() as *const std::ffi::c_void
                              length: s.len()
                            encoding: 4u64] // NSUTF8StringEncoding
    }
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

/// Clipboard content with type information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardContent {
    /// The text content of the clipboard (if any).
    pub text: Option<String>,
    /// Available pasteboard types (e.g. "public.utf8-plain-text", "public.html").
    pub types: Vec<String>,
    /// Number of items on the pasteboard.
    pub item_count: usize,
    /// Change count — increments on each clipboard modification.
    pub change_count: i64,
}

/// Read the current clipboard contents.
///
/// Returns the text content (if available) along with metadata about
/// what types are on the pasteboard.
#[must_use]
pub fn read_clipboard() -> ClipboardContent {
    let cls = objc_class("NSPasteboard");
    if cls.is_null() {
        return ClipboardContent {
            text: None,
            types: vec![],
            item_count: 0,
            change_count: 0,
        };
    }

    let pb: *mut Object = unsafe { msg_send![cls, generalPasteboard] };
    if pb.is_null() {
        return ClipboardContent {
            text: None,
            types: vec![],
            item_count: 0,
            change_count: 0,
        };
    }

    // Change count.
    let change_count: i64 = unsafe { msg_send![pb, changeCount] };

    // Item count.
    let items: *mut Object = unsafe { msg_send![pb, pasteboardItems] };
    let item_count: usize = if items.is_null() {
        0
    } else {
        unsafe { msg_send![items, count] }
    };

    // Available types.
    let types_ns: *mut Object = unsafe { msg_send![pb, types] };
    let mut types = Vec::new();
    if !types_ns.is_null() {
        let count: usize = unsafe { msg_send![types_ns, count] };
        for i in 0..count {
            let t: *mut Object = unsafe { msg_send![types_ns, objectAtIndex: i] };
            if !t.is_null() {
                types.push(ns_string_to_rust(t));
            }
        }
    }

    // Text content (most common use case).
    let type_str = ns_string_from_str("public.utf8-plain-text");
    let text_ns: *mut Object = unsafe { msg_send![pb, stringForType: type_str] };
    let text = if text_ns.is_null() {
        None
    } else {
        let s = ns_string_to_rust(text_ns);
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    };

    ClipboardContent {
        text,
        types,
        item_count,
        change_count,
    }
}

/// Write text to the clipboard, replacing existing contents.
///
/// Returns the new change count on success.
pub fn write_clipboard(text: &str) -> Result<i64, String> {
    let cls = objc_class("NSPasteboard");
    if cls.is_null() {
        return Err("NSPasteboard unavailable".to_string());
    }

    let pb: *mut Object = unsafe { msg_send![cls, generalPasteboard] };
    if pb.is_null() {
        return Err("generalPasteboard is nil".to_string());
    }

    // Clear existing contents.
    unsafe {
        let _: i64 = msg_send![pb, clearContents];
    }

    // Set new string content.
    let ns_text = ns_string_from_str(text);
    let type_str = ns_string_from_str("public.utf8-plain-text");
    let ok: bool = unsafe { msg_send![pb, setString: ns_text forType: type_str] };

    if ok {
        let count: i64 = unsafe { msg_send![pb, changeCount] };
        Ok(count)
    } else {
        Err("setString:forType: returned NO".to_string())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_clipboard_returns_content() {
        // Should not panic regardless of clipboard state.
        let content = read_clipboard();
        assert!(content.item_count >= 0);
    }

    #[test]
    fn clipboard_content_serializes() {
        let content = read_clipboard();
        let json = serde_json::to_string(&content).unwrap();
        assert!(json.contains("change_count"));
    }

    #[test]
    fn write_then_read_clipboard_round_trips() {
        let test_text = "axterminator_clipboard_test_12345";
        let result = write_clipboard(test_text);
        assert!(result.is_ok(), "write failed: {:?}", result);

        let content = read_clipboard();
        assert_eq!(content.text.as_deref(), Some(test_text));
    }
}
