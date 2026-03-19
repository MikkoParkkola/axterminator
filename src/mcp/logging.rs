//! MCP structured logging — `notifications/message` notifications.
//!
//! MCP defines a `logging/setLevel` method and `notifications/message` notification
//! that clients display in their log panels. This module provides:
//!
//! - A [`LogLevel`] enum covering the four MCP log levels.
//! - A [`McpLogger`] that wraps a stdout writer and emits structured MCP log
//!   notifications while routing human-readable output to `tracing`.
//! - Per-tool call instrumentation via [`ToolCallSpan`].
//!
//! # Wire format
//!
//! ```json
//! {"jsonrpc":"2.0","method":"notifications/message","params":{"level":"info","data":"ax_click completed in 2ms"}}
//! ```
//!
//! Logging always goes to the MCP notification channel (stdout). Tracing
//! mirrors the same messages to stderr for developer observation.

use std::fmt;
use std::io::{self, Write};
use std::time::Instant;

use serde_json::json;

// ---------------------------------------------------------------------------
// Log level
// ---------------------------------------------------------------------------

/// MCP log levels, ordered from least to most severe.
///
/// Matches the `level` field in `notifications/message` params.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    /// Verbose diagnostic information.
    Debug,
    /// Normal operational messages.
    Info,
    /// Recoverable anomalies.
    Warning,
    /// Unrecoverable errors.
    Error,
}

impl LogLevel {
    /// Wire representation used in JSON notifications.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// McpLogger
// ---------------------------------------------------------------------------

/// Emits MCP `notifications/message` to an underlying `Write` handle.
///
/// The logger is intentionally cheap to clone: it holds only a reference
/// to the shared writer guarded by the caller.
///
/// # Example
///
/// ```rust,ignore
/// let logger = McpLogger::new(&mut stdout_lock);
/// logger.info("ax_click: element found");
/// ```
pub struct McpLogger<'w, W: Write> {
    out: &'w mut W,
}

impl<'w, W: Write> McpLogger<'w, W> {
    /// Create a logger wrapping `out`.
    pub fn new(out: &'w mut W) -> Self {
        Self { out }
    }

    /// Emit a notification at the given level.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if writing to the underlying writer fails.
    pub fn log(&mut self, level: LogLevel, message: &str) -> io::Result<()> {
        emit_mcp_log(self.out, level, message)
    }

    /// Emit a debug-level log notification.
    ///
    /// # Errors
    ///
    /// Propagates I/O errors from the underlying writer.
    pub fn debug(&mut self, message: &str) -> io::Result<()> {
        self.log(LogLevel::Debug, message)
    }

    /// Emit an info-level log notification.
    ///
    /// # Errors
    ///
    /// Propagates I/O errors from the underlying writer.
    pub fn info(&mut self, message: &str) -> io::Result<()> {
        self.log(LogLevel::Info, message)
    }

    /// Emit a warning-level log notification.
    ///
    /// # Errors
    ///
    /// Propagates I/O errors from the underlying writer.
    pub fn warning(&mut self, message: &str) -> io::Result<()> {
        self.log(LogLevel::Warning, message)
    }

    /// Emit an error-level log notification.
    ///
    /// # Errors
    ///
    /// Propagates I/O errors from the underlying writer.
    pub fn error(&mut self, message: &str) -> io::Result<()> {
        self.log(LogLevel::Error, message)
    }
}

// ---------------------------------------------------------------------------
// ToolCallSpan
// ---------------------------------------------------------------------------

/// RAII span that measures tool call duration and emits a completion log.
///
/// Drop the span (or call [`ToolCallSpan::finish`]) to emit the result.
///
/// # Example
///
/// ```rust,ignore
/// let span = ToolCallSpan::start("ax_click");
/// // … perform work …
/// span.finish_ok(&mut stdout); // emits: "ax_click completed in 3ms"
/// ```
pub struct ToolCallSpan {
    tool_name: &'static str,
    started: Instant,
}

impl ToolCallSpan {
    /// Begin timing a tool call.
    #[must_use]
    pub fn start(tool_name: &'static str) -> Self {
        tracing::debug!(tool = tool_name, "tool call started");
        Self {
            tool_name,
            started: Instant::now(),
        }
    }

    /// Emit a success notification and return elapsed milliseconds.
    ///
    /// # Errors
    ///
    /// Propagates any I/O error from the underlying writer.
    pub fn finish_ok<W: Write>(self, out: &mut W) -> io::Result<u64> {
        let elapsed_ms = self.elapsed_ms();
        let msg = format!("{} completed in {}ms", self.tool_name, elapsed_ms);
        tracing::debug!(tool = self.tool_name, elapsed_ms, "tool call succeeded");
        emit_mcp_log(out, LogLevel::Info, &msg)?;
        Ok(elapsed_ms)
    }

    /// Emit a failure notification and return elapsed milliseconds.
    ///
    /// # Errors
    ///
    /// Propagates any I/O error from the underlying writer.
    pub fn finish_err<W: Write>(self, out: &mut W, reason: &str) -> io::Result<u64> {
        let elapsed_ms = self.elapsed_ms();
        let msg = format!("{} failed in {}ms: {}", self.tool_name, elapsed_ms, reason);
        tracing::warn!(tool = self.tool_name, elapsed_ms, %reason, "tool call failed");
        emit_mcp_log(out, LogLevel::Warning, &msg)?;
        Ok(elapsed_ms)
    }

    fn elapsed_ms(&self) -> u64 {
        #[allow(clippy::cast_possible_truncation)]
        let ms = self.started.elapsed().as_millis() as u64;
        ms
    }
}

// ---------------------------------------------------------------------------
// Core emit function (free function — no borrow issues in the server loop)
// ---------------------------------------------------------------------------

