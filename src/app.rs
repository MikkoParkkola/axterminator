//! Application wrapper for `AXTerminator`

#![allow(clippy::useless_conversion)]

use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::accessibility::{
    self, attributes, create_application_element, get_attribute, AXUIElementRef,
};
use crate::element::AXElement;
use crate::error::{AXError, AXResult};
use crate::sync::SyncEngine;

/// Application wrapper providing the main entry point for GUI automation
pub struct AXApp {
    /// Process ID of the application
    pub(crate) pid: i32,
    /// Bundle identifier (e.g., "com.apple.Safari")
    pub(crate) bundle_id: Option<String>,
    /// Application name
    pub(crate) name: Option<String>,
    /// Root accessibility element
    pub(crate) element: AXUIElementRef,
    /// Synchronization engine for `wait_for_idle`
    sync_engine: Arc<SyncEngine>,
}

// Manual Debug implementation (Arc<SyncEngine> doesn't implement Debug)
impl std::fmt::Debug for AXApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AXApp")
            .field("pid", &self.pid)
            .field("bundle_id", &self.bundle_id)
            .field("name", &self.name)
            .field("element", &self.element)
            .field("sync_mode", &self.sync_engine.mode())
            .finish()
    }
}

// Safety: AXUIElementRef is thread-safe for read operations
unsafe impl Send for AXApp {}
unsafe impl Sync for AXApp {}

impl AXApp {
    /// Connect to an application — Rust-native version returning `AXResult`.
    pub fn connect_native(
        name: Option<&str>,
        bundle_id: Option<&str>,
        pid: Option<u32>,
    ) -> AXResult<Self> {
        Self::connect_impl(name, bundle_id, pid)
    }

    /// Find element — returns `AXResult` for Rust-native callers.
    pub fn find_native(&self, query: &str, timeout_ms: Option<u64>) -> AXResult<AXElement> {
        let timeout = timeout_ms.map(Duration::from_millis);
        self.find_element(query, timeout)
    }

    /// Wait for idle — returns plain bool for Rust-native callers.
    pub fn wait_idle_native(&self, timeout_ms: u64) -> bool {
        self.sync_engine
            .wait_for_idle(Duration::from_millis(timeout_ms))
    }

    /// Screenshot — returns `AXResult` for Rust-native callers.
    pub fn screenshot_native(&self) -> AXResult<Vec<u8>> {
        self.capture_screenshot()
    }

    /// Windows — returns `AXResult` for Rust-native callers.
    pub fn windows_native(&self) -> AXResult<Vec<AXElement>> {
        self.get_windows()
    }

    /// Core connection logic returning `AXResult` — usable without the Python interpreter.
    pub fn connect_impl(
        name: Option<&str>,
        bundle_id: Option<&str>,
        pid: Option<u32>,
    ) -> AXResult<Self> {
        let resolved_pid = if let Some(p) = pid {
            p as i32
        } else if let Some(bid) = bundle_id {
            Self::pid_from_bundle_id(bid)?
        } else if let Some(n) = name {
            Self::pid_from_name(n)?
        } else {
            return Err(AXError::InvalidQuery(
                "Must provide name, bundle_id, or pid".into(),
            ));
        };

        let element = create_application_element(resolved_pid)?;
        let sync_engine = Arc::new(SyncEngine::new(resolved_pid, element));

        Ok(Self {
            pid: resolved_pid,
            bundle_id: bundle_id.map(String::from),
            name: name.map(String::from),
            element,
            sync_engine,
        })
    }

    /// Get PID from bundle identifier using `NSRunningApplication`
    fn pid_from_bundle_id(bundle_id: &str) -> AXResult<i32> {
        let output = Command::new("osascript")
            .args([
                "-e",
                &format!(
                    "tell application \"System Events\" to unix id of (processes whose bundle identifier is \"{bundle_id}\")"
                ),
            ])
            .output()
            .map_err(|e| AXError::SystemError(e.to_string()))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let pid_str = stdout.trim();

        if pid_str.is_empty() || pid_str == "missing value" {
            return Err(AXError::AppNotFound(format!(
                "Application not found: {bundle_id}"
            )));
        }

        pid_str
            .parse::<i32>()
            .map_err(|_| AXError::SystemError("Failed to parse PID".into()))
    }

