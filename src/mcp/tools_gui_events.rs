//! CGEvent helpers, process enumeration, and value-extraction for GUI tools.
//!
//! Contains the platform-level macOS event dispatch logic used by:
//! - `ax_key_press` — keyboard event generation via `CGEventPostToPid`
//! - `ax_scroll`   — scroll wheel events via `CGScrollWheelChanged`
//! - `ax_drag`     — mouse drag sequence via `CGEvent` mouse events
//!
//! Also contains shared value-extraction helpers used across GUI handlers.

use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Running apps enumeration
// ---------------------------------------------------------------------------

/// Return a JSON array of all running processes.
///
/// Uses `sysinfo` to enumerate processes.
pub(crate) fn list_running_apps() -> Vec<serde_json::Value> {
    use sysinfo::System;

    let mut sys = System::new_all();
    sys.refresh_all();

    let mut apps: Vec<serde_json::Value> = sys
        .processes()
        .values()
        .filter_map(|proc| {
            let name = proc.name().to_string_lossy().to_string();
            if name.is_empty() {
                return None;
            }
            let pid = i64::from(proc.pid().as_u32());
            Some(json!({ "name": name, "pid": pid }))
        })
        .collect();

    // Sort by name for deterministic output.
    apps.sort_by(|a, b| {
        let na = a["name"].as_str().unwrap_or("");
        let nb = b["name"].as_str().unwrap_or("");
        na.cmp(nb)
    });

    apps
}

// ---------------------------------------------------------------------------
// Key press CGEvent
// ---------------------------------------------------------------------------

/// Parse a key combo string and post `CGEvent`s to the target PID.
///
/// Supported modifiers (case-insensitive): `cmd`, `ctrl`, `opt`, `alt`,
/// `shift`.  The final token is the key name.
pub(super) fn parse_and_post_key_event(pid: i32, keys: &str) -> Result<(), String> {
    use core_graphics::event::{CGEvent, CGEventFlags};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|()| "Failed to create CGEventSource".to_string())?;

    let parts: Vec<&str> = keys.split('+').map(str::trim).collect();
    let (modifier_parts, key_part) = match parts.split_last() {
        Some((k, mods)) => (mods, *k),
        None => return Err(format!("Empty key string: '{keys}'")),
    };

    let key_code =
        key_name_to_code(key_part).ok_or_else(|| format!("Unknown key: '{key_part}'"))?;

    let flags = modifier_parts
        .iter()
        .fold(CGEventFlags::CGEventFlagNull, |acc, &m| {
            acc | modifier_to_flag(m)
        });

    let key_down = CGEvent::new_keyboard_event(source.clone(), key_code, true)
        .map_err(|()| "Failed to create key-down event".to_string())?;
    key_down.set_flags(flags);
    key_down.post_to_pid(pid);

    let key_up = CGEvent::new_keyboard_event(source, key_code, false)
        .map_err(|()| "Failed to create key-up event".to_string())?;
    key_up.set_flags(flags);
    key_up.post_to_pid(pid);

    Ok(())
}

/// Map a modifier name to a `CGEventFlags` bit.
fn modifier_to_flag(modifier: &str) -> core_graphics::event::CGEventFlags {
    use core_graphics::event::CGEventFlags;
    match modifier.to_lowercase().as_str() {
        "cmd" | "command" => CGEventFlags::CGEventFlagCommand,
        "ctrl" | "control" => CGEventFlags::CGEventFlagControl,
        "opt" | "alt" | "option" => CGEventFlags::CGEventFlagAlternate,
        "shift" => CGEventFlags::CGEventFlagShift,
        _ => CGEventFlags::CGEventFlagNull,
    }
}

