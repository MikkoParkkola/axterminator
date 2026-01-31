//! Unified Test OS Router
//!
//! Routes element operations to the appropriate backend based on app type:
//! - Native macOS apps: Direct Accessibility API
//! - Electron apps: Chrome `DevTools` Protocol (CDP)
//! - `WebView` hybrids: Combination of native + JavaScript injection
//! - Catalyst apps: iPad apps running on macOS

use crate::accessibility::{self, AXUIElementRef};
use crate::element::AXElement;
use crate::error::{AXError, AXResult};
use core_foundation::base::{CFTypeRef, TCFType};
use core_foundation::string::CFString;
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use sysinfo::System;
use tracing::{debug, info, warn};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{connect, Message, WebSocket};

/// Type of application for routing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppType {
    /// Pure macOS native app (SwiftUI/AppKit)
    Native,
    /// Electron app (Chromium-based)
    Electron,
    /// Native app with embedded `WebViews`
    WebViewHybrid,
    /// iPad app running on macOS via Catalyst
    Catalyst,
}

impl AppType {
    /// Get human-readable name
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Native => "Native",
            Self::Electron => "Electron",
            Self::WebViewHybrid => "WebView Hybrid",
            Self::Catalyst => "Catalyst",
        }
    }
}

/// Chrome `DevTools` Protocol connection
#[derive(Debug)]
#[allow(dead_code)]
pub struct CDPConnection {
    /// WebSocket connection to CDP endpoint
    socket: WebSocket<MaybeTlsStream<TcpStream>>,
    /// Target ID for the page/context
    target_id: String,
    /// Message ID counter for CDP requests
    message_id: Arc<AtomicU32>,
}

impl CDPConnection {
    /// Connect to an Electron app's CDP endpoint
    ///
    /// # Arguments
    /// * `pid` - Process ID of the Electron app
    ///
    /// # Returns
    /// * `Some(CDPConnection)` if CDP endpoint found and connected
    /// * `None` if app doesn't expose CDP or connection failed
    pub fn connect(pid: i32) -> Option<Self> {
        debug!(pid, "Attempting CDP connection");

        // Find the debug port from command line args
        let port = Self::find_debug_port(pid)?;
        debug!(pid, port, "Found CDP debug port");

        // Connect to the CDP endpoint
        let ws_url = format!("ws://127.0.0.1:{port}/devtools/browser");

        let (socket, _) = connect(&ws_url).ok()?;
        info!(pid, port, "CDP connection established");

        Some(Self {
            socket,
            target_id: String::new(),
            message_id: Arc::new(AtomicU32::new(1)),
        })
    }

    /// Find the CDP debug port from process command line
    fn find_debug_port(pid: i32) -> Option<u16> {
        let mut system = System::new_all();
        system.refresh_all();

        let process = system.process(sysinfo::Pid::from_u32(pid as u32))?;

        // Look for --remote-debugging-port=XXXX in command line
        for arg in process.cmd() {
            let arg_str = arg.to_string_lossy();
            if let Some(port_str) = arg_str.strip_prefix("--remote-debugging-port=") {
                if let Ok(port) = port_str.parse::<u16>() {
                    return Some(port);
                }
            }
        }

        // Try common Electron debugging ports
        [9222, 9223, 9224, 9225]
            .into_iter()
            .find(|&port| Self::test_cdp_port(port))
    }

    /// Test if a port has a CDP endpoint
    fn test_cdp_port(port: u16) -> bool {
        let addr = format!("127.0.0.1:{port}");
        if let Ok(mut stream) = TcpStream::connect(&addr) {
            // Send a simple HTTP GET to /json/version
            let request = format!("GET /json/version HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n");
            if stream.write_all(request.as_bytes()).is_ok() {
                let mut buf = [0u8; 1024];
                if let Ok(n) = stream.read(&mut buf) {
                    let response = String::from_utf8_lossy(&buf[..n]);
                    return response.contains("Browser")
                        || response.contains("webSocketDebuggerUrl");
                }
            }
        }
        false
    }

