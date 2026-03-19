//! MCP progress notifications — `notifications/progress`.
//!
//! Long-running tools (those expected to take >100 ms) send incremental
//! progress updates so clients can show spinners and status text.
//!
//! # Wire format
//!
//! ```json
//! {
//!   "jsonrpc": "2.0",
//!   "method": "notifications/progress",
//!   "params": {
//!     "progressToken": "ax_get_tree-0000000000000001",
//!     "progress": 33,
//!     "total": 100,
//!     "message": "Scanning layer 1/3…"
//!   }
//! }
//! ```
//!
//! `progressToken` is a `u64` counter rendered as a fixed-width hex string
//! to keep it lexicographically sortable and globally unique per process run.
//!
//! # Usage
//!
//! ```rust,ignore
//! let token = next_progress_token();
//! emit_progress(&mut out, token, 0, 100, "Starting…")?;
//! // … do work …
//! emit_progress(&mut out, token, 50, 100, "Halfway done")?;
//! emit_progress(&mut out, token, 100, 100, "Complete")?;
//! ```

use std::io::{self, Write};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::json;

// ---------------------------------------------------------------------------
// Token generator
// ---------------------------------------------------------------------------

/// Global monotonic counter for progress token generation.
///
/// Using a process-wide counter rather than per-tool counters avoids token
/// collisions when the server processes requests concurrently.
static PROGRESS_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Allocate a new unique progress token.
///
/// Tokens are formatted as 16 hex digits, e.g. `"000000000000002a"`, to be
/// both unique within a process run and lexicographically sortable.
///
/// # Example
///
/// ```rust,ignore
/// let token = next_progress_token();
/// assert_eq!(token.len(), 16);
/// ```
#[must_use]
pub fn next_progress_token() -> String {
    let n = PROGRESS_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{n:016x}")
}

// ---------------------------------------------------------------------------
// Emit helpers
// ---------------------------------------------------------------------------

/// Emit a single `notifications/progress` line to `out`.
///
/// `progress` and `total` are both in the range `[0, total]`.  A `progress`
/// equal to `total` conventionally signals completion, but the server does not
/// enforce this — callers are responsible for sending a final notification.
///
/// `message` is optional; passing an empty string omits the field from the
/// JSON to keep the payload compact.
///
/// # Errors
///
/// Returns an I/O error if writing to `out` or flushing fails.
///
/// # Panics
///
/// Cannot panic: the JSON structure is statically defined.
pub fn emit_progress<W: Write>(
    out: &mut W,
    token: &str,
    progress: u32,
    total: u32,
    message: &str,
) -> io::Result<()> {
    let params = if message.is_empty() {
        json!({
            "progressToken": token,
            "progress":      progress,
            "total":         total
        })
    } else {
        json!({
            "progressToken": token,
            "progress":      progress,
            "total":         total,
            "message":       message
        })
    };

    let notif = json!({
        "jsonrpc": "2.0",
        "method":  "notifications/progress",
        "params":  params
    });

    let line = serde_json::to_string(&notif).expect("notification serialization cannot fail");
    writeln!(out, "{line}")?;
    out.flush()
}

// ---------------------------------------------------------------------------
// ProgressReporter — RAII helper for multi-step operations
// ---------------------------------------------------------------------------

/// Drives progress for a multi-step operation with a fixed step count.
///
/// Each call to [`ProgressReporter::step`] advances the cursor by one and
/// emits a `notifications/progress` notification.  The total is set at
/// construction time.
///
/// # Example
///
/// ```rust,ignore
/// let mut reporter = ProgressReporter::new(&mut stdout, 3);
/// reporter.step("Scanning layer 1/3")?;
/// reporter.step("Scanning layer 2/3")?;
/// reporter.step("Scanning layer 3/3")?;
/// ```
pub struct ProgressReporter<'w, W: Write> {
    out: &'w mut W,
    token: String,
    current: u32,
    total: u32,
}

impl<'w, W: Write> ProgressReporter<'w, W> {
    /// Create a reporter for `total` steps.
    ///
    /// Allocates a fresh progress token automatically.
    #[must_use]
    pub fn new(out: &'w mut W, total: u32) -> Self {
        Self {
            out,
            token: next_progress_token(),
            current: 0,
            total,
        }
    }

    /// The progress token assigned to this reporter.
    #[must_use]
    pub fn token(&self) -> &str {
        &self.token
    }

    /// Advance one step, emitting a progress notification with `message`.
    ///
    /// # Errors
    ///
    /// Propagates I/O errors from the underlying writer.
    pub fn step(&mut self, message: &str) -> io::Result<()> {
        self.current = self.current.saturating_add(1).min(self.total);
        emit_progress(self.out, &self.token, self.current, self.total, message)
    }

