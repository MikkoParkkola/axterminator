//! Continuous background monitoring — audio VAD + camera gesture detection.
//!
//! ## Overview
//!
//! The `watch` module runs two independent background tasks that push events
//! to MCP clients via `notifications/claude/channel`.  Neither task accumulates
//! binary sensor data: each window is processed in-place and immediately dropped.
//!
//! | Component | Max RAM | Lifecycle |
//! |-----------|---------|-----------|
//! | Audio window | ~320 KB | Captured → VAD → transcribe → **dropped** |
//! | Camera frames | ~1.6 MB | prev + curr JPEG → motion → gesture → curr becomes prev → old **dropped** |
//! | Event channel | ~50 KB | `bounded(100)` of short text strings |
//!
//! ## Usage
//!
//! ```rust,no_run
//! use axterminator::watch::{WatchCoordinator, WatchConfig};
//!
//! let config = WatchConfig {
//!     audio_enabled: true,
//!     camera_enabled: false,
//!     ..WatchConfig::default()
//! };
//! let (coordinator, event_rx) = WatchCoordinator::start(config);
//! // Pass event_rx to the MCP server loop that emits channel notifications.
//! coordinator.stop();
//! ```
//!
//! ## Feature gate
//!
//! This module requires `--features watch`.  Absent the flag, neither audio
//! nor camera hardware is touched and no TCC dialogs are triggered.

pub mod audio_watcher;
pub mod camera_watcher;
pub mod ring_buffer;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the watch coordinator.
///
/// All fields have sensible defaults via [`WatchConfig::default`].
#[derive(Debug, Clone)]
pub struct WatchConfig {
    /// Enable continuous audio capture and speech detection.
    pub audio_enabled: bool,
    /// Enable continuous camera capture and gesture detection.
    pub camera_enabled: bool,
    /// Voice activity threshold in dBFS.  Audio windows below this level
    /// are discarded without transcription.  Default: `−40.0 dB`.
    pub audio_vad_threshold_db: f32,
    /// Duration of each audio capture window in seconds.  Default: `5.0`.
    pub audio_window_secs: f32,
    /// Milliseconds between camera frame captures.  Default: `2000` (2 s).
    pub camera_poll_interval_ms: u64,
    /// Minimum relative JPEG size difference to trigger gesture detection.
    /// Range `[0.0, 1.0]`.  Default: `0.05` (5 %).
    pub camera_motion_threshold: f32,
}

impl Default for WatchConfig {
    /// Construct a `WatchConfig` with conservative defaults.
    ///
    /// - Both sensors off — must be explicitly enabled.
    /// - Audio: 5-second windows, −40 dB VAD gate.
    /// - Camera: 2-second poll, 5% motion threshold.
    fn default() -> Self {
        Self {
            audio_enabled: false,
            camera_enabled: false,
            audio_vad_threshold_db: -40.0,
            audio_window_secs: 5.0,
            camera_poll_interval_ms: 2000,
            camera_motion_threshold: 0.05,
        }
    }
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// An event produced by one of the background watchers.
///
/// Events are small text values — never binary sensor data.
#[derive(Debug, Clone)]
pub enum WatchEvent {
    /// A speech segment was recognised above the VAD threshold.
    Speech {
        /// Transcribed text from SFSpeechRecognizer.
        text: String,
        /// Recognition confidence in `[0.0, 1.0]`.
        confidence: f32,
        /// ISO 8601 UTC timestamp.
        timestamp: String,
    },
    /// A hand or face gesture was detected in the camera frame.
    Gesture {
        /// Canonical snake_case gesture name (e.g. `"thumbs_up"`).
        gesture: String,
        /// Vision framework confidence in `[0.0, 1.0]`.
        confidence: f32,
        /// Hand descriptor (e.g. `"left"`, `"right"`, `"face"`).
        hand: String,
        /// ISO 8601 UTC timestamp.
        timestamp: String,
    },
    /// A non-fatal error occurred inside a watcher.
    Error {
        /// Which watcher produced this error (`"audio_watcher"` / `"camera_watcher"`).
        source: String,
        /// Human-readable error description.
        message: String,
    },
}

// ---------------------------------------------------------------------------
// Coordinator
// ---------------------------------------------------------------------------

/// Coordinator that owns the watcher task handles and the cancellation token.
///
/// Create via [`WatchCoordinator::start`].  Drop or call [`WatchCoordinator::stop`]
/// to shut down all watchers cleanly.
pub struct WatchCoordinator {
    pub(crate) cancel: CancellationToken,
    pub(crate) audio_handle: Option<JoinHandle<()>>,
    pub(crate) camera_handle: Option<JoinHandle<()>>,
}

/// Event channel capacity.  If MCP delivery lags, the oldest event is dropped.
const EVENT_CHANNEL_CAPACITY: usize = 100;

impl WatchCoordinator {
    /// Start the requested watchers and return the coordinator plus an event receiver.
    ///
    /// The caller must poll `event_rx` (or forward it to the MCP emit loop) — if the
    /// channel fills up, the watcher silently drops the oldest event and continues.
    ///
    /// # Panics
    ///
    /// Panics if called outside a Tokio runtime context.
    #[must_use]
    pub fn start(config: WatchConfig) -> (Self, mpsc::Receiver<WatchEvent>) {
        let cancel = CancellationToken::new();
        let (event_tx, event_rx) = mpsc::channel(EVENT_CHANNEL_CAPACITY);

        let audio_handle = spawn_audio_watcher(&config, &cancel, event_tx.clone());
        let camera_handle = spawn_camera_watcher(&config, &cancel, event_tx);

        info!(
            audio = config.audio_enabled,
            camera = config.camera_enabled,
            "watch coordinator started"
        );

        (
            Self {
                cancel,
                audio_handle,
                camera_handle,
            },
            event_rx,
        )
    }

