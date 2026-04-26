//! Security model — modes, app filtering, audit logging, rate limiting.
//!
//! ## Security modes (AXTERMINATOR_SECURITY_MODE)
//!
//! | Mode | Behaviour |
//! |------|-----------|
//! | `normal` (default) | All tools allowed; mutating calls logged. |
//! | `safe` | Scripting tools blocked; `tools/list` reflects the restriction. |
//! | `sandboxed` | Read-only tools only; all writes return a policy error. |
//!
//! ## App policy (~/.config/axterminator/security.toml)
//!
//! Optional TOML file with `allowed` and `denied` string arrays of app names
//! or bundle IDs.  Absent file → allow everything.
//!
//! ```toml
//! allowed = ["Calculator", "com.apple.Safari"]
//! denied  = ["com.apple.Keychain-Access", "1Password"]
//! ```
//!
//! ## Audit log (~/.local/share/axterminator/audit.jsonl)
//!
//! Every mutating tool call appended as one JSON line:
//!
//! ```json
//! {"ts":"2025-11-05T12:00:00Z","tool":"ax_click","args":{...},"result":"ok"}
//! ```
//!
//! ## Rate limiting
//!
//! Sliding 1-second window; defaults 50 calls/s (override with
//! `AXTERMINATOR_RATE_LIMIT_RPS`).

use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

use serde_json::Value;
use tracing::warn;

// ---------------------------------------------------------------------------
// Security mode
// ---------------------------------------------------------------------------

/// Operational security mode, sourced from `AXTERMINATOR_SECURITY_MODE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityMode {
    /// All tools available; mutating actions are logged.
    Normal,
    /// Scripting tools blocked; destructive actions require confirmation.
    Safe,
    /// Read-only tools only — observe and inspect, never mutate.
    Sandboxed,
}

impl SecurityMode {
    /// Resolve the mode from the environment, defaulting to [`Normal`][Self::Normal].
    #[must_use]
    pub fn from_env() -> Self {
        match std::env::var("AXTERMINATOR_SECURITY_MODE")
            .as_deref()
            .unwrap_or("")
        {
            "safe" => Self::Safe,
            "sandboxed" => Self::Sandboxed,
            _ => Self::Normal,
        }
    }

    /// Return `true` when `tool_name` is permitted in this mode.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use axterminator::mcp::security::SecurityMode;
    ///
    /// assert!(SecurityMode::Normal.is_tool_allowed("ax_click"));
    /// assert!(!SecurityMode::Safe.is_tool_allowed("ax_run_script"));
    /// assert!(!SecurityMode::Sandboxed.is_tool_allowed("ax_click"));
    /// assert!(SecurityMode::Sandboxed.is_tool_allowed("ax_screenshot"));
    /// ```
    #[must_use]
    pub fn is_tool_allowed(&self, tool_name: &str) -> bool {
        match self {
            Self::Normal => true,
            Self::Safe => !is_script_tool(tool_name),
            Self::Sandboxed => is_read_only_tool(tool_name),
        }
    }

    /// Human-readable policy violation message for a blocked tool.
    #[must_use]
    pub fn blocked_message(&self, tool_name: &str) -> String {
        match self {
            Self::Normal => unreachable!("Normal mode never blocks"),
            Self::Safe => {
                format!("Tool '{tool_name}' is blocked in safe mode (scripting disabled)")
            }
            Self::Sandboxed => {
                format!("Tool '{tool_name}' is blocked in sandboxed mode (read-only)")
            }
        }
    }
}

/// Tools that execute arbitrary scripts or shell commands.
fn is_script_tool(name: &str) -> bool {
    matches!(
        name,
        "ax_run_script" | "ax_shell" | "ax_eval" | "ax_exec" | "ax_script"
    )
}

/// Tools permitted in sandboxed (read-only) mode.
fn is_read_only_tool(name: &str) -> bool {
    matches!(
        name,
        "ax_is_accessible"
            | "ax_connect"
            | "ax_list_apps"
            | "ax_find"
            | "ax_find_visual"
            | "ax_get_tree"
            | "ax_get_attributes"
            | "ax_screenshot"
            | "ax_get_value"
            | "ax_list_windows"
            | "ax_assert"
            | "ax_wait_idle"
            | "ax_query"
            | "ax_analyze"
            | "ax_app_profile"
            | "ax_watch_start"
            | "ax_watch_stop"
            | "ax_watch_status"
    )
}

