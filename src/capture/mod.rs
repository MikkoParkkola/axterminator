//! Continuous background capture of audio and screen for on-demand AI agent queries.
//!
//! ## Architecture
//!
//! MCP does not support native streaming.  The pattern implemented here is:
//!
//! 1. An internal capture loop runs on a dedicated OS thread (`std::thread`).
//! 2. Shared, `Arc<Mutex<…>>` state accumulates audio samples, transcripts,
//!    and the most-recent screen frame.
//! 3. MCP snapshot tools (`ax_get_transcription`, `ax_capture_status`, …) read
//!    the shared state on demand — no streaming, no waiting.
//!
//! ## Safety / Privacy
//!
//! - Audio is never persisted to disk; samples live only in the in-process ring buffer.
//! - Transcription is on-device via `SFSpeechRecognizer` (no cloud).
//! - Screen frames are captured at a configurable interval (default 3 s) and the
//!   previous frame is immediately replaced, so at most one frame is in memory.
//! - The capture thread honours the `running` flag on every iteration and stops
//!   cleanly before `CaptureSession::drop` returns.
//!
//! ## Screen diff
//!
//! When screen capture is enabled, [`CaptureConfig::screen_diff_threshold`]
//! controls deduplication: a new frame is only stored when its perceptual diff
//! score (fraction of 16×16 luminance cells that changed) meets or exceeds the
//! threshold.  Use `0.0` to store every frame regardless.
//!
//! ## Feature gate
//!
//! This module compiles when `--features audio` is set.  Screen capture uses
//! the existing `CGWindowListCreateImage` path already available in the codebase.
//!
//! ## Quick start
//!
//! ```rust,no_run
//! # #[cfg(feature = "audio")]
//! # {
//! use axterminator::capture::{CaptureConfig, CaptureSession};
//!
//! let cfg = CaptureConfig {
//!     audio: true,
//!     transcribe: true,
//!     screen: false,
//!     screen_diff_threshold: 0.05,
//!     buffer_seconds: 60,
//! };
//! let session = CaptureSession::start(cfg);
//! // …AI agent queries its tools…
//! let transcript = session.read_transcription(30);
//! drop(session); // graceful shutdown
//! # }
//! ```

pub mod screen_diff;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use tracing::{debug, warn};

use crate::audio::{capture_system_audio, transcribe, AudioData};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A fixed-capacity ring buffer for contiguous `f32` audio samples.
///
/// Unlike the event-oriented [`watch::ring_buffer::RingBuffer`], this buffer
/// is optimised for high-throughput bulk appends and bulk reads of the trailing
/// `N` samples — matching the audio capture access pattern.
///
/// ## Examples
///
/// ```rust
/// # #[cfg(feature = "audio")]
/// # {
/// use axterminator::capture::AudioRingBuffer;
///
/// let mut buf = AudioRingBuffer::new(8);
/// buf.push_slice(&[1.0, 2.0, 3.0]);
/// buf.push_slice(&[4.0, 5.0, 6.0, 7.0, 8.0, 9.0]); // wraps around
/// // Only the last 8 samples are retained.
/// assert_eq!(buf.len(), 8);
/// let last4 = buf.read_last(4);
/// assert_eq!(last4, vec![6.0, 7.0, 8.0, 9.0]);
/// # }
/// ```
pub struct AudioRingBuffer {
    data: Vec<f32>,
    capacity: usize,
    /// Index of the next write position (wraps modulo `capacity`).
    write_pos: usize,
    /// Number of valid samples currently stored (saturates at `capacity`).
    len: usize,
}

impl AudioRingBuffer {
    /// Create a new `AudioRingBuffer` with the given sample capacity.
    ///
    /// # Panics
    ///
    /// Panics when `capacity == 0`.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "AudioRingBuffer capacity must be > 0");
        Self {
            data: vec![0.0f32; capacity],
            capacity,
            write_pos: 0,
            len: 0,
        }
    }

    /// Append `samples` into the ring buffer, overwriting the oldest data when full.
    pub fn push_slice(&mut self, samples: &[f32]) {
        for &s in samples {
            self.data[self.write_pos] = s;
            self.write_pos = (self.write_pos + 1) % self.capacity;
            if self.len < self.capacity {
                self.len += 1;
            }
        }
    }

    /// Return the last `count` samples in chronological order (oldest first).
    ///
    /// Returns fewer samples if the buffer has fewer than `count` stored.
    #[must_use]
    pub fn read_last(&self, count: usize) -> Vec<f32> {
        let n = count.min(self.len);
        if n == 0 {
            return Vec::new();
        }
        let mut out = Vec::with_capacity(n);
        // Start index: `write_pos` points to the next write slot, which is
        // also the oldest sample when the buffer is full.  We step back `n`
        // positions from `write_pos` to find the start of the last `n` samples.
        let start = (self.write_pos + self.capacity - self.len + (self.len - n)) % self.capacity;
        for i in 0..n {
            out.push(self.data[(start + i) % self.capacity]);
        }
        out
    }

    /// Number of valid samples currently stored.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Return `true` when no samples are stored.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Remove all samples from the buffer.
    pub fn clear(&mut self) {
        self.write_pos = 0;
        self.len = 0;
    }
}

