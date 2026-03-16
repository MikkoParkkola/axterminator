//! Docker Browser Test Targets via Neko.
//!
//! Launches [Neko](https://github.com/m1k1o/neko) containers as isolated,
//! reproducible browser test targets.  Each container exposes:
//!
//! - **CDP** (Chrome DevTools Protocol) on a dedicated TCP port for scripting.
//! - **VNC** on a separate port for visual inspection or screenshot capture.
//!
//! # Supported browsers
//!
//! | [`BrowserType`]  | Docker image suffix |
//! |-----------------|---------------------|
//! | `Chromium`      | `chromium`          |
//! | `Firefox`       | `firefox`           |
//! | `Brave`         | `brave`             |
//! | `Edge`          | `microsoft-edge`    |
//!
//! # Example
//!
//! ```rust,no_run
//! use axterminator::docker_browser::{DockerManager, NekoConfig, BrowserType};
//!
//! let mut mgr = DockerManager::new();
//! let browser = mgr.launch(NekoConfig::chromium()).unwrap();
//! println!("CDP: {}", browser.cdp_url());
//! mgr.stop(&browser).unwrap();
//! ```

use std::collections::HashMap;
use std::process::Command;

use crate::error::{AXError, AXResult};

// ── Public configuration ──────────────────────────────────────────────────────

/// Browser variant hosted by the Neko container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BrowserType {
    /// Chromium — enables full CDP feature set.
    Chromium,
    /// Mozilla Firefox.
    Firefox,
    /// Brave Browser.
    Brave,
    /// Microsoft Edge (Chromium-based).
    Edge,
}

impl BrowserType {
    /// Neko Docker image name suffix for this browser.
    #[must_use]
    pub fn image_suffix(self) -> &'static str {
        match self {
            Self::Chromium => "chromium",
            Self::Firefox => "firefox",
            Self::Brave => "brave",
            Self::Edge => "microsoft-edge",
        }
    }

    /// Fully-qualified Neko Docker image tag.
    ///
    /// # Example
    ///
    /// ```rust
    /// use axterminator::docker_browser::BrowserType;
    /// assert_eq!(BrowserType::Chromium.docker_image(), "ghcr.io/m1k1o/neko/chromium:latest");
    /// ```
    #[must_use]
    pub fn docker_image(self) -> String {
        format!("ghcr.io/m1k1o/neko/{}:latest", self.image_suffix())
    }

    /// Human-readable display name.
    #[must_use]
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Chromium => "Chromium",
            Self::Firefox => "Firefox",
            Self::Brave => "Brave",
            Self::Edge => "Microsoft Edge",
        }
    }

    /// Whether this browser supports the full Chrome DevTools Protocol.
    ///
    /// Chromium and Edge are Chromium-based; Firefox uses a subset via
    /// Remote Debugging Protocol.
    #[must_use]
    pub fn supports_full_cdp(self) -> bool {
        matches!(self, Self::Chromium | Self::Edge)
    }
}

/// Configuration for a Neko browser container.
///
/// Build with [`NekoConfig::chromium`], [`NekoConfig::firefox`], or
/// [`NekoConfig::builder`] for custom settings.
///
/// # Example
///
/// ```rust
/// use axterminator::docker_browser::{NekoConfig, BrowserType};
/// let cfg = NekoConfig::builder()
///     .browser(BrowserType::Firefox)
///     .cdp_port(9333)
///     .vnc_port(5910)
///     .build();
/// assert_eq!(cfg.browser, BrowserType::Firefox);
/// ```
#[derive(Debug, Clone)]
pub struct NekoConfig {
    /// Browser to run inside the container.
    pub browser: BrowserType,
    /// Virtual desktop width in pixels.
    pub width: u32,
    /// Virtual desktop height in pixels.
    pub height: u32,
    /// Host port mapped to the container's CDP endpoint.
    pub cdp_port: u16,
    /// Host port mapped to the container's VNC endpoint.
    pub vnc_port: u16,
    /// Optional Neko admin password (defaults to `"admin"`).
    pub admin_password: String,
    /// Container name prefix (defaults to `"neko"`).
    pub name_prefix: String,
}

impl NekoConfig {
    /// Default configuration for Chromium at 1920×1080.
    #[must_use]
    pub fn chromium() -> Self {
        Self::for_browser(BrowserType::Chromium)
    }

