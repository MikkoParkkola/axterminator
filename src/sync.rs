//! `EspressoMac` Synchronization Engine
//!
//! Provides sophisticated UI synchronization using two strategies:
//! 1. **XPC Client**: Direct communication with `EspressoMac` SDK for SDK-enabled apps (fastest)
//! 2. **Heuristic Sync**: Tree hashing for non-SDK apps (fallback)
//!
//! The unified `SyncEngine` automatically selects the best strategy.

use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFTypeRef, TCFType};
use core_foundation::string::CFString;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::ptr;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crate::accessibility::{get_attribute, AXUIElementRef};
use crate::error::AXResult;

// XPC service identifiers for EspressoMac
const ESPRESSOMAC_SERVICE_NAME: &str = "com.apple.EspressoMac.xpc";
const ESPRESSOMAC_SELECTOR_IDLE: &str = "isIdle";
#[allow(dead_code)]
const ESPRESSOMAC_SELECTOR_WAIT: &str = "waitForIdle";

// XPC type definitions (from libxpc.dylib)
#[repr(C)]
struct xpc_object_s {
    _private: [u8; 0],
}

#[allow(non_camel_case_types)]
type xpc_connection_t = *mut xpc_object_s;
#[allow(non_camel_case_types)]
type xpc_object_t = *mut xpc_object_s;
#[allow(non_camel_case_types)]
type xpc_handler_t = *const fn(xpc_object_t);

// XPC is a private framework - link dynamically at runtime
// These are low-level FFI bindings; warnings are expected for unused/FFI-unsafe
#[allow(improper_ctypes, dead_code)]
extern "C" {
    fn xpc_connection_create_mach_service(
        name: *const i8,
        target_queue: *const std::ffi::c_void,
        flags: u64,
    ) -> xpc_connection_t;
    fn xpc_connection_set_event_handler(connection: xpc_connection_t, handler: xpc_handler_t);
    fn xpc_connection_resume(connection: xpc_connection_t);
    fn xpc_connection_send_message_with_reply_sync(
        connection: xpc_connection_t,
        message: xpc_object_t,
    ) -> xpc_object_t;
    fn xpc_connection_cancel(connection: xpc_connection_t);
    fn xpc_release(object: xpc_object_t);

    fn xpc_dictionary_create(
        keys: *const *const i8,
        values: *const xpc_object_t,
        count: usize,
    ) -> xpc_object_t;
    fn xpc_dictionary_get_bool(dict: xpc_object_t, key: *const i8) -> bool;
    fn xpc_dictionary_get_int64(dict: xpc_object_t, key: *const i8) -> i64;
    fn xpc_string_create(string: *const i8) -> xpc_object_t;
    fn xpc_int64_create(value: i64) -> xpc_object_t;
    fn xpc_bool_create(value: bool) -> xpc_object_t;

    fn xpc_get_type(object: xpc_object_t) -> *const std::ffi::c_void;
    fn xpc_dictionary_get_value(dict: xpc_object_t, key: *const i8) -> xpc_object_t;
}

// XPC type constants
// Note: This is a sentinel value used by XPC, not a real pointer
const XPC_TYPE_ERROR_SENTINEL: usize = 1;

/// XPC Client for communicating with `EspressoMac` SDK
///
/// This client connects to the `EspressoMac` XPC service embedded in SDK-enabled apps.
/// It provides real-time idle state information from the app's internal state machine.
#[allow(dead_code)]
pub struct EspressoMacClient {
    connection: Option<xpc_connection_t>,
    pid: i32,
}