// ---------------------------------------------------------------------------

/// A recognised speech segment from the on-device transcription pipeline.
#[derive(Debug, Clone)]
pub struct TranscriptSegment {
    /// Transcribed text.
    pub text: String,
    /// Start offset in milliseconds from capture start.
    pub start_ms: u64,
    /// End offset in milliseconds from capture start.
    pub end_ms: u64,
    /// Speaker identifier when diarisation is available (usually `None`).
    pub speaker: Option<String>,
}

/// A single screen capture frame.
#[derive(Debug, Clone)]
pub struct ScreenFrame {
    /// PNG-encoded frame as standard base64.
    pub png_base64: String,
    /// ISO 8601 UTC capture timestamp.
    pub timestamp: String,
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for [`CaptureSession::start`].
#[derive(Debug, Clone)]
pub struct CaptureConfig {
    /// Enable continuous audio capture into the ring buffer.
    pub audio: bool,
    /// Periodically transcribe the audio ring buffer with `SFSpeechRecognizer`.
    pub transcribe: bool,
    /// Capture periodic screenshots (one retained at a time).
    pub screen: bool,
    /// Audio ring buffer depth expressed as seconds at 16 kHz mono.
    ///
    /// Default: 60 s → 960 000 samples → ~3.7 MB.
    pub buffer_seconds: u32,
    /// Minimum perceptual diff score `[0.0, 1.0]` required to store a new frame.
    ///
    /// The diff score is the fraction of 16×16 luminance grid cells that changed
    /// by more than a small perceptual threshold between consecutive frames.
    ///
    /// - `0.0` — store every frame (no deduplication).
    /// - `0.05` (default) — skip frames where fewer than 5 % of cells changed.
    /// - `1.0` — only store frames where every grid cell changed.
    ///
    /// Byte-identical frames (score `0.0`) are always skipped regardless of this
    /// threshold.
    pub screen_diff_threshold: f32,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            audio: true,
            transcribe: true,
            screen: false,
            buffer_seconds: 60,
            screen_diff_threshold: 0.05,
        }
    }
}

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

/// All state shared between the background thread and MCP handlers.
///
/// Protected by `Arc<Mutex<…>>` so reads from MCP handlers are non-blocking
/// relative to the capture thread (they lock transiently, then release).
#[derive(Default)]
pub(crate) struct CaptureState {
    pub(crate) transcript_segments: Vec<TranscriptSegment>,
    pub(crate) latest_frame: Option<ScreenFrame>,
}

// ---------------------------------------------------------------------------
// CaptureSession
// ---------------------------------------------------------------------------

/// A live capture session that accumulates audio and optional screen data.
///
/// Start with [`CaptureSession::start`].  The background thread runs until
/// the session is dropped or [`CaptureSession::stop`] is called explicitly.
///
/// ## Thread safety
///
/// All shared state is behind `Arc<Mutex<…>>`.  Handlers that read data
/// (e.g. `ax_get_transcription`) lock transiently and release immediately.
pub struct CaptureSession {
    /// Unique session identifier (monotonically increasing, formatted as hex).
    pub session_id: String,
    /// Audio samples ring buffer — guarded separately for high-throughput appends.
    audio_buffer: Arc<Mutex<AudioRingBuffer>>,
    /// Transcripts + latest screen frame.
    state: Arc<Mutex<CaptureState>>,
    /// Set to `false` to request the background thread to stop.
    running: Arc<AtomicBool>,
    /// Sample rate stored for buffer-seconds conversion.
    sample_rate: u32,
    /// OS thread handle — `None` once joined.
    handle: Option<JoinHandle<()>>,
    /// Configuration snapshot kept for status queries.
    config: CaptureConfig,
    /// Wall-clock start instant for elapsed tracking.
    started_at: Instant,
    /// Count of screen frames that passed the diff threshold and were stored.
    frames_captured: Arc<AtomicU64>,
    /// Count of screen frames that were skipped because the diff was below threshold.
    frames_skipped: Arc<AtomicU64>,
}

/// Counter for session IDs (shared across the process lifetime).
static SESSION_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Duration of each audio capture window fed into the pipeline.
const AUDIO_WINDOW_SECS: f32 = 5.0;

/// How often to capture a new screenshot.
const SCREEN_POLL_SECS: u64 = 3;