    /// Default configuration for Firefox at 1920×1080.
    #[must_use]
    pub fn firefox() -> Self {
        Self::for_browser(BrowserType::Firefox)
    }

    /// Default configuration for Brave at 1920×1080.
    #[must_use]
    pub fn brave() -> Self {
        Self::for_browser(BrowserType::Brave)
    }

    /// Default configuration for Edge at 1920×1080.
    #[must_use]
    pub fn edge() -> Self {
        Self::for_browser(BrowserType::Edge)
    }

    /// Start a [`NekoConfigBuilder`] for custom configuration.
    #[must_use]
    pub fn builder() -> NekoConfigBuilder {
        NekoConfigBuilder::default()
    }

    fn for_browser(browser: BrowserType) -> Self {
        let base_cdp = 9222u16;
        let base_vnc = 5900u16;
        Self {
            browser,
            width: 1920,
            height: 1080,
            cdp_port: base_cdp,
            vnc_port: base_vnc,
            admin_password: "admin".into(),
            name_prefix: "neko".into(),
        }
    }
}

/// Builder for [`NekoConfig`].
#[derive(Debug, Default)]
pub struct NekoConfigBuilder {
    browser: Option<BrowserType>,
    width: Option<u32>,
    height: Option<u32>,
    cdp_port: Option<u16>,
    vnc_port: Option<u16>,
    admin_password: Option<String>,
    name_prefix: Option<String>,
}

impl NekoConfigBuilder {
    /// Set the browser type.
    #[must_use]
    pub fn browser(mut self, browser: BrowserType) -> Self {
        self.browser = Some(browser);
        self
    }

    /// Set virtual desktop dimensions.
    #[must_use]
    pub fn dimensions(mut self, width: u32, height: u32) -> Self {
        self.width = Some(width);
        self.height = Some(height);
        self
    }

    /// Set the CDP host port.
    #[must_use]
    pub fn cdp_port(mut self, port: u16) -> Self {
        self.cdp_port = Some(port);
        self
    }

    /// Set the VNC host port.
    #[must_use]
    pub fn vnc_port(mut self, port: u16) -> Self {
        self.vnc_port = Some(port);
        self
    }

    /// Set the Neko admin password.
    #[must_use]
    pub fn admin_password(mut self, password: impl Into<String>) -> Self {
        self.admin_password = Some(password.into());
        self
    }

    /// Set the container name prefix.
    #[must_use]
    pub fn name_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.name_prefix = Some(prefix.into());
        self
    }

    /// Build the [`NekoConfig`], applying defaults for unset fields.
    #[must_use]
    pub fn build(self) -> NekoConfig {
        let base = NekoConfig::for_browser(self.browser.unwrap_or(BrowserType::Chromium));
        NekoConfig {
            browser: self.browser.unwrap_or(base.browser),
            width: self.width.unwrap_or(base.width),
            height: self.height.unwrap_or(base.height),
            cdp_port: self.cdp_port.unwrap_or(base.cdp_port),
            vnc_port: self.vnc_port.unwrap_or(base.vnc_port),
            admin_password: self.admin_password.unwrap_or(base.admin_password),
            name_prefix: self.name_prefix.unwrap_or(base.name_prefix),
        }
    }
}

// ── Live container handle ─────────────────────────────────────────────────────

/// Handle to a running Neko browser container.
///
/// Obtained from [`DockerManager::launch`].  Use [`DockerManager::stop`] to
/// remove the container when the test completes.
///
/// # Example
///
/// ```rust,no_run
/// use axterminator::docker_browser::{DockerManager, NekoConfig};
///
/// let mut mgr = DockerManager::new();
/// let browser = mgr.launch(NekoConfig::chromium()).unwrap();
/// assert_eq!(browser.cdp_url(), "ws://127.0.0.1:9222/devtools/browser");
/// mgr.stop(&browser).unwrap();
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NekoBrowser {
    /// Docker container ID (short or long form).
    pub(crate) container_id: String,
    /// Host port serving the CDP WebSocket.
    pub(crate) cdp_port: u16,
    /// Host port serving VNC.
    pub(crate) vnc_port: u16,
    /// Browser running inside the container.
    pub(crate) browser: BrowserType,
}

