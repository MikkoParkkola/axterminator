//! Electron App Skill Profiles — high-level semantic metadata for Electron apps.
//!
//! # Purpose
//!
//! Raw CDP DOM manipulation is powerful but fragile.  This module provides a
//! **profile registry** that maps well-known Electron apps to their CSS selectors,
//! keyboard shortcuts, and capability sets so that higher-level code can compose
//! semantic actions without hard-coding DOM knowledge at every call site.
//!
//! # Design
//!
//! A [`ProfileRegistry`] owns a [`HashMap`] of [`AppProfile`] values, keyed by
//! normalised app name.  Profiles are inserted either via [`ProfileRegistry::register`]
//! or loaded from [`ProfileRegistry::with_builtins`].  Lookups are case-insensitive.
//!
//! Builtin profiles ship for VS Code, Slack, Chrome, Terminal, and Finder — the
//! five most common Electron/WebView apps encountered in macOS automation.
//!
//! # Example
//!
//! ```rust
//! use axterminator::electron_profiles::{AppCapability, ProfileRegistry};
//!
//! let registry = ProfileRegistry::with_builtins();
//!
//! // Capability-based discovery
//! let chat_apps = registry.find_by_capability(&AppCapability::Chat);
//! assert!(chat_apps.iter().any(|p| p.name == "Slack"));
//!
//! // CSS selector lookup
//! let selector = registry.get_selector("vscode", "editor_tab");
//! assert!(selector.is_some());
//!
//! // Keyboard shortcut lookup
//! let shortcut = registry.get_shortcut("vscode", "command_palette");
//! assert_eq!(shortcut, Some("Meta+Shift+P"));
//! ```

use std::collections::HashMap;

// ── Public types ──────────────────────────────────────────────────────────────

/// Semantic capability class for an Electron app.
///
/// Used to discover apps that support a given interaction pattern, e.g. all
/// apps with [`AppCapability::Chat`] can receive `send_message` actions.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AppCapability {
    /// Real-time messaging (Slack, Discord, Teams).
    Chat,
    /// Email client (Spark, Mimestream, Superhuman).
    Email,
    /// Calendar scheduling.
    Calendar,
    /// Source code editor (VS Code, Cursor, Zed).
    CodeEditor,
    /// Web browser (Chrome, Edge, Arc).
    Browser,
    /// Shell / terminal emulator.
    Terminal,
    /// File system browser.
    FileManager,
    /// Custom capability not covered by the canonical set.
    Custom(String),
}

/// High-level semantic profile for a single Electron app.
///
/// Contains everything needed to automate the app without hard-coding DOM
/// knowledge at the call site: CSS selectors by semantic name, keyboard
/// shortcuts by action name, and the CDP debug port when known.
///
/// Construct via [`AppProfile::builder`] or directly with struct literal syntax.
#[derive(Debug, Clone)]
pub struct AppProfile {
    /// Human-readable app name (e.g. `"VS Code"`).
    pub name: String,
    /// macOS bundle identifier (e.g. `"com.microsoft.VSCode"`).
    pub app_id: String,
    /// CDP debug port, when the app exposes one.
    ///
    /// `None` means the port is unknown or variable.
    pub cdp_port: Option<u16>,
    /// Semantic capabilities this app supports.
    pub capabilities: Vec<AppCapability>,
    /// CSS selector map: `semantic_name → selector`.
    ///
    /// Keys are lowercase, hyphen-separated action descriptors, e.g.
    /// `"message_input"`, `"send_button"`, `"channel_list"`.
    pub selectors: HashMap<String, String>,
    /// Keyboard shortcut map: `action_name → shortcut`.
    ///
    /// Modifiers use `Meta` (Cmd on macOS), `Ctrl`, `Shift`, `Alt`.
    /// Keys are the same lowercase descriptors as `selectors`.
    pub shortcuts: HashMap<String, String>,
}