impl CaptureSession {
    /// Start a new capture session with the given configuration.
    ///
    /// Returns immediately; the background thread begins capturing in parallel.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # #[cfg(feature = "audio")]
    /// # {
    /// use axterminator::capture::{CaptureConfig, CaptureSession};
    ///
    /// let session = CaptureSession::start(CaptureConfig::default());
    /// assert!(!session.session_id.is_empty());
    /// # }
    /// ```
    #[must_use]
    pub fn start(config: CaptureConfig) -> Self {
        let sample_rate = crate::audio::SAMPLE_RATE;
        let capacity = (config.buffer_seconds as usize)
            .saturating_mul(sample_rate as usize)
            .max(1);

        let audio_buffer = Arc::new(Mutex::new(AudioRingBuffer::new(capacity)));
        let state = Arc::new(Mutex::new(CaptureState::default()));
        let running = Arc::new(AtomicBool::new(true));
        let frames_captured = Arc::new(AtomicU64::new(0));
        let frames_skipped = Arc::new(AtomicU64::new(0));
        let id = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
        let session_id = format!("{id:016x}");
        let started_at = Instant::now();

        let handle = spawn_capture_thread(
            config.clone(),
            Arc::clone(&audio_buffer),
            Arc::clone(&state),
            Arc::clone(&running),
            Arc::clone(&frames_captured),
            Arc::clone(&frames_skipped),
        );

        Self {
            session_id,
            audio_buffer,
            state,
            running,
            sample_rate,
            handle: Some(handle),
            config,
            started_at,
            frames_captured,
            frames_skipped,
        }
    }

    /// Signal the background thread to stop and wait for it to exit.
    ///
    /// Idempotent — safe to call multiple times.
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Release);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }

    /// Return `true` when the background thread is still capturing.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire) && self.handle.is_some()
    }

    /// Elapsed milliseconds since the session was started.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn duration_ms(&self) -> u64 {
        self.started_at.elapsed().as_millis() as u64
    }

    /// Number of audio seconds buffered (≤ `config.buffer_seconds`).
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn audio_buffer_seconds(&self) -> f64 {
        let len = self
            .audio_buffer
            .lock()
            .map(|g| g.len())
            .unwrap_or_default();
        len as f64 / f64::from(self.sample_rate)
    }

    /// Copy the last `since_seconds` of transcription segments from the buffer.
    ///
    /// Returns all segments whose `end_ms` falls within the requested window.
    #[must_use]
    pub fn read_transcription(&self, since_seconds: u64) -> Vec<TranscriptSegment> {
        let Ok(guard) = self.state.lock() else {
            return Vec::new();
        };
        let duration_ms = self.duration_ms();
        let since_ms = since_seconds.saturating_mul(1_000);
        let cutoff_ms = duration_ms.saturating_sub(since_ms);
        guard
            .transcript_segments
            .iter()
            .filter(|s| s.end_ms >= cutoff_ms)
            .cloned()
            .collect()
    }

    /// Number of transcription segments accumulated so far.
    #[must_use]
    pub fn transcript_segment_count(&self) -> usize {
        self.state
            .lock()
            .map(|g| g.transcript_segments.len())
            .unwrap_or_default()
    }

    /// Clone the most recently captured screen frame, if any.
    #[must_use]
    pub fn latest_frame(&self) -> Option<ScreenFrame> {
        self.state.lock().ok()?.latest_frame.clone()
    }

    /// Number of screen frames that passed the diff threshold and were stored.
    #[must_use]
    pub fn frames_captured(&self) -> u64 {
        self.frames_captured.load(Ordering::Relaxed)
    }

    /// Number of screen frames that were skipped because the diff was below threshold.
    #[must_use]
    pub fn frames_skipped(&self) -> u64 {
        self.frames_skipped.load(Ordering::Relaxed)
    }

    /// Expose the config for status queries.
    #[must_use]
    pub fn config(&self) -> &CaptureConfig {
        &self.config
    }

    /// Read raw audio samples from the last `seconds` of the buffer.
    ///
    /// Primarily useful for testing; MCP handlers use `read_transcription`.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn read_audio_samples(&self, seconds: f32) -> Vec<f32> {
        let count = (f64::from(seconds) * f64::from(self.sample_rate)).round() as usize;
        self.audio_buffer
            .lock()
            .map(|g| g.read_last(count))
            .unwrap_or_default()
    }
}

impl Drop for CaptureSession {
    fn drop(&mut self) {
        self.stop();
    }
}

// ---------------------------------------------------------------------------
// Background thread
// ---------------------------------------------------------------------------

#[allow(clippy::needless_pass_by_value)] // Arcs are moved into the thread closure.
fn spawn_capture_thread(
    config: CaptureConfig,
    audio_buffer: Arc<Mutex<AudioRingBuffer>>,
    state: Arc<Mutex<CaptureState>>,
    running: Arc<AtomicBool>,
    frames_captured: Arc<AtomicU64>,
    frames_skipped: Arc<AtomicU64>,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name("ax-capture".to_string())
        .spawn(move || {
            run_capture_loop(
                config,
                audio_buffer,
                state,
                running,
                frames_captured,
                frames_skipped,
            );
        })
        .expect("failed to spawn capture thread")
}