/// Return `true` for tools that mutate state and should be audit-logged.
///
/// Covers everything that is NOT purely read-only — connections, clicks,
/// typing, scripting, workflow mutations, etc.
#[must_use]
pub fn is_mutating_tool(name: &str) -> bool {
    !matches!(
        name,
        "ax_is_accessible"
            | "ax_list_apps"
            | "ax_find"
            | "ax_find_visual"
            | "ax_get_tree"
            | "ax_get_attributes"
            | "ax_screenshot"
            | "ax_get_value"
            | "ax_list_windows"
            | "ax_assert"
            | "ax_query"
            | "ax_analyze"
            | "ax_app_profile"
            | "ax_watch_status"
    )
}

// ---------------------------------------------------------------------------
// App policy
// ---------------------------------------------------------------------------

/// Per-app allow/deny policy loaded from `~/.config/axterminator/security.toml`.
pub struct AppPolicy {
    /// Explicit allowlist; empty means "allow everything not denied".
    allowed: HashSet<String>,
    /// Explicit denylist; checked first.
    denied: HashSet<String>,
}

impl AppPolicy {
    /// Load the policy file, returning a permissive default on any error.
    ///
    /// Silently allows everything when the file is absent — the security file
    /// is opt-in.
    #[must_use]
    pub fn load() -> Self {
        let path = config_dir().join("security.toml");
        match fs::read_to_string(&path) {
            Ok(content) => Self::parse(&content),
            Err(_) => Self::permissive(),
        }
    }

    /// Return `true` when `app_id` (name or bundle ID) is permitted.
    ///
    /// Evaluation order: denied → allowed (empty = permit all) → permit.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use axterminator::mcp::security::AppPolicy;
    ///
    /// let policy = AppPolicy::permissive();
    /// assert!(policy.is_app_allowed("Calculator"));
    /// ```
    #[must_use]
    pub fn is_app_allowed(&self, app_id: &str) -> bool {
        if self.denied.contains(app_id) {
            return false;
        }
        self.allowed.is_empty() || self.allowed.contains(app_id)
    }

    /// Return true when no allow/deny policy is configured.
    #[must_use]
    pub fn is_permissive(&self) -> bool {
        self.allowed.is_empty() && self.denied.is_empty()
    }

    /// A permissive policy that allows every application.
    #[must_use]
    pub fn permissive() -> Self {
        Self {
            allowed: HashSet::new(),
            denied: HashSet::new(),
        }
    }

    /// Parse a TOML string into a policy without pulling in a TOML crate.
    ///
    /// Accepts only the two array keys `allowed` and `denied`; anything else
    /// is ignored.  The format is:
    /// ```toml
    /// allowed = ["Calculator", "com.apple.Safari"]
    /// denied  = ["com.apple.Keychain-Access"]
    /// ```
    pub(crate) fn parse(content: &str) -> Self {
        let mut allowed = HashSet::new();
        let mut denied = HashSet::new();

        for line in content.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("allowed") {
                extract_string_array(rest, &mut allowed);
            } else if let Some(rest) = line.strip_prefix("denied") {
                extract_string_array(rest, &mut denied);
            }
        }

        Self { allowed, denied }
    }
}

/// Extract quoted strings from a TOML array literal `= ["a", "b"]`.
fn extract_string_array(s: &str, out: &mut HashSet<String>) {
    // Find the `[...]` region.
    let Some(start) = s.find('[') else { return };
    let Some(end) = s.find(']') else { return };
    if end <= start {
        return;
    }
    let inner = &s[start + 1..end];
    for part in inner.split(',') {
        let trimmed = part.trim().trim_matches('"').trim_matches('\'');
        if !trimmed.is_empty() {
            out.insert(trimmed.to_string());
        }
    }
}

// ---------------------------------------------------------------------------
// Rate limiter
// ---------------------------------------------------------------------------

