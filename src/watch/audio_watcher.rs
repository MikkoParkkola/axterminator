//! Continuous audio monitoring with voice activity detection.
//!
//! ## Architecture
//!
//! Runs as a `tokio::spawn` background task.  Each iteration:
//!
//! 1. Sleep for `audio_window_secs`.
//! 2. Capture a single audio window via the existing `capture_microphone()`.
//! 3. Compute RMS energy — ~0.01 ms, no ML, no disk.
//! 4. If RMS > `vad_threshold_db`, transcribe via `transcribe()`.
//! 5. If transcript is non-empty, send `WatchEvent::Speech` through the channel.
//! 6. Audio samples are **dropped immediately** after this sequence.
//!
//! ## Memory
//!
//! At most one 5-second audio window lives in RAM at any time:
//! 5 s × 16 000 Hz × 4 bytes = 320 KB.  No accumulation occurs.
//!
//! ## Disk
//!
//! Zero disk writes.  The existing `transcribe()` function writes a temporary
//! WAV to `/tmp` and unlinks it immediately — that contract is unchanged.

use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use super::{WatchConfig, WatchEvent};

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Run the audio watcher until `cancel` is signalled.
///
/// This function **blocks** the current tokio task — it should always be
/// wrapped in `tokio::spawn`.
///
/// # Panics
///
/// Never panics; all errors are logged and sent as `WatchEvent::Error`.
pub async fn run_audio_watcher(
    config: WatchConfig,
    event_tx: mpsc::Sender<WatchEvent>,
    cancel: CancellationToken,
) {
    debug!("audio watcher starting");
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                debug!("audio watcher cancelled");
                break;
            }
            _ = tokio::time::sleep(Duration::from_secs_f32(config.audio_window_secs)) => {
                process_audio_window(&config, &event_tx).await;
                // Channel full → oldest event was already dropped by the bounded sender.
                // We continue regardless; the watcher never blocks on delivery.
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Per-window logic
// ---------------------------------------------------------------------------

/// Capture one audio window, run VAD, transcribe if speech, emit event.
///
/// Audio samples are dropped at the end of this function — zero accumulation.
async fn process_audio_window(config: &WatchConfig, event_tx: &mpsc::Sender<WatchEvent>) {
    // Capture is blocking (AVAudioEngine); offload to avoid starving the runtime.
    let window_secs = config.audio_window_secs;
    let vad_threshold = config.audio_vad_threshold_db;

    let result =
        tokio::task::spawn_blocking(move || capture_and_analyse(window_secs, vad_threshold)).await;

    match result {
        Ok(Some(event)) => {
            // try_send: channel bounded(100); drop on full rather than block.
            if event_tx.try_send(event).is_err() {
                debug!("audio event channel full — event dropped");
            }
        }
        Ok(None) => {
            debug!("audio window below VAD threshold or empty transcript");
        }
        Err(join_err) => {
            warn!(error = %join_err, "audio capture task panicked");
            let _ = event_tx.try_send(WatchEvent::Error {
                source: "audio_watcher".into(),
                message: join_err.to_string(),
            });
        }
    }
}

/// Capture one window, measure RMS, transcribe if speech.
///
/// Returns `Some(WatchEvent::Speech)` when speech was detected and transcribed,
/// `None` when the window was silent or the transcript was empty.
///
/// # Safety concerns
///
/// This is called inside `spawn_blocking` — all I/O is synchronous here.
fn capture_and_analyse(window_secs: f32, vad_threshold_db: f32) -> Option<WatchEvent> {
    #[cfg(feature = "audio")]
    {
        use crate::audio::{capture_microphone, transcribe};

        let audio = match capture_microphone(window_secs) {
            Ok(a) => a,
            Err(e) => {
                warn!(error = %e, "microphone capture failed");
                return Some(WatchEvent::Error {
                    source: "audio_watcher".into(),
                    message: e.to_string(),
                });
            }
        };

        let rms_db = compute_rms_db(&audio.samples);
        debug!(rms_db, threshold = vad_threshold_db, "audio VAD check");

        // Emit periodic level event at debug level; callers can filter.
        // We skip it here to keep the channel lean — audio level is noise.

        if rms_db < vad_threshold_db {
            // Silent window: samples drop here.
            return None;
        }

        let timestamp = current_timestamp();

        let text = match transcribe(&audio) {
            Ok(t) => t,
            Err(e) => {
                warn!(error = %e, "transcription failed");
                return Some(WatchEvent::Error {
                    source: "audio_watcher".into(),
                    message: e.to_string(),
                });
            }
        };
        // Audio samples drop here unconditionally.

        if text.trim().is_empty() {
            return None;
        }

        Some(WatchEvent::Speech {
            text,
            confidence: 1.0, // SFSpeechRecognizer doesn't expose per-word confidence
            timestamp,
        })
    }

    #[cfg(not(feature = "audio"))]
    {
        let _ = (window_secs, vad_threshold_db);
        None
    }
}

// ---------------------------------------------------------------------------
// VAD helper
// ---------------------------------------------------------------------------

/// Compute the signal energy in decibels (dBFS) for a slice of float PCM samples.
///
/// - Silence (all zeros): returns `−∞` (clamped to `−96.0 dB`).
/// - Normal speech:  approximately `−20 dB`.
/// - Clipping / maximum: approximately `0 dB`.
///
/// # Examples
///
/// ```rust
/// use axterminator::watch::audio_watcher::compute_rms_db;
///
/// // Silence → close to the floor
/// assert!(compute_rms_db(&[0.0f32; 1024]) < -90.0);
///
/// // Full-scale sine-like signal → near 0 dB
/// assert!(compute_rms_db(&[1.0f32; 1024]) > -1.0);
/// ```
#[must_use]
pub fn compute_rms_db(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return -96.0;
    }
    let mean_sq = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
    let rms = mean_sq.sqrt();
    // Clamp RMS to avoid log10(0); floor at ~-96 dB (16-bit noise floor)
    20.0 * rms.max(1e-5_f32).log10()
}