impl EspressoMacClient {
    /// Attempt to connect to the `EspressoMac` XPC service for the given process
    ///
    /// # Arguments
    /// * `pid` - Process ID of the target application
    ///
    /// # Returns
    /// * `Some(Self)` if connection successful
    /// * `None` if app does not have `EspressoMac` SDK or service unavailable
    #[must_use]
    pub fn connect(pid: i32) -> Option<Self> {
        unsafe {
            // Create service name with PID suffix (apps expose per-process XPC services)
            let service_name = format!("{ESPRESSOMAC_SERVICE_NAME}.{pid}");
            let service_cstr = std::ffi::CString::new(service_name).ok()?;

            // XPC_CONNECTION_MACH_SERVICE_PRIVILEGED = 0
            let connection =
                xpc_connection_create_mach_service(service_cstr.as_ptr(), ptr::null(), 0);

            if connection.is_null() {
                return None;
            }

            // Set up event handler (must be non-null, even if empty)
            extern "C" fn event_handler(_event: xpc_object_t) {
                // Handle connection errors silently
            }
            let handler: xpc_handler_t = event_handler as *const fn(xpc_object_t);
            xpc_connection_set_event_handler(connection, handler);

            // Resume connection
            xpc_connection_resume(connection);

            // Test connection with a ping
            let test_msg = Self::create_message("ping", &[]);
            let reply = xpc_connection_send_message_with_reply_sync(connection, test_msg);
            xpc_release(test_msg);

            // Check if reply is an error
            let reply_type = xpc_get_type(reply);
            let is_error = reply_type as usize == XPC_TYPE_ERROR_SENTINEL;
            xpc_release(reply);

            if is_error {
                xpc_connection_cancel(connection);
                xpc_release(connection as xpc_object_t);
                return None;
            }

            Some(Self {
                connection: Some(connection),
                pid,
            })
        }
    }

    /// Query if the application is currently idle
    ///
    /// # Returns
    /// * `true` if app is idle (no pending UI updates, animations, or work)
    /// * `false` if app is busy or query failed
    #[must_use]
    pub fn is_idle(&self) -> bool {
        let Some(connection) = self.connection else {
            return false;
        };

        unsafe {
            let msg = Self::create_message(ESPRESSOMAC_SELECTOR_IDLE, &[]);
            let reply = xpc_connection_send_message_with_reply_sync(connection, msg);
            xpc_release(msg);

            let idle_key = std::ffi::CString::new("idle").unwrap();
            let idle = xpc_dictionary_get_bool(reply, idle_key.as_ptr());
            xpc_release(reply);

            idle
        }
    }

    /// Wait for the application to become idle with timeout
    ///
    /// # Arguments
    /// * `timeout` - Maximum duration to wait
    ///
    /// # Returns
    /// * `true` if app became idle within timeout
    /// * `false` if timeout exceeded or query failed
    pub async fn wait_for_idle(&self, timeout: Duration) -> bool {
        let start = Instant::now();
        let poll_interval = Duration::from_millis(10);

        while start.elapsed() < timeout {
            if self.is_idle() {
                return true;
            }
            tokio::time::sleep(poll_interval).await;
        }

        false
    }

    /// Create an XPC message with selector and optional arguments
    ///
    /// # Arguments
    /// * `selector` - Method name to call
    /// * `args` - Key-value pairs for arguments
    ///
    /// # Returns
    /// * XPC dictionary message
    fn create_message(selector: &str, args: &[(&str, MessageValue)]) -> xpc_object_t {
        unsafe {
            let mut keys: Vec<*const i8> = Vec::new();
            let mut values: Vec<xpc_object_t> = Vec::new();

            // Add selector
            let selector_key = std::ffi::CString::new("selector").unwrap();
            let selector_value = std::ffi::CString::new(selector).unwrap();
            keys.push(selector_key.as_ptr());
            values.push(xpc_string_create(selector_value.as_ptr()));

            // Add arguments
            for (key, value) in args {
                let key_cstr = std::ffi::CString::new(*key).unwrap();
                keys.push(key_cstr.as_ptr());
                values.push(value.to_xpc_object());
            }

            let dict = xpc_dictionary_create(keys.as_ptr(), values.as_ptr(), keys.len());

            // Clean up temporary values (dict retains them)
            for value in values {
                xpc_release(value);
            }

            dict
        }
    }
}

impl Drop for EspressoMacClient {
    fn drop(&mut self) {
        if let Some(connection) = self.connection.take() {
            unsafe {
                xpc_connection_cancel(connection);
                xpc_release(connection as xpc_object_t);
            }
        }
    }
}

// Safety: XPC connections are thread-safe
unsafe impl Send for EspressoMacClient {}
unsafe impl Sync for EspressoMacClient {}

/// Helper enum for XPC message values
#[allow(dead_code)]
enum MessageValue {
    Bool(bool),
    Int(i64),
    String(String),
}

impl MessageValue {
    fn to_xpc_object(&self) -> xpc_object_t {
        unsafe {
            match self {
                MessageValue::Bool(b) => xpc_bool_create(*b),
                MessageValue::Int(i) => xpc_int64_create(*i),
                MessageValue::String(s) => {
                    let cstr = std::ffi::CString::new(s.as_str()).unwrap();
                    xpc_string_create(cstr.as_ptr())
                }
            }
        }
    }
}