/// Sliding 1-second window rate limiter.
///
/// Reset the window the first time a call arrives after at least one second
/// has elapsed since the window opened.  This is a simple and cheap guard —
/// not a token-bucket — sufficient for the ~50 RPS design target.
pub struct RateLimiter {
    window_start: Instant,
    count: u32,
    limit_per_second: u32,
}

impl RateLimiter {
    /// Create a limiter, reading `AXTERMINATOR_RATE_LIMIT_RPS` (default 50).
    #[must_use]
    pub fn new() -> Self {
        let limit = std::env::var("AXTERMINATOR_RATE_LIMIT_RPS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(50);
        Self {
            window_start: Instant::now(),
            count: 0,
            limit_per_second: limit,
        }
    }

    /// Record one call and return `true` if within the rate limit.
    ///
    /// Resets the sliding window when more than one second has elapsed.
    pub fn check(&mut self) -> bool {
        if self.window_start.elapsed().as_secs() >= 1 {
            self.window_start = Instant::now();
            self.count = 0;
        }
        self.count += 1;
        self.count <= self.limit_per_second
    }

    /// Current calls recorded in the active window (for diagnostics).
    #[must_use]
    pub fn current_count(&self) -> u32 {
        self.count
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Audit log
// ---------------------------------------------------------------------------

/// Append-only JSONL audit log at `~/.local/share/axterminator/audit.jsonl`.
///
/// Each record is one JSON line:
/// ```json
/// {"ts":"2025-11-05T12:00:00.000Z","tool":"ax_click","args":{...},"result":"ok"}
/// ```
pub struct AuditLog {
    writer: Option<BufWriter<std::fs::File>>,
}

impl AuditLog {
    /// Open (or create) the audit log file, making parent directories as needed.
    #[must_use]
    pub fn open() -> Self {
        let path = audit_log_path();
        let writer = try_open_log(&path);
        Self { writer }
    }

    /// Append one audit entry for a completed tool call.
    ///
    /// Silently swallows I/O errors — audit logging is best-effort and must
    /// never abort a tool result.
    pub fn record(&mut self, tool: &str, args: &Value, result: &str) {
        let Some(w) = self.writer.as_mut() else {
            return;
        };
        let ts = utc_timestamp();
        // Compact JSON — one line per record.
        let line = format!(
            "{{\"ts\":\"{ts}\",\"tool\":\"{tool}\",\"args\":{args},\"result\":\"{result}\"}}\n"
        );
        // Best-effort: ignore write/flush errors.
        let _ = w.write_all(line.as_bytes());
        let _ = w.flush();
    }
}

/// Attempt to open the log file; returns `None` on failure (best-effort).
fn try_open_log(path: &PathBuf) -> Option<BufWriter<std::fs::File>> {
    if let Some(parent) = path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            warn!(path = %parent.display(), error = %e, "audit log dir creation failed");
            return None;
        }
    }
    match OpenOptions::new().create(true).append(true).open(path) {
        Ok(f) => Some(BufWriter::new(f)),
        Err(e) => {
            warn!(path = %path.display(), error = %e, "audit log open failed");
            None
        }
    }
}

// ---------------------------------------------------------------------------
// SecurityGuard — unified entry point used by Server
// ---------------------------------------------------------------------------

/// Bundled security state held by the MCP server.
///
/// All fields are `pub(crate)` so `server.rs` can construct and hold the
/// guard, while `server_handlers.rs` drives it through the `check_*` and
/// `audit` methods.
pub struct SecurityGuard {
    pub(crate) mode: SecurityMode,
    pub(crate) app_policy: AppPolicy,
    pub(crate) rate_limiter: Mutex<RateLimiter>,
    pub(crate) audit: Mutex<AuditLog>,
}

impl SecurityGuard {
    /// Initialise from environment and config file.
    #[must_use]
    pub fn new() -> Self {
        Self {
            mode: SecurityMode::from_env(),
            app_policy: AppPolicy::load(),
            rate_limiter: Mutex::new(RateLimiter::new()),
            audit: Mutex::new(AuditLog::open()),
        }
    }

    /// Check the rate limit. Returns `Err` with a JSON-RPC –32000 message when exceeded.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` when the rate limit is exceeded.
    pub fn check_rate_limit(&self) -> Result<(), String> {
        let mut limiter = self
            .rate_limiter
            .lock()
            .expect("rate limiter lock poisoned");
        if limiter.check() {
            Ok(())
        } else {
            Err("Rate limit exceeded — too many tool calls per second".to_string())
        }
    }