/// Write a single `notifications/message` line to `out`.
///
/// This is a free function so server.rs can call it without holding a
/// `McpLogger` reference through the borrow-checker's lifetime rules.
///
/// # Errors
///
/// Returns an I/O error if `writeln!` or `flush` fails.
///
/// # Panics
///
/// Cannot panic: the JSON structure is statically defined.
pub fn emit_mcp_log<W: Write>(out: &mut W, level: LogLevel, message: &str) -> io::Result<()> {
    let notif = json!({
        "jsonrpc": "2.0",
        "method": "notifications/message",
        "params": {
            "level": level.as_str(),
            "data": message
        }
    });
    let line = serde_json::to_string(&notif).expect("notification serialization cannot fail");
    writeln!(out, "{line}")?;
    out.flush()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn parse_notification(line: &str) -> Value {
        serde_json::from_str(line).expect("valid JSON")
    }

    #[test]
    fn emit_mcp_log_writes_valid_jsonrpc_notification() {
        // GIVEN: an in-memory writer
        let mut buf = Vec::<u8>::new();
        // WHEN: emitting an info log
        emit_mcp_log(&mut buf, LogLevel::Info, "hello world").unwrap();
        // THEN: the output is a valid notifications/message JSON line
        let line = String::from_utf8(buf).unwrap();
        let v = parse_notification(line.trim());
        assert_eq!(v["jsonrpc"], "2.0");
        assert_eq!(v["method"], "notifications/message");
        assert_eq!(v["params"]["level"], "info");
        assert_eq!(v["params"]["data"], "hello world");
    }

    #[test]
    fn log_level_wire_names_are_lowercase() {
        // GIVEN: all log levels
        // THEN: wire names match MCP spec casing
        assert_eq!(LogLevel::Debug.as_str(), "debug");
        assert_eq!(LogLevel::Info.as_str(), "info");
        assert_eq!(LogLevel::Warning.as_str(), "warning");
        assert_eq!(LogLevel::Error.as_str(), "error");
    }

    #[test]
    fn log_level_ordering_debug_lt_error() {
        // GIVEN: level ordering contract
        assert!(LogLevel::Debug < LogLevel::Info);
        assert!(LogLevel::Info < LogLevel::Warning);
        assert!(LogLevel::Warning < LogLevel::Error);
    }

    #[test]
    fn mcp_logger_info_emits_correct_level() {
        // GIVEN: a McpLogger wrapping a buffer
        let mut buf = Vec::<u8>::new();
        {
            let mut logger = McpLogger::new(&mut buf);
            // WHEN: info is called
            logger.info("test message").unwrap();
        }
        // THEN: level field is "info"
        let v = parse_notification(String::from_utf8(buf).unwrap().trim());
        assert_eq!(v["params"]["level"], "info");
    }

    #[test]
    fn mcp_logger_error_emits_correct_level() {
        let mut buf = Vec::<u8>::new();
        {
            let mut logger = McpLogger::new(&mut buf);
            logger.error("boom").unwrap();
        }
        let v = parse_notification(String::from_utf8(buf).unwrap().trim());
        assert_eq!(v["params"]["level"], "error");
    }

    #[test]
    fn mcp_logger_warning_emits_correct_level() {
        let mut buf = Vec::<u8>::new();
        {
            let mut logger = McpLogger::new(&mut buf);
            logger.warning("watch out").unwrap();
        }
        let v = parse_notification(String::from_utf8(buf).unwrap().trim());
        assert_eq!(v["params"]["level"], "warning");
    }

    #[test]
    fn mcp_logger_debug_emits_correct_level() {
        let mut buf = Vec::<u8>::new();
        {
            let mut logger = McpLogger::new(&mut buf);
            logger.debug("verbose").unwrap();
        }
        let v = parse_notification(String::from_utf8(buf).unwrap().trim());
        assert_eq!(v["params"]["level"], "debug");
    }

    #[test]
    fn tool_call_span_finish_ok_includes_tool_name_and_duration() {
        // GIVEN: a span for a named tool
        let span = ToolCallSpan::start("ax_click");
        let mut buf = Vec::<u8>::new();
        // WHEN: finishing successfully
        let elapsed = span.finish_ok(&mut buf).unwrap();
        // THEN: notification data mentions the tool and a millisecond duration
        let v = parse_notification(String::from_utf8(buf).unwrap().trim());
        let data = v["params"]["data"].as_str().unwrap();
        assert!(data.contains("ax_click"), "data: {data}");
        assert!(data.contains("completed"), "data: {data}");
        assert!(data.contains("ms"), "data: {data}");
        assert!(
            elapsed < 10_000,
            "sanity: elapsed {elapsed}ms should be < 10s"
        );
    }

    #[test]
    fn tool_call_span_finish_err_includes_reason() {
        // GIVEN: a span and an error reason
        let span = ToolCallSpan::start("ax_find");
        let mut buf = Vec::<u8>::new();
        // WHEN: finishing with an error
        span.finish_err(&mut buf, "element not found").unwrap();
        // THEN: data contains tool name, "failed", and the reason
        let v = parse_notification(String::from_utf8(buf).unwrap().trim());
        let data = v["params"]["data"].as_str().unwrap();
        assert!(data.contains("ax_find"), "data: {data}");
        assert!(data.contains("failed"), "data: {data}");
        assert!(data.contains("element not found"), "data: {data}");
    }

    #[test]
    fn emit_mcp_log_terminates_with_newline() {
        let mut buf = Vec::<u8>::new();
        emit_mcp_log(&mut buf, LogLevel::Info, "check newline").unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.ends_with('\n'), "output must be newline-terminated");
    }
}