    /// Get PID from application name
    fn pid_from_name(name: &str) -> AXResult<i32> {
        let output = Command::new("pgrep")
            .args(["-x", name])
            .output()
            .map_err(|e| AXError::SystemError(e.to_string()))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let pid_str = stdout.lines().next().unwrap_or("").trim();

        if pid_str.is_empty() {
            return Err(AXError::AppNotFound(format!(
                "Application not found: {name}"
            )));
        }

        pid_str
            .parse::<i32>()
            .map_err(|_| AXError::SystemError("Failed to parse PID".into()))
    }

    /// Find element with optional timeout
    fn find_element(&self, query: &str, timeout: Option<Duration>) -> AXResult<AXElement> {
        let start = Instant::now();
        let timeout = timeout.unwrap_or(Duration::from_millis(100));

        loop {
            match self.search_element(query) {
                Ok(element) => return Ok(element),
                Err(_) if start.elapsed() >= timeout => {
                    return Err(AXError::ElementNotFound(query.to_string()));
                }
                Err(_) => {
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        }
    }

    /// Search for element (single attempt)
    fn search_element(&self, query: &str) -> AXResult<AXElement> {
        // Parse the query into search criteria
        let criteria = SearchCriteria::parse(query)?;

        let cache_key = crate::cache::CacheKey {
            pid: self.pid,
            query: query.to_string(),
        };

        if let Some(cached) = crate::cache::global_cache().get(&cache_key) {
            if cached.exists() {
                return Ok(cached);
            }
        }

        // Perform breadth-first search of accessibility tree
        let result = self.breadth_first_search(&criteria)?;

        crate::cache::global_cache().put(cache_key, result.clone());

        Ok(result)
    }

    /// Find element by role and attributes
    pub fn find_element_by_role(
        &self,
        role: &str,
        title: Option<&str>,
        identifier: Option<&str>,
        label: Option<&str>,
    ) -> AXResult<AXElement> {
        let criteria = SearchCriteria {
            role: Some(role.to_string()),
            text_any: None,
            title: title.map(String::from),
            description: None,
            value: None,
            identifier: identifier.map(String::from),
            label: label.map(String::from),
        };

        self.breadth_first_search(&criteria)
    }

    /// Capture screenshot of the application's frontmost window.
    ///
    /// Uses `CGWindowListCopyWindowInfo` to find the window ID by PID,
    /// then `screencapture -l <windowID>` for the actual capture. Falls back to
    /// a region-based capture derived from the AX window bounds when the
    /// primary method fails.
    fn capture_screenshot(&self) -> AXResult<Vec<u8>> {
        let wid = self.cg_window_id()?;
        let temp_path = format!("/tmp/axterminator_screenshot_{}.png", self.pid);

        let output = Command::new("screencapture")
            .args(["-l", &wid.to_string(), "-o", "-x", &temp_path])
            .output()
            .map_err(|e| AXError::SystemError(e.to_string()))?;

        if !output.status.success() {
            return self.capture_screenshot_fallback();
        }

        let data = std::fs::read(&temp_path).map_err(|e| AXError::SystemError(e.to_string()))?;
        let _ = std::fs::remove_file(&temp_path);
        Ok(data)
    }

    /// Return the `CGWindowID` for this app's frontmost normal window.
    ///
    /// Iterates the system-wide window list via `CGWindowListCopyWindowInfo`,
    /// filters by owner PID and window layer 0 (normal windows only, excluding
    /// menu bars, overlays, and other system-managed layers), and returns the
    /// first matching window number.
    ///
    /// # Safety
    ///
    /// The `CGWindowListCopyWindowInfo` FFI call is safe: both arguments are
    /// plain integer constants and the function does not dereference any
    /// caller-supplied pointer. All CF objects are wrapped immediately under
    /// ownership/get rules, so no leaks occur on the Rust side.
    fn cg_window_id(&self) -> AXResult<u32> {
        use core_foundation::array::CFArray;
        use core_foundation::base::{CFType, TCFType};
        use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
        use core_foundation::number::{CFNumber, CFNumberRef};
        use core_foundation::string::CFString;

        #[link(name = "CoreGraphics", kind = "framework")]
        extern "C" {
            fn CGWindowListCopyWindowInfo(
                option: u32,
                relative_to: u32,
            ) -> core_foundation::array::CFArrayRef;
        }

        const K_CG_WINDOW_LIST_OPTION_ALL: u32 = 0;
        const K_CG_NULL_WINDOW_ID: u32 = 0;

        // SAFETY: constants-only call; returns a +1 retained CFArray or NULL.
        let array_ref =
            unsafe { CGWindowListCopyWindowInfo(K_CG_WINDOW_LIST_OPTION_ALL, K_CG_NULL_WINDOW_ID) };

        if array_ref.is_null() {
            return Err(AXError::SystemError(
                "CGWindowListCopyWindowInfo returned null".into(),
            ));
        }

        // SAFETY: array_ref is +1 retained (Create rule).
        let array = unsafe { CFArray::<CFType>::wrap_under_create_rule(array_ref) };

        let pid_key = CFString::new("kCGWindowOwnerPID");
        let wid_key = CFString::new("kCGWindowNumber");
        let layer_key = CFString::new("kCGWindowLayer");

        for i in 0..array.len() {
            let Some(entry) = array.get(i) else { continue };

            // SAFETY: each entry in CGWindowListCopyWindowInfo is a CFDictionary.
            let dict = unsafe {
                CFDictionary::<CFString, CFType>::wrap_under_get_rule(
                    entry.as_concrete_TypeRef() as CFDictionaryRef
                )
            };

            let Some(pid_cf) = dict.find(&pid_key) else {
                continue;
            };
            // SAFETY: kCGWindowOwnerPID values are always CFNumber.
            let pid_num = unsafe {
                CFNumber::wrap_under_get_rule(pid_cf.as_concrete_TypeRef() as CFNumberRef)
            };
            let Some(pid) = pid_num.to_i32() else {
                continue;
            };
            if pid != self.pid {
                continue;
            }

            // Layer 0 = normal application window; skip menu bars, overlays, etc.
            if let Some(layer_cf) = dict.find(&layer_key) {
                // SAFETY: kCGWindowLayer values are always CFNumber.
                let layer_num = unsafe {
                    CFNumber::wrap_under_get_rule(layer_cf.as_concrete_TypeRef() as CFNumberRef)
                };
                if layer_num.to_i32().is_some_and(|l| l != 0) {
                    continue;
                }
            }

            let Some(wid_cf) = dict.find(&wid_key) else {
                continue;
            };
            // SAFETY: kCGWindowNumber values are always CFNumber.
            let wid_num = unsafe {
                CFNumber::wrap_under_get_rule(wid_cf.as_concrete_TypeRef() as CFNumberRef)
            };
            if let Some(wid) = wid_num.to_i32() {
                return Ok(wid as u32);
            }
        }

        Err(AXError::SystemError(format!(
            "No window found for PID {} via CGWindowListCopyWindowInfo",
            self.pid
        )))
    }

    /// Fallback screenshot using the AX window bounds for a region capture.
    ///
    /// Invoked when the window-ID path fails (e.g., sandboxed app, missing
    /// Screen Recording permission for the CGWindowList API).
    fn capture_screenshot_fallback(&self) -> AXResult<Vec<u8>> {
        let windows = self.get_windows()?;
        let win = windows
            .first()
            .ok_or_else(|| AXError::SystemError("No windows to screenshot".into()))?;
        let (x, y, w, h) = win
            .bounds()
            .ok_or_else(|| AXError::SystemError("Window has no bounds".into()))?;

        let temp_path = format!("/tmp/axterminator_screenshot_{}.png", self.pid);
        let region = format!("{},{},{},{}", x as i32, y as i32, w as i32, h as i32);
        let output = Command::new("screencapture")
            .args(["-R", &region, "-x", &temp_path])
            .output()
            .map_err(|e| AXError::SystemError(e.to_string()))?;

        if !output.status.success() {
            return Err(AXError::ActionFailed(
                "Screenshot failed (both window-id and region methods)".into(),
            ));
        }

        let data = std::fs::read(&temp_path).map_err(|e| AXError::SystemError(e.to_string()))?;
        let _ = std::fs::remove_file(&temp_path);
        Ok(data)
    }

    /// Get all windows
    fn get_windows(&self) -> AXResult<Vec<AXElement>> {
        let windows_ref = get_attribute(self.element, attributes::AX_WINDOWS)?;
        // cf_array_to_vec now properly retains each element
        let windows = cf_array_to_vec(windows_ref)
            .ok_or_else(|| AXError::SystemError("Failed to get windows array".into()))?;

        accessibility::release_cf(windows_ref);

        // Each window is already retained, AXElement::new takes ownership
        Ok(windows.into_iter().map(AXElement::new).collect())
    }

    /// Get main window
    pub fn get_main_window(&self) -> AXResult<AXElement> {
        // get_attribute returns a retained reference (Copy rule)
        let main_window_ref = get_attribute(self.element, attributes::AX_MAIN_WINDOW)?;
        // AXElement::new takes ownership of the retained reference
        Ok(AXElement::new(main_window_ref as AXUIElementRef))
    }

    /// Perform breadth-first search for element matching criteria.
    ///
    /// Searches within application windows first to exclude menu bars and system
    /// UI elements (AXMenuBar, AXMenu, AXMenuItem) from the traversal. Falls
    /// back to searching from the app root when no windows are present.
    ///
    /// # Memory management
    /// - `self.element` is NOT retained — it is borrowed from `self`.
    /// - Elements returned by `cf_array_to_vec` ARE retained; callers must
    ///   release them when no longer needed.
    /// - The matched element is returned with its existing retain count (owned).
    fn breadth_first_search(&self, criteria: &SearchCriteria) -> AXResult<AXElement> {
        use core_foundation::base::CFTypeRef;
        use std::collections::VecDeque;

        let mut queue: VecDeque<AXUIElementRef> = VecDeque::new();

        // Prefer window-scoped search: excludes AXMenuBar and global UI elements.
        let mut seeded_from_windows = false;
        if let Ok(windows_ref) = get_attribute(self.element, attributes::AX_WINDOWS) {
            if let Some(windows) = cf_array_to_vec(windows_ref) {
                for win in &windows {
                    if self.element_matches(*win, criteria) {
                        // Release all other retained window refs before returning.
                        for other in &windows {
                            if !std::ptr::eq(*other, *win) {
                                accessibility::release_cf(*other as CFTypeRef);
                            }
                        }
                        accessibility::release_cf(windows_ref);
                        return Ok(AXElement::new(*win));
                    }
                    // Seed the queue with this window's children.
                    if let Ok(children_ref) = get_attribute(*win, attributes::AX_CHILDREN) {
                        if let Some(children) = cf_array_to_vec(children_ref) {
                            for child in children {
                                queue.push_back(child);
                            }
                        }
                        accessibility::release_cf(children_ref);
                    }
                    // Window ref is no longer needed — we only use its children.
                    accessibility::release_cf(*win as CFTypeRef);
                }
                seeded_from_windows = true;
            }
            accessibility::release_cf(windows_ref);
        }

        // Fallback: no windows found — search from the app root element.
        if !seeded_from_windows {
            if self.element_matches(self.element, criteria) {
                // Retain because self owns the original; AXElement takes ownership.
                let _ = accessibility::retain_cf(self.element as CFTypeRef);
                return Ok(AXElement::new(self.element));
            }
            if let Ok(children_ref) = get_attribute(self.element, attributes::AX_CHILDREN) {
                if let Some(children) = cf_array_to_vec(children_ref) {
                    for child in children {
                        queue.push_back(child);
                    }
                }
                accessibility::release_cf(children_ref);
            }
        }

        // BFS over the queue; all elements are retained by cf_array_to_vec.
        while let Some(current) = queue.pop_front() {
            if self.element_matches(current, criteria) {
                // Release every element still waiting in the queue.
                for elem in queue {
                    accessibility::release_cf(elem as CFTypeRef);
                }
                return Ok(AXElement::new(current));
            }

            if let Ok(children_ref) = get_attribute(current, attributes::AX_CHILDREN) {
                if let Some(children) = cf_array_to_vec(children_ref) {
                    for child in children {
                        queue.push_back(child);
                    }
                }
                accessibility::release_cf(children_ref);
            }

            // Did not match — release this element.
            accessibility::release_cf(current as CFTypeRef);
        }

        Err(AXError::ElementNotFound(format!("{criteria:?}")))
    }

    /// Check if element matches search criteria
    fn element_matches(&self, element: AXUIElementRef, criteria: &SearchCriteria) -> bool {
        // OR-match: simple text query hits any text-bearing attribute
        if let Some(needle) = &criteria.text_any {
            let attrs: &[&str] = &[
                attributes::AX_TITLE,
                attributes::AX_DESCRIPTION,
                attributes::AX_VALUE,
                attributes::AX_LABEL,
                attributes::AX_IDENTIFIER,
            ];
            let found = attrs.iter().any(|attr| {
                get_attribute(element, attr).is_ok_and(|r| {
                    let matched =
                        cf_string_to_string(r).is_some_and(|s| s.contains(needle.as_str()));
                    accessibility::release_cf(r);
                    matched
                })
            });
            return found;
        }

        // Check role
        if let Some(required_role) = &criteria.role {
            if let Ok(role_ref) = get_attribute(element, attributes::AX_ROLE) {
                let matches = cf_string_to_string(role_ref).is_some_and(|r| &r == required_role);
                accessibility::release_cf(role_ref);
                if !matches {
                    return false;
                }
            } else {
                return false;
            }
        }

        // Check title
        if let Some(required_title) = &criteria.title {
            if let Ok(title_ref) = get_attribute(element, attributes::AX_TITLE) {
                let matches =
                    cf_string_to_string(title_ref).is_some_and(|t| t.contains(required_title));
                accessibility::release_cf(title_ref);
                if !matches {
                    return false;
                }
            } else {
                return false;
            }
        }

        // Check description
        if let Some(required_desc) = &criteria.description {
            if let Ok(desc_ref) = get_attribute(element, attributes::AX_DESCRIPTION) {
                let matches = cf_string_to_string(desc_ref)
                    .is_some_and(|d| d.contains(required_desc.as_str()));
                accessibility::release_cf(desc_ref);
                if !matches {
                    return false;
                }
            } else {
                return false;
            }
        }

        // Check value
        if let Some(required_value) = &criteria.value {
            if let Ok(value_ref) = get_attribute(element, attributes::AX_VALUE) {
                let matches = cf_string_to_string(value_ref)
                    .is_some_and(|v| v.contains(required_value.as_str()));
                accessibility::release_cf(value_ref);
                if !matches {
                    return false;
                }
            } else {
                return false;
            }
        }

        // Check identifier
        if let Some(required_id) = &criteria.identifier {
            if let Ok(id_ref) = get_attribute(element, attributes::AX_IDENTIFIER) {
                let matches = cf_string_to_string(id_ref).is_some_and(|i| &i == required_id);
                accessibility::release_cf(id_ref);
                if !matches {
                    return false;
                }
            } else {
                return false;
            }
        }

        // Check label
        if let Some(required_label) = &criteria.label {
            if let Ok(label_ref) = get_attribute(element, attributes::AX_LABEL) {
                let matches =
                    cf_string_to_string(label_ref).is_some_and(|l| l.contains(required_label));
                accessibility::release_cf(label_ref);
                if !matches {
                    return false;
                }
            } else {
                return false;
            }
        }

        true
    }
}

impl Drop for AXApp {
    fn drop(&mut self) {
        // Release the accessibility element reference
        accessibility::release_cf(self.element.cast());
    }
}

/// Search criteria for element matching
#[derive(Debug, Clone)]
struct SearchCriteria {
    role: Option<String>,
    /// OR-match: true if ANY text-bearing attribute contains this value.
    /// Set by simple-text queries; title/identifier/label remain None when this is Some.
    text_any: Option<String>,
    title: Option<String>,
    description: Option<String>,
    value: Option<String>,
    identifier: Option<String>,
    label: Option<String>,
}

impl SearchCriteria {
    /// Parse a query string into search criteria
    ///
    /// Supports:
    /// - Simple text: "Save" -> matches title/label/identifier
    /// - Role: "role:AXButton"
    /// - Combined: "role:AXButton title:Save"
    /// - XPath-like: "//`AXButton`[@`AXTitle`='Save']"
    fn parse(query: &str) -> AXResult<Self> {
        let query = query.trim();

        // XPath-like syntax: //AXButton[@AXTitle='Save']
        if query.starts_with("//") {
            return Self::parse_xpath(query);
        }

        // Check for key:value pairs
        if query.contains(':') {
            return Self::parse_key_value(query);
        }

        // Simple text query - OR-match against any text-bearing attribute
        Ok(Self {
            role: None,
            text_any: Some(query.to_string()),
            title: None,
            description: None,
            value: None,
            identifier: None,
            label: None,
        })
    }

    /// Parse XPath-like query: //`AXButton`[@`AXTitle`='Save']
    fn parse_xpath(query: &str) -> AXResult<Self> {
        let mut criteria = Self {
            role: None,
            text_any: None,
            title: None,
            description: None,
            value: None,
            identifier: None,
            label: None,
        };

        // Extract role: //ROLE[@...]
        if let Some(role_end) = query.find('[').or(Some(query.len())) {
            let role = query[2..role_end].trim();
            if !role.is_empty() {
                criteria.role = Some(role.to_string());
            }
        }

        // Extract attributes: [@AXTitle='Save']
        for attr_match in query.match_indices("[@") {
            let start = attr_match.0 + 2;
            if let Some(end) = query[start..].find(']') {
                let attr_str = &query[start..start + end];
                if let Some((key, value)) = attr_str.split_once('=') {
                    let key = key.trim();
                    let value = value.trim().trim_matches(|c| c == '\'' || c == '"');

                    match key {
                        "AXTitle" => criteria.title = Some(value.to_string()),
                        "AXIdentifier" => criteria.identifier = Some(value.to_string()),
                        "AXLabel" => criteria.label = Some(value.to_string()),
                        _ => {}
                    }
                }
            }
        }

        Ok(criteria)
    }

    /// Parse key:value query: "role:AXButton title:Save"
    fn parse_key_value(query: &str) -> AXResult<Self> {
        let mut criteria = Self {
            role: None,
            text_any: None,
            title: None,
            description: None,
            value: None,
            identifier: None,
            label: None,
        };

        for part in query.split_whitespace() {
            if let Some((key, value)) = part.split_once(':') {
                match key.trim() {
                    "role" => criteria.role = Some(value.trim().to_string()),
                    "title" => criteria.title = Some(value.trim().to_string()),
                    "description" => criteria.description = Some(value.trim().to_string()),
                    "value" => criteria.value = Some(value.trim().to_string()),
                    "identifier" | "id" => criteria.identifier = Some(value.trim().to_string()),
                    "label" => criteria.label = Some(value.trim().to_string()),
                    _ => return Err(AXError::InvalidQuery(format!("Unknown key: {key}"))),
                }
            }
        }

        Ok(criteria)
    }
}

/// Convert `CFString` to Rust String
fn cf_string_to_string(cf_ref: core_foundation::base::CFTypeRef) -> Option<String> {
    use core_foundation::base::TCFType;
    use core_foundation::string::CFString;

    if cf_ref.is_null() {
        return None;
    }

    unsafe {
        let cf_string = CFString::wrap_under_get_rule(cf_ref.cast());
        Some(cf_string.to_string())
    }
}

/// Convert `CFArray` to Vec of `AXUIElementRef`
///
/// IMPORTANT: Each element is retained (`CFRetain`) before being returned.
/// Caller is responsible for releasing (`CFRelease`) when done.
fn cf_array_to_vec(cf_ref: core_foundation::base::CFTypeRef) -> Option<Vec<AXUIElementRef>> {
    use core_foundation::array::CFArray;
    use core_foundation::base::{CFType, CFTypeRef, TCFType};

    if cf_ref.is_null() {
        return None;
    }

    unsafe {
        let cf_array: CFArray<CFType> = CFArray::wrap_under_get_rule(cf_ref.cast());
        let count = cf_array.len();
        let mut result = Vec::with_capacity(count as usize);

        for i in 0..count {
            if let Some(element_ref) = cf_array.get(i) {
                let element_ptr = element_ref.as_concrete_TypeRef() as AXUIElementRef;
                if !element_ptr.is_null() {
                    // CRITICAL: Retain each element so it survives after cf_array is dropped
                    let _ = accessibility::retain_cf(element_ptr as CFTypeRef);
                    result.push(element_ptr);
                }
            }
        }

        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_criteria_parse_simple_text() {
        // GIVEN: Simple text query
        let query = "Save";

        // WHEN: Parsing
        let criteria = SearchCriteria::parse(query).unwrap();

        // THEN: OR-match field is set; per-attribute fields remain None
        assert_eq!(criteria.role, None);
        assert_eq!(criteria.text_any, Some("Save".to_string()));
        assert_eq!(criteria.title, None);
        assert_eq!(criteria.identifier, None);
        assert_eq!(criteria.label, None);
        assert_eq!(criteria.description, None);
        assert_eq!(criteria.value, None);
    }

    #[test]
    fn test_search_criteria_parse_description_and_value_keys() {
        // GIVEN: key:value query using "description" and "value"
        let query = "description:Search value:42";

        // WHEN: Parsing
        let criteria = SearchCriteria::parse(query).unwrap();

        // THEN: description and value are populated; other fields remain None
        assert_eq!(criteria.text_any, None);
        assert_eq!(criteria.role, None);
        assert_eq!(criteria.title, None);
        assert_eq!(criteria.description, Some("Search".to_string()));
        assert_eq!(criteria.value, Some("42".to_string()));
        assert_eq!(criteria.identifier, None);
        assert_eq!(criteria.label, None);
    }

    #[test]
    fn test_search_criteria_parse_role_only() {
        // GIVEN: Role query
        let query = "role:AXButton";

        // WHEN: Parsing
        let criteria = SearchCriteria::parse(query).unwrap();

        // THEN: Should extract role only
        assert_eq!(criteria.role, Some("AXButton".to_string()));
        assert_eq!(criteria.title, None);
        assert_eq!(criteria.identifier, None);
        assert_eq!(criteria.label, None);
    }

    #[test]
    fn test_search_criteria_parse_combined() {
        // GIVEN: Combined role and title query
        let query = "role:AXButton title:Save";

        // WHEN: Parsing
        let criteria = SearchCriteria::parse(query).unwrap();

        // THEN: Should extract both
        assert_eq!(criteria.role, Some("AXButton".to_string()));
        assert_eq!(criteria.title, Some("Save".to_string()));
        assert_eq!(criteria.identifier, None);
        assert_eq!(criteria.label, None);
    }

    #[test]
    fn test_search_criteria_parse_xpath_role_only() {
        // GIVEN: XPath with role only
        let query = "//AXButton";

        // WHEN: Parsing
        let criteria = SearchCriteria::parse(query).unwrap();

        // THEN: Should extract role
        assert_eq!(criteria.role, Some("AXButton".to_string()));
        assert_eq!(criteria.title, None);
    }

    #[test]
    fn test_search_criteria_parse_xpath_with_title() {
        // GIVEN: XPath with role and title
        let query = "//AXButton[@AXTitle='Save']";

        // WHEN: Parsing
        let criteria = SearchCriteria::parse(query).unwrap();

        // THEN: Should extract role and title
        assert_eq!(criteria.role, Some("AXButton".to_string()));
        assert_eq!(criteria.title, Some("Save".to_string()));
    }

    #[test]
    fn test_search_criteria_parse_xpath_multiple_attributes() {
        // GIVEN: XPath with multiple attributes
        let query = "//AXButton[@AXTitle='Save'][@AXIdentifier='save_btn']";

        // WHEN: Parsing
        let criteria = SearchCriteria::parse(query).unwrap();

        // THEN: Should extract all attributes
        assert_eq!(criteria.role, Some("AXButton".to_string()));
        assert_eq!(criteria.title, Some("Save".to_string()));
        assert_eq!(criteria.identifier, Some("save_btn".to_string()));
    }

    #[test]
    fn test_search_criteria_parse_identifier_alias() {
        // GIVEN: Query using 'id' instead of 'identifier'
        let query = "role:AXButton id:save_btn";

        // WHEN: Parsing
        let criteria = SearchCriteria::parse(query).unwrap();

        // THEN: Should accept 'id' as alias
        assert_eq!(criteria.identifier, Some("save_btn".to_string()));
    }

    #[test]
    fn test_search_criteria_parse_invalid_key() {
        // GIVEN: Invalid key in query
        let query = "role:AXButton invalid:value";

        // WHEN: Parsing
        let result = SearchCriteria::parse(query);

        // THEN: Should return error
        assert!(result.is_err());
        match result {
            Err(AXError::InvalidQuery(msg)) => assert!(msg.contains("invalid")),
            _ => panic!("Expected InvalidQuery error"),
        }
    }

    #[test]
    fn test_cf_string_conversion_null_safety() {
        // GIVEN: Null CFTypeRef
        let null_ref: core_foundation::base::CFTypeRef = std::ptr::null();

        // WHEN: Converting to string
        let result = cf_string_to_string(null_ref);

        // THEN: Should return None
        assert!(result.is_none());
    }

    #[test]
    fn test_cf_array_conversion_null_safety() {
        // GIVEN: Null CFTypeRef
        let null_ref: core_foundation::base::CFTypeRef = std::ptr::null();

        // WHEN: Converting to vec
        let result = cf_array_to_vec(null_ref);

        // THEN: Should return None
        assert!(result.is_none());
    }
}