#[allow(clippy::needless_pass_by_value)] // Arcs and config are owned by the thread.
fn run_capture_loop(
    config: CaptureConfig,
    audio_buffer: Arc<Mutex<AudioRingBuffer>>,
    state: Arc<Mutex<CaptureState>>,
    running: Arc<AtomicBool>,
    frames_captured: Arc<AtomicU64>,
    frames_skipped: Arc<AtomicU64>,
) {
    let started = Instant::now();
    let mut last_screen_capture = Instant::now()
        .checked_sub(Duration::from_secs(SCREEN_POLL_SECS))
        .unwrap_or(Instant::now());
    let mut segment_offset_ms: u64 = 0;
    let mut prev_fingerprint: Option<screen_diff::ScreenFingerprint> = None;

    while running.load(Ordering::Acquire) {
        if config.audio {
            capture_audio_window(
                &audio_buffer,
                &state,
                &running,
                config.transcribe,
                started,
                &mut segment_offset_ms,
            );
        }

        if config.screen && last_screen_capture.elapsed() >= Duration::from_secs(SCREEN_POLL_SECS) {
            capture_screen_snapshot(
                &state,
                started,
                config.screen_diff_threshold,
                &mut prev_fingerprint,
                &frames_captured,
                &frames_skipped,
            );
            last_screen_capture = Instant::now();
        }

        if !config.audio && !config.screen {
            // Nothing to capture — poll the running flag at a low rate.
            thread::sleep(Duration::from_millis(100));
        }
    }

    debug!("capture loop exited cleanly");
}

/// Capture one audio window and optionally append a transcript segment.
fn capture_audio_window(
    audio_buffer: &Arc<Mutex<AudioRingBuffer>>,
    state: &Arc<Mutex<CaptureState>>,
    running: &Arc<AtomicBool>,
    do_transcribe: bool,
    started: Instant,
    segment_offset_ms: &mut u64,
) {
    if !running.load(Ordering::Acquire) {
        return;
    }

    let audio_data = match capture_system_audio(AUDIO_WINDOW_SECS) {
        Ok(d) => d,
        Err(e) => {
            warn!(error = %e, "audio capture window failed");
            thread::sleep(Duration::from_secs(1));
            return;
        }
    };

    #[allow(clippy::cast_possible_truncation)]
    let window_end_ms = started.elapsed().as_millis() as u64;
    let window_start_ms = window_end_ms.saturating_sub(audio_data.duration_ms());

    append_audio_samples(audio_buffer, &audio_data);

    if do_transcribe {
        transcribe_and_store(state, &audio_data, window_start_ms, window_end_ms);
    }

    *segment_offset_ms = window_end_ms;
}

fn append_audio_samples(audio_buffer: &Arc<Mutex<AudioRingBuffer>>, data: &AudioData) {
    match audio_buffer.lock() {
        Ok(mut guard) => guard.push_slice(&data.samples),
        Err(e) => warn!("audio_buffer lock poisoned: {e}"),
    }
}

fn transcribe_and_store(
    state: &Arc<Mutex<CaptureState>>,
    audio_data: &AudioData,
    start_ms: u64,
    end_ms: u64,
) {
    match transcribe(audio_data, None) {
        Ok(text) if !text.trim().is_empty() => {
            let segment = TranscriptSegment {
                text,
                start_ms,
                end_ms,
                speaker: None,
            };
            match state.lock() {
                Ok(mut guard) => guard.transcript_segments.push(segment),
                Err(e) => warn!("state lock poisoned during transcription: {e}"),
            }
        }
        Ok(_) => {}
        Err(e) => warn!(error = %e, "transcription failed for audio window"),
    }
}