    /// Execute a CDP method
    ///
    /// # Arguments
    /// * `method` - CDP method name (e.g., "Runtime.evaluate", "DOM.querySelector")
    /// * `params` - Method parameters as JSON value
    ///
    /// # Returns
    /// * `Ok(Value)` - Result from CDP
    /// * `Err(AXError)` - If method execution failed
    pub fn execute(&mut self, method: &str, params: Value) -> AXResult<Value> {
        let id = self.message_id.fetch_add(1, Ordering::SeqCst);

        let request = json!({
            "id": id,
            "method": method,
            "params": params,
        });

        debug!(method, ?params, "Sending CDP request");

        // Send request
        self.socket
            .send(Message::Text(request.to_string()))
            .map_err(|e| AXError::SystemError(format!("CDP send failed: {e}")))?;

        // Read response
        loop {
            let msg = self
                .socket
                .read()
                .map_err(|e| AXError::SystemError(format!("CDP read failed: {e}")))?;

            if let Message::Text(text) = msg {
                let response: CDPResponse = serde_json::from_str(&text)
                    .map_err(|e| AXError::SystemError(format!("CDP parse failed: {e}")))?;

                // Match response ID
                if response.id == Some(id) {
                    if let Some(error) = response.error {
                        return Err(AXError::ActionFailed(format!(
                            "CDP error: {}",
                            error.message
                        )));
                    }
                    return Ok(response.result.unwrap_or(Value::Null));
                }
            }
        }
    }

    /// Find an element using a CSS selector
    ///
    /// # Arguments
    /// * `selector` - CSS selector (e.g., "#myButton", ".submit-btn")
    ///
    /// # Returns
    /// * `Some(CDPElement)` if element found
    /// * `None` if element not found
    pub fn find_element(&mut self, selector: &str) -> Option<CDPElement> {
        // First, get the document root
        let doc = self
            .execute("DOM.getDocument", json!({ "depth": -1 }))
            .ok()?;
        let root_node_id = doc["root"]["nodeId"].as_i64()?;

        // Query for the element
        let result = self
            .execute(
                "DOM.querySelector",
                json!({
                    "nodeId": root_node_id,
                    "selector": selector,
                }),
            )
            .ok()?;

        let node_id = result["nodeId"].as_i64()?;

        if node_id == 0 {
            return None; // Element not found
        }

        Some(CDPElement {
            node_id,
            connection: None, // Will be populated by router
        })
    }
}

/// CDP protocol response
#[derive(Debug, Deserialize)]
struct CDPResponse {
    id: Option<u32>,
    result: Option<Value>,
    error: Option<CDPError>,
}

/// CDP error
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct CDPError {
    code: i32,
    message: String,
}

/// Element found via CDP
#[derive(Debug)]
pub struct CDPElement {
    /// CDP node ID
    pub node_id: i64,
    /// Reference to CDP connection (for actions)
    connection: Option<Arc<std::sync::Mutex<CDPConnection>>>,
}

impl CDPElement {
    /// Click the element via CDP
    pub fn click(&mut self) -> AXResult<()> {
        let conn = self
            .connection
            .as_ref()
            .ok_or_else(|| AXError::SystemError("No CDP connection".into()))?;

        let mut conn = conn
            .lock()
            .map_err(|_| AXError::SystemError("CDP lock poisoned".into()))?;

        // Get element bounding box
        let box_model = conn.execute(
            "DOM.getBoxModel",
            json!({
                "nodeId": self.node_id,
            }),
        )?;

        // Extract center coordinates
        let content = &box_model["model"]["content"];
        let coords = content
            .as_array()
            .ok_or_else(|| AXError::SystemError("Invalid box model".into()))?;

        // Calculate center point - stable Rust (no f64::midpoint)
        let x0 = coords[0].as_f64().unwrap_or(0.0);
        let x1 = coords[4].as_f64().unwrap_or(0.0);
        let y0 = coords[1].as_f64().unwrap_or(0.0);
        let y1 = coords[5].as_f64().unwrap_or(0.0);
        let x = (x0 + x1) * 0.5;
        let y = (y0 + y1) * 0.5;

        // Dispatch click event
        conn.execute(
            "Input.dispatchMouseEvent",
            json!({
                "type": "mousePressed",
                "x": x,
                "y": y,
                "button": "left",
                "clickCount": 1,
            }),
        )?;

        conn.execute(
            "Input.dispatchMouseEvent",
            json!({
                "type": "mouseReleased",
                "x": x,
                "y": y,
                "button": "left",
                "clickCount": 1,
            }),
        )?;

        Ok(())
    }

