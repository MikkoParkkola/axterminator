//! Application wrapper for AXTerminator

use pyo3::prelude::*;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::accessibility::{
    self, attributes, create_application_element, get_attribute, AXUIElementRef,
};
use crate::element::AXElement;
use crate::error::{AXError, AXResult};
use crate::ActionMode;

/// Application wrapper providing the main entry point for GUI automation
#[pyclass]
#[derive(Debug)]
pub struct AXApp {
    /// Process ID of the application
    pub(crate) pid: i32,
    /// Bundle identifier (e.g., "com.apple.Safari")
    pub(crate) bundle_id: Option<String>,
    /// Application name
    pub(crate) name: Option<String>,
    /// Root accessibility element
    pub(crate) element: AXUIElementRef,
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
    /// * `role` - Accessibility role (e.g., "AXButton")
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
    /// Uses EspressoMac SDK if available, otherwise falls back to heuristic detection.
    ///
    /// # Arguments
    /// * `timeout_ms` - Timeout in milliseconds (default: 5000)
    #[pyo3(signature = (timeout_ms=5000))]
    fn wait_for_idle(&self, timeout_ms: u64) -> bool {
        self.wait_for_stable(Duration::from_millis(timeout_ms))
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

        Ok(Self {
            pid: resolved_pid,
            bundle_id: bundle_id.map(String::from),
            name: name.map(String::from),
            element,
        })
    }

    /// Get PID from bundle identifier using NSRunningApplication
    fn pid_from_bundle_id(bundle_id: &str) -> PyResult<i32> {
        let output = Command::new("osascript")
            .args([
                "-e",
                &format!(
                    "tell application \"System Events\" to unix id of (processes whose bundle identifier is \"{}\")",
                    bundle_id
                ),
            ])
            .output()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let pid_str = stdout.trim();

        if pid_str.is_empty() || pid_str == "missing value" {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                "Application not found: {}",
                bundle_id
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
                "Application not found: {}",
                name
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
        // TODO: Implement actual tree search
        // This is a placeholder that will be replaced with proper implementation
        Err(AXError::ElementNotFound(query.to_string()))
    }

    /// Find element by role and attributes
    fn find_element_by_role(
        &self,
        role: &str,
        title: Option<&str>,
        identifier: Option<&str>,
        label: Option<&str>,
    ) -> AXResult<AXElement> {
        // TODO: Implement role-based search
        Err(AXError::ElementNotFound(format!("role={}", role)))
    }

    /// Wait for UI to stabilize (heuristic approach)
    fn wait_for_stable(&self, timeout: Duration) -> bool {
        let start = Instant::now();
        let mut stable_count = 0;
        let mut last_hash = 0u64;

        while start.elapsed() < timeout {
            let current_hash = self.hash_accessibility_tree();
            if current_hash == last_hash {
                stable_count += 1;
                if stable_count >= 3 {
                    return true;
                }
            } else {
                stable_count = 0;
                last_hash = current_hash;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }

    /// Hash the accessibility tree for change detection
    fn hash_accessibility_tree(&self) -> u64 {
        // TODO: Implement proper tree hashing
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        self.pid.hash(&mut hasher);
        hasher.finish()
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
        // TODO: Implement window enumeration
        Ok(vec![])
    }

    /// Get main window
    fn get_main_window(&self) -> AXResult<AXElement> {
        // TODO: Implement main window retrieval
        Err(AXError::ElementNotFound("main window".into()))
    }
}

impl Drop for AXApp {
    fn drop(&mut self) {
        // Release the accessibility element reference
        accessibility::release_cf(self.element as _);
    }
}