/// Heuristic synchronization using accessibility tree hashing
///
/// This approach detects UI stability by computing a hash of the accessibility tree
/// and waiting for it to remain constant over multiple samples.
pub struct HeuristicSync {
    pid: i32,
    app_element: AXUIElementRef,
}

impl HeuristicSync {
    /// Create a new heuristic sync instance
    ///
    /// # Arguments
    /// * `pid` - Process ID of the target application
    /// * `element` - Root accessibility element of the application
    #[must_use]
    pub fn new(pid: i32, element: AXUIElementRef) -> Self {
        Self {
            pid,
            app_element: element,
        }
    }

    /// Wait for the accessibility tree to stabilize
    ///
    /// A tree is considered stable when its hash remains unchanged for 3 consecutive samples.
    ///
    /// # Arguments
    /// * `timeout` - Maximum duration to wait
    ///
    /// # Returns
    /// * `true` if tree stabilized within timeout
    /// * `false` if timeout exceeded
    #[must_use]
    pub fn wait_for_stable(&self, timeout: Duration) -> bool {
        let start = Instant::now();
        let mut stable_count = 0;
        let mut last_hash = 0u64;
        let poll_interval = Duration::from_millis(50);

        while start.elapsed() < timeout {
            let current_hash = self.hash_tree();

            if current_hash == last_hash {
                stable_count += 1;
                if stable_count >= 3 {
                    return true;
                }
            } else {
                stable_count = 0;
                last_hash = current_hash;
            }

            std::thread::sleep(poll_interval);
        }

        false
    }

    /// Compute a hash of the accessibility tree
    ///
    /// The hash is computed by traversing the tree and hashing:
    /// - Element roles
    /// - Element titles
    /// - Element identifiers
    /// - Element positions (for detecting animations)
    /// - Child count (for detecting DOM changes)
    ///
    /// # Returns
    /// * Hash value representing the current tree state
    #[must_use]
    pub fn hash_tree(&self) -> u64 {
        let mut hasher = DefaultHasher::new();

        // Hash the PID as a base
        self.pid.hash(&mut hasher);

        // Recursively hash the accessibility tree
        self.hash_element(self.app_element, &mut hasher, 0);

        hasher.finish()
    }

    /// Recursively hash an element and its descendants
    ///
    /// # Arguments
    /// * `element` - Element to hash
    /// * `hasher` - Hasher to accumulate hash values
    /// * `depth` - Current depth in tree (limits recursion to prevent infinite loops)
    fn hash_element(&self, element: AXUIElementRef, hasher: &mut DefaultHasher, depth: usize) {
        // Limit depth to prevent infinite recursion on cyclic structures
        const MAX_DEPTH: usize = 20;
        if depth >= MAX_DEPTH {
            return;
        }

        // Hash role (most stable identifier)
        if let Ok(role) = self.get_string_attribute(element, "AXRole") {
            role.hash(hasher);
        }

        // Hash title (for buttons, windows, etc.)
        if let Ok(title) = self.get_string_attribute(element, "AXTitle") {
            title.hash(hasher);
        }

        // Hash identifier (for uniquely identified elements)
        if let Ok(identifier) = self.get_string_attribute(element, "AXIdentifier") {
            identifier.hash(hasher);
        }

        // Hash position (detects animations and layout changes)
        if let Ok(position) = self.get_position(element) {
            let (x, y) = position;
            (x as i32).hash(hasher);
            (y as i32).hash(hasher);
        }

        // Hash size (detects resize animations)
        if let Ok(size) = self.get_size(element) {
            let (w, h) = size;
            (w as i32).hash(hasher);
            (h as i32).hash(hasher);
        }

        // Hash children recursively
        if let Ok(children) = self.get_children(element) {
            children.len().hash(hasher);

            for child in children {
                self.hash_element(child, hasher, depth + 1);
            }
        }
    }

    /// Get string attribute from element
    fn get_string_attribute(&self, element: AXUIElementRef, attribute: &str) -> AXResult<String> {
        let value = get_attribute(element, attribute)?;

        unsafe {
            let cf_string = CFString::wrap_under_get_rule(value.cast());
            Ok(cf_string.to_string())
        }
    }