    /// Emit a completion notification (`progress == total`).
    ///
    /// # Errors
    ///
    /// Propagates I/O errors from the underlying writer.
    pub fn complete(&mut self, message: &str) -> io::Result<()> {
        emit_progress(self.out, &self.token, self.total, self.total, message)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn parse_line(buf: &[u8]) -> Value {
        let line = String::from_utf8_lossy(buf);
        serde_json::from_str(line.trim()).expect("valid JSON notification")
    }

    // -----------------------------------------------------------------------
    // next_progress_token
    // -----------------------------------------------------------------------

    #[test]
    fn next_progress_token_returns_sixteen_hex_digits() {
        // GIVEN: fresh token request
        let token = next_progress_token();
        // THEN: exactly 16 lowercase hex characters
        assert_eq!(token.len(), 16, "token: {token}");
        assert!(
            token.chars().all(|c| c.is_ascii_hexdigit()),
            "token: {token}"
        );
    }

    #[test]
    fn next_progress_token_is_unique_on_each_call() {
        // GIVEN: two consecutive requests
        let a = next_progress_token();
        let b = next_progress_token();
        // THEN: tokens differ
        assert_ne!(a, b);
    }

    #[test]
    fn next_progress_token_is_monotonically_increasing() {
        // GIVEN: two consecutive tokens
        let a = u64::from_str_radix(&next_progress_token(), 16).unwrap();
        let b = u64::from_str_radix(&next_progress_token(), 16).unwrap();
        // THEN: b > a
        assert!(b > a);
    }

    // -----------------------------------------------------------------------
    // emit_progress
    // -----------------------------------------------------------------------

    #[test]
    fn emit_progress_writes_valid_jsonrpc_notification() {
        // GIVEN: a buffer
        let mut buf = Vec::<u8>::new();
        // WHEN: emitting a progress notification
        emit_progress(&mut buf, "tok-1", 33, 100, "step 1").unwrap();
        // THEN: well-formed MCP notification
        let v = parse_line(&buf);
        assert_eq!(v["jsonrpc"], "2.0");
        assert_eq!(v["method"], "notifications/progress");
        assert_eq!(v["params"]["progressToken"], "tok-1");
        assert_eq!(v["params"]["progress"], 33);
        assert_eq!(v["params"]["total"], 100);
        assert_eq!(v["params"]["message"], "step 1");
    }

    #[test]
    fn emit_progress_omits_message_field_when_empty() {
        // GIVEN: empty message string
        let mut buf = Vec::<u8>::new();
        emit_progress(&mut buf, "tok-2", 0, 10, "").unwrap();
        // THEN: no "message" key in params
        let v = parse_line(&buf);
        assert!(
            v["params"].get("message").is_none(),
            "message should be absent"
        );
    }

    #[test]
    fn emit_progress_terminates_with_newline() {
        let mut buf = Vec::<u8>::new();
        emit_progress(&mut buf, "tok-3", 1, 1, "done").unwrap();
        assert!(String::from_utf8(buf).unwrap().ends_with('\n'));
    }

    #[test]
    fn emit_progress_progress_equals_total_signals_completion() {
        let mut buf = Vec::<u8>::new();
        emit_progress(&mut buf, "tok-4", 5, 5, "Complete").unwrap();
        let v = parse_line(&buf);
        assert_eq!(v["params"]["progress"], v["params"]["total"]);
    }

    // -----------------------------------------------------------------------
    // ProgressReporter
    // -----------------------------------------------------------------------

    #[test]
    fn progress_reporter_step_advances_progress_by_one() {
        // GIVEN: reporter with 3 steps
        let mut buf = Vec::<u8>::new();
        {
            let mut reporter = ProgressReporter::new(&mut buf, 3);
            // WHEN: first step
            reporter.step("step 1").unwrap();
        }
        // THEN: progress is 1, total is 3
        let v = parse_line(&buf);
        assert_eq!(v["params"]["progress"], 1);
        assert_eq!(v["params"]["total"], 3);
        assert_eq!(v["params"]["message"], "step 1");
    }

    #[test]
    fn progress_reporter_emits_one_notification_per_step() {
        // GIVEN: reporter with 2 steps
        let mut buf = Vec::<u8>::new();
        {
            let mut reporter = ProgressReporter::new(&mut buf, 2);
            reporter.step("a").unwrap();
            reporter.step("b").unwrap();
        }
        // THEN: exactly 2 newline-delimited JSON objects
        let text = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = text.trim_end().split('\n').collect();
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn progress_reporter_complete_sets_progress_to_total() {
        let mut buf = Vec::<u8>::new();
        {
            let mut reporter = ProgressReporter::new(&mut buf, 5);
            reporter.complete("all done").unwrap();
        }
        let v = parse_line(&buf);
        assert_eq!(v["params"]["progress"], 5);
        assert_eq!(v["params"]["total"], 5);
    }

    #[test]
    fn progress_reporter_step_does_not_exceed_total() {
        // GIVEN: reporter already at max
        let mut buf = Vec::<u8>::new();
        {
            let mut reporter = ProgressReporter::new(&mut buf, 1);
            reporter.step("first").unwrap();
            // WHEN: stepping past the total
            reporter.step("overflow attempt").unwrap();
        }
        // THEN: progress is clamped to total (1)
        let text = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = text.trim_end().split('\n').collect();
        let second: Value = serde_json::from_str(lines[1]).unwrap();
        let p = second["params"]["progress"].as_u64().unwrap();
        let t = second["params"]["total"].as_u64().unwrap();
        assert_eq!(p, t, "progress must not exceed total");
    }

    #[test]
    fn progress_reporter_all_steps_share_same_token() {
        // GIVEN: reporter with 2 steps
        let mut buf = Vec::<u8>::new();
        let token;
        {
            let mut reporter = ProgressReporter::new(&mut buf, 2);
            token = reporter.token().to_string();
            reporter.step("a").unwrap();
            reporter.step("b").unwrap();
        }
        // THEN: both notifications carry the same token
        for line in String::from_utf8(buf).unwrap().trim_end().split('\n') {
            let v: Value = serde_json::from_str(line).unwrap();
            assert_eq!(v["params"]["progressToken"], token.as_str());
        }
    }
}
