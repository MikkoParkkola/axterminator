//! MCP channel event emitter for the watch system.
//!
//! When a `WatchEvent` arrives from the watcher background tasks, this
//! module formats it as a `notifications/claude/channel` JSON-RPC
//! notification and writes it to the MCP stdout transport.
//!
//! ## Protocol
//!
//! The `claude/channel` capability is an experimental MCP extension used by
//! Claude Code to receive push notifications from MCP servers.  The server
//! declares the capability in its `initialize` response:
//!
//! ```json
//! { "experimental": { "claude/channel": {} } }
//! ```
//!
//! Notifications have the form:
//!
//! ```json
//! {
//!   "jsonrpc": "2.0",
//!   "method": "notifications/claude/channel",
//!   "params": {
//!     "content": "[speech detected] \"hello\" (confidence: 95%)",
//!     "meta": {
//!       "source": "axterminator",
//!       "event": "speech",
//!       "severity": "info",
//!       "timestamp": "2026-03-20T14:22:01Z"
//!     }
//!   }
//! }
//! ```

use std::io::Write;

use serde_json::{Value, json};
use tracing::debug;

use crate::mcp::protocol::JsonRpcNotification;

#[cfg(feature = "watch")]
use crate::watch::WatchEvent;

// ---------------------------------------------------------------------------
// Notification formatting
// ---------------------------------------------------------------------------

/// Convert a [`WatchEvent`] into a channel notification JSON value.
///
/// Returns `None` for `WatchEvent::Error` — errors are logged but not pushed
/// to the client channel to avoid noise.
#[cfg(feature = "watch")]
#[must_use]
pub fn event_to_channel_notification(event: &WatchEvent) -> Option<Value> {
    match event {
        WatchEvent::Speech {
            text,
            confidence,
            timestamp,
        } => Some(build_notification(
            format!(
                "[speech detected] \"{}\" (confidence: {:.0}%)",
                text,
                confidence * 100.0
            ),
            "speech",
            timestamp,
        )),

        WatchEvent::Gesture {
            gesture,
            confidence,
            hand,
            timestamp,
        } => Some(build_notification(
            format!(
                "[gesture detected] {} by {} hand (confidence: {:.0}%)",
                gesture,
                hand,
                confidence * 100.0
            ),
            "gesture",
            timestamp,
        )),

        WatchEvent::Error { source, message } => {
            debug!(source = %source, error = %message, "watcher error — not forwarded to channel");
            None
        }
    }
}

/// Build the notification `params` value.
fn build_notification(content: String, event_type: &str, timestamp: &str) -> Value {
    json!({
        "method": "notifications/claude/channel",
        "params": {
            "content": content,
            "meta": {
                "source": "axterminator",
                "event": event_type,
                "severity": "info",
                "timestamp": timestamp
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Emit helper
// ---------------------------------------------------------------------------

/// Write a channel notification to the MCP transport sink.
///
/// # Errors
///
/// Returns an I/O error if writing to `out` fails.
pub fn emit_channel_notification(out: &mut impl Write, params: Value) -> std::io::Result<()> {
    let notif = JsonRpcNotification {
        jsonrpc: "2.0",
        method: "notifications/claude/channel",
        params,
    };
    let json = serde_json::to_string(&notif).expect("notification serialization cannot fail");
    debug!(bytes = json.len(), "sending channel notification");
    writeln!(out, "{json}")?;
    out.flush()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(all(test, feature = "watch"))]
mod tests {
    use super::*;
    use crate::watch::WatchEvent;

    #[test]
    fn speech_event_produces_notification() {
        // GIVEN: a Speech event
        let event = WatchEvent::Speech {
            text: "hello world".into(),
            confidence: 0.95,
            timestamp: "2026-03-20T14:22:01Z".into(),
        };
        // WHEN: converted
        let notif = event_to_channel_notification(&event);
        // THEN: Some is returned with correct content
        let notif = notif.expect("expected Some notification");
        let content = notif["params"]["content"].as_str().unwrap();
        assert!(content.contains("speech detected"), "got: {content}");
        assert!(content.contains("hello world"), "got: {content}");
        assert!(content.contains("95%"), "got: {content}");
    }

    #[test]
    fn gesture_event_produces_notification() {
        // GIVEN: a Gesture event
        let event = WatchEvent::Gesture {
            gesture: "thumbs_up".into(),
            confidence: 0.87,
            hand: "right".into(),
            timestamp: "2026-03-20T14:22:01Z".into(),
        };
        // WHEN: converted
        let notif = event_to_channel_notification(&event).unwrap();
        let content = notif["params"]["content"].as_str().unwrap();
        assert!(content.contains("gesture detected"), "got: {content}");
        assert!(content.contains("thumbs_up"), "got: {content}");
        assert!(content.contains("right"), "got: {content}");
        assert!(content.contains("87%"), "got: {content}");
    }

    #[test]
    fn error_event_returns_none() {
        // GIVEN: an Error event (not pushed to client)
        let event = WatchEvent::Error {
            source: "audio_watcher".into(),
            message: "mic unavailable".into(),
        };
        // WHEN: converted
        let notif = event_to_channel_notification(&event);
        // THEN: None — errors are not forwarded
        assert!(notif.is_none());
    }

    #[test]
    fn notification_meta_has_required_fields() {
        // GIVEN: any event that produces a notification
        let event = WatchEvent::Speech {
            text: "test".into(),
            confidence: 1.0,
            timestamp: "2026-03-20T00:00:00Z".into(),
        };
        let notif = event_to_channel_notification(&event).unwrap();
        let meta = &notif["params"]["meta"];
        assert_eq!(meta["source"], "axterminator");
        assert_eq!(meta["severity"], "info");
        assert_eq!(meta["timestamp"], "2026-03-20T00:00:00Z");
        assert_eq!(meta["event"], "speech");
    }

    #[test]
    fn emit_channel_notification_writes_valid_json_line() {
        // GIVEN: a notification value
        let params = json!({
            "content": "test notification",
            "meta": { "source": "test" }
        });
        let mut buf = Vec::<u8>::new();
        // WHEN: emitted
        emit_channel_notification(&mut buf, params).unwrap();
        // THEN: output is a valid JSON line ending with newline
        let output = String::from_utf8(buf).unwrap();
        assert!(output.ends_with('\n'));
        let v: serde_json::Value = serde_json::from_str(output.trim()).unwrap();
        assert_eq!(v["method"], "notifications/claude/channel");
    }

    #[test]
    fn confidence_zero_percent_formats_correctly() {
        let event = WatchEvent::Speech {
            text: "inaudible".into(),
            confidence: 0.0,
            timestamp: "2026-03-20T00:00:00Z".into(),
        };
        let notif = event_to_channel_notification(&event).unwrap();
        let content = notif["params"]["content"].as_str().unwrap();
        assert!(content.contains("0%"), "got: {content}");
    }

    #[test]
    fn confidence_hundred_percent_formats_correctly() {
        let event = WatchEvent::Gesture {
            gesture: "stop".into(),
            confidence: 1.0,
            hand: "left".into(),
            timestamp: "2026-03-20T00:00:00Z".into(),
        };
        let notif = event_to_channel_notification(&event).unwrap();
        let content = notif["params"]["content"].as_str().unwrap();
        assert!(content.contains("100%"), "got: {content}");
    }
}