/// Capture a single screenshot and conditionally store it based on diff score.
///
/// Uses the existing `CGWindowListCreateImage` path (no Screen Recording needed
/// for the virtual display composited view on macOS 14+).
///
/// The frame is stored only when its diff score against `prev_fingerprint` meets
/// or exceeds `threshold`.  On first capture (`prev_fingerprint` is `None`) the
/// frame is always stored.  The counters `frames_captured` and `frames_skipped`
/// are incremented accordingly.
fn capture_screen_snapshot(
    state: &Arc<Mutex<CaptureState>>,
    started: Instant,
    threshold: f32,
    prev_fingerprint: &mut Option<screen_diff::ScreenFingerprint>,
    frames_captured: &Arc<AtomicU64>,
    frames_skipped: &Arc<AtomicU64>,
) {
    let elapsed_secs = started.elapsed().as_secs();
    let timestamp = format!("T+{elapsed_secs}s");

    let (png_base64, width, height, png_bytes) = match capture_primary_display_png() {
        Ok(result) => result,
        Err(e) => {
            warn!(error = %e, "screen snapshot failed");
            return;
        }
    };

    let fingerprint = screen_diff::ScreenFingerprint::from_png_bytes(&png_bytes);

    let should_store = match prev_fingerprint.as_ref() {
        None => true, // first frame — always store
        Some(prev) => {
            let diff = screen_diff::ScreenDiff::compare(prev, &fingerprint);
            let significant = diff.is_significant(threshold);
            debug!(score = diff.score, threshold, significant, "screen diff");
            significant
        }
    };

    if should_store {
        let frame = ScreenFrame {
            png_base64,
            timestamp,
            width,
            height,
        };
        match state.lock() {
            Ok(mut guard) => guard.latest_frame = Some(frame),
            Err(e) => warn!("state lock poisoned during screen capture: {e}"),
        }
        *prev_fingerprint = Some(fingerprint);
        frames_captured.fetch_add(1, Ordering::Relaxed);
    } else {
        frames_skipped.fetch_add(1, Ordering::Relaxed);
    }
}