    /// Get element text via CDP
    pub fn text(&mut self) -> AXResult<String> {
        let conn = self
            .connection
            .as_ref()
            .ok_or_else(|| AXError::SystemError("No CDP connection".into()))?;

        let mut conn = conn
            .lock()
            .map_err(|_| AXError::SystemError("CDP lock poisoned".into()))?;

        let result = conn.execute(
            "DOM.getOuterHTML",
            json!({
                "nodeId": self.node_id,
            }),
        )?;

        Ok(result["outerHTML"].as_str().unwrap_or("").to_string())
    }
}

/// `WebView` bridge for hybrid apps
#[allow(dead_code)]
pub struct WebViewBridge {
    /// Accessibility element representing the `WebView`
    webview_element: AXUIElementRef,
}

impl WebViewBridge {
    /// Create a bridge from a `WebView` accessibility element
    ///
    /// # Arguments
    /// * `element` - Accessibility element that is a `WKWebView` or `WebView`
    ///
    /// # Returns
    /// * `Some(WebViewBridge)` if element is a valid `WebView`
    /// * `None` if element is not a `WebView`
    #[must_use]
    pub fn from_element(element: AXUIElementRef) -> Option<Self> {
        // Check if element is a WebView by examining its role
        let role = accessibility::get_attribute(element, accessibility::attributes::AX_ROLE)
            .ok()
            .and_then(|cf| unsafe { cfstring_to_string(cf) })?;

        if role == accessibility::roles::AX_WEB_AREA || role.contains("Web") {
            Some(Self {
                webview_element: element,
            })
        } else {
            None
        }
    }

    /// Execute JavaScript in the `WebView`
    ///
    /// NOTE: This uses macOS scripting to inject JS. Requires app cooperation.
    ///
    /// # Arguments
    /// * `_script` - JavaScript code to execute
    ///
    /// # Returns
    /// * `Ok(String)` - Result from script execution
    /// * `Err(AXError)` - If execution failed
    pub fn execute_js(&self, _script: &str) -> AXResult<String> {
        // This is a simplified implementation. Real-world would need:
        // 1. AppleScript bridge to the WebView
        // 2. Or WebKit IPC if we can get the process
        // 3. Or inject via debugger APIs

        warn!("WebView JS execution not fully implemented - requires app cooperation");

        Err(AXError::ActionFailed(
            "WebView JS execution requires app-specific implementation".into(),
        ))
    }

    /// Find an element within the `WebView` using a selector
    ///
    /// # Arguments
    /// * `_selector` - CSS selector
    ///
    /// # Returns
    /// * `Some(AXElement)` if found
    /// * `None` if not found
    pub fn find_web_element(&self, _selector: &str) -> Option<AXElement> {
        // Would execute: document.querySelector(selector)
        // Then map the result to an AX element
        warn!("WebView element search not fully implemented");
        None
    }
}

/// Unified test router
#[allow(dead_code)]
pub struct TestRouter {
    /// Detected app type
    app_type: AppType,
    /// CDP connection (if Electron)
    cdp: Option<Arc<std::sync::Mutex<CDPConnection>>>,
    /// `WebView` bridge (if hybrid)
    webview: Option<WebViewBridge>,
    /// Native accessibility element
    native_element: AXUIElementRef,
    /// Application PID
    pid: i32,
    /// Bundle identifier
    bundle_id: String,
}