impl AppProfile {
    /// Create a profile builder with the mandatory identity fields.
    ///
    /// # Example
    ///
    /// ```rust
    /// use axterminator::electron_profiles::AppProfile;
    ///
    /// let profile = AppProfile::builder("VS Code", "com.microsoft.VSCode")
    ///     .cdp_port(9222)
    ///     .build();
    ///
    /// assert_eq!(profile.name, "VS Code");
    /// assert_eq!(profile.cdp_port, Some(9222));
    /// ```
    #[must_use]
    pub fn builder(name: impl Into<String>, app_id: impl Into<String>) -> AppProfileBuilder {
        AppProfileBuilder::new(name, app_id)
    }

    /// Returns `true` when the profile declares a given [`AppCapability`].
    ///
    /// ```rust
    /// use axterminator::electron_profiles::{AppCapability, ProfileRegistry};
    ///
    /// let reg = ProfileRegistry::with_builtins();
    /// let vscode = reg.detect("vscode").unwrap();
    /// assert!(vscode.has_capability(&AppCapability::CodeEditor));
    /// assert!(!vscode.has_capability(&AppCapability::Chat));
    /// ```
    #[must_use]
    pub fn has_capability(&self, cap: &AppCapability) -> bool {
        self.capabilities.contains(cap)
    }
}

// ── Builder ───────────────────────────────────────────────────────────────────

/// Fluent builder for [`AppProfile`].
#[derive(Debug)]
pub struct AppProfileBuilder {
    profile: AppProfile,
}

impl AppProfileBuilder {
    fn new(name: impl Into<String>, app_id: impl Into<String>) -> Self {
        Self {
            profile: AppProfile {
                name: name.into(),
                app_id: app_id.into(),
                cdp_port: None,
                capabilities: Vec::new(),
                selectors: HashMap::new(),
                shortcuts: HashMap::new(),
            },
        }
    }

    /// Set the CDP debug port.
    #[must_use]
    pub fn cdp_port(mut self, port: u16) -> Self {
        self.profile.cdp_port = Some(port);
        self
    }

    /// Append a capability.
    #[must_use]
    pub fn capability(mut self, cap: AppCapability) -> Self {
        self.profile.capabilities.push(cap);
        self
    }

    /// Register a CSS selector under a semantic name.
    #[must_use]
    pub fn selector(mut self, name: impl Into<String>, css: impl Into<String>) -> Self {
        self.profile.selectors.insert(name.into(), css.into());
        self
    }

    /// Register a keyboard shortcut under an action name.
    #[must_use]
    pub fn shortcut(mut self, action: impl Into<String>, keys: impl Into<String>) -> Self {
        self.profile.shortcuts.insert(action.into(), keys.into());
        self
    }

    /// Finalise and return the [`AppProfile`].
    #[must_use]
    pub fn build(self) -> AppProfile {
        self.profile
    }
}

// ── Registry ──────────────────────────────────────────────────────────────────

/// Registry of [`AppProfile`] records, keyed by normalised app name.
///
/// All lookups are case-insensitive and treat `-`, `_`, and whitespace as
/// equivalent so that `"VS Code"`, `"vscode"`, and `"vs_code"` all resolve to
/// the same entry.
///
/// # Example
///
/// ```rust
/// use axterminator::electron_profiles::{AppCapability, AppProfile, ProfileRegistry};
///
/// let mut registry = ProfileRegistry::default();
/// let profile = AppProfile::builder("Notion", "notion.id")
///     .capability(AppCapability::Email)   // using Email as example custom
///     .selector("search_input", ".notion-search-input")
///     .build();
///
/// registry.register(profile);
///
/// assert!(registry.detect("notion").is_some());
/// assert!(registry.detect("NOTION").is_some());
/// assert!(registry.detect("no-match").is_none());
/// ```
#[derive(Debug, Default)]
pub struct ProfileRegistry {
    /// Profiles keyed by normalised app name.
    profiles: HashMap<String, AppProfile>,
}

impl ProfileRegistry {
    /// Create a new empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a registry pre-populated with all builtin profiles.
    ///
    /// Builtin profiles cover VS Code, Slack, Chrome, Terminal, and Finder.
    #[must_use]
    pub fn with_builtins() -> Self {
        let mut reg = Self::new();
        for profile in builtin_profiles() {
            reg.register(profile);
        }
        reg
    }