impl NekoBrowser {
    /// WebSocket URL for CDP connection.
    ///
    /// Pass this to [`crate::electron_cdp::ElectronConnection::connect`] after
    /// the container is ready.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use axterminator::docker_browser::{DockerManager, NekoConfig};
    /// # let mut mgr = DockerManager::new();
    /// let browser = mgr.launch(NekoConfig::chromium()).unwrap();
    /// assert_eq!(browser.cdp_url(), "ws://127.0.0.1:9222/devtools/browser");
    /// ```
    #[must_use]
    pub fn cdp_url(&self) -> String {
        format!("ws://127.0.0.1:{}/devtools/browser", self.cdp_port)
    }

    /// Host + port of the VNC endpoint.
    #[must_use]
    pub fn vnc_addr(&self) -> String {
        format!("127.0.0.1:{}", self.vnc_port)
    }

    /// The browser running inside this container.
    #[must_use]
    pub fn browser(&self) -> BrowserType {
        self.browser
    }

    /// Docker container ID.
    #[must_use]
    pub fn container_id(&self) -> &str {
        &self.container_id
    }

    /// Capture a screenshot via the CDP `Page.captureScreenshot` command.
    ///
    /// Returns raw PNG bytes.  Requires a live CDP connection — this method
    /// performs the full round-trip internally so callers need not hold a
    /// persistent [`crate::electron_cdp::ElectronConnection`].
    ///
    /// # Errors
    ///
    /// * [`AXError::SystemError`] — CDP transport failure or base64 decode error.
    /// * [`AXError::AppNotFound`] — No CDP listener on the configured port.
    pub fn screenshot(&self) -> AXResult<Vec<u8>> {
        self.screenshot_via(&RealCdpShooter)
    }

    /// Construct a test-only handle without Docker.
    ///
    /// Used exclusively in unit tests to build [`NekoBrowser`] values without
    /// running `docker run`.
    #[cfg(test)]
    #[must_use]
    pub fn new_for_test(
        container_id: &str,
        cdp_port: u16,
        vnc_port: u16,
        browser: BrowserType,
    ) -> Self {
        Self {
            container_id: container_id.to_string(),
            cdp_port,
            vnc_port,
            browser,
        }
    }

    // Internal: injectable screenshotter (seam for tests).
    pub(crate) fn screenshot_via(&self, shooter: &dyn ScreenshotShooter) -> AXResult<Vec<u8>> {
        shooter.capture(self.cdp_port)
    }
}

// ── Screenshot seam ───────────────────────────────────────────────────────────

/// Trait for capturing screenshots — injectable in tests.
///
/// The production implementation (`RealCdpShooter`) connects to the live CDP
/// endpoint.  Tests inject a `MockCdpShooter` to avoid Docker.
pub(crate) trait ScreenshotShooter {
    fn capture(&self, cdp_port: u16) -> AXResult<Vec<u8>>;
}

/// Production screenshotter: connects to CDP and calls `Page.captureScreenshot`.
struct RealCdpShooter;

impl ScreenshotShooter for RealCdpShooter {
    fn capture(&self, cdp_port: u16) -> AXResult<Vec<u8>> {
        use crate::electron_cdp::probe_cdp_port;
        use tungstenite::connect as ws_connect;

        if !probe_cdp_port(cdp_port) {
            return Err(AXError::AppNotFound(format!(
                "No CDP endpoint on port {cdp_port}"
            )));
        }

        let ws_url = format!("ws://127.0.0.1:{cdp_port}/devtools/browser");
        let (mut socket, _) = ws_connect(&ws_url)
            .map_err(|e| AXError::SystemError(format!("CDP connect: {e}")))?;

        let request = serde_json::json!({
            "id": 1,
            "method": "Page.captureScreenshot",
            "params": { "format": "png" }
        });

        socket
            .send(tungstenite::Message::Text(request.to_string()))
            .map_err(|e| AXError::SystemError(format!("CDP send: {e}")))?;

        loop {
            let msg = socket
                .read()
                .map_err(|e| AXError::SystemError(format!("CDP read: {e}")))?;

            if let tungstenite::Message::Text(text) = msg {
                let resp: serde_json::Value = serde_json::from_str(&text)
                    .map_err(|e| AXError::SystemError(format!("CDP parse: {e}")))?;

                if resp["id"].as_u64() == Some(1) {
                    let b64 = resp["result"]["data"]
                        .as_str()
                        .ok_or_else(|| AXError::SystemError("No screenshot data".into()))?;

                    return base64_decode(b64);
                }
            }
        }
    }
}

