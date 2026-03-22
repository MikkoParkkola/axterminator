//! System context: battery, volume, brightness, dark mode, network, locale, etc.
//!
//! All queries are local — no network calls, no TCC permissions required.
//! Uses a mix of ObjC runtime, CoreAudio, IOKit, and sysctl.

use objc::runtime::Object;
#[allow(unused_imports)]
use objc::{msg_send, sel, sel_impl};
use serde::{Deserialize, Serialize};

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

/// Complete system context snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemContext {
    // --- Power ---
    pub battery_level: Option<f64>,
    pub battery_charging: Option<bool>,
    pub power_source: Option<String>,

    // --- Display ---
    pub dark_mode: bool,
    pub screen_width: f64,
    pub screen_height: f64,
    pub screen_scale: f64,

    // --- Audio ---
    pub system_volume: Option<f32>,
    pub output_muted: Option<bool>,

    // --- Locale & Time ---
    pub locale: String,
    pub language: String,
    pub timezone: String,
    pub timezone_offset_secs: i64,

    // --- System ---
    pub macos_version: String,
    pub hostname: String,
    pub username: String,
    pub uptime_secs: f64,
    pub physical_memory_gb: f64,

    // --- Network ---
    pub wifi_enabled: Option<bool>,
    pub wifi_ssid: Option<String>,
    pub active_interfaces: Vec<NetworkInterface>,

    // --- Input ---
    pub keyboard_layout: Option<String>,

    // --- Focus ---
    pub frontmost_app: Option<String>,
}

/// Network interface info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInterface {
    pub name: String,
    pub ipv4: Option<String>,
}

/// Collect a complete system context snapshot.
///
/// All queries are non-blocking and local. Any individual query that fails
/// returns a `None` / default value rather than failing the entire snapshot.
#[must_use]
pub fn collect_system_context() -> SystemContext {
    SystemContext {
        battery_level: battery_level(),
        battery_charging: battery_charging(),
        power_source: power_source(),
        dark_mode: is_dark_mode(),
        screen_width: screen_width(),
        screen_height: screen_height(),
        screen_scale: screen_scale(),
        system_volume: system_volume(),
        output_muted: output_muted(),
        locale: current_locale(),
        language: current_language(),
        timezone: current_timezone(),
        timezone_offset_secs: timezone_offset(),
        macos_version: macos_version(),
        hostname: hostname(),
        username: username(),
        uptime_secs: system_uptime(),
        physical_memory_gb: physical_memory_gb(),
        wifi_enabled: wifi_enabled(),
        wifi_ssid: wifi_ssid(),
        active_interfaces: active_network_interfaces(),
        keyboard_layout: keyboard_layout(),
        frontmost_app: frontmost_app_name(),
    }
}

// ---------------------------------------------------------------------------
// Power (IOKit / pmset)
// ---------------------------------------------------------------------------