    /// Register an [`AppProfile`], making it discoverable via [`detect`](Self::detect).
    ///
    /// Overwrites any existing profile registered under the same normalised name.
    pub fn register(&mut self, profile: AppProfile) {
        let key = normalise_name(&profile.name);
        self.profiles.insert(key, profile);
    }

    /// Look up a profile by app name.
    ///
    /// Name matching is case-insensitive.  Returns `None` when no profile is
    /// registered for the given name.
    #[must_use]
    pub fn detect(&self, app_name: &str) -> Option<&AppProfile> {
        self.profiles.get(&normalise_name(app_name))
    }

    /// Return all profiles that declare a given [`AppCapability`].
    ///
    /// # Example
    ///
    /// ```rust
    /// use axterminator::electron_profiles::{AppCapability, ProfileRegistry};
    ///
    /// let reg = ProfileRegistry::with_builtins();
    /// let editors = reg.find_by_capability(&AppCapability::CodeEditor);
    /// assert!(editors.iter().any(|p| p.name == "VS Code"));
    /// ```
    #[must_use]
    pub fn find_by_capability(&self, cap: &AppCapability) -> Vec<&AppProfile> {
        self.profiles
            .values()
            .filter(|p| p.has_capability(cap))
            .collect()
    }

    /// Look up a CSS selector by semantic name for a given app.
    ///
    /// Returns `None` when the app is unknown or the selector is not registered.
    ///
    /// # Example
    ///
    /// ```rust
    /// use axterminator::electron_profiles::ProfileRegistry;
    ///
    /// let reg = ProfileRegistry::with_builtins();
    /// let sel = reg.get_selector("slack", "message_input");
    /// assert!(sel.is_some());
    /// ```
    #[must_use]
    pub fn get_selector(&self, app_name: &str, semantic_name: &str) -> Option<&str> {
        self.detect(app_name)
            .and_then(|p| p.selectors.get(semantic_name))
            .map(String::as_str)
    }

    /// Look up a keyboard shortcut by action name for a given app.
    ///
    /// Returns `None` when the app is unknown or no shortcut is registered.
    ///
    /// # Example
    ///
    /// ```rust
    /// use axterminator::electron_profiles::ProfileRegistry;
    ///
    /// let reg = ProfileRegistry::with_builtins();
    /// let sc = reg.get_shortcut("vscode", "command_palette");
    /// assert_eq!(sc, Some("Meta+Shift+P"));
    /// ```
    #[must_use]
    pub fn get_shortcut(&self, app_name: &str, action: &str) -> Option<&str> {
        self.detect(app_name)
            .and_then(|p| p.shortcuts.get(action))
            .map(String::as_str)
    }

    /// Total number of registered profiles.
    #[must_use]
    pub fn len(&self) -> usize {
        self.profiles.len()
    }

    /// Returns `true` when the registry contains no profiles.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.profiles.is_empty()
    }
}

// ── Builtin profiles ──────────────────────────────────────────────────────────

/// Return the canonical set of builtin [`AppProfile`] records.
///
/// Profiles are sourced from the official DOM structure / accessibility trees of
/// each app at the time this module was authored.  CSS selectors target stable
/// structural attributes (`[data-qa]`, ARIA roles, component class prefixes) in
/// preference to volatile generated class names.
///
/// # Profiles
///
/// * **VS Code** — code editor, CDP port 9222, command palette, file/editor ops.
/// * **Slack** — chat, message input, channel navigation.
/// * **Chrome** — browser, address bar, tab controls.
/// * **Terminal** — macOS built-in terminal, shell input.
/// * **Finder** — macOS file manager via `WebKit`, sidebar and file view.
#[must_use]
pub fn builtin_profiles() -> Vec<AppProfile> {
    vec![
        vscode_profile(),
        slack_profile(),
        chrome_profile(),
        terminal_profile(),
        finder_profile(),
    ]
}

// ── Individual builtin builders ───────────────────────────────────────────────