/// Decode a standard base64 string without an external crate.
///
/// Only the RFC 4648 alphabet is needed for CDP's `Page.captureScreenshot`.
fn base64_decode(input: &str) -> AXResult<Vec<u8>> {
    // Standard base64 alphabet
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut lookup = [255u8; 256];
    for (i, &c) in TABLE.iter().enumerate() {
        lookup[c as usize] = i as u8;
    }

    let clean: Vec<u8> = input.bytes().filter(|&b| b != b'=').collect();
    let mut out = Vec::with_capacity(clean.len() * 3 / 4);

    for chunk in clean.chunks(4) {
        let vals: Vec<u8> = chunk
            .iter()
            .map(|&b| lookup[b as usize])
            .collect();

        if vals.iter().any(|&v| v == 255) {
            return Err(AXError::SystemError("Invalid base64 character".into()));
        }

        match vals.as_slice() {
            [a, b, c, d] => {
                out.push((a << 2) | (b >> 4));
                out.push((b << 4) | (c >> 2));
                out.push((c << 6) | d);
            }
            [a, b, c] => {
                out.push((a << 2) | (b >> 4));
                out.push((b << 4) | (c >> 2));
            }
            [a, b] => {
                out.push((a << 2) | (b >> 4));
            }
            _ => {}
        }
    }

    Ok(out)
}

// ── Docker command abstraction (seam for tests) ───────────────────────────────

/// Abstraction over Docker CLI commands.
///
/// Inject a [`MockDockerRunner`] in tests to avoid real Docker.
pub(crate) trait DockerRunner {
    fn run_container(&self, args: &[&str]) -> AXResult<String>;
    fn stop_container(&self, container_id: &str) -> AXResult<()>;
    fn rm_container(&self, container_id: &str) -> AXResult<()>;
    fn list_neko_containers(&self) -> AXResult<Vec<String>>;
}

/// Production runner: delegates to the `docker` CLI binary.
struct RealDockerRunner;

impl DockerRunner for RealDockerRunner {
    fn run_container(&self, args: &[&str]) -> AXResult<String> {
        let output = Command::new("docker")
            .arg("run")
            .args(args)
            .output()
            .map_err(|e| AXError::SystemError(format!("docker run: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AXError::SystemError(format!("docker run failed: {stderr}")));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn stop_container(&self, container_id: &str) -> AXResult<()> {
        let status = Command::new("docker")
            .args(["stop", container_id])
            .status()
            .map_err(|e| AXError::SystemError(format!("docker stop: {e}")))?;

        status.success().then_some(()).ok_or_else(|| {
            AXError::SystemError(format!("docker stop {container_id} failed"))
        })
    }

    fn rm_container(&self, container_id: &str) -> AXResult<()> {
        let status = Command::new("docker")
            .args(["rm", "-f", container_id])
            .status()
            .map_err(|e| AXError::SystemError(format!("docker rm: {e}")))?;

        status.success().then_some(()).ok_or_else(|| {
            AXError::SystemError(format!("docker rm {container_id} failed"))
        })
    }

    fn list_neko_containers(&self) -> AXResult<Vec<String>> {
        let output = Command::new("docker")
            .args(["ps", "-a", "-q", "--filter", "label=axterminator.neko=1"])
            .output()
            .map_err(|e| AXError::SystemError(format!("docker ps: {e}")))?;

        let ids = String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect();

        Ok(ids)
    }
}

// ── Manager ───────────────────────────────────────────────────────────────────

/// Manages the lifecycle of Neko browser containers.
///
/// Tracks every container launched in the current session so [`cleanup`]
/// can remove orphans even if individual [`stop`] calls were skipped.
///
/// [`cleanup`]: DockerManager::cleanup
/// [`stop`]: DockerManager::stop
///
/// # Example
///
/// ```rust,no_run
/// use axterminator::docker_browser::{DockerManager, NekoConfig, BrowserType};
///
/// let mut mgr = DockerManager::new();
///
/// // Launch two browsers simultaneously
/// let chrome = mgr.launch(NekoConfig::chromium()).unwrap();
/// let firefox = mgr.launch(NekoConfig::firefox()).unwrap();
///
/// // ... run tests ...
///
/// // Explicit cleanup
/// mgr.stop(&chrome).unwrap();
/// mgr.stop(&firefox).unwrap();
/// // or bulk: mgr.cleanup();
/// ```
pub struct DockerManager {
    runner: Box<dyn DockerRunner>,
    /// Tracks container IDs launched in this session.
    active: HashMap<String, NekoBrowser>,
}

impl std::fmt::Debug for DockerManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DockerManager")
            .field("active_count", &self.active.len())
            .finish()
    }
}

impl DockerManager {
    /// Create a manager backed by the real `docker` CLI.
    #[must_use]
    pub fn new() -> Self {
        Self::with_runner(Box::new(RealDockerRunner))
    }