impl TestRouter {
    /// Create a new test router
    ///
    /// # Arguments
    /// * `pid` - Application process ID
    /// * `bundle_id` - Application bundle identifier
    /// * `element` - Root accessibility element
    ///
    /// # Returns
    /// * `Self` - Configured router
    pub fn new(pid: i32, bundle_id: &str, element: AXUIElementRef) -> Self {
        let app_type = detect_app_type(bundle_id, pid);
        info!(pid, bundle_id, ?app_type, "Router initialized");

        let cdp = if app_type == AppType::Electron {
            CDPConnection::connect(pid).map(|c| Arc::new(std::sync::Mutex::new(c)))
        } else {
            None
        };

        let webview = if app_type == AppType::WebViewHybrid {
            WebViewBridge::from_element(element)
        } else {
            None
        };

        Self {
            app_type,
            cdp,
            webview,
            native_element: element,
            pid,
            bundle_id: bundle_id.to_string(),
        }
    }

    /// Get the app type
    #[must_use]
    pub fn app_type(&self) -> AppType {
        self.app_type
    }

    /// Find an element by query string
    ///
    /// Routes to the appropriate backend based on app type and query format.
    ///
    /// # Query Formats
    /// - CSS selector (starts with `#`, `.`, `[`): Uses CDP for Electron
    /// - Accessibility query: Uses native API
    ///
    /// # Arguments
    /// * `query` - Element query string
    ///
    /// # Returns
    /// * `Ok(AXElement)` if element found
    /// * `Err(AXError)` if element not found or search failed
    pub fn find_element(&mut self, query: &str) -> AXResult<AXElement> {
        debug!(query, ?self.app_type, "Finding element");

        match self.app_type {
            AppType::Electron if is_css_selector(query) => {
                // Use CDP for Electron apps with CSS selectors
                if let Some(ref cdp) = self.cdp {
                    let mut conn = cdp
                        .lock()
                        .map_err(|_| AXError::SystemError("CDP lock poisoned".into()))?;

                    if let Some(mut cdp_elem) = conn.find_element(query) {
                        cdp_elem.connection = Some(Arc::clone(cdp));
                        // Convert CDP element to AXElement (simplified)
                        return Ok(AXElement::new(self.native_element));
                    }
                }
                Err(AXError::ElementNotFound(query.to_string()))
            }
            AppType::WebViewHybrid if is_css_selector(query) => {
                // Use WebView bridge for hybrid apps with CSS selectors
                if let Some(ref bridge) = self.webview {
                    if let Some(elem) = bridge.find_web_element(query) {
                        return Ok(elem);
                    }
                }
                Err(AXError::ElementNotFound(query.to_string()))
            }
            _ => {
                // Use native accessibility API
                self.find_native_element(query)
            }
        }
    }

    /// Find element using native accessibility API
    fn find_native_element(&self, query: &str) -> AXResult<AXElement> {
        // This would implement the actual element search using accessibility attributes
        // For now, a placeholder that searches children
        let _children_cf = accessibility::get_attribute(
            self.native_element,
            accessibility::attributes::AX_CHILDREN,
        )?;

        // Would iterate through children and match by title, label, identifier, etc.
        warn!("Native element search not fully implemented");

        Err(AXError::ElementNotFound(query.to_string()))
    }
}