/// Map a human-readable key name to a macOS virtual key code.
///
/// Only the most common keys are covered.  Unknown names return `None`.
#[allow(clippy::too_many_lines)]
pub(crate) fn key_name_to_code(name: &str) -> Option<u16> {
    match name.to_lowercase().as_str() {
        // Letters
        "a" => Some(0),
        "b" => Some(11),
        "c" => Some(8),
        "d" => Some(2),
        "e" => Some(14),
        "f" => Some(3),
        "g" => Some(5),
        "h" => Some(4),
        "i" => Some(34),
        "j" => Some(38),
        "k" => Some(40),
        "l" => Some(37),
        "m" => Some(46),
        "n" => Some(45),
        "o" => Some(31),
        "p" => Some(35),
        "q" => Some(12),
        "r" => Some(15),
        "s" => Some(1),
        "t" => Some(17),
        "u" => Some(32),
        "v" => Some(9),
        "w" => Some(13),
        "x" => Some(7),
        "y" => Some(16),
        "z" => Some(6),
        // Digits
        "0" => Some(29),
        "1" => Some(18),
        "2" => Some(19),
        "3" => Some(20),
        "4" => Some(21),
        "5" => Some(23),
        "6" => Some(22),
        "7" => Some(26),
        "8" => Some(28),
        "9" => Some(25),
        // Navigation
        "return" | "enter" => Some(36),
        "tab" => Some(48),
        "space" => Some(49),
        "delete" | "backspace" => Some(51),
        "escape" | "esc" => Some(53),
        "left" => Some(123),
        "right" => Some(124),
        "down" => Some(125),
        "up" => Some(126),
        "home" => Some(115),
        "end" => Some(119),
        "pageup" | "page_up" => Some(116),
        "pagedown" | "page_down" => Some(121),
        "forwarddelete" | "forward_delete" => Some(117),
        // Function keys
        "f1" => Some(122),
        "f2" => Some(120),
        "f3" => Some(99),
        "f4" => Some(118),
        "f5" => Some(96),
        "f6" => Some(97),
        "f7" => Some(98),
        "f8" => Some(100),
        "f9" => Some(101),
        "f10" => Some(109),
        "f11" => Some(103),
        "f12" => Some(111),
        "f13" => Some(105),
        "f14" => Some(107),
        "f15" => Some(113),
        "f16" => Some(106),
        "f17" => Some(64),
        "f18" => Some(79),
        "f19" => Some(80),
        "f20" => Some(90),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Scroll CGEvent
// ---------------------------------------------------------------------------

/// Compute `(delta_x, delta_y)` scroll amounts for a direction and amount.
///
/// `CGScrollWheel` uses axis-1 = vertical, axis-2 = horizontal.
/// Positive axis-1 = scroll up; negative = scroll down (follows HID convention).
pub(crate) const fn scroll_deltas(direction: &str, amount: u32) -> (i32, i32) {
    #[allow(clippy::cast_possible_wrap)] // amount is clamped 1..=100 by callers
    let ticks = amount as i32;
    match direction.as_bytes() {
        b"up" => (0, ticks),
        b"down" => (0, -ticks),
        b"left" => (-ticks, 0),
        _ => (ticks, 0), // "right"
    }
}

/// Post a `CGScrollWheelChanged` event at the current cursor position.
///
/// Uses `CGEventCreateScrollWheelEvent2` via the `highsierra` feature of the
/// `core-graphics` crate.  `kCGScrollEventUnitLine` = 1 (raw value; the crate
/// exposes only the type alias, not named unit constants).
///
/// axis-1 = vertical (positive = up), axis-2 = horizontal (positive = right).
pub(super) fn post_scroll_event(dx: i32, dy: i32) -> Result<(), String> {
    use core_graphics::event::CGEvent;
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|()| "Failed to create CGEventSource".to_string())?;

    // unit=1 → kCGScrollEventUnitLine; wheel1=vertical, wheel2=horizontal.
    let event = CGEvent::new_scroll_event(source, 1_u32, 2, dy, dx, 0)
        .map_err(|()| "Failed to create scroll event".to_string())?;
    event.post(core_graphics::event::CGEventTapLocation::HID);
    Ok(())
}

// ---------------------------------------------------------------------------
// Drag CGEvent
// ---------------------------------------------------------------------------

/// Post mouse-drag events from `from` to `to` via the HID tap.
pub(super) fn post_drag_event(from: (f64, f64), to: (f64, f64)) -> Result<(), String> {
    use core_graphics::event::{CGEvent, CGEventType, CGMouseButton};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    use core_graphics::geometry::CGPoint;

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|()| "Failed to create CGEventSource".to_string())?;

    let from_pt = CGPoint::new(from.0, from.1);
    let to_pt = CGPoint::new(to.0, to.1);

    // Mouse-down at source.
    let down = CGEvent::new_mouse_event(
        source.clone(),
        CGEventType::LeftMouseDown,
        from_pt,
        CGMouseButton::Left,
    )
    .map_err(|()| "Failed to create mouse-down event".to_string())?;
    down.post(core_graphics::event::CGEventTapLocation::HID);

    // Drag event to destination.
    let drag = CGEvent::new_mouse_event(
        source.clone(),
        CGEventType::LeftMouseDragged,
        to_pt,
        CGMouseButton::Left,
    )
    .map_err(|()| "Failed to create drag event".to_string())?;
    drag.post(core_graphics::event::CGEventTapLocation::HID);

    // Mouse-up at destination.
    let up =
        CGEvent::new_mouse_event(source, CGEventType::LeftMouseUp, to_pt, CGMouseButton::Left)
            .map_err(|()| "Failed to create mouse-up event".to_string())?;
    up.post(core_graphics::event::CGEventTapLocation::HID);

    Ok(())
}

// ---------------------------------------------------------------------------
// Property reader
// ---------------------------------------------------------------------------

/// Read a named property from an element as a string.
///
/// Boolean properties are normalised to `"true"` / `"false"`.
pub(crate) fn read_element_property(el: &crate::element::AXElement, property: &str) -> String {
    match property {
        "value" => el.value().unwrap_or_default(),
        "title" => el.title().unwrap_or_default(),
        "role" => el.role().unwrap_or_default(),
        "enabled" => el.enabled().to_string(),
        "focused" => el.focused().to_string(),
        _ => String::new(),
    }
}

// ---------------------------------------------------------------------------
// Shared arg-extraction helper
// ---------------------------------------------------------------------------

/// Extract `(app, query)` from a tool arguments `Value`.
pub(crate) fn extract_app_query(args: &Value) -> Result<(String, String), String> {
    let app = args["app"]
        .as_str()
        .ok_or_else(|| "Missing required field: app".to_string())?
        .to_string();
    let query = args["query"]
        .as_str()
        .ok_or_else(|| "Missing required field: query".to_string())?
        .to_string();
    Ok((app, query))
}
