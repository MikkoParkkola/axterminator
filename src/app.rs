//! Application wrapper for `AXTerminator`

#![allow(clippy::useless_conversion)]

use pyo3::prelude::*;
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
#[pyclass]
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

#[pymethods]
impl AXApp {
    /// Get the process ID
    #[getter]
    fn pid(&self) -> i32 {
        self.pid
    }

    /// Get the bundle identifier
    #[getter]
    fn bundle_id(&self) -> Option<String> {
        self.bundle_id.clone()
    }

    /// Check if the application is running
    fn is_running(&self) -> bool {
        // Check if process exists
        std::fs::metadata(format!("/proc/{}", self.pid)).is_ok()
            || Command::new("kill")
                .args(["-0", &self.pid.to_string()])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
    }

    /// Find an element by query
    ///
    /// # Arguments
    /// * `query` - Element query (title, role, identifier, or xpath)
    /// * `timeout_ms` - Optional timeout in milliseconds
    ///
    /// # Example
    /// ```python
    /// button = app.find("Save")
    /// button = app.find("role:AXButton title:Save")
    /// button = app.find(role="AXButton", title="Save")
    /// ```
    #[pyo3(signature = (query, timeout_ms=None))]
    fn find(&self, query: &str, timeout_ms: Option<u64>) -> PyResult<AXElement> {
        let timeout = timeout_ms.map(Duration::from_millis);
        self.find_element(query, timeout)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Find an element by role and optional attributes
    ///
    /// # Arguments
    /// * `role` - Accessibility role (e.g., "`AXButton`")
    /// * `title` - Optional title attribute
    /// * `identifier` - Optional identifier attribute
    /// * `label` - Optional label attribute
    #[pyo3(signature = (role, title=None, identifier=None, label=None))]
    fn find_by_role(
        &self,
        role: &str,
        title: Option<&str>,
        identifier: Option<&str>,
        label: Option<&str>,
    ) -> PyResult<AXElement> {
        self.find_element_by_role(role, title, identifier, label)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Wait for an element to appear
    ///
    /// # Arguments
    /// * `query` - Element query
    /// * `timeout_ms` - Timeout in milliseconds (default: 5000)
    #[pyo3(signature = (query, timeout_ms=5000))]
    fn wait_for_element(&self, query: &str, timeout_ms: u64) -> PyResult<AXElement> {
        self.find_element(query, Some(Duration::from_millis(timeout_ms)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Wait for the application to become idle
    ///
    /// Uses `EspressoMac` SDK if available, otherwise falls back to heuristic detection.
    ///
    /// # Arguments
    /// * `timeout_ms` - Timeout in milliseconds (default: 5000)
    #[pyo3(signature = (timeout_ms=5000))]
    fn wait_for_idle(&self, timeout_ms: u64) -> bool {
        self.sync_engine
            .wait_for_idle(Duration::from_millis(timeout_ms))
    }

    /// Check if the application is currently idle (non-blocking)
    ///
    /// # Returns
    /// * `true` if app is idle
    /// * `false` if app is busy
    fn is_idle(&self) -> bool {
        self.sync_engine.is_idle()
    }

    /// Take a screenshot of the application window
    ///
    /// # Returns
    /// PNG image data as bytes
    fn screenshot(&self) -> PyResult<Vec<u8>> {
        self.capture_screenshot()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Get all windows of the application
    fn windows(&self) -> PyResult<Vec<AXElement>> {
        self.get_windows()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Get the main window
    fn main_window(&self) -> PyResult<AXElement> {
        self.get_main_window()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Terminate the application
    fn terminate(&self) -> PyResult<()> {
        Command::new("kill")
            .arg(self.pid.to_string())
            .output()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(())
    }
}

impl AXApp {
    /// Connect to an application
    pub fn connect(
        name: Option<&str>,
        bundle_id: Option<&str>,
        pid: Option<u32>,
    ) -> PyResult<Self> {
        // Find PID from name or bundle_id if not provided
        let resolved_pid = if let Some(p) = pid {
            p as i32
        } else if let Some(bid) = bundle_id {
            Self::pid_from_bundle_id(bid)?
        } else if let Some(n) = name {
            Self::pid_from_name(n)?
        } else {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "Must provide name, bundle_id, or pid",
            ));
        };

        let element = create_application_element(resolved_pid)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

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
    fn pid_from_bundle_id(bundle_id: &str) -> PyResult<i32> {
        let output = Command::new("osascript")
            .args([
                "-e",
                &format!(
                    "tell application \"System Events\" to unix id of (processes whose bundle identifier is \"{bundle_id}\")"
                ),
            ])
            .output()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let pid_str = stdout.trim();

        if pid_str.is_empty() || pid_str == "missing value" {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                "Application not found: {bundle_id}"
            )));
        }

        pid_str
            .parse::<i32>()
            .map_err(|_| pyo3::exceptions::PyRuntimeError::new_err("Failed to parse PID"))
    }

    /// Get PID from application name
    fn pid_from_name(name: &str) -> PyResult<i32> {
        let output = Command::new("pgrep")
            .args(["-x", name])
            .output()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let pid_str = stdout.lines().next().unwrap_or("").trim();

        if pid_str.is_empty() {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                "Application not found: {name}"
            )));
        }

        pid_str
            .parse::<i32>()
            .map_err(|_| pyo3::exceptions::PyRuntimeError::new_err("Failed to parse PID"))
    }

    /// Find element with optional timeout
    fn find_element(&self, query: &str, timeout: Option<Duration>) -> AXResult<AXElement> {
        let start = Instant::now();
        let timeout = timeout.unwrap_or(Duration::from_millis(100));

        loop {
            match self.search_element(query) {
                Ok(element) => return Ok(element),
                Err(e) if start.elapsed() >= timeout => {
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

        // Try cache first (disabled temporarily)
        // let cache_key = crate::cache::CacheKey {
        //     pid: self.pid,
        //     query: query.to_string(),
        // };
        //
        // if let Some(cached) = crate::cache::global_cache().get(&cache_key) {
        //     if cached.exists() {
        //         return Ok(cached);
        //     }
        // }

        // Perform breadth-first search of accessibility tree
        let result = self.breadth_first_search(&criteria)?;

        // Cache the result (disabled temporarily)
        // crate::cache::global_cache().put(cache_key, result.clone());

        Ok(result)
    }

    /// Find element by role and attributes
    fn find_element_by_role(
        &self,
        role: &str,
        title: Option<&str>,
        identifier: Option<&str>,
        label: Option<&str>,
    ) -> AXResult<AXElement> {
        let criteria = SearchCriteria {
            role: Some(role.to_string()),
            title: title.map(String::from),
            identifier: identifier.map(String::from),
            label: label.map(String::from),
        };

        self.breadth_first_search(&criteria)
    }

    /// Capture screenshot of the application
    fn capture_screenshot(&self) -> AXResult<Vec<u8>> {
        // Use screencapture command for now
        let temp_path = format!("/tmp/axterminator_screenshot_{}.png", self.pid);

        let output = Command::new("screencapture")
            .args(["-l", &self.window_id()?, "-o", "-x", &temp_path])
            .output()
            .map_err(|e| AXError::SystemError(e.to_string()))?;

        if !output.status.success() {
            return Err(AXError::SystemError("Screenshot failed".into()));
        }

        let data = std::fs::read(&temp_path).map_err(|e| AXError::SystemError(e.to_string()))?;
        let _ = std::fs::remove_file(&temp_path);

        Ok(data)
    }

    /// Get window ID for screencapture
    fn window_id(&self) -> AXResult<String> {
        // Get window ID via CGWindowListCopyWindowInfo
        let output = Command::new("osascript")
            .args([
                "-e",
                &format!(
                    "tell application \"System Events\" to id of window 1 of (processes whose unix id is {})",
                    self.pid
                ),
            ])
            .output()
            .map_err(|e| AXError::SystemError(e.to_string()))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.trim().to_string())
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
    fn get_main_window(&self) -> AXResult<AXElement> {
        // get_attribute returns a retained reference (Copy rule)
        let main_window_ref = get_attribute(self.element, attributes::AX_MAIN_WINDOW)?;
        // AXElement::new takes ownership of the retained reference
        Ok(AXElement::new(main_window_ref as AXUIElementRef))
    }

    /// Perform breadth-first search for element matching criteria
    ///
    /// Memory management:
    /// - Root element (self.element) is NOT retained - it's borrowed from self
    /// - Children from `cf_array_to_vec` ARE retained and must be released if not used
    /// - The matching element is returned with ownership (retained)
    fn breadth_first_search(&self, criteria: &SearchCriteria) -> AXResult<AXElement> {
        use core_foundation::base::CFTypeRef;
        use std::collections::VecDeque;

        // First check the root element itself
        if self.element_matches(self.element, criteria) {
            // Need to retain since self owns the original
            let _ = accessibility::retain_cf(self.element as CFTypeRef);
            return Ok(AXElement::new(self.element));
        }

        // Queue holds (element, is_root) - root doesn't need releasing
        let mut queue: VecDeque<(AXUIElementRef, bool)> = VecDeque::new();

        // Get children of root (these are retained by cf_array_to_vec)
        if let Ok(children_ref) = get_attribute(self.element, attributes::AX_CHILDREN) {
            if let Some(children) = cf_array_to_vec(children_ref) {
                for child in children {
                    queue.push_back((child, false)); // not root, needs release
                }
            }
            accessibility::release_cf(children_ref);
        }

        while let Some((current, _is_root)) = queue.pop_front() {
            // Check if current element matches criteria
            if self.element_matches(current, criteria) {
                // Element is already retained from cf_array_to_vec, pass ownership
                // Release remaining elements in queue
                for (elem, _) in queue {
                    accessibility::release_cf(elem as CFTypeRef);
                }
                return Ok(AXElement::new(current));
            }

            // Get children and add to queue (they're retained by cf_array_to_vec)
            if let Ok(children_ref) = get_attribute(current, attributes::AX_CHILDREN) {
                if let Some(children) = cf_array_to_vec(children_ref) {
                    for child in children {
                        queue.push_back((child, false));
                    }
                }
                accessibility::release_cf(children_ref);
            }

            // Release this element since we didn't match it
            accessibility::release_cf(current as CFTypeRef);
        }

        Err(AXError::ElementNotFound(format!("{criteria:?}")))
    }

    /// Check if element matches search criteria
    fn element_matches(&self, element: AXUIElementRef, criteria: &SearchCriteria) -> bool {
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
    title: Option<String>,
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

        // Simple text query - match against title, label, or identifier
        Ok(Self {
            role: None,
            title: Some(query.to_string()),
            identifier: Some(query.to_string()),
            label: Some(query.to_string()),
        })
    }

    /// Parse XPath-like query: //`AXButton`[@`AXTitle`='Save']
    fn parse_xpath(query: &str) -> AXResult<Self> {
        let mut criteria = Self {
            role: None,
            title: None,
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
            title: None,
            identifier: None,
            label: None,
        };

        for part in query.split_whitespace() {
            if let Some((key, value)) = part.split_once(':') {
                match key.trim() {
                    "role" => criteria.role = Some(value.trim().to_string()),
                    "title" => criteria.title = Some(value.trim().to_string()),
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

        // THEN: Should match against title, identifier, and label
        assert_eq!(criteria.role, None);
        assert_eq!(criteria.title, Some("Save".to_string()));
        assert_eq!(criteria.identifier, Some("Save".to_string()));
        assert_eq!(criteria.label, Some("Save".to_string()));
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