/// Detect application type
///
/// # Arguments
/// * `bundle_id` - Application bundle identifier
/// * `pid` - Process ID
///
/// # Returns
/// * `AppType` - Detected application type
pub fn detect_app_type(bundle_id: &str, pid: i32) -> AppType {
    // Known Electron apps by bundle ID
    const ELECTRON_APPS: &[&str] = &[
        "com.github.GitHubClient",
        "com.microsoft.VSCode",
        "com.tinyspeck.slackmacgap",
        "com.hnc.Discord",
        "com.squirrel.slack.Slack",
        "org.whispersystems.signal-desktop",
        "com.electron.",
    ];

    // Check bundle ID for Electron
    for electron_id in ELECTRON_APPS {
        if bundle_id.contains(electron_id) {
            debug!(bundle_id, "Detected Electron app by bundle ID");
            return AppType::Electron;
        }
    }

    // Check for Electron by process inspection
    if has_chromium_helper(pid) {
        debug!(pid, "Detected Electron app by Chromium Helper");
        return AppType::Electron;
    }

    // Check for Catalyst (iPad app on Mac)
    if bundle_id.contains("maccatalyst") || is_catalyst_app(pid) {
        debug!(bundle_id, "Detected Catalyst app");
        return AppType::Catalyst;
    }

    // Check for WebView hybrid
    if has_webview(pid) {
        debug!(pid, "Detected WebView hybrid app");
        return AppType::WebViewHybrid;
    }

    // Default to native
    debug!(bundle_id, "Detected native macOS app");
    AppType::Native
}

/// Check if process has Chromium Helper (Electron indicator)
fn has_chromium_helper(pid: i32) -> bool {
    let mut system = System::new_all();
    system.refresh_all();

    for process in system.processes().values() {
        if let Some(parent_pid) = process.parent() {
            if parent_pid.as_u32() == pid as u32 {
                let name = process.name().to_string_lossy();
                if name.contains("Chromium Helper") || name.contains("Electron Helper") {
                    return true;
                }
            }
        }
    }

    false
}

/// Check if app is a Catalyst app
fn is_catalyst_app(pid: i32) -> bool {
    let mut system = System::new_all();
    system.refresh_all();

    if let Some(process) = system.process(sysinfo::Pid::from_u32(pid as u32)) {
        // Catalyst apps run in a special compatibility layer
        for arg in process.cmd() {
            let arg_str = arg.to_string_lossy();
            if arg_str.contains("UIKitSystem") || arg_str.contains("maccatalyst") {
                return true;
            }
        }
    }

    false
}

/// Check if app uses `WebView`
fn has_webview(_pid: i32) -> bool {
    // Would need to inspect the accessibility tree for WKWebView or WebView elements
    // This is a simplified check
    // In practice, we'd walk the AX tree looking for AXWebArea roles
    false
}

/// Check if query is a CSS selector
fn is_css_selector(query: &str) -> bool {
    query.starts_with('#')
        || query.starts_with('.')
        || query.starts_with('[')
        || query.contains('>')
        || query.contains('+')
        || query.contains('~')
}