    /// Create a manager with a custom runner (used in tests).
    #[must_use]
    pub(crate) fn with_runner(runner: Box<dyn DockerRunner>) -> Self {
        Self {
            runner,
            active: HashMap::new(),
        }
    }

    /// Launch a Neko container for the given configuration.
    ///
    /// The container is started detached (`-d`) and the host ports in
    /// `config` are forwarded to the container's CDP and VNC ports.
    ///
    /// # Errors
    ///
    /// * [`AXError::SystemError`] — Docker daemon not running, image pull
    ///   failure, or port already in use.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use axterminator::docker_browser::{DockerManager, NekoConfig};
    /// let mut mgr = DockerManager::new();
    /// let browser = mgr.launch(NekoConfig::chromium()).unwrap();
    /// ```
    pub fn launch(&mut self, config: NekoConfig) -> AXResult<NekoBrowser> {
        let container_name = self.unique_name(&config);
        let args = build_docker_args(&config, &container_name);
        let args_refs: Vec<&str> = args.iter().map(String::as_str).collect();

        let container_id = self.runner.run_container(&args_refs)?;

        let browser = NekoBrowser {
            container_id: container_id.clone(),
            cdp_port: config.cdp_port,
            vnc_port: config.vnc_port,
            browser: config.browser,
        };

        self.active.insert(container_id, browser.clone());
        Ok(browser)
    }

    /// Stop and remove a specific Neko container.
    ///
    /// # Errors
    ///
    /// Returns [`AXError::SystemError`] if the Docker CLI fails.
    pub fn stop(&mut self, browser: &NekoBrowser) -> AXResult<()> {
        self.runner.stop_container(&browser.container_id)?;
        self.runner.rm_container(&browser.container_id)?;
        self.active.remove(&browser.container_id);
        Ok(())
    }

    /// Remove all Neko containers tracked in this session AND any labelled
    /// `axterminator.neko=1` found via `docker ps`.
    ///
    /// Returns the total number of containers removed.
    ///
    /// This is the safe teardown path for CI runners where individual `stop`
    /// calls may have been skipped due to test panics.
    pub fn cleanup(&mut self) -> usize {
        // Combine tracked + discovered containers
        let tracked: Vec<String> = self.active.keys().cloned().collect();
        let discovered = self.runner.list_neko_containers().unwrap_or_default();
        let mut ids: Vec<String> = tracked;
        for id in discovered {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }

        let count = ids.len();
        for id in &ids {
            let _ = self.runner.stop_container(id);
            let _ = self.runner.rm_container(id);
        }
        self.active.clear();
        count
    }