    /// Check whether `tool_name` is permitted in the current security mode.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` when the tool is blocked.
    pub fn check_tool_allowed(&self, tool_name: &str) -> Result<(), String> {
        if self.mode.is_tool_allowed(tool_name) {
            Ok(())
        } else {
            Err(self.mode.blocked_message(tool_name))
        }
    }

    /// Check whether `app_id` is permitted by the app policy.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` when the app is blocked.
    pub fn check_app_allowed(&self, app_id: &str) -> Result<(), String> {
        if self.app_policy.is_app_allowed(app_id) {
            Ok(())
        } else {
            Err(format!("App '{app_id}' is blocked by security policy"))
        }
    }

    /// Return true when no app allow/deny policy is configured.
    #[must_use]
    pub fn app_policy_is_permissive(&self) -> bool {
        self.app_policy.is_permissive()
    }

    /// Append an audit record for a completed mutating tool call.
    ///
    /// No-op for read-only tools (determined by [`is_mutating_tool`]).
    pub fn audit_tool_call(&self, tool: &str, args: &Value, result: &str) {
        if !is_mutating_tool(tool) {
            return;
        }
        if let Ok(mut log) = self.audit.lock() {
            log.record(tool, args, result);
        }
    }

    /// The active security mode (used by `tools/list` filtering).
    #[must_use]
    pub fn mode(&self) -> SecurityMode {
        self.mode
    }
}

impl Default for SecurityGuard {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

fn config_dir() -> PathBuf {
    home_dir().join(".config").join("axterminator")
}

fn audit_log_path() -> PathBuf {
    home_dir()
        .join(".local")
        .join("share")
        .join("axterminator")
        .join("audit.jsonl")
}

// ---------------------------------------------------------------------------
// Time helper — no chrono dependency
// ---------------------------------------------------------------------------

/// Format the current wall-clock time as an approximate RFC 3339 UTC string.
///
/// Uses `std::time::SystemTime`; precision is seconds.  A proper chrono/time
/// dependency is not warranted for audit timestamps.
fn utc_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Convert epoch seconds to a naive Y-M-D H:M:S UTC string.
    epoch_to_iso8601(secs)
}

/// Convert Unix epoch seconds to a compact ISO 8601 UTC string (no sub-second).
fn epoch_to_iso8601(secs: u64) -> String {
    // Days since epoch, leapyear-aware.
    let (year, month, day, h, m, s) = epoch_to_ymd_hms(secs);
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

fn epoch_to_ymd_hms(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    let s = (secs % 60) as u32;
    let total_min = secs / 60;
    let m = (total_min % 60) as u32;
    let total_h = total_min / 60;
    let h = (total_h % 24) as u32;
    let total_days = total_h / 24;

    // Gregorian calendar computation (no chrono dependency).
    let (year, month, day) = days_to_ymd(total_days as u32);
    (year, month, day, h, m, s)
}

fn days_to_ymd(days: u32) -> (u32, u32, u32) {
    // Algorithm from http://www.howardhinnant.com/date_algorithms.html
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // SecurityMode
    // -----------------------------------------------------------------------

    #[test]
    fn normal_mode_allows_all_tools() {
        // GIVEN: normal mode
        // WHEN/THEN: every tool is permitted
        let mode = SecurityMode::Normal;
        assert!(mode.is_tool_allowed("ax_click"));
        assert!(mode.is_tool_allowed("ax_run_script"));
        assert!(mode.is_tool_allowed("ax_screenshot"));
    }

    #[test]
    fn safe_mode_blocks_script_tools() {
        // GIVEN: safe mode
        let mode = SecurityMode::Safe;
        // WHEN: checking script tool
        // THEN: blocked
        assert!(!mode.is_tool_allowed("ax_run_script"));
        assert!(!mode.is_tool_allowed("ax_shell"));
    }

    #[test]
    fn safe_mode_allows_non_script_tools() {
        let mode = SecurityMode::Safe;
        assert!(mode.is_tool_allowed("ax_click"));
        assert!(mode.is_tool_allowed("ax_screenshot"));
        assert!(mode.is_tool_allowed("ax_find"));
    }

    #[test]
    fn sandboxed_mode_allows_read_only_tools() {
        // GIVEN: sandboxed mode
        let mode = SecurityMode::Sandboxed;
        // WHEN: checking read-only tools
        // THEN: all permitted
        assert!(mode.is_tool_allowed("ax_screenshot"));
        assert!(mode.is_tool_allowed("ax_find"));
        assert!(mode.is_tool_allowed("ax_get_tree"));
        assert!(mode.is_tool_allowed("ax_list_apps"));
        assert!(mode.is_tool_allowed("ax_get_value"));
    }

    #[test]
    fn sandboxed_mode_blocks_mutating_tools() {
        let mode = SecurityMode::Sandboxed;
        assert!(!mode.is_tool_allowed("ax_click"));
        assert!(!mode.is_tool_allowed("ax_type"));
        assert!(!mode.is_tool_allowed("ax_set_value"));
    }

    #[test]
    fn blocked_message_names_the_tool() {
        let mode = SecurityMode::Sandboxed;
        let msg = mode.blocked_message("ax_click");
        assert!(msg.contains("ax_click"), "message: {msg}");
    }

    // -----------------------------------------------------------------------
    // AppPolicy
    // -----------------------------------------------------------------------

    #[test]
    fn permissive_policy_allows_any_app() {
        // GIVEN: no config — permissive default
        let policy = AppPolicy::permissive();
        assert!(policy.is_app_allowed("Calculator"));
        assert!(policy.is_app_allowed("com.apple.Keychain-Access"));
        assert!(policy.is_permissive());
    }

    #[test]
    fn denied_app_is_blocked_even_with_empty_allowlist() {
        // GIVEN: policy that denies 1Password
        let policy = AppPolicy::parse("denied = [\"1Password\"]");
        // THEN: blocked
        assert!(!policy.is_app_allowed("1Password"));
        // THEN: others still allowed
        assert!(policy.is_app_allowed("Calculator"));
        assert!(!policy.is_permissive());
    }

    #[test]
    fn allowlist_blocks_unlisted_apps() {
        // GIVEN: policy that only allows Calculator
        let policy = AppPolicy::parse("allowed = [\"Calculator\"]");
        // THEN: Calculator ok
        assert!(policy.is_app_allowed("Calculator"));
        // THEN: Safari blocked
        assert!(!policy.is_app_allowed("Safari"));
        assert!(!policy.is_permissive());
    }

    #[test]
    fn denied_takes_precedence_over_allowed() {
        // GIVEN: app is both allowed and denied (edge case)
        let policy = AppPolicy::parse("allowed = [\"X\"]\ndenied = [\"X\"]");
        // THEN: deny wins
        assert!(!policy.is_app_allowed("X"));
    }

    #[test]
    fn policy_parse_handles_bundle_ids() {
        let policy = AppPolicy::parse("allowed = [\"com.apple.Safari\"]");
        assert!(policy.is_app_allowed("com.apple.Safari"));
        assert!(!policy.is_app_allowed("com.apple.Finder"));
    }

    #[test]
    fn policy_parse_ignores_unknown_keys() {
        // GIVEN: TOML with extra unknown keys
        let policy = AppPolicy::parse("unknown = [\"something\"]\ndenied = [\"Bad\"]");
        assert!(!policy.is_app_allowed("Bad"));
        assert!(policy.is_app_allowed("Good"));
    }

    // -----------------------------------------------------------------------
    // RateLimiter
    // -----------------------------------------------------------------------

    #[test]
    fn rate_limiter_allows_calls_within_limit() {
        // GIVEN: limiter with 50 RPS (default)
        let mut rl = RateLimiter {
            window_start: Instant::now(),
            count: 0,
            limit_per_second: 5,
        };
        // WHEN: 5 calls within 1 second
        for _ in 0..5 {
            assert!(rl.check(), "should allow calls within limit");
        }
    }

    #[test]
    fn rate_limiter_blocks_call_over_limit() {
        // GIVEN: limiter already at limit
        let mut rl = RateLimiter {
            window_start: Instant::now(),
            count: 0,
            limit_per_second: 3,
        };
        rl.check();
        rl.check();
        rl.check();
        // WHEN: 4th call
        // THEN: blocked
        assert!(!rl.check());
    }

    #[test]
    fn rate_limiter_resets_after_one_second() {
        // GIVEN: limiter with exhausted window from 2 seconds ago
        let mut rl = RateLimiter {
            window_start: Instant::now() - std::time::Duration::from_secs(2),
            count: 100,
            limit_per_second: 3,
        };
        // WHEN: new call after window expired
        // THEN: window resets, call allowed
        assert!(rl.check());
        assert_eq!(rl.current_count(), 1);
    }

    // -----------------------------------------------------------------------
    // is_mutating_tool
    // -----------------------------------------------------------------------

    #[test]
    fn screenshot_is_not_mutating() {
        assert!(!is_mutating_tool("ax_screenshot"));
    }

    #[test]
    fn click_is_mutating() {
        assert!(is_mutating_tool("ax_click"));
    }

    #[test]
    fn connect_is_mutating() {
        assert!(is_mutating_tool("ax_connect"));
    }

    #[test]
    fn find_is_not_mutating() {
        assert!(!is_mutating_tool("ax_find"));
    }

    // -----------------------------------------------------------------------
    // epoch_to_iso8601
    // -----------------------------------------------------------------------

    #[test]
    fn epoch_zero_is_unix_epoch() {
        // 1970-01-01T00:00:00Z
        assert_eq!(epoch_to_iso8601(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn known_epoch_timestamp_is_correct() {
        // 2025-11-05T12:00:00Z = 1762344000 (verified: calendar.timegm(2025,11,5,12,0,0))
        assert_eq!(epoch_to_iso8601(1_762_344_000), "2025-11-05T12:00:00Z");
    }

    // -----------------------------------------------------------------------
    // SecurityGuard integration
    // -----------------------------------------------------------------------

    #[test]
    fn guard_check_tool_allowed_normal_mode_passes() {
        // GIVEN: guard in normal mode (no env var set in test)
        let guard = SecurityGuard {
            mode: SecurityMode::Normal,
            app_policy: AppPolicy::permissive(),
            rate_limiter: Mutex::new(RateLimiter {
                window_start: Instant::now(),
                count: 0,
                limit_per_second: 50,
            }),
            audit: Mutex::new(AuditLog { writer: None }),
        };
        // WHEN: any tool
        // THEN: allowed
        assert!(guard.check_tool_allowed("ax_click").is_ok());
    }

    #[test]
    fn guard_check_tool_blocked_in_sandboxed_mode() {
        let guard = SecurityGuard {
            mode: SecurityMode::Sandboxed,
            app_policy: AppPolicy::permissive(),
            rate_limiter: Mutex::new(RateLimiter {
                window_start: Instant::now(),
                count: 0,
                limit_per_second: 50,
            }),
            audit: Mutex::new(AuditLog { writer: None }),
        };
        assert!(guard.check_tool_allowed("ax_click").is_err());
        assert!(guard.check_tool_allowed("ax_screenshot").is_ok());
    }

    #[test]
    fn guard_rate_limit_exceeded_returns_err() {
        let guard = SecurityGuard {
            mode: SecurityMode::Normal,
            app_policy: AppPolicy::permissive(),
            rate_limiter: Mutex::new(RateLimiter {
                window_start: Instant::now(),
                count: 50,
                limit_per_second: 50,
            }),
            audit: Mutex::new(AuditLog { writer: None }),
        };
        // count is already at limit — next call exceeds it
        assert!(guard.check_rate_limit().is_err());
    }

    #[test]
    fn guard_app_denied_returns_err() {
        let guard = SecurityGuard {
            mode: SecurityMode::Normal,
            app_policy: AppPolicy::parse("denied = [\"BadApp\"]"),
            rate_limiter: Mutex::new(RateLimiter {
                window_start: Instant::now(),
                count: 0,
                limit_per_second: 50,
            }),
            audit: Mutex::new(AuditLog { writer: None }),
        };
        assert!(guard.check_app_allowed("BadApp").is_err());
        assert!(guard.check_app_allowed("GoodApp").is_ok());
    }
}