    /// Signal all watchers to stop and await their clean termination.
    ///
    /// Returns immediately if the watchers have already exited.
    pub async fn stop(self) {
        self.cancel.cancel();
        debug!("watch coordinator: cancellation signalled");

        if let Some(h) = self.audio_handle {
            let _ = h.await;
        }
        if let Some(h) = self.camera_handle {
            let _ = h.await;
        }

        info!("watch coordinator stopped");
    }

    /// Return `true` when the cancellation token has been fired.
    #[must_use]
    pub fn is_stopped(&self) -> bool {
        self.cancel.is_cancelled()
    }

    /// Return the current status for the `ax_watch_status` tool.
    #[must_use]
    pub fn status(&self) -> WatchStatus {
        WatchStatus {
            audio_running: self.audio_handle.is_some() && !self.is_stopped(),
            camera_running: self.camera_handle.is_some() && !self.is_stopped(),
        }
    }
}

/// Status snapshot for the `ax_watch_status` MCP tool.
#[derive(Debug, Clone)]
pub struct WatchStatus {
    /// Whether the audio watcher task was started and is not yet cancelled.
    pub audio_running: bool,
    /// Whether the camera watcher task was started and is not yet cancelled.
    pub camera_running: bool,
}

// ---------------------------------------------------------------------------
// Spawn helpers
// ---------------------------------------------------------------------------

fn spawn_audio_watcher(
    config: &WatchConfig,
    cancel: &CancellationToken,
    event_tx: mpsc::Sender<WatchEvent>,
) -> Option<JoinHandle<()>> {
    if !config.audio_enabled {
        return None;
    }
    let cfg = config.clone();
    let tok = cancel.clone();
    Some(tokio::spawn(async move {
        audio_watcher::run_audio_watcher(cfg, event_tx, tok).await;
    }))
}

fn spawn_camera_watcher(
    config: &WatchConfig,
    cancel: &CancellationToken,
    event_tx: mpsc::Sender<WatchEvent>,
) -> Option<JoinHandle<()>> {
    if !config.camera_enabled {
        return None;
    }
    let cfg = config.clone();
    let tok = cancel.clone();
    Some(tokio::spawn(async move {
        camera_watcher::run_camera_watcher(cfg, event_tx, tok).await;
    }))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // WatchConfig
    // -----------------------------------------------------------------------

    #[test]
    fn watch_config_defaults_are_conservative() {
        // GIVEN: default config
        let cfg = WatchConfig::default();
        // THEN: both sensors are off by default
        assert!(!cfg.audio_enabled, "audio must be opt-in");
        assert!(!cfg.camera_enabled, "camera must be opt-in");
    }

    #[test]
    fn watch_config_defaults_have_sane_thresholds() {
        let cfg = WatchConfig::default();
        assert_eq!(cfg.audio_vad_threshold_db, -40.0);
        assert_eq!(cfg.audio_window_secs, 5.0);
        assert_eq!(cfg.camera_poll_interval_ms, 2000);
        assert!((cfg.camera_motion_threshold - 0.05).abs() < 1e-6);
    }

    #[test]
    fn watch_config_clone_is_independent() {
        let mut cfg = WatchConfig::default();
        let cfg2 = cfg.clone();
        cfg.audio_enabled = true;
        assert!(
            cfg.audio_enabled,
            "original should reflect its own mutation"
        );
        assert!(!cfg2.audio_enabled, "clone must be independent");
    }

    // -----------------------------------------------------------------------
    // WatchCoordinator (no hardware — only structural behaviour)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn coordinator_starts_with_both_disabled_no_handles() {
        // GIVEN: both sensors disabled
        let cfg = WatchConfig::default(); // audio=false, camera=false
                                          // WHEN: coordinator started
        let (coord, _rx) = WatchCoordinator::start(cfg);
        // THEN: no tasks spawned → handles are None
        assert!(coord.audio_handle.is_none());
        assert!(coord.camera_handle.is_none());
        coord.stop().await;
    }