fn battery_level() -> Option<f64> {
    // Use IOPSCopyPowerSourcesInfo via ObjC/CoreFoundation.
    // Simpler: parse `pmset -g batt` output.
    let output = std::process::Command::new("pmset")
        .args(["-g", "batt"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    // Line like: "-InternalBattery-0 (id=...)	72%; charging; 1:23 remaining"
    for line in text.lines() {
        if let Some(pct_pos) = line.find('%') {
            // Walk backward from % to find the number.
            let before = &line[..pct_pos];
            let num_start = before.rfind(|c: char| !c.is_ascii_digit()).map_or(0, |i| i + 1);
            if let Ok(pct) = before[num_start..].parse::<f64>() {
                return Some(pct);
            }
        }
    }
    None
}

fn battery_charging() -> Option<bool> {
    let output = std::process::Command::new("pmset")
        .args(["-g", "batt"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    if text.contains("charging") && !text.contains("discharging") {
        Some(true)
    } else if text.contains("discharging") || text.contains("charged") || text.contains("AC Power") {
        Some(false) // Discharging or fully charged on AC.
    } else {
        None
    }
}

fn power_source() -> Option<String> {
    let output = std::process::Command::new("pmset")
        .args(["-g", "batt"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    if text.contains("AC Power") {
        Some("AC".to_string())
    } else if text.contains("Battery Power") {
        Some("Battery".to_string())
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Display
// ---------------------------------------------------------------------------

fn is_dark_mode() -> bool {
    let cls = objc_class("NSAppearance");
    if cls.is_null() {
        // Fallback: check defaults.
        return std::process::Command::new("defaults")
            .args(["read", "-g", "AppleInterfaceStyle"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("Dark"))
            .unwrap_or(false);
    }

    // NSApp.effectiveAppearance.name contains "Dark" for dark mode.
    let app_cls = objc_class("NSApplication");
    if app_cls.is_null() {
        return false;
    }
    let shared: *mut Object = unsafe { msg_send![app_cls, sharedApplication] };
    if shared.is_null() {
        return false;
    }
    let appearance: *mut Object = unsafe { msg_send![shared, effectiveAppearance] };
    if appearance.is_null() {
        // Fallback for CLI context where NSApp might not be fully initialized.
        return std::process::Command::new("defaults")
            .args(["read", "-g", "AppleInterfaceStyle"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("Dark"))
            .unwrap_or(false);
    }
    let name: *mut Object = unsafe { msg_send![appearance, name] };
    let name_str = ns_string_to_rust(name);
    name_str.contains("Dark")
}

fn screen_width() -> f64 {
    let cls = objc_class("NSScreen");
    if cls.is_null() {
        return 0.0;
    }
    let main: *mut Object = unsafe { msg_send![cls, mainScreen] };
    if main.is_null() {
        return 0.0;
    }
    // NSScreen.frame returns NSRect { origin: NSPoint, size: NSSize }
    // NSRect is { f64, f64, f64, f64 } on 64-bit.
    let frame: (f64, f64, f64, f64) = unsafe { msg_send![main, frame] };
    frame.2 // width
}

fn screen_height() -> f64 {
    let cls = objc_class("NSScreen");
    if cls.is_null() {
        return 0.0;
    }
    let main: *mut Object = unsafe { msg_send![cls, mainScreen] };
    if main.is_null() {
        return 0.0;
    }
    let frame: (f64, f64, f64, f64) = unsafe { msg_send![main, frame] };
    frame.3 // height
}

fn screen_scale() -> f64 {
    let cls = objc_class("NSScreen");
    if cls.is_null() {
        return 1.0;
    }
    let main: *mut Object = unsafe { msg_send![cls, mainScreen] };
    if main.is_null() {
        return 1.0;
    }
    let scale: f64 = unsafe { msg_send![main, backingScaleFactor] };
    if scale > 0.0 { scale } else { 1.0 }
}

// ---------------------------------------------------------------------------
// Audio volume
// ---------------------------------------------------------------------------

fn system_volume() -> Option<f32> {
    // Use osascript to query system volume (simpler than CoreAudio FFI).
    let output = std::process::Command::new("osascript")
        .args(["-e", "output volume of (get volume settings)"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    text.parse::<f32>().ok().map(|v| v / 100.0) // Normalize to 0.0–1.0.
}

fn output_muted() -> Option<bool> {
    let output = std::process::Command::new("osascript")
        .args(["-e", "output muted of (get volume settings)"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    match text.as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Locale & Time
// ---------------------------------------------------------------------------

fn current_locale() -> String {
    let cls = objc_class("NSLocale");
    if cls.is_null() {
        return String::new();
    }
    let current: *mut Object = unsafe { msg_send![cls, currentLocale] };
    if current.is_null() {
        return String::new();
    }
    let ident: *mut Object = unsafe { msg_send![current, localeIdentifier] };
    ns_string_to_rust(ident)
}

fn current_language() -> String {
    let cls = objc_class("NSLocale");
    if cls.is_null() {
        return String::new();
    }
    let current: *mut Object = unsafe { msg_send![cls, currentLocale] };
    if current.is_null() {
        return String::new();
    }
    let lang: *mut Object = unsafe { msg_send![current, languageCode] };
    ns_string_to_rust(lang)
}

fn current_timezone() -> String {
    let cls = objc_class("NSTimeZone");
    if cls.is_null() {
        return String::new();
    }
    let tz: *mut Object = unsafe { msg_send![cls, localTimeZone] };
    if tz.is_null() {
        return String::new();
    }
    let name: *mut Object = unsafe { msg_send![tz, name] };
    ns_string_to_rust(name)
}

fn timezone_offset() -> i64 {
    let cls = objc_class("NSTimeZone");
    if cls.is_null() {
        return 0;
    }
    let tz: *mut Object = unsafe { msg_send![cls, localTimeZone] };
    if tz.is_null() {
        return 0;
    }
    unsafe { msg_send![tz, secondsFromGMT] }
}

// ---------------------------------------------------------------------------
// System info
// ---------------------------------------------------------------------------

fn macos_version() -> String {
    let cls = objc_class("NSProcessInfo");
    if cls.is_null() {
        return String::new();
    }
    let info: *mut Object = unsafe { msg_send![cls, processInfo] };
    if info.is_null() {
        return String::new();
    }
    let ver: *mut Object = unsafe { msg_send![info, operatingSystemVersionString] };
    ns_string_to_rust(ver)
}

fn hostname() -> String {
    let cls = objc_class("NSProcessInfo");
    if cls.is_null() {
        return String::new();
    }
    let info: *mut Object = unsafe { msg_send![cls, processInfo] };
    if info.is_null() {
        return String::new();
    }
    let name: *mut Object = unsafe { msg_send![info, hostName] };
    ns_string_to_rust(name)
}

fn username() -> String {
    std::env::var("USER").unwrap_or_default()
}

fn system_uptime() -> f64 {
    let cls = objc_class("NSProcessInfo");
    if cls.is_null() {
        return 0.0;
    }
    let info: *mut Object = unsafe { msg_send![cls, processInfo] };
    if info.is_null() {
        return 0.0;
    }
    unsafe { msg_send![info, systemUptime] }
}

fn physical_memory_gb() -> f64 {
    let cls = objc_class("NSProcessInfo");
    if cls.is_null() {
        return 0.0;
    }
    let info: *mut Object = unsafe { msg_send![cls, processInfo] };
    if info.is_null() {
        return 0.0;
    }
    let bytes: u64 = unsafe { msg_send![info, physicalMemory] };
    bytes as f64 / (1024.0 * 1024.0 * 1024.0)
}

// ---------------------------------------------------------------------------
// Network
// ---------------------------------------------------------------------------

fn wifi_enabled() -> Option<bool> {
    let output = std::process::Command::new("networksetup")
        .args(["-getairportpower", "en0"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    if text.contains("On") {
        Some(true)
    } else if text.contains("Off") {
        Some(false)
    } else {
        None
    }
}

fn wifi_ssid() -> Option<String> {
    // On macOS 14+, `networksetup -getairportnetwork en0` works without Location Services.
    let output = std::process::Command::new("networksetup")
        .args(["-getairportnetwork", "en0"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    // Output: "Current Wi-Fi Network: MyNetwork"
    text.strip_prefix("Current Wi-Fi Network: ")
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty() && !s.contains("not associated"))
}

fn active_network_interfaces() -> Vec<NetworkInterface> {
    let output = match std::process::Command::new("ifconfig")
        .args(["-a"])
        .output()
    {
        Ok(o) => o,
        Err(_) => return vec![],
    };

    let text = String::from_utf8_lossy(&output.stdout);
    let mut interfaces = Vec::new();
    let mut current_name = String::new();

    for line in text.lines() {
        if !line.starts_with('\t') && !line.starts_with(' ') {
            // Interface header: "en0: flags=..."
            if let Some(colon) = line.find(':') {
                current_name = line[..colon].to_string();
            }
        } else if line.contains("inet ") && !line.contains("inet6") && !current_name.is_empty() {
            // IPv4 line: "	inet 192.168.1.5 netmask ..."
            let parts: Vec<&str> = line.split_whitespace().collect();
            if let Some(pos) = parts.iter().position(|&s| s == "inet") {
                if let Some(&ip) = parts.get(pos + 1) {
                    if ip != "127.0.0.1" {
                        interfaces.push(NetworkInterface {
                            name: current_name.clone(),
                            ipv4: Some(ip.to_string()),
                        });
                    }
                }
            }
        }
    }

    interfaces
}

// ---------------------------------------------------------------------------
// Input
// ---------------------------------------------------------------------------

fn keyboard_layout() -> Option<String> {
    // Use TISCopyCurrentKeyboardInputSource → kTISPropertyLocalizedName.
    // Simpler: parse `defaults read` for keyboard layout.
    let output = std::process::Command::new("defaults")
        .args(["read", "com.apple.HIToolbox", "AppleSelectedInputSources"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    // Look for "KeyboardLayout Name" = "..."
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.contains("KeyboardLayout Name") || trimmed.contains("Input Mode") {
            // Extract the value after the = sign.
            if let Some(eq) = trimmed.find('=') {
                let val = trimmed[eq + 1..]
                    .trim()
                    .trim_matches('"')
                    .trim_matches(';')
                    .trim_matches('"')
                    .trim()
                    .to_string();
                if !val.is_empty() {
                    return Some(val);
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Frontmost app
// ---------------------------------------------------------------------------

fn frontmost_app_name() -> Option<String> {
    let ws_cls = objc_class("NSWorkspace");
    if ws_cls.is_null() {
        return None;
    }
    let ws: *mut Object = unsafe { msg_send![ws_cls, sharedWorkspace] };
    if ws.is_null() {
        return None;
    }
    let app: *mut Object = unsafe { msg_send![ws, frontmostApplication] };
    if app.is_null() {
        return None;
    }
    let name: *mut Object = unsafe { msg_send![app, localizedName] };
    let s = ns_string_to_rust(name);
    if s.is_empty() { None } else { Some(s) }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_system_context_does_not_panic() {
        let ctx = collect_system_context();
        // At minimum these should always have values on macOS.
        assert!(!ctx.macos_version.is_empty());
        assert!(!ctx.hostname.is_empty());
        assert!(!ctx.locale.is_empty());
        assert!(!ctx.timezone.is_empty());
        assert!(ctx.physical_memory_gb > 0.0);
        assert!(ctx.uptime_secs > 0.0);
    }

    #[test]
    fn system_context_serializes_to_json() {
        let ctx = collect_system_context();
        let json = serde_json::to_string(&ctx).unwrap();
        assert!(json.contains("macos_version"));
        assert!(json.contains("dark_mode"));
        assert!(json.contains("locale"));
    }

    #[test]
    fn screen_dimensions_are_positive() {
        let w = screen_width();
        let h = screen_height();
        // CI might not have a display, but on real macOS these should be > 0.
        if w > 0.0 {
            assert!(h > 0.0);
            assert!(screen_scale() >= 1.0);
        }
    }

    #[test]
    fn dark_mode_returns_bool() {
        // Just verify it doesn't panic.
        let _ = is_dark_mode();
    }

    #[test]
    fn locale_is_not_empty() {
        assert!(!current_locale().is_empty());
    }

    #[test]
    fn timezone_is_not_empty() {
        assert!(!current_timezone().is_empty());
    }
}