// ---------------------------------------------------------------------------
// Timestamp helper
// ---------------------------------------------------------------------------

/// Return an ISO 8601 UTC timestamp string for the current moment.
///
/// Format: `2026-03-20T14:22:01Z` (second precision — sufficient for events).
///
/// Public re-export for use by sibling modules (e.g. `camera_watcher`).
pub fn current_timestamp_pub() -> String {
    current_timestamp()
}

fn current_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Manual ISO 8601 formatting — avoids pulling in `chrono` or `time`.
    let (y, mo, d, h, mi, s) = epoch_secs_to_parts(secs);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

/// Decompose Unix epoch seconds into (year, month, day, hour, min, sec).
///
/// Gregorian calendar, valid for years 1970–2099.  Uses the civil-calendar
/// algorithm by Howard Hinnant (public domain).
pub(crate) fn epoch_secs_to_parts(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    let s = (secs % 60) as u32;
    let m = ((secs / 60) % 60) as u32;
    let h = ((secs / 3600) % 24) as u32;
    let z = (secs / 86400) as i64 + 719_468; // days since 0000-03-01
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // day-of-era [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // year-of-era
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day-of-year from Mar 1
    let mp = (5 * doy + 2) / 153; // month from Mar (0=Mar)
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y_adj = if mo <= 2 { y + 1 } else { y };
    (y_adj as u32, mo as u32, d as u32, h, m, s)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // compute_rms_db
    // -----------------------------------------------------------------------

    #[test]
    fn rms_db_all_zeros_returns_near_floor() {
        // GIVEN: silence (all zeros)
        let samples = vec![0.0f32; 16_000];
        // WHEN: RMS computed
        let db = compute_rms_db(&samples);
        // THEN: near the -96 dB noise floor
        assert!(db < -90.0, "expected < -90 dB for silence, got {db}");
    }

    #[test]
    fn rms_db_full_scale_sine_near_zero_db() {
        // GIVEN: full-scale constant +1.0 signal
        let samples = vec![1.0f32; 16_000];
        // WHEN: RMS computed (RMS of [1.0; N] = 1.0, 20*log10(1)=0)
        let db = compute_rms_db(&samples);
        // THEN: approximately 0 dBFS
        assert!(
            db > -1.0 && db <= 1.0,
            "expected ~0 dB for full scale, got {db}"
        );
    }

    #[test]
    fn rms_db_half_amplitude_is_about_minus_six_db() {
        // GIVEN: constant 0.5 amplitude (−6.02 dBFS)
        let samples = vec![0.5f32; 16_000];
        let db = compute_rms_db(&samples);
        assert!(
            (-7.0..=-5.0).contains(&db),
            "expected ~-6 dB for 0.5 amplitude, got {db}"
        );
    }

    #[test]
    fn rms_db_empty_slice_returns_floor() {
        // GIVEN: empty slice
        let db = compute_rms_db(&[]);
        // THEN: returns -96 dB floor
        assert_eq!(db, -96.0);
    }

    #[test]
    fn rms_db_speech_level_exceeds_minus_forty_db() {
        // GIVEN: modest speech-level signal (~0.01 amplitude = -40 dBFS)
        let samples = vec![0.012f32; 16_000];
        let db = compute_rms_db(&samples);
        // THEN: above the -40 dB default VAD threshold
        assert!(db > -40.0, "expected above VAD threshold, got {db}");
    }

    // -----------------------------------------------------------------------
    // Timestamp formatting
    // -----------------------------------------------------------------------

    #[test]
    fn current_timestamp_has_iso8601_format() {
        // GIVEN: call at any time
        let ts = current_timestamp();
        // THEN: matches YYYY-MM-DDTHH:MM:SSZ pattern
        assert_eq!(ts.len(), 20, "timestamp wrong length: {ts}");
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[7..8], "-");
        assert_eq!(&ts[10..11], "T");
        assert_eq!(&ts[13..14], ":");
        assert_eq!(&ts[16..17], ":");
        assert_eq!(&ts[19..20], "Z");
    }

    #[test]
    fn epoch_secs_to_parts_known_date() {
        // GIVEN: Unix timestamp for 2026-03-20T00:00:00Z
        // Verified: python3 -c "import datetime; print(int(datetime.datetime(2026,3,20).timestamp()))"
        // = 1773961200 (UTC-aware: use datetime.utctimetuple approach)
        // UTC: python3 -c "import calendar,datetime; print(calendar.timegm(datetime.date(2026,3,20).timetuple()))"
        let (y, mo, d, h, mi, s) = epoch_secs_to_parts(1_773_964_800); // 2026-03-20 00:00:00 UTC
        assert_eq!(y, 2026);
        assert_eq!(mo, 3);
        assert_eq!(d, 20);
        assert_eq!(h, 0);
        assert_eq!(mi, 0);
        assert_eq!(s, 0);
    }

    #[test]
    fn epoch_secs_to_parts_unix_epoch() {
        // GIVEN: epoch = 1970-01-01T00:00:00Z
        let (y, mo, d, h, mi, s) = epoch_secs_to_parts(0);
        assert_eq!(y, 1970);
        assert_eq!(mo, 1);
        assert_eq!(d, 1);
        assert_eq!(h, 0);
        assert_eq!(mi, 0);
        assert_eq!(s, 0);
    }

    // -----------------------------------------------------------------------
    // WatchConfig defaults (accessible through this module's tests)
    // -----------------------------------------------------------------------

    #[test]
    fn default_vad_threshold_is_negative_forty_db() {
        let cfg = WatchConfig::default();
        assert_eq!(cfg.audio_vad_threshold_db, -40.0);
    }

    #[test]
    fn default_audio_window_is_five_seconds() {
        let cfg = WatchConfig::default();
        assert_eq!(cfg.audio_window_secs, 5.0);
    }
}