    #[tokio::test]
    async fn coordinator_stop_marks_cancelled() {
        // GIVEN: coordinator with no tasks
        let cfg = WatchConfig::default();
        let (coord, _rx) = WatchCoordinator::start(cfg);
        let cancel_clone = coord.cancel.clone();
        assert!(!cancel_clone.is_cancelled());
        // WHEN: stopped
        coord.stop().await;
        // THEN: token is cancelled
        assert!(cancel_clone.is_cancelled());
    }

    #[tokio::test]
    async fn coordinator_is_stopped_reflects_cancellation() {
        let cfg = WatchConfig::default();
        let (coord, _rx) = WatchCoordinator::start(cfg);
        assert!(!coord.is_stopped());
        let cancel = coord.cancel.clone();
        cancel.cancel();
        assert!(coord.is_stopped());
        coord.stop().await;
    }

    #[tokio::test]
    async fn coordinator_status_both_disabled() {
        let cfg = WatchConfig::default();
        let (coord, _rx) = WatchCoordinator::start(cfg);
        let status = coord.status();
        assert!(!status.audio_running);
        assert!(!status.camera_running);
        coord.stop().await;
    }

    // -----------------------------------------------------------------------
    // Event channel capacity
    // -----------------------------------------------------------------------

    #[test]
    fn event_channel_capacity_is_100() {
        assert_eq!(EVENT_CHANNEL_CAPACITY, 100);
    }

    #[tokio::test]
    async fn event_channel_drops_on_full_without_blocking() {
        // GIVEN: channel of capacity 100 with no receiver consuming
        let (tx, mut rx) = mpsc::channel::<WatchEvent>(100);
        // WHEN: 105 events are sent with try_send
        let mut sent = 0usize;
        let mut dropped = 0usize;
        for i in 0..105 {
            let event = WatchEvent::Error {
                source: "test".into(),
                message: format!("event {i}"),
            };
            if tx.try_send(event).is_ok() {
                sent += 1;
            } else {
                dropped += 1;
            }
        }
        // THEN: exactly 100 sent, 5 dropped
        assert_eq!(sent, 100);
        assert_eq!(dropped, 5);
        // AND: channel is drainable
        let mut count = 0;
        while rx.try_recv().is_ok() {
            count += 1;
        }
        assert_eq!(count, 100);
    }

    // -----------------------------------------------------------------------
    // WatchEvent structure
    // -----------------------------------------------------------------------

    #[test]
    fn watch_event_speech_fields_are_accessible() {
        let e = WatchEvent::Speech {
            text: "hello".into(),
            confidence: 0.9,
            timestamp: "2026-03-20T00:00:00Z".into(),
        };
        if let WatchEvent::Speech {
            text, confidence, ..
        } = e
        {
            assert_eq!(text, "hello");
            assert!((confidence - 0.9).abs() < 1e-6);
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn watch_event_gesture_fields_are_accessible() {
        let e = WatchEvent::Gesture {
            gesture: "thumbs_up".into(),
            confidence: 0.95,
            hand: "right".into(),
            timestamp: "2026-03-20T00:00:00Z".into(),
        };
        if let WatchEvent::Gesture {
            gesture,
            confidence,
            hand,
            ..
        } = e
        {
            assert_eq!(gesture, "thumbs_up");
            assert!(confidence > 0.9);
            assert_eq!(hand, "right");
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn watch_event_error_fields_are_accessible() {
        let e = WatchEvent::Error {
            source: "audio_watcher".into(),
            message: "mic unavailable".into(),
        };
        if let WatchEvent::Error { source, message } = e {
            assert_eq!(source, "audio_watcher");
            assert!(message.contains("mic"));
        } else {
            panic!("wrong variant");
        }
    }
}