/// VS Code profile — code editor with deep CDP integration on port 9222.
fn vscode_profile() -> AppProfile {
    AppProfile::builder("VS Code", "com.microsoft.VSCode")
        .cdp_port(9222)
        .capability(AppCapability::CodeEditor)
        .capability(AppCapability::Terminal)
        // Selectors
        .selector("editor_tab", ".tab.active")
        .selector("editor_input", ".monaco-editor .inputarea")
        .selector("problems_panel", "#workbench.panel.markers")
        .selector("terminal_input", ".xterm-helper-textarea")
        .selector("sidebar_explorer", ".explorer-viewlet")
        .selector("quick_open_input", ".quick-input-widget .input")
        .selector("status_bar", "#workbench.parts.statusbar")
        // Shortcuts
        .shortcut("command_palette", "Meta+Shift+P")
        .shortcut("quick_open", "Meta+P")
        .shortcut("new_terminal", "Ctrl+`")
        .shortcut("toggle_sidebar", "Meta+B")
        .shortcut("find_in_files", "Meta+Shift+F")
        .shortcut("go_to_line", "Ctrl+G")
        .shortcut("save", "Meta+S")
        .shortcut("close_editor", "Meta+W")
        .build()
}

/// Slack profile — team chat with `[data-qa]` stable attribute selectors.
fn slack_profile() -> AppProfile {
    AppProfile::builder("Slack", "com.tinyspeck.slackmacgap")
        .capability(AppCapability::Chat)
        // Selectors — Slack uses data-qa attributes which are stable across releases
        .selector("message_input", "[data-qa='message_input']")
        .selector("send_button", "[data-qa='texty_send_btn']")
        .selector("channel_list", "[data-qa='channel_sidebar']")
        .selector("search_input", "[data-qa='search_input']")
        .selector("channel_header", "[data-qa='channel_name']")
        .selector("message_list", "[data-qa='message_list']")
        .selector("thread_input", "[data-qa='thread_input']")
        // Shortcuts
        .shortcut("new_message", "Meta+K")
        .shortcut("search", "Meta+G")
        .shortcut("mark_all_read", "Shift+Esc")
        .shortcut("next_unread", "Alt+Shift+Down")
        .build()
}

/// Chrome profile — browser with standard `DevTools` selectors.
fn chrome_profile() -> AppProfile {
    AppProfile::builder("Chrome", "com.google.Chrome")
        .capability(AppCapability::Browser)
        // Selectors — Chromium's omnibox and tab bar are structurally stable
        .selector("address_bar", "#omnibox-input")
        .selector("new_tab_button", ".new-tab-button")
        .selector("tab_strip", ".tab-strip")
        .selector("active_tab", ".tab.active")
        .selector("back_button", "#back-button")
        .selector("forward_button", "#forward-button")
        .selector("reload_button", "#reload-button")
        // Shortcuts
        .shortcut("new_tab", "Meta+T")
        .shortcut("close_tab", "Meta+W")
        .shortcut("reopen_tab", "Meta+Shift+T")
        .shortcut("address_bar", "Meta+L")
        .shortcut("find_in_page", "Meta+F")
        .shortcut("developer_tools", "Meta+Alt+I")
        .shortcut("downloads", "Meta+Shift+J")
        .build()
}

/// Terminal profile — macOS built-in Terminal.app.
fn terminal_profile() -> AppProfile {
    AppProfile::builder("Terminal", "com.apple.Terminal")
        .capability(AppCapability::Terminal)
        // Terminal.app uses WebKit rendering — selectors target the terminal view
        .selector("terminal_view", ".xterm-screen")
        .selector("terminal_input", ".xterm-helper-textarea")
        // Shortcuts
        .shortcut("new_window", "Meta+N")
        .shortcut("new_tab", "Meta+T")
        .shortcut("close_tab", "Meta+W")
        .shortcut("clear", "Meta+K")
        .shortcut("find", "Meta+F")
        .build()
}

/// Finder profile — macOS file manager.
fn finder_profile() -> AppProfile {
    AppProfile::builder("Finder", "com.apple.finder")
        .capability(AppCapability::FileManager)
        // Selectors — Finder uses native views, limited CSS surface
        .selector("sidebar", ".sidebar")
        .selector("file_list", ".file-list")
        .selector("search_input", "[role='searchbox']")
        .selector("path_bar", ".path-bar")
        // Shortcuts
        .shortcut("new_window", "Meta+N")
        .shortcut("new_folder", "Meta+Shift+N")
        .shortcut("search", "Meta+F")
        .shortcut("go_home", "Meta+Shift+H")
        .shortcut("go_to_folder", "Meta+Shift+G")
        .shortcut("show_info", "Meta+I")
        .build()
}