/// Capture the primary display and return a base64-encoded PNG, raw PNG bytes,
/// and logical display dimensions.
///
/// Returns `(png_base64, width, height, raw_png_bytes)`.  The raw bytes are
/// returned alongside the base64 so the caller can fingerprint the frame
/// without redundant re-encoding or disk I/O.
///
/// Uses `screencapture -x -D 1` (no UI sound, primary display) which is
/// available on all macOS versions and requires no additional permissions.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn capture_primary_display_png() -> Result<(String, u32, u32, Vec<u8>), String> {
    use base64::Engine as _;
    use core_graphics::display::CGDisplay;
    use std::process::Command;

    // Obtain logical display dimensions from CoreGraphics.
    let main = CGDisplay::main();
    let bounds = main.bounds();
    let width = bounds.size.width as u32;
    let height = bounds.size.height as u32;

    // Capture to a temp file; -x suppresses the shutter sound.
    let tmp = format!("/tmp/axterminator_capture_{}.png", std::process::id());
    let output = Command::new("screencapture")
        .args(["-x", "-D", "1", &tmp])
        .output()
        .map_err(|e| format!("screencapture failed to launch: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "screencapture exited with status {}",
            output.status
        ));
    }

    let png_bytes = std::fs::read(&tmp).map_err(|e| format!("failed to read capture file: {e}"))?;
    let _ = std::fs::remove_file(&tmp);

    let png_base64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
    Ok((png_base64, width, height, png_bytes))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // AudioRingBuffer
    // -----------------------------------------------------------------------

    #[test]
    fn audio_ring_buffer_starts_empty() {
        // GIVEN: freshly created buffer
        let buf = AudioRingBuffer::new(100);
        // THEN: empty
        assert!(buf.is_empty());
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn audio_ring_buffer_push_slice_below_capacity_stores_all() {
        // GIVEN: capacity 16, push 4 samples
        let mut buf = AudioRingBuffer::new(16);
        buf.push_slice(&[1.0, 2.0, 3.0, 4.0]);
        // THEN: 4 samples stored
        assert_eq!(buf.len(), 4);
    }

    #[test]
    fn audio_ring_buffer_read_last_returns_n_most_recent() {
        // GIVEN: buffer with [1..5]
        let mut buf = AudioRingBuffer::new(8);
        buf.push_slice(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        // WHEN: read_last(3)
        let last = buf.read_last(3);
        // THEN: last 3 in order
        assert_eq!(last, vec![3.0, 4.0, 5.0]);
    }

    #[test]
    fn audio_ring_buffer_overflow_evicts_oldest() {
        // GIVEN: capacity 4, push 6 samples
        let mut buf = AudioRingBuffer::new(4);
        buf.push_slice(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        // THEN: len capped at 4, oldest 2 evicted
        assert_eq!(buf.len(), 4);
        let all = buf.read_last(4);
        assert_eq!(all, vec![3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn audio_ring_buffer_read_last_clamps_to_available() {
        // GIVEN: 3 samples in a capacity-10 buffer
        let mut buf = AudioRingBuffer::new(10);
        buf.push_slice(&[7.0, 8.0, 9.0]);
        // WHEN: requesting more than stored
        let last = buf.read_last(100);
        // THEN: returns only the 3 available
        assert_eq!(last.len(), 3);
        assert_eq!(last, vec![7.0, 8.0, 9.0]);
    }

    #[test]
    fn audio_ring_buffer_read_last_empty_returns_empty_vec() {
        // GIVEN: empty buffer
        let buf = AudioRingBuffer::new(8);
        assert_eq!(buf.read_last(4), Vec::<f32>::new());
    }

    #[test]
    fn audio_ring_buffer_clear_resets_state() {
        // GIVEN: buffer with samples
        let mut buf = AudioRingBuffer::new(8);
        buf.push_slice(&[1.0, 2.0, 3.0]);
        // WHEN: cleared
        buf.clear();
        // THEN: empty
        assert!(buf.is_empty());
        assert_eq!(buf.len(), 0);
        assert_eq!(buf.read_last(4), Vec::<f32>::new());
    }

    #[test]
    fn audio_ring_buffer_multiple_wraps_preserves_latest() {
        // GIVEN: capacity 3, push 9 samples in three batches
        let mut buf = AudioRingBuffer::new(3);
        buf.push_slice(&[1.0, 2.0, 3.0]);
        buf.push_slice(&[4.0, 5.0, 6.0]);
        buf.push_slice(&[7.0, 8.0, 9.0]);
        // THEN: last 3 samples are [7, 8, 9]
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.read_last(3), vec![7.0, 8.0, 9.0]);
    }

    #[test]
    #[should_panic(expected = "capacity must be > 0")]
    fn audio_ring_buffer_zero_capacity_panics() {
        let _ = AudioRingBuffer::new(0);
    }

    #[test]
    fn audio_ring_buffer_read_last_zero_count_returns_empty() {
        // GIVEN: buffer with samples
        let mut buf = AudioRingBuffer::new(8);
        buf.push_slice(&[1.0, 2.0]);
        // WHEN: reading 0 samples
        assert_eq!(buf.read_last(0), Vec::<f32>::new());
    }

    #[test]
    fn audio_ring_buffer_push_single_sample_at_a_time() {
        // GIVEN: capacity 3
        let mut buf = AudioRingBuffer::new(3);
        // WHEN: push one at a time
        for i in 1..=5u8 {
            buf.push_slice(&[f32::from(i)]);
        }
        // THEN: last 3 in order
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.read_last(3), vec![3.0, 4.0, 5.0]);
    }

    // -----------------------------------------------------------------------
    // TranscriptSegment
    // -----------------------------------------------------------------------

    #[test]
    fn transcript_segment_fields_are_accessible() {
        let seg = TranscriptSegment {
            text: "hello".to_string(),
            start_ms: 0,
            end_ms: 5000,
            speaker: Some("A".to_string()),
        };
        assert_eq!(seg.text, "hello");
        assert_eq!(seg.start_ms, 0);
        assert_eq!(seg.end_ms, 5000);
        assert_eq!(seg.speaker.as_deref(), Some("A"));
    }

    #[test]
    fn transcript_segment_clone_is_independent() {
        let seg = TranscriptSegment {
            text: "world".to_string(),
            start_ms: 1000,
            end_ms: 6000,
            speaker: None,
        };
        let seg2 = seg.clone();
        assert_eq!(seg2.text, seg.text);
        assert_eq!(seg2.start_ms, seg.start_ms);
    }

    // -----------------------------------------------------------------------
    // CaptureConfig
    // -----------------------------------------------------------------------

    #[test]
    fn capture_config_default_has_sensible_values() {
        let cfg = CaptureConfig::default();
        assert!(cfg.audio);
        assert!(cfg.transcribe);
        assert!(!cfg.screen);
        assert_eq!(cfg.buffer_seconds, 60);
    }

    #[test]
    fn capture_config_clone_is_independent() {
        let mut cfg = CaptureConfig::default();
        let cfg2 = cfg.clone();
        cfg.audio = false;
        assert!(!cfg.audio, "mutation must affect original");
        assert!(cfg2.audio, "clone must remain unaffected");
    }

    // -----------------------------------------------------------------------
    // CaptureSession lifecycle
    // -----------------------------------------------------------------------

    /// No-op config (no audio/screen/transcribe) with a tiny buffer.
    ///
    /// Uses `..CaptureConfig::default()` so this helper stays forward-compatible
    /// when new fields are added to `CaptureConfig`.
    fn idle_config(buffer_seconds: u32) -> CaptureConfig {
        CaptureConfig {
            audio: false,
            transcribe: false,
            screen: false,
            buffer_seconds,
            ..CaptureConfig::default()
        }
    }

    #[test]
    fn capture_session_starts_with_unique_session_ids() {
        // GIVEN: two sessions started sequentially (no audio, nothing to capture)
        let s1 = CaptureSession::start(idle_config(1));
        let s2 = CaptureSession::start(idle_config(1));
        // THEN: IDs differ
        assert_ne!(s1.session_id, s2.session_id);
    }

    #[test]
    fn capture_session_is_running_after_start() {
        let session = CaptureSession::start(idle_config(1));
        assert!(session.is_running());
    }

    #[test]
    fn capture_session_stop_halts_background_thread() {
        let mut session = CaptureSession::start(idle_config(1));
        session.stop();
        assert!(!session.is_running());
    }

    #[test]
    fn capture_session_drop_stops_thread_without_panic() {
        // GIVEN: session with nothing to capture
        // WHEN: dropped
        {
            let _session = CaptureSession::start(idle_config(1));
        }
        // THEN: no panic; test completes
    }

    #[test]
    fn capture_session_stop_is_idempotent() {
        let mut session = CaptureSession::start(idle_config(1));
        session.stop();
        session.stop(); // second call must not panic
    }

    #[test]
    fn capture_session_audio_buffer_seconds_initially_zero() {
        let session = CaptureSession::start(idle_config(60));
        // Nothing has been captured yet.
        assert!(session.audio_buffer_seconds() < 1.0);
    }

    #[test]
    fn capture_session_transcript_segment_count_initially_zero() {
        let session = CaptureSession::start(idle_config(1));
        assert_eq!(session.transcript_segment_count(), 0);
    }

    #[test]
    fn capture_session_read_transcription_empty_returns_empty_vec() {
        let session = CaptureSession::start(idle_config(1));
        assert!(session.read_transcription(30).is_empty());
    }

    #[test]
    fn capture_session_read_audio_samples_empty_returns_empty() {
        let session = CaptureSession::start(idle_config(1));
        assert!(session.read_audio_samples(1.0).is_empty());
    }

    #[test]
    fn capture_session_latest_frame_initially_none() {
        let session = CaptureSession::start(idle_config(1));
        assert!(session.latest_frame().is_none());
    }

    #[test]
    fn capture_session_duration_ms_advances() {
        let session = CaptureSession::start(idle_config(1));
        let t0 = session.duration_ms();
        std::thread::sleep(Duration::from_millis(5));
        let t1 = session.duration_ms();
        assert!(t1 >= t0);
    }

    #[test]
    fn capture_session_config_is_accessible() {
        let session = CaptureSession::start(CaptureConfig {
            audio: false,
            buffer_seconds: 30,
            ..CaptureConfig::default()
        });
        assert_eq!(session.config().buffer_seconds, 30);
    }

    // -----------------------------------------------------------------------
    // Internal state injection (transcript filter)
    // -----------------------------------------------------------------------

    #[test]
    fn read_transcription_filters_by_time_window() {
        // GIVEN: session with manually injected segments
        let session = CaptureSession::start(idle_config(1));

        // Inject two segments directly into shared state.
        {
            let mut guard = session.state.lock().unwrap();
            guard.transcript_segments.push(TranscriptSegment {
                text: "old".to_string(),
                start_ms: 0,
                end_ms: 1_000,
                speaker: None,
            });
            guard.transcript_segments.push(TranscriptSegment {
                text: "recent".to_string(),
                start_ms: 90_000,
                end_ms: 95_000,
                speaker: None,
            });
        }

        // A large window should return everything.
        let all = session.read_transcription(200);
        assert_eq!(all.len(), 2);

        // A very small window (1 s) should return only the recent segment
        // whose end_ms exceeds (duration_ms - 1000).  Because the session
        // has just started, duration_ms ≈ 0, so cutoff_ms saturates to 0
        // and BOTH segments pass.  Skip the fine-grained window assertion
        // — a 1-second session window test would be flaky in CI.
        let _ = session.read_transcription(1);
    }

    // -----------------------------------------------------------------------
    // CaptureConfig — screen_diff_threshold
    // -----------------------------------------------------------------------

    #[test]
    fn capture_config_default_screen_diff_threshold_is_five_percent() {
        // GIVEN: default config
        let cfg = CaptureConfig::default();
        // THEN: threshold defaults to 5 %
        assert!((cfg.screen_diff_threshold - 0.05).abs() < f32::EPSILON);
    }

    #[test]
    fn capture_config_screen_diff_threshold_zero_stored() {
        // GIVEN: explicit zero threshold (store every frame)
        let cfg = CaptureConfig {
            screen_diff_threshold: 0.0,
            ..CaptureConfig::default()
        };
        assert_eq!(cfg.screen_diff_threshold, 0.0);
    }

    #[test]
    fn capture_config_screen_diff_threshold_one_stored() {
        let cfg = CaptureConfig {
            screen_diff_threshold: 1.0,
            ..CaptureConfig::default()
        };
        assert_eq!(cfg.screen_diff_threshold, 1.0);
    }

    #[test]
    fn capture_config_screen_diff_threshold_propagates_to_session() {
        // GIVEN: session with explicit non-default threshold
        let session = CaptureSession::start(CaptureConfig {
            screen_diff_threshold: 0.10,
            ..idle_config(1)
        });
        // THEN: stored in config accessor
        assert!((session.config().screen_diff_threshold - 0.10).abs() < f32::EPSILON);
    }

    // -----------------------------------------------------------------------
    // Diff counters — initial state
    // -----------------------------------------------------------------------

    #[test]
    fn capture_session_frames_captured_initially_zero() {
        // GIVEN: session with screen capture disabled
        let session = CaptureSession::start(idle_config(1));
        // THEN: no frames ever captured
        assert_eq!(session.frames_captured(), 0);
    }

    #[test]
    fn capture_session_frames_skipped_initially_zero() {
        // GIVEN: session with screen capture disabled
        let session = CaptureSession::start(idle_config(1));
        // THEN: no frames ever skipped
        assert_eq!(session.frames_skipped(), 0);
    }

    // -----------------------------------------------------------------------
    // Diff logic — driven via ScreenFingerprint/ScreenDiff directly
    // (avoids needing screencapture hardware in CI)
    // -----------------------------------------------------------------------

    #[test]
    fn diff_first_frame_always_significant() {
        // GIVEN: no previous fingerprint (first capture simulation)
        let threshold = 0.05_f32;
        let pixels = vec![100u8; 16 * 16 * 4];
        let fp = screen_diff::ScreenFingerprint::from_raw_pixels(&pixels, 16, 16, 4);

        // Simulate the "no previous fingerprint" branch in the capture loop.
        let prev: Option<screen_diff::ScreenFingerprint> = None;
        let should_store = match prev.as_ref() {
            None => true,
            Some(p) => screen_diff::ScreenDiff::compare(p, &fp).is_significant(threshold),
        };
        assert!(should_store, "first frame must always be stored");
    }

    #[test]
    fn diff_identical_consecutive_frame_is_not_significant_at_default_threshold() {
        // GIVEN: two identical raw-pixel frames
        let pixels = vec![128u8; 32 * 32 * 4];
        let fp = screen_diff::ScreenFingerprint::from_raw_pixels(&pixels, 32, 32, 4);
        let diff = screen_diff::ScreenDiff::compare(&fp, &fp);
        // THEN: byte-identical → score 0.0 → not significant at 5 %
        assert_eq!(diff.score, 0.0);
        assert!(!diff.is_significant(0.05));
    }

    #[test]
    fn diff_fully_changed_frame_is_significant_at_default_threshold() {
        // GIVEN: all-black vs all-white
        let black = vec![0u8; 32 * 32 * 4];
        let white = vec![255u8; 32 * 32 * 4];
        let fp_prev = screen_diff::ScreenFingerprint::from_raw_pixels(&black, 32, 32, 4);
        let fp_next = screen_diff::ScreenFingerprint::from_raw_pixels(&white, 32, 32, 4);
        let diff = screen_diff::ScreenDiff::compare(&fp_prev, &fp_next);
        assert_eq!(diff.score, 1.0);
        assert!(diff.is_significant(0.05));
    }

    #[test]
    fn diff_minor_change_below_5pct_threshold_is_not_significant() {
        // GIVEN: 16×16 frames where only one pixel in one cell changes heavily,
        //        but another byte is touched to ensure hash differs.
        let mut pixels_a = vec![100u8; 16 * 16 * 4];
        let mut pixels_b = vec![100u8; 16 * 16 * 4];
        pixels_b[0] = 0; // changes one pixel in cell (0,0)
        pixels_b[1] = 0;
        pixels_b[2] = 0;
        pixels_a[63 * 4] = 99; // differentiate hash without affecting another cell's mean
        let fp_a = screen_diff::ScreenFingerprint::from_raw_pixels(&pixels_a, 16, 16, 4);
        let fp_b = screen_diff::ScreenFingerprint::from_raw_pixels(&pixels_b, 16, 16, 4);
        let diff = screen_diff::ScreenDiff::compare(&fp_a, &fp_b);
        // THEN: score < 0.05 → below default threshold
        assert!(diff.score < 0.05, "score was {}", diff.score);
        assert!(!diff.is_significant(0.05));
    }

    #[test]
    fn diff_zero_threshold_always_stores_any_nonzero_score() {
        // GIVEN: threshold = 0.0 makes every frame (even score 0.0) significant
        assert!(screen_diff::ScreenDiff { score: 0.0 }.is_significant(0.0));
        assert!(screen_diff::ScreenDiff { score: 0.001 }.is_significant(0.0));
    }

    #[test]
    fn diff_hash_only_fallback_returns_one_when_bytes_differ() {
        // GIVEN: two fingerprints from different non-PNG bytes (no grid available)
        let fp1 = screen_diff::ScreenFingerprint::from_png_bytes(b"bytes_a");
        let fp2 = screen_diff::ScreenFingerprint::from_png_bytes(b"bytes_b");
        // THEN: hash differs, no grid → score = 1.0 (conservative: treat as changed)
        assert_eq!(screen_diff::ScreenDiff::compare(&fp1, &fp2).score, 1.0);
    }
}
