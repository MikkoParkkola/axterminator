//! Continuous camera monitoring with motion detection and gesture recognition.
//!
//! ## Architecture
//!
//! Runs as a `tokio::spawn` background task.  Each iteration:
//!
//! 1. Sleep for `camera_poll_interval_ms`.
//! 2. Capture one JPEG frame via the existing `capture_frame()`.
//! 3. Compare with the previous frame using [`estimate_motion`].
//! 4. If motion > `camera_motion_threshold`, run `detect_gestures()`.
//! 5. If a gesture is found, send `WatchEvent::Gesture` through the channel.
//! 6. Replace the previous frame with the current frame; drop the old one.
//!
//! ## Memory
//!
//! At most two JPEG frames live in RAM simultaneously:
//! - Previous frame: ~800 KB (1280×720 JPEG at 90% quality)
//! - Current frame:  ~800 KB
//! - Total: ~1.6 MB — zero accumulation.
//!
//! ## Camera indicator
//!
//! macOS activates the hardware camera indicator light during each capture.
//! This is OS-enforced and cannot be suppressed.  The 2-second poll interval
//! keeps capture brief and predictable.

use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use super::{WatchConfig, WatchEvent};

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Run the camera watcher until `cancel` is signalled.
///
/// This function **blocks** the current tokio task — it should always be
/// wrapped in `tokio::spawn`.
pub async fn run_camera_watcher(
    config: WatchConfig,
    event_tx: mpsc::Sender<WatchEvent>,
    cancel: CancellationToken,
) {
    debug!("camera watcher starting");
    // Previous frame: None until the first capture succeeds.
    let mut prev_frame: Option<Vec<u8>> = None;

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                debug!("camera watcher cancelled");
                break;
            }
            _ = tokio::time::sleep(Duration::from_millis(config.camera_poll_interval_ms)) => {
                prev_frame = process_camera_frame(&config, &event_tx, prev_frame).await;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Per-frame logic
// ---------------------------------------------------------------------------

/// Capture one frame, detect motion vs. `prev`, optionally detect gestures.
///
/// Returns the new "previous frame" for the next iteration.
/// The old `prev` frame is dropped at the end of this function.
async fn process_camera_frame(
    config: &WatchConfig,
    event_tx: &mpsc::Sender<WatchEvent>,
    prev_frame: Option<Vec<u8>>,
) -> Option<Vec<u8>> {
    let threshold = config.camera_motion_threshold;

    let result =
        tokio::task::spawn_blocking(move || capture_and_analyse(threshold, prev_frame)).await;

    match result {
        Ok((new_frame, maybe_event)) => {
            if let Some(event) = maybe_event {
                if event_tx.try_send(event).is_err() {
                    debug!("camera event channel full — event dropped");
                }
            }
            new_frame
        }
        Err(join_err) => {
            warn!(error = %join_err, "camera capture task panicked");
            let _ = event_tx.try_send(WatchEvent::Error {
                source: "camera_watcher".into(),
                message: join_err.to_string(),
            });
            None // reset prev frame on panic
        }
    }
}

/// Capture one frame, measure motion, run gesture detection when motion detected.
///
/// Returns `(new_prev_frame, Option<WatchEvent>)`.  The old previous frame is
/// implicitly dropped when this function returns and `prev_frame` goes out of
/// scope.
fn capture_and_analyse(
    motion_threshold: f32,
    prev_frame: Option<Vec<u8>>,
) -> (Option<Vec<u8>>, Option<WatchEvent>) {
    #[cfg(feature = "camera")]
    {
        use crate::camera::{capture_frame, detect_gestures};

        let image = match capture_frame(None) {
            Ok(img) => img,
            Err(e) => {
                warn!(error = %e, "camera frame capture failed");
                return (
                    prev_frame,
                    Some(WatchEvent::Error {
                        source: "camera_watcher".into(),
                        message: e.to_string(),
                    }),
                );
            }
        };

        // Motion detection: compare current JPEG with previous.
        let motion = prev_frame
            .as_deref()
            .map(|prev| estimate_motion(prev, &image.jpeg_data))
            .unwrap_or(0.0); // no previous frame → no motion event

        debug!(motion, threshold = motion_threshold, "camera motion check");

        if motion <= motion_threshold {
            // No significant motion: swap frame, drop old prev.
            return (Some(image.jpeg_data), None);
        }

        // Motion detected: run gesture recognition.
        // `image` already owns the JPEG bytes — pass it directly.
        let full_image = image;

        let gestures = match detect_gestures(&full_image) {
            Ok(g) => g,
            Err(e) => {
                warn!(error = %e, "gesture detection failed");
                return (
                    Some(full_image.jpeg_data),
                    Some(WatchEvent::Error {
                        source: "camera_watcher".into(),
                        message: e.to_string(),
                    }),
                );
            }
        };

        let timestamp = super::audio_watcher::current_timestamp_pub();
        // Take the highest-confidence gesture, if any.
        let event = gestures
            .into_iter()
            .max_by(|a, b| {
                a.confidence
                    .partial_cmp(&b.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|d| WatchEvent::Gesture {
                gesture: d.gesture.as_name().to_string(),
                confidence: d.confidence,
                hand: format!("{:?}", d.hand).to_lowercase(),
                timestamp,
            });

        (Some(full_image.jpeg_data), event)
    }

    #[cfg(not(feature = "camera"))]
    {
        let _ = (motion_threshold, prev_frame);
        (None, None)
    }
}

// ---------------------------------------------------------------------------
// Motion detection
// ---------------------------------------------------------------------------

/// Estimate motion between two JPEG frames as a value in `[0.0, 1.0]`.
///
/// Uses the relative difference in JPEG byte-length as a fast proxy for
/// image content change.  JPEG encoding is content-adaptive: a significantly
/// different scene produces a measurably different file size without
/// decoding a single pixel.
///
/// This is O(1) — no pixel decoding, no decompression — and contributes
/// essentially zero CPU overhead between captures.
///
/// ## Limitations
///
/// - Uniform colour changes (e.g. slow fade) may not register.
/// - The threshold `0.05` (5% size difference) is tunable via `WatchConfig`.
///
/// # Examples
///
/// ```rust
/// use axterminator::watch::camera_watcher::estimate_motion;
///
/// // Identical frames → 0.0
/// assert_eq!(estimate_motion(b"AAAA", b"AAAA"), 0.0);
///
/// // Completely different sizes → up to 1.0
/// let big = vec![0u8; 1000];
/// let small = vec![0u8; 100];
/// let motion = estimate_motion(&big, &small);
/// assert!(motion > 0.5);
/// ```
#[must_use]
pub fn estimate_motion(prev: &[u8], curr: &[u8]) -> f32 {
    if prev.is_empty() {
        return 0.0;
    }
    let diff = (prev.len() as f32 - curr.len() as f32).abs();
    (diff / prev.len() as f32).min(1.0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // estimate_motion
    // -----------------------------------------------------------------------

    #[test]
    fn motion_identical_frames_returns_zero() {
        // GIVEN: two identical byte slices
        let frame = vec![42u8; 800_000];
        // WHEN: compared
        let motion = estimate_motion(&frame, &frame);
        // THEN: zero motion
        assert_eq!(motion, 0.0);
    }

    #[test]
    fn motion_different_sizes_returns_positive() {
        // GIVEN: frames of different lengths
        let big = vec![0u8; 1000];
        let small = vec![0u8; 700];
        // WHEN: compared
        let motion = estimate_motion(&big, &small);
        // THEN: positive motion value
        assert!(motion > 0.0, "expected positive motion, got {motion}");
    }

    #[test]
    fn motion_completely_different_sizes_is_bounded_to_one() {
        // GIVEN: one very large, one tiny
        let big = vec![0u8; 10_000];
        let tiny = vec![0u8; 1];
        let motion = estimate_motion(&big, &tiny);
        // THEN: capped at 1.0
        assert!(motion <= 1.0);
        assert!(motion > 0.9);
    }

    #[test]
    fn motion_empty_prev_returns_zero() {
        // GIVEN: empty previous frame (first capture ever)
        let motion = estimate_motion(&[], &[1, 2, 3]);
        assert_eq!(motion, 0.0);
    }

    #[test]
    fn motion_five_percent_size_diff_near_threshold() {
        // GIVEN: frames with exactly 5% size difference
        let prev = vec![0u8; 1000];
        let curr = vec![0u8; 950]; // 5% smaller
        let motion = estimate_motion(&prev, &curr);
        // THEN: motion is ~0.05
        assert!(
            (motion - 0.05).abs() < 0.001,
            "expected ~0.05, got {motion}"
        );
    }

    #[test]
    fn motion_same_length_same_content_zero() {
        // GIVEN: frames with same content and same length
        let data = vec![255u8; 500];
        let motion = estimate_motion(&data, &data.clone());
        assert_eq!(motion, 0.0);
    }

    // -----------------------------------------------------------------------
    // WatchConfig camera defaults
    // -----------------------------------------------------------------------

    #[test]
    fn default_camera_poll_interval_is_two_seconds() {
        let cfg = WatchConfig::default();
        assert_eq!(cfg.camera_poll_interval_ms, 2000);
    }

    #[test]
    fn default_camera_motion_threshold_is_five_percent() {
        let cfg = WatchConfig::default();
        assert!((cfg.camera_motion_threshold - 0.05).abs() < 1e-6);
    }
}