// ── Name normalisation ────────────────────────────────────────────────────────

/// Normalise an app name for case-insensitive, separator-insensitive lookup.
///
/// Converts to lowercase and strips all separators (`-`, `_`, whitespace) so
/// that `"VS Code"`, `"vs-code"`, `"vs_code"`, and `"vscode"` all produce the
/// same key (`"vscode"`).
fn normalise_name(name: &str) -> String {
    name.chars()
        .filter(|c| !matches!(c, '-' | '_' | ' ' | '\t'))
        .collect::<String>()
        .to_lowercase()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helpers ────────────────────────────────────────────────────────────

    fn minimal_profile(name: &str) -> AppProfile {
        AppProfile::builder(name, format!("com.example.{}", name.to_lowercase())).build()
    }

    // ── normalise_name ─────────────────────────────────────────────────────

    #[test]
    fn normalise_name_lowercases_and_strips_spaces() {
        // GIVEN / WHEN / THEN: "VS Code" → "vscode"
        assert_eq!(normalise_name("VS Code"), "vscode");
    }

    #[test]
    fn normalise_name_strips_hyphens() {
        // GIVEN / WHEN / THEN
        assert_eq!(normalise_name("vs-code"), "vscode");
    }

    #[test]
    fn normalise_name_strips_underscores() {
        // GIVEN / WHEN / THEN
        assert_eq!(normalise_name("vs_code"), "vscode");
    }

    #[test]
    fn normalise_name_strips_leading_trailing_whitespace() {
        // GIVEN / WHEN / THEN
        assert_eq!(normalise_name("  Slack  "), "slack");
    }

    // ── ProfileRegistry::register / detect ────────────────────────────────

    #[test]
    fn register_and_detect_profile_by_exact_name() {
        // GIVEN
        let mut registry = ProfileRegistry::new();
        registry.register(minimal_profile("Notion"));
        // WHEN
        let found = registry.detect("Notion");
        // THEN
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Notion");
    }

    #[test]
    fn detect_is_case_insensitive() {
        // GIVEN
        let mut registry = ProfileRegistry::new();
        registry.register(minimal_profile("Slack"));
        // WHEN: query with different casing
        let found = registry.detect("SLACK");
        // THEN
        assert!(found.is_some());
    }

    #[test]
    fn detect_unknown_app_returns_none() {
        // GIVEN: Empty registry
        let registry = ProfileRegistry::new();
        // WHEN
        let found = registry.detect("NonExistentApp");
        // THEN
        assert!(found.is_none());
    }

    #[test]
    fn register_overwrites_existing_profile() {
        // GIVEN: First profile registered
        let mut registry = ProfileRegistry::new();
        registry.register(minimal_profile("App"));
        // WHEN: Re-register with different app_id
        let updated = AppProfile::builder("App", "com.example.updated").build();
        registry.register(updated);
        // THEN: New profile wins
        let found = registry.detect("App").unwrap();
        assert_eq!(found.app_id, "com.example.updated");
    }

    // ── builtin_profiles ───────────────────────────────────────────────────

    #[test]
    fn builtin_profiles_includes_vscode() {
        // GIVEN / WHEN
        let reg = ProfileRegistry::with_builtins();
        // THEN: VS Code detectable under various name forms
        assert!(reg.detect("VS Code").is_some());
        assert!(reg.detect("vscode").is_some());
        assert!(reg.detect("vs-code").is_some());
    }

    #[test]
    fn builtin_profiles_includes_slack() {
        // GIVEN / WHEN
        let reg = ProfileRegistry::with_builtins();
        // THEN
        assert!(reg.detect("Slack").is_some());
    }

    #[test]
    fn builtin_profiles_includes_chrome() {
        // GIVEN / WHEN
        let reg = ProfileRegistry::with_builtins();
        // THEN
        assert!(reg.detect("Chrome").is_some());
    }

    #[test]
    fn builtin_profiles_includes_terminal_and_finder() {
        // GIVEN / WHEN
        let reg = ProfileRegistry::with_builtins();
        // THEN
        assert!(reg.detect("Terminal").is_some());
        assert!(reg.detect("Finder").is_some());
    }

    #[test]
    fn builtin_profiles_five_entries_total() {
        // GIVEN / WHEN
        let reg = ProfileRegistry::with_builtins();
        // THEN: exactly 5 builtin profiles
        assert_eq!(reg.len(), 5);
    }

    // ── AppProfile::has_capability ─────────────────────────────────────────

    #[test]
    fn vscode_has_code_editor_and_terminal_capabilities() {
        // GIVEN
        let reg = ProfileRegistry::with_builtins();
        let vscode = reg.detect("vscode").unwrap();
        // THEN
        assert!(vscode.has_capability(&AppCapability::CodeEditor));
        assert!(vscode.has_capability(&AppCapability::Terminal));
    }

    #[test]
    fn vscode_does_not_have_chat_capability() {
        // GIVEN
        let reg = ProfileRegistry::with_builtins();
        let vscode = reg.detect("vscode").unwrap();
        // THEN
        assert!(!vscode.has_capability(&AppCapability::Chat));
    }

    #[test]
    fn slack_has_chat_capability() {
        // GIVEN
        let reg = ProfileRegistry::with_builtins();
        let slack = reg.detect("slack").unwrap();
        // THEN
        assert!(slack.has_capability(&AppCapability::Chat));
    }

    #[test]
    fn chrome_has_browser_capability() {
        // GIVEN
        let reg = ProfileRegistry::with_builtins();
        let chrome = reg.detect("chrome").unwrap();
        // THEN
        assert!(chrome.has_capability(&AppCapability::Browser));
    }

    // ── AppProfile::builder — cdp_port ─────────────────────────────────────

    #[test]
    fn vscode_profile_has_cdp_port_9222() {
        // GIVEN
        let reg = ProfileRegistry::with_builtins();
        // WHEN
        let vscode = reg.detect("vscode").unwrap();
        // THEN
        assert_eq!(vscode.cdp_port, Some(9222));
    }

    #[test]
    fn profile_without_cdp_port_is_none() {
        // GIVEN: Slack has no fixed CDP port
        let reg = ProfileRegistry::with_builtins();
        let slack = reg.detect("slack").unwrap();
        // THEN
        assert!(slack.cdp_port.is_none());
    }

    // ── get_selector ───────────────────────────────────────────────────────

    #[test]
    fn get_selector_returns_css_for_known_name() {
        // GIVEN
        let reg = ProfileRegistry::with_builtins();
        // WHEN
        let sel = reg.get_selector("slack", "message_input");
        // THEN: real Slack data-qa selector
        assert_eq!(sel, Some("[data-qa='message_input']"));
    }

    #[test]
    fn get_selector_returns_none_for_unknown_semantic_name() {
        // GIVEN
        let reg = ProfileRegistry::with_builtins();
        // WHEN
        let sel = reg.get_selector("vscode", "nonexistent_selector");
        // THEN
        assert!(sel.is_none());
    }

    #[test]
    fn get_selector_returns_none_for_unknown_app() {
        // GIVEN
        let reg = ProfileRegistry::with_builtins();
        // WHEN
        let sel = reg.get_selector("nonexistent_app", "anything");
        // THEN
        assert!(sel.is_none());
    }

    #[test]
    fn get_selector_vscode_editor_tab() {
        // GIVEN
        let reg = ProfileRegistry::with_builtins();
        // WHEN
        let sel = reg.get_selector("vscode", "editor_tab");
        // THEN
        assert_eq!(sel, Some(".tab.active"));
    }

    // ── get_shortcut ───────────────────────────────────────────────────────

    #[test]
    fn get_shortcut_returns_keybinding_for_known_action() {
        // GIVEN
        let reg = ProfileRegistry::with_builtins();
        // WHEN
        let sc = reg.get_shortcut("vscode", "command_palette");
        // THEN
        assert_eq!(sc, Some("Meta+Shift+P"));
    }

    #[test]
    fn get_shortcut_returns_none_for_unknown_action() {
        // GIVEN
        let reg = ProfileRegistry::with_builtins();
        // WHEN
        let sc = reg.get_shortcut("vscode", "nonexistent_action");
        // THEN
        assert!(sc.is_none());
    }

    #[test]
    fn get_shortcut_returns_none_for_unknown_app() {
        // GIVEN
        let reg = ProfileRegistry::with_builtins();
        // WHEN
        let sc = reg.get_shortcut("not_an_app", "save");
        // THEN
        assert!(sc.is_none());
    }

    #[test]
    fn get_shortcut_slack_new_message() {
        // GIVEN
        let reg = ProfileRegistry::with_builtins();
        // WHEN
        let sc = reg.get_shortcut("slack", "new_message");
        // THEN
        assert_eq!(sc, Some("Meta+K"));
    }

    // ── find_by_capability ─────────────────────────────────────────────────

    #[test]
    fn find_by_capability_chat_returns_slack() {
        // GIVEN
        let reg = ProfileRegistry::with_builtins();
        // WHEN
        let apps = reg.find_by_capability(&AppCapability::Chat);
        // THEN
        assert!(apps.iter().any(|p| p.name == "Slack"));
    }

    #[test]
    fn find_by_capability_code_editor_returns_vscode() {
        // GIVEN
        let reg = ProfileRegistry::with_builtins();
        // WHEN
        let apps = reg.find_by_capability(&AppCapability::CodeEditor);
        // THEN
        assert!(apps.iter().any(|p| p.name == "VS Code"));
    }

    #[test]
    fn find_by_capability_file_manager_returns_finder() {
        // GIVEN
        let reg = ProfileRegistry::with_builtins();
        // WHEN
        let apps = reg.find_by_capability(&AppCapability::FileManager);
        // THEN
        assert!(apps.iter().any(|p| p.name == "Finder"));
    }

    #[test]
    fn find_by_capability_email_returns_empty_for_builtins() {
        // GIVEN: None of the 5 builtins have Email capability
        let reg = ProfileRegistry::with_builtins();
        // WHEN
        let apps = reg.find_by_capability(&AppCapability::Email);
        // THEN
        assert!(apps.is_empty());
    }

    // ── Custom capability ──────────────────────────────────────────────────

    #[test]
    fn custom_capability_survives_round_trip() {
        // GIVEN
        let mut registry = ProfileRegistry::new();
        let profile = AppProfile::builder("Figma", "com.figma.Desktop")
            .capability(AppCapability::Custom("Design".into()))
            .build();
        registry.register(profile);
        // WHEN
        let found = registry.detect("figma").unwrap();
        // THEN
        assert!(found.has_capability(&AppCapability::Custom("Design".into())));
        assert!(!found.has_capability(&AppCapability::Custom("Other".into())));
    }

    // ── AppProfileBuilder ──────────────────────────────────────────────────

    #[test]
    fn builder_constructs_profile_with_all_fields() {
        // GIVEN
        let profile = AppProfile::builder("TestApp", "com.test.App")
            .cdp_port(8888)
            .capability(AppCapability::Browser)
            .selector("nav", "nav.main")
            .shortcut("quit", "Meta+Q")
            .build();
        // THEN
        assert_eq!(profile.name, "TestApp");
        assert_eq!(profile.app_id, "com.test.App");
        assert_eq!(profile.cdp_port, Some(8888));
        assert!(profile.has_capability(&AppCapability::Browser));
        assert_eq!(
            profile.selectors.get("nav").map(String::as_str),
            Some("nav.main")
        );
        assert_eq!(
            profile.shortcuts.get("quit").map(String::as_str),
            Some("Meta+Q")
        );
    }

    // ── is_empty / len ─────────────────────────────────────────────────────

    #[test]
    fn empty_registry_is_empty_and_len_zero() {
        // GIVEN
        let reg = ProfileRegistry::new();
        // THEN
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn registry_len_increments_on_register() {
        // GIVEN
        let mut reg = ProfileRegistry::new();
        assert_eq!(reg.len(), 0);
        // WHEN
        reg.register(minimal_profile("A"));
        reg.register(minimal_profile("B"));
        // THEN
        assert_eq!(reg.len(), 2);
        assert!(!reg.is_empty());
    }
}