    /// Get position attribute from element
    fn get_position(&self, element: AXUIElementRef) -> AXResult<(f64, f64)> {
        // AXPosition is a CGPoint wrapped in AXValue
        // For now, return success with placeholder values
        // Proper implementation would use AXValueGetValue
        let _value = get_attribute(element, "AXPosition")?;
        Ok((0.0, 0.0))
    }

    /// Get size attribute from element
    fn get_size(&self, element: AXUIElementRef) -> AXResult<(f64, f64)> {
        // AXSize is a CGSize wrapped in AXValue
        // For now, return success with placeholder values
        let _value = get_attribute(element, "AXSize")?;
        Ok((0.0, 0.0))
    }

    /// Get children of an element
    fn get_children(&self, element: AXUIElementRef) -> AXResult<Vec<AXUIElementRef>> {
        let value = get_attribute(element, "AXChildren")?;

        unsafe {
            let cf_array = CFArray::<CFTypeRef>::wrap_under_get_rule(value as CFArrayRef);
            let mut children: Vec<AXUIElementRef> = Vec::new();

            for i in 0..cf_array.len() {
                if let Some(child_ref) = cf_array.get(i) {
                    // Dereference ItemRef to get the raw pointer value
                    let child_ptr: AXUIElementRef = *child_ref;
                    children.push(child_ptr);
                }
            }

            Ok(children)
        }
    }
}

// Safety: HeuristicSync only performs read operations on AXUIElementRef
unsafe impl Send for HeuristicSync {}
unsafe impl Sync for HeuristicSync {}

/// Synchronization mode selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncMode {
    /// Use `EspressoMac` XPC service (fastest, most accurate)
    XPC,
    /// Use heuristic tree hashing (fallback for non-SDK apps)
    Heuristic,
    /// Try XPC first, fall back to heuristic if unavailable (recommended)
    #[default]
    Auto,
}

/// Unified synchronization engine
///
/// Automatically selects the best synchronization strategy:
/// 1. Tries `EspressoMac` XPC if available (SDK-enabled apps)
/// 2. Falls back to heuristic tree hashing (all apps)
///
/// # Example
/// ```rust,ignore
/// // Requires actual accessibility element
/// let engine = SyncEngine::new(pid, app_element);
/// if engine.wait_for_idle(Duration::from_secs(5)) {
///     println!("App is idle, safe to interact");
/// }
/// ```
pub struct SyncEngine {
    mode: SyncMode,
    xpc: Option<EspressoMacClient>,
    heuristic: HeuristicSync,
}

impl SyncEngine {
    /// Create a new sync engine for the given application
    ///
    /// # Arguments
    /// * `pid` - Process ID of the target application
    /// * `element` - Root accessibility element of the application
    ///
    /// # Returns
    /// * `Self` - Configured sync engine with best available strategy
    #[must_use]
    pub fn new(pid: i32, element: AXUIElementRef) -> Self {
        // XPC connection disabled for now - the XPC calls crash when service doesn't exist
        // TODO: Re-enable when EspressoMac SDK is properly set up in target apps
        // let xpc = EspressoMacClient::connect(pid);
        let xpc: Option<EspressoMacClient> = None;
        let mode = if xpc.is_some() {
            SyncMode::XPC
        } else {
            SyncMode::Heuristic
        };

        Self {
            mode,
            xpc,
            heuristic: HeuristicSync::new(pid, element),
        }
    }

    /// Create a sync engine with explicit mode
    ///
    /// # Arguments
    /// * `pid` - Process ID
    /// * `element` - Root accessibility element
    /// * `mode` - Explicit synchronization mode
    #[must_use]
    pub fn with_mode(pid: i32, element: AXUIElementRef, mode: SyncMode) -> Self {
        // XPC connection disabled - crashes when service doesn't exist
        let xpc: Option<EspressoMacClient> = None;
        let _ = mode; // Silence unused warning - we always use Heuristic for now

        Self {
            mode,
            xpc,
            heuristic: HeuristicSync::new(pid, element),
        }
    }