    /// Number of containers currently tracked by this manager.
    #[must_use]
    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    /// Generate a unique container name based on prefix + browser + cdp port.
    fn unique_name(&self, config: &NekoConfig) -> String {
        format!(
            "{}-{}-{}",
            config.name_prefix,
            config.browser.image_suffix(),
            config.cdp_port,
        )
    }
}

impl Default for DockerManager {
    fn default() -> Self {
        Self::new()
    }
}

// ── Docker CLI argument construction ─────────────────────────────────────────

/// Build the `docker run` argument list for a Neko container.
fn build_docker_args(config: &NekoConfig, name: &str) -> Vec<String> {
    vec![
        "--detach".into(),
        "--name".into(),
        name.to_string(),
        "--label".into(),
        "axterminator.neko=1".into(),
        "--publish".into(),
        format!("{}:9222", config.cdp_port),
        "--publish".into(),
        format!("{}:5900", config.vnc_port),
        "--shm-size".into(),
        "2g".into(),
        "--env".into(),
        format!("NEKO_SCREEN={}x{}@30", config.width, config.height),
        "--env".into(),
        format!("NEKO_PASSWORD_ADMIN={}", config.admin_password),
        "--env".into(),
        "NEKO_REMOTE_DEBUGGING_PORT=9222".into(),
        config.browser.docker_image(),
    ]
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Mock infrastructure ───────────────────────────────────────────────

    /// Mock Docker runner — no real containers, no filesystem side-effects.
    struct MockDockerRunner {
        /// Pre-configured return value for `run_container`.
        next_id: std::cell::Cell<u64>,
        /// Whether `run_container` should simulate failure.
        fail_launch: bool,
        /// Containers available for `list_neko_containers`.
        discoverable: Vec<String>,
    }

    impl MockDockerRunner {
        fn new() -> Self {
            Self {
                next_id: std::cell::Cell::new(1),
                fail_launch: false,
                discoverable: vec![],
            }
        }

        fn failing() -> Self {
            Self {
                fail_launch: true,
                ..Self::new()
            }
        }

        fn with_discoverable(ids: Vec<String>) -> Self {
            Self {
                discoverable: ids,
                ..Self::new()
            }
        }
    }

    impl DockerRunner for MockDockerRunner {
        fn run_container(&self, _args: &[&str]) -> AXResult<String> {
            if self.fail_launch {
                return Err(AXError::SystemError("docker daemon not running".into()));
            }
            let id = self.next_id.get();
            self.next_id.set(id + 1);
            Ok(format!("mock-container-{id:04}"))
        }

        fn stop_container(&self, _container_id: &str) -> AXResult<()> {
            Ok(())
        }

        fn rm_container(&self, _container_id: &str) -> AXResult<()> {
            Ok(())
        }

        fn list_neko_containers(&self) -> AXResult<Vec<String>> {
            Ok(self.discoverable.clone())
        }
    }

    /// Mock screenshot injector — returns a predictable 3-byte PNG stub.
    struct MockCdpShooter {
        pixels: Vec<u8>,
    }

    impl MockCdpShooter {
        fn new(pixels: Vec<u8>) -> Self {
            Self { pixels }
        }
    }

    impl ScreenshotShooter for MockCdpShooter {
        fn capture(&self, _cdp_port: u16) -> AXResult<Vec<u8>> {
            Ok(self.pixels.clone())
        }
    }

    // ── BrowserType ───────────────────────────────────────────────────────

    #[test]
    fn browser_type_image_suffix_maps_correctly() {
        // GIVEN / WHEN / THEN — each variant produces the right suffix
        assert_eq!(BrowserType::Chromium.image_suffix(), "chromium");
        assert_eq!(BrowserType::Firefox.image_suffix(), "firefox");
        assert_eq!(BrowserType::Brave.image_suffix(), "brave");
        assert_eq!(BrowserType::Edge.image_suffix(), "microsoft-edge");
    }

    #[test]
    fn browser_type_docker_image_uses_ghcr_prefix() {
        // GIVEN
        let browser = BrowserType::Chromium;
        // WHEN
        let image = browser.docker_image();
        // THEN
        assert_eq!(image, "ghcr.io/m1k1o/neko/chromium:latest");
        assert!(image.starts_with("ghcr.io/m1k1o/neko/"));
    }

    #[test]
    fn browser_type_cdp_support_chromium_and_edge_only() {
        // GIVEN / WHEN / THEN
        assert!(BrowserType::Chromium.supports_full_cdp());
        assert!(BrowserType::Edge.supports_full_cdp());
        assert!(!BrowserType::Firefox.supports_full_cdp());
        assert!(!BrowserType::Brave.supports_full_cdp());
    }

    // ── NekoConfig defaults ───────────────────────────────────────────────

    #[test]
    fn neko_config_chromium_defaults_sensible() {
        // GIVEN
        let cfg = NekoConfig::chromium();
        // THEN
        assert_eq!(cfg.browser, BrowserType::Chromium);
        assert_eq!(cfg.width, 1920);
        assert_eq!(cfg.height, 1080);
        assert_eq!(cfg.cdp_port, 9222);
        assert_eq!(cfg.vnc_port, 5900);
        assert_eq!(cfg.admin_password, "admin");
        assert_eq!(cfg.name_prefix, "neko");
    }

    #[test]
    fn neko_config_builder_overrides_browser_and_ports() {
        // GIVEN
        let cfg = NekoConfig::builder()
            .browser(BrowserType::Firefox)
            .cdp_port(9333)
            .vnc_port(5910)
            .build();
        // THEN
        assert_eq!(cfg.browser, BrowserType::Firefox);
        assert_eq!(cfg.cdp_port, 9333);
        assert_eq!(cfg.vnc_port, 5910);
        // Defaults preserved for unset fields
        assert_eq!(cfg.width, 1920);
        assert_eq!(cfg.height, 1080);
    }

    #[test]
    fn neko_config_builder_full_customisation() {
        // GIVEN
        let cfg = NekoConfig::builder()
            .browser(BrowserType::Brave)
            .dimensions(1280, 720)
            .cdp_port(9400)
            .vnc_port(5920)
            .admin_password("s3cr3t")
            .name_prefix("ci-neko")
            .build();
        // THEN
        assert_eq!(cfg.browser, BrowserType::Brave);
        assert_eq!(cfg.width, 1280);
        assert_eq!(cfg.height, 720);
        assert_eq!(cfg.cdp_port, 9400);
        assert_eq!(cfg.vnc_port, 5920);
        assert_eq!(cfg.admin_password, "s3cr3t");
        assert_eq!(cfg.name_prefix, "ci-neko");
    }

    // ── NekoBrowser ───────────────────────────────────────────────────────

    #[test]
    fn neko_browser_cdp_url_format_correct() {
        // GIVEN
        let browser = NekoBrowser::new_for_test("abc123", 9222, 5900, BrowserType::Chromium);
        // WHEN
        let url = browser.cdp_url();
        // THEN: WebSocket URL pointing to 127.0.0.1 with correct port
        assert_eq!(url, "ws://127.0.0.1:9222/devtools/browser");
    }

    #[test]
    fn neko_browser_cdp_url_reflects_custom_port() {
        // GIVEN: Custom CDP port
        let browser = NekoBrowser::new_for_test("def456", 9400, 5920, BrowserType::Firefox);
        // WHEN
        let url = browser.cdp_url();
        // THEN
        assert_eq!(url, "ws://127.0.0.1:9400/devtools/browser");
    }

    #[test]
    fn neko_browser_vnc_addr_format_correct() {
        // GIVEN
        let browser = NekoBrowser::new_for_test("ghi789", 9222, 5901, BrowserType::Chromium);
        // WHEN / THEN
        assert_eq!(browser.vnc_addr(), "127.0.0.1:5901");
    }

    #[test]
    fn neko_browser_screenshot_via_mock_returns_expected_bytes() {
        // GIVEN: Mock shooter that returns a synthetic PNG stub
        let browser = NekoBrowser::new_for_test("mock-id", 9222, 5900, BrowserType::Chromium);
        let expected = vec![0x89u8, 0x50, 0x4E, 0x47]; // PNG magic bytes
        let shooter = MockCdpShooter::new(expected.clone());
        // WHEN
        let result = browser.screenshot_via(&shooter);
        // THEN
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), expected);
    }

    // ── DockerManager — container lifecycle ───────────────────────────────

    #[test]
    fn docker_manager_launch_tracks_container() {
        // GIVEN
        let mut mgr = DockerManager::with_runner(Box::new(MockDockerRunner::new()));
        // WHEN
        let browser = mgr.launch(NekoConfig::chromium()).unwrap();
        // THEN: manager tracks it and returns valid handle
        assert_eq!(mgr.active_count(), 1);
        assert!(!browser.container_id().is_empty());
        assert_eq!(browser.cdp_port, 9222);
        assert_eq!(browser.browser(), BrowserType::Chromium);
    }

    #[test]
    fn docker_manager_stop_removes_from_tracking() {
        // GIVEN: A running container
        let mut mgr = DockerManager::with_runner(Box::new(MockDockerRunner::new()));
        let browser = mgr.launch(NekoConfig::chromium()).unwrap();
        assert_eq!(mgr.active_count(), 1);
        // WHEN
        mgr.stop(&browser).unwrap();
        // THEN: no longer tracked
        assert_eq!(mgr.active_count(), 0);
    }

    #[test]
    fn docker_manager_multiple_browsers_tracked_independently() {
        // GIVEN
        let mut mgr = DockerManager::with_runner(Box::new(MockDockerRunner::new()));
        // WHEN: launch three different browsers
        let chrome = mgr.launch(NekoConfig::chromium()).unwrap();
        let firefox = mgr.launch(NekoConfig::firefox()).unwrap();
        let brave = mgr.launch(NekoConfig::brave()).unwrap();
        // THEN: all three tracked, all have distinct container IDs
        assert_eq!(mgr.active_count(), 3);
        let ids = [
            chrome.container_id(),
            firefox.container_id(),
            brave.container_id(),
        ];
        assert_ne!(ids[0], ids[1]);
        assert_ne!(ids[1], ids[2]);
        assert_ne!(ids[0], ids[2]);
    }

    #[test]
    fn docker_manager_cleanup_removes_all_tracked_containers() {
        // GIVEN: Two active containers
        let mut mgr = DockerManager::with_runner(Box::new(MockDockerRunner::new()));
        mgr.launch(NekoConfig::chromium()).unwrap();
        mgr.launch(NekoConfig::firefox()).unwrap();
        assert_eq!(mgr.active_count(), 2);
        // WHEN
        let removed = mgr.cleanup();
        // THEN: all removed, tracker cleared
        assert_eq!(removed, 2);
        assert_eq!(mgr.active_count(), 0);
    }

    #[test]
    fn docker_manager_cleanup_also_removes_discovered_orphans() {
        // GIVEN: Runner knows about orphan containers not tracked by manager
        let runner = MockDockerRunner::with_discoverable(vec![
            "orphan-001".into(),
            "orphan-002".into(),
        ]);
        let mut mgr = DockerManager::with_runner(Box::new(runner));
        // WHEN: cleanup with zero tracked but two discovered
        let removed = mgr.cleanup();
        // THEN: both orphans removed
        assert_eq!(removed, 2);
    }

    #[test]
    fn docker_manager_launch_failure_returns_error() {
        // GIVEN: Docker daemon not available
        let mut mgr = DockerManager::with_runner(Box::new(MockDockerRunner::failing()));
        // WHEN
        let result = mgr.launch(NekoConfig::chromium());
        // THEN: explicit error, no panic, nothing tracked
        assert!(result.is_err());
        assert_eq!(mgr.active_count(), 0);
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("docker daemon"));
    }

    // ── build_docker_args ─────────────────────────────────────────────────

    #[test]
    fn build_docker_args_includes_required_flags() {
        // GIVEN
        let cfg = NekoConfig::chromium();
        // WHEN
        let args = build_docker_args(&cfg, "test-neko-chromium-9222");
        // THEN: detached, port mappings, shm, env vars, image present
        assert!(args.contains(&"--detach".into()));
        assert!(args.contains(&"--shm-size".into()));
        assert!(args.contains(&"2g".into()));
        assert!(args.contains(&"--label".into()));
        assert!(args.contains(&"axterminator.neko=1".into()));
        assert!(args.iter().any(|a| a.contains("9222:9222")));
        assert!(args.iter().any(|a| a.contains("5900:5900")));
        assert!(args.iter().any(|a| a.contains("ghcr.io/m1k1o/neko/chromium")));
    }

    #[test]
    fn build_docker_args_respects_custom_ports_and_resolution() {
        // GIVEN: Custom ports and resolution
        let cfg = NekoConfig::builder()
            .browser(BrowserType::Firefox)
            .cdp_port(9500)
            .vnc_port(5950)
            .dimensions(1280, 720)
            .build();
        // WHEN
        let args = build_docker_args(&cfg, "neko-firefox-9500");
        // THEN
        assert!(args.iter().any(|a| a.contains("9500:9222")));
        assert!(args.iter().any(|a| a.contains("5950:5900")));
        assert!(args.iter().any(|a| a.contains("1280x720")));
        assert!(args.iter().any(|a| a.contains("ghcr.io/m1k1o/neko/firefox")));
    }

    // ── base64_decode ─────────────────────────────────────────────────────

    #[test]
    fn base64_decode_round_trips_hello() {
        // GIVEN: "Hello" base64-encoded
        let encoded = "SGVsbG8=";
        // WHEN
        let result = base64_decode(encoded).unwrap();
        // THEN
        assert_eq!(result, b"Hello");
    }

    #[test]
    fn base64_decode_rejects_invalid_characters() {
        // GIVEN: Invalid character '!'
        let result = base64_decode("SGVs!G8=");
        // THEN
        assert!(result.is_err());
    }
}