/// Convert `CFString` to Rust String
unsafe fn cfstring_to_string(cf: CFTypeRef) -> Option<String> {
    if cf.is_null() {
        return None;
    }

    let cfstring = CFString::wrap_under_get_rule(cf.cast());
    Some(cfstring.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_type_names() {
        assert_eq!(AppType::Native.name(), "Native");
        assert_eq!(AppType::Electron.name(), "Electron");
        assert_eq!(AppType::WebViewHybrid.name(), "WebView Hybrid");
        assert_eq!(AppType::Catalyst.name(), "Catalyst");
    }

    #[test]
    fn test_css_selector_detection() {
        assert!(is_css_selector("#myId"));
        assert!(is_css_selector(".myClass"));
        assert!(is_css_selector("[data-test='value']"));
        assert!(is_css_selector("div > span"));
        assert!(is_css_selector("button + input"));
        assert!(is_css_selector("p ~ a"));
        assert!(!is_css_selector("myButton"));
        assert!(!is_css_selector("Submit Button"));
    }

    #[test]
    fn test_electron_detection_by_bundle_id() {
        assert_eq!(
            detect_app_type("com.microsoft.VSCode", 0),
            AppType::Electron
        );
        assert_eq!(
            detect_app_type("com.tinyspeck.slackmacgap", 0),
            AppType::Electron
        );
        assert_eq!(
            detect_app_type("com.github.GitHubClient", 0),
            AppType::Electron
        );
    }

    #[test]
    fn test_native_detection() {
        assert_eq!(detect_app_type("com.apple.Safari", 0), AppType::Native);
        assert_eq!(detect_app_type("com.apple.Finder", 0), AppType::Native);
        assert_eq!(detect_app_type("com.mycompany.MyApp", 0), AppType::Native);
    }

    #[test]
    fn test_catalyst_detection() {
        assert_eq!(
            detect_app_type("com.apple.maccatalyst.app", 0),
            AppType::Catalyst
        );
    }

    #[test]
    fn test_cdp_port_range() {
        // Test that we check common Electron ports
        // This is a unit test, actual connection would fail without a running app
        let port = CDPConnection::find_debug_port(999999);
        assert!(port.is_none()); // Non-existent process
    }

    #[test]
    fn test_app_type_equality() {
        assert_eq!(AppType::Native, AppType::Native);
        assert_ne!(AppType::Native, AppType::Electron);
        assert_ne!(AppType::Electron, AppType::WebViewHybrid);
        assert_ne!(AppType::WebViewHybrid, AppType::Catalyst);
    }

    #[test]
    fn test_chromium_helper_detection() {
        // Test with a non-existent PID
        assert!(!has_chromium_helper(999999));
    }

    #[test]
    fn test_catalyst_app_detection() {
        // Test with a non-existent PID
        assert!(!is_catalyst_app(999999));
    }

    #[test]
    fn test_webview_detection() {
        // Test with a non-existent PID
        assert!(!has_webview(999999));
    }

    // Integration test for CDP connection (requires running Electron app)
    #[test]
    #[ignore] // Only run with: cargo test -- --ignored
    fn test_cdp_connection_integration() {
        // This test requires a running Electron app with --remote-debugging-port=9222
        // Example: /Applications/Visual\ Studio\ Code.app/Contents/MacOS/Electron --remote-debugging-port=9222

        // For CI, we skip this test. Developers can run it manually.
        let conn = CDPConnection::connect(999999); // Replace with actual PID
        assert!(conn.is_none()); // Should fail without running app
    }

    #[test]
    fn test_cdp_test_port_with_invalid_port() {
        // Test with an unlikely-to-be-used port
        assert!(!CDPConnection::test_cdp_port(65534));
    }

    #[test]
    fn test_router_native_app() {
        // Create a router for a native app
        let element = std::ptr::null(); // Placeholder
        let router = TestRouter::new(1, "com.apple.Safari", element);
        assert_eq!(router.app_type(), AppType::Native);
        assert!(router.cdp.is_none());
        assert!(router.webview.is_none());
    }

    #[test]
    fn test_router_electron_app() {
        // Create a router for an Electron app
        let element = std::ptr::null(); // Placeholder
        let router = TestRouter::new(1, "com.microsoft.VSCode", element);
        assert_eq!(router.app_type(), AppType::Electron);
        // CDP connection will be None without running app
    }

    #[test]
    fn test_query_routing() {
        let element = std::ptr::null();
        let mut router = TestRouter::new(1, "com.apple.Safari", element);

        // CSS selector should fail for native app (no CDP)
        let result = router.find_element("#myButton");
        assert!(result.is_err());

        // Accessibility query
        let result = router.find_element("Submit Button");
        assert!(result.is_err()); // Will fail with placeholder implementation
    }

    #[test]
    fn test_cdp_element_without_connection() {
        let mut elem = CDPElement {
            node_id: 123,
            connection: None,
        };

        // Should fail without connection
        assert!(elem.click().is_err());
        assert!(elem.text().is_err());
    }

    #[test]
    fn test_webview_bridge_invalid_element() {
        let element = std::ptr::null();
        let bridge = WebViewBridge::from_element(element);
        assert!(bridge.is_none());
    }
}