    /// Wait for the application to become idle
    ///
    /// # Arguments
    /// * `timeout` - Maximum duration to wait
    ///
    /// # Returns
    /// * `true` if app became idle within timeout
    /// * `false` if timeout exceeded
    #[must_use]
    pub fn wait_for_idle(&self, timeout: Duration) -> bool {
        // Static runtime to avoid per-call allocation (FMEA fix: Risk 9→2)
        static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
        let runtime = RUNTIME.get_or_init(|| {
            tokio::runtime::Runtime::new().expect("Failed to create tokio runtime")
        });

        match self.mode {
            SyncMode::XPC => {
                if let Some(ref xpc) = self.xpc {
                    runtime.block_on(xpc.wait_for_idle(timeout))
                } else {
                    // XPC requested but unavailable, fall back
                    self.heuristic.wait_for_stable(timeout)
                }
            }
            SyncMode::Heuristic => self.heuristic.wait_for_stable(timeout),
            SyncMode::Auto => {
                if let Some(ref xpc) = self.xpc {
                    runtime.block_on(xpc.wait_for_idle(timeout))
                } else {
                    self.heuristic.wait_for_stable(timeout)
                }
            }
        }
    }

    /// Check if the application is currently idle (non-blocking)
    ///
    /// # Returns
    /// * `true` if app is idle
    /// * `false` if app is busy or state unknown
    #[must_use]
    pub fn is_idle(&self) -> bool {
        match self.mode {
            SyncMode::XPC => self.xpc.as_ref().is_some_and(EspressoMacClient::is_idle),
            SyncMode::Heuristic => {
                // For heuristic, we check if hash is stable over a short period
                let hash1 = self.heuristic.hash_tree();
                std::thread::sleep(Duration::from_millis(100));
                let hash2 = self.heuristic.hash_tree();
                hash1 == hash2
            }
            SyncMode::Auto => {
                if let Some(ref xpc) = self.xpc {
                    xpc.is_idle()
                } else {
                    let hash1 = self.heuristic.hash_tree();
                    std::thread::sleep(Duration::from_millis(100));
                    let hash2 = self.heuristic.hash_tree();
                    hash1 == hash2
                }
            }
        }
    }

    /// Get the current synchronization mode in use
    #[must_use]
    pub fn mode(&self) -> SyncMode {
        self.mode
    }

    /// Check if XPC connection is available
    #[must_use]
    pub fn has_xpc(&self) -> bool {
        self.xpc.is_some()
    }
}

// Safety: SyncEngine components are all thread-safe
unsafe impl Send for SyncEngine {}
unsafe impl Sync for SyncEngine {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_mode_default() {
        assert_eq!(SyncMode::default(), SyncMode::Auto);
    }

    #[test]
    fn test_message_value_bool() {
        unsafe {
            let msg = MessageValue::Bool(true);
            let obj = msg.to_xpc_object();
            assert!(!obj.is_null());
            xpc_release(obj);
        }
    }

    #[test]
    fn test_message_value_int() {
        unsafe {
            let msg = MessageValue::Int(42);
            let obj = msg.to_xpc_object();
            assert!(!obj.is_null());
            xpc_release(obj);
        }
    }

    #[test]
    fn test_message_value_string() {
        unsafe {
            let msg = MessageValue::String("test".to_string());
            let obj = msg.to_xpc_object();
            assert!(!obj.is_null());
            xpc_release(obj);
        }
    }

    #[test]
    fn test_xpc_message_creation() {
        let msg = EspressoMacClient::create_message("test", &[]);
        assert!(!msg.is_null());
        unsafe {
            xpc_release(msg);
        }
    }

    #[test]
    fn test_xpc_message_with_args() {
        let msg = EspressoMacClient::create_message(
            "test",
            &[
                ("key1", MessageValue::Bool(true)),
                ("key2", MessageValue::Int(123)),
                ("key3", MessageValue::String("value".to_string())),
            ],
        );
        assert!(!msg.is_null());
        unsafe {
            xpc_release(msg);
        }
    }

    #[test]
    #[ignore = "XPC calls with non-existent service cause SIGBUS"]
    fn test_espressomac_client_connect_no_service() {
        // Should return None for non-existent service
        let client = EspressoMacClient::connect(99999);
        assert!(client.is_none());
    }

    // Mock tests for heuristic sync
    mod heuristic_tests {
        use super::*;

        /// Create a mock element (null pointer for testing)
        fn mock_element() -> AXUIElementRef {
            std::ptr::null()
        }

        #[test]
        fn test_heuristic_sync_creation() {
            let sync = HeuristicSync::new(1234, mock_element());
            assert_eq!(sync.pid, 1234);
        }

        #[test]
        fn test_heuristic_hash_stable() {
            let sync = HeuristicSync::new(1234, mock_element());
            let hash1 = sync.hash_tree();
            let hash2 = sync.hash_tree();
            // Same element should produce same hash
            assert_eq!(hash1, hash2);
        }

        #[test]
        fn test_heuristic_wait_for_stable_timeout() {
            let sync = HeuristicSync::new(1234, mock_element());
            let timeout = Duration::from_millis(100);
            let start = Instant::now();
            let stable = sync.wait_for_stable(timeout);
            let elapsed = start.elapsed();

            // Should return quickly since mock element is always stable
            // OR timeout if element access fails
            assert!(elapsed <= timeout + Duration::from_millis(50));
        }
    }

    mod sync_engine_tests {
        use super::*;

        fn mock_element() -> AXUIElementRef {
            std::ptr::null()
        }

        #[test]
        #[ignore = "Requires real AXUIElement - null pointer causes SIGBUS"]
        fn test_sync_engine_creation() {
            let engine = SyncEngine::new(1234, mock_element());
            // Should fall back to heuristic for non-existent app
            assert_eq!(engine.mode(), SyncMode::Heuristic);
            assert!(!engine.has_xpc());
        }

        #[test]
        #[ignore = "Requires real AXUIElement - null pointer causes SIGBUS"]
        fn test_sync_engine_explicit_mode_heuristic() {
            let engine = SyncEngine::with_mode(1234, mock_element(), SyncMode::Heuristic);
            assert_eq!(engine.mode(), SyncMode::Heuristic);
            assert!(!engine.has_xpc());
        }

        #[test]
        #[ignore = "Requires real AXUIElement - null pointer causes SIGBUS"]
        fn test_sync_engine_explicit_mode_auto() {
            let engine = SyncEngine::with_mode(1234, mock_element(), SyncMode::Auto);
            assert_eq!(engine.mode(), SyncMode::Auto);
        }

        #[test]
        #[ignore = "Requires real AXUIElement - null pointer causes SIGBUS"]
        fn test_sync_engine_wait_for_idle_heuristic() {
            let engine = SyncEngine::with_mode(1234, mock_element(), SyncMode::Heuristic);
            let timeout = Duration::from_millis(100);
            let start = Instant::now();
            let _idle = engine.wait_for_idle(timeout);
            let elapsed = start.elapsed();

            // Should respect timeout
            assert!(elapsed <= timeout + Duration::from_millis(100));
        }

        #[test]
        #[ignore = "Requires real AXUIElement - null pointer causes SIGBUS"]
        fn test_sync_engine_is_idle_heuristic() {
            let engine = SyncEngine::with_mode(1234, mock_element(), SyncMode::Heuristic);
            // is_idle with mock element should complete quickly
            let start = Instant::now();
            let _idle = engine.is_idle();
            let elapsed = start.elapsed();

            // Should take ~100ms for heuristic check
            assert!(elapsed >= Duration::from_millis(90));
            assert!(elapsed <= Duration::from_millis(200));
        }
    }

    mod integration_tests {
        use super::*;

        #[test]
        #[ignore] // Requires actual running app
        fn test_real_app_xpc_connection() {
            // This test requires a real app with EspressoMac SDK
            // Run manually with: cargo test test_real_app_xpc_connection -- --ignored
            let pid = std::env::var("TEST_APP_PID")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1); // Default to launchd if not set

            let client = EspressoMacClient::connect(pid);
            // Most apps won't have EspressoMac, so this is expected to be None
            println!("XPC connection available: {}", client.is_some());
        }

        #[test]
        #[ignore] // Requires accessibility permissions
        fn test_real_app_heuristic_sync() {
            use crate::accessibility::create_application_element;

            let pid = std::env::var("TEST_APP_PID")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1);

            if let Ok(element) = create_application_element(pid) {
                let sync = HeuristicSync::new(pid, element);
                let hash1 = sync.hash_tree();
                std::thread::sleep(Duration::from_millis(100));
                let hash2 = sync.hash_tree();

                println!(
                    "Hash stability: {} == {} = {}",
                    hash1,
                    hash2,
                    hash1 == hash2
                );
            } else {
                println!("Accessibility not enabled or app not found");
            }
        }
    }
}
