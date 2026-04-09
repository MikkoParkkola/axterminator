//! Voice Activity Detection (VAD) pre-filter using the Silero VAD ONNX model.
//!
//! This module is compiled only when `--features vad` is set.  It provides a
//! lightweight gate that determines whether a chunk of audio contains speech
//! before dispatching to an STT engine, eliminating wasteful transcription of
//! silence and noise.
//!
//! ## Why not `vad-rs`?
//!
//! The `vad-rs` crate pins `ort = "=2.0.0-rc.9"` which is incompatible with
//! axterminator's existing `ort = "2.0.0-rc.12"` (parakeet feature).  To avoid
//! duplicate ONNX Runtime binaries and link conflicts, this module implements
//! the Silero VAD forward pass directly against the same `ort` version already
//! in the dependency tree.  The implementation follows the identical ONNX
//! interface described in the Silero VAD model card.
//!
//! ## Model
//!
//! Silero VAD is a stateful LSTM model with the following ONNX interface:
//!
//! | Input  | Shape      | dtype | Notes                         |
//! |--------|------------|-------|-------------------------------|
//! | input  | [1, N]     | f32   | PCM samples, 8 kHz or 16 kHz |
//! | sr     | [1]        | i64   | Sample rate (8000 or 16000)   |
//! | h      | [2, 1, 64] | f32   | LSTM hidden state (in-place)  |
//! | c      | [2, 1, 64] | f32   | LSTM cell state (in-place)    |
//!
//! | Output | Shape      | dtype | Notes                         |
//! |--------|------------|-------|-------------------------------|
//! | output | [1, 1]     | f32   | Speech probability [0, 1]     |
//! | hn     | [2, 1, 64] | f32   | Updated hidden state          |
//! | cn     | [2, 1, 64] | f32   | Updated cell state            |
//!
//! ## Model file
//!
//! Download the Silero VAD ONNX model:
//! ```text
//! mkdir -p ~/.axterminator/models
//! curl -L https://github.com/snakers4/silero-vad/raw/master/files/silero_vad.onnx \
//!      -o ~/.axterminator/models/silero_vad.onnx
//! ```
//!
//! Override the default path with `AXTERMINATOR_VAD_MODEL`.
//!
//! ## Threshold
//!
//! Speech probability ≥ `threshold` (default `0.5`, env `AXTERMINATOR_VAD_THRESHOLD`)
//! is classified as speech.  Lower values increase sensitivity (more false positives);
//! higher values reduce sensitivity (more false negatives).
//!
//! ## Performance
//!
//! Silero VAD is ~1.8 MB and runs in <2 ms for 1 second of audio at 16 kHz on
//! modern hardware.  The ONNX session is initialised lazily on first call and
//! cached for the process lifetime.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;
use tracing::debug;

use super::AudioError;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default speech probability threshold.
const DEFAULT_THRESHOLD: f32 = 0.5;

/// Silero VAD LSTM hidden/cell state flat length: 2 × 1 × 64.
const STATE_LEN: usize = 2 * 64;

// ---------------------------------------------------------------------------
// Process-lifetime session cache
// ---------------------------------------------------------------------------

static VAD_SESSION: OnceLock<Mutex<VadSession>> = OnceLock::new();

/// Stateful Silero VAD ONNX session.
///
/// The LSTM carries `h`/`c` state between consecutive `compute` calls so that
/// VAD operates correctly on a stream of overlapping audio windows.  Call
/// [`reset`][VadSession::reset] between unrelated audio segments.
struct VadSession {
    session: Session,
    /// Flattened `[2, 1, 64]` LSTM hidden state.
    h: Vec<f32>,
    /// Flattened `[2, 1, 64]` LSTM cell state.
    c: Vec<f32>,
}

impl VadSession {
    fn new(model_path: &std::path::Path) -> Result<Self, AudioError> {
        let session = Session::builder()
            .map_err(|e| AudioError::Framework(format!("ORT builder: {e}")))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| AudioError::Framework(format!("ORT opt level: {e}")))?
            .with_intra_threads(1)
            .map_err(|e| AudioError::Framework(format!("ORT intra threads: {e}")))?
            .commit_from_file(model_path)
            .map_err(|e| AudioError::Framework(format!("ORT load VAD model: {e}")))?;
        Ok(Self {
            session,
            h: vec![0.0f32; STATE_LEN],
            c: vec![0.0f32; STATE_LEN],
        })
    }

    /// Run one Silero VAD forward pass and return the speech probability.
    fn compute(&mut self, samples: &[f32], sample_rate: u32) -> Result<f32, AudioError> {
        let n = samples.len();
        let sr_val = i64::from(sample_rate);

        let input = Tensor::<f32>::from_array(([1usize, n], samples.to_vec().into_boxed_slice()))
            .map_err(|e| AudioError::Framework(format!("VAD input tensor: {e}")))?;
        let sr = Tensor::<i64>::from_array(([1usize], vec![sr_val].into_boxed_slice()))
            .map_err(|e| AudioError::Framework(format!("VAD sr tensor: {e}")))?;
        let h = Tensor::<f32>::from_array(([2usize, 1, 64], self.h.clone().into_boxed_slice()))
            .map_err(|e| AudioError::Framework(format!("VAD h tensor: {e}")))?;
        let c = Tensor::<f32>::from_array(([2usize, 1, 64], self.c.clone().into_boxed_slice()))
            .map_err(|e| AudioError::Framework(format!("VAD c tensor: {e}")))?;

        let outputs = self
            .session
            .run(ort::inputs!["input" => input, "sr" => sr, "h" => h, "c" => c])
            .map_err(|e| AudioError::Framework(format!("VAD inference: {e}")))?;

        // Update stateful LSTM tensors.
        self.h = extract_f32_vec(&outputs, "hn")?;
        self.c = extract_f32_vec(&outputs, "cn")?;

        // Extract scalar speech probability from output[0][0].
        let (_, prob_data) = outputs
            .get("output")
            .ok_or_else(|| AudioError::Framework("VAD missing 'output' tensor".to_string()))?
            .try_extract_tensor::<f32>()
            .map_err(|e| AudioError::Framework(format!("VAD output extract: {e}")))?;

        Ok(prob_data.first().copied().unwrap_or(0.0))
    }

    fn reset(&mut self) {
        self.h.fill(0.0);
        self.c.fill(0.0);
    }
}

/// Extract a flat `Vec<f32>` from a named output tensor.
fn extract_f32_vec(
    outputs: &ort::session::SessionOutputs<'_>,
    name: &str,
) -> Result<Vec<f32>, AudioError> {
    let (_, data) = outputs
        .get(name)
        .ok_or_else(|| AudioError::Framework(format!("VAD missing '{name}' tensor")))?
        .try_extract_tensor::<f32>()
        .map_err(|e| AudioError::Framework(format!("VAD '{name}' extract: {e}")))?;
    Ok(data.to_vec())
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Voice activity detector that gates audio through a Silero speech probability check.
///
/// `VadDetector` is `Clone` and cheap to construct — it holds only a threshold
/// value.  The underlying ONNX session is shared process-wide via a
/// [`OnceLock`] and loaded lazily on first use.
///
/// # Feature gate
///
/// This type is only available with `--features vad`.
///
/// # Examples
///
/// ```no_run
/// # #[cfg(feature = "vad")]
/// # {
/// use axterminator::audio::vad::VadDetector;
///
/// let vad = VadDetector::from_env();
/// let silence = vec![0.0f32; 16_000];
/// // Model absent in typical CI — returns true (pass-through).
/// let _ = vad.has_speech(&silence, 16_000);
/// # }
/// ```
#[derive(Clone)]
pub struct VadDetector {
    threshold: f32,
}

impl VadDetector {
    /// Create a `VadDetector` with an explicit speech probability threshold.
    ///
    /// `threshold` should be in `(0.0, 1.0)`.  The recommended default is `0.5`.
    #[must_use]
    pub fn new(threshold: f32) -> Self {
        Self { threshold }
    }

    /// Create a `VadDetector` reading threshold from environment variables.
    ///
    /// | Variable                     | Default |
    /// |------------------------------|---------|
    /// | `AXTERMINATOR_VAD_THRESHOLD` | `0.5`   |
    #[must_use]
    pub fn from_env() -> Self {
        let threshold = std::env::var("AXTERMINATOR_VAD_THRESHOLD")
            .ok()
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(DEFAULT_THRESHOLD);
        Self::new(threshold)
    }

    /// Return `true` when `samples` contains speech above the configured threshold.
    ///
    /// The Silero VAD ONNX session is initialised on the first call and cached
    /// for the process lifetime.  When the model file is absent this function
    /// returns `Ok(true)` (pass-through) so the caller degrades gracefully
    /// instead of erroring out.
    ///
    /// # Arguments
    ///
    /// * `samples` — normalised f32 PCM in `[-1.0, 1.0]`.
    /// * `sample_rate` — must be `8_000` or `16_000`.
    ///
    /// # Errors
    ///
    /// Returns [`AudioError::Framework`] when the session exists but inference fails.
    pub fn has_speech(&self, samples: &[f32], sample_rate: u32) -> Result<bool, AudioError> {
        if samples.is_empty() {
            return Ok(false);
        }

        let model_path = vad_model_path()?;

        if !model_path.exists() {
            debug!(path = %model_path.display(), "VAD model absent — pass-through");
            return Ok(true);
        }

        // Stable initialisation pattern: get existing session or initialise once.
        if VAD_SESSION.get().is_none() {
            let session = VadSession::new(&model_path)?;
            let _ = VAD_SESSION.set(Mutex::new(session));
        }
        let session_mutex = VAD_SESSION
            .get()
            .expect("VAD session was just initialised above");

        let prob = session_mutex
            .lock()
            .map_err(|_| AudioError::Framework("VAD session mutex poisoned".to_string()))?
            .compute(samples, sample_rate)?;

        debug!(prob, threshold = self.threshold, "VAD speech probability");
        Ok(prob >= self.threshold)
    }

    /// Reset the LSTM state of the shared session.
    ///
    /// Call between unrelated audio streams to prevent stale LSTM state from
    /// affecting speech probability estimates.  No-op when the session has not
    /// yet been initialised.
    pub fn reset_state(&self) {
        if let Some(mutex) = VAD_SESSION.get() {
            if let Ok(mut guard) = mutex.lock() {
                guard.reset();
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

/// Resolve the Silero VAD model path.
///
/// Priority: `AXTERMINATOR_VAD_MODEL` env var → `~/.axterminator/models/silero_vad.onnx`.
///
/// # Errors
///
/// Returns [`AudioError::Transcription`] when `$HOME` is unset.
pub(crate) fn vad_model_path() -> Result<PathBuf, AudioError> {
    if let Ok(path) = std::env::var("AXTERMINATOR_VAD_MODEL") {
        return Ok(PathBuf::from(path));
    }
    let home = std::env::var("HOME").map_err(|_| {
        AudioError::Transcription("Cannot determine $HOME for VAD model path".to_string())
    })?;
    Ok(PathBuf::from(home)
        .join(".axterminator")
        .join("models")
        .join("silero_vad.onnx"))
}

/// Return `true` when the Silero VAD model file is present on disk.
#[must_use]
pub fn model_file_present() -> bool {
    vad_model_path().map(|p| p.exists()).unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // VadDetector construction
    // -----------------------------------------------------------------------

    #[test]
    fn vad_detector_new_stores_threshold() {
        // GIVEN: an explicit threshold of 0.7
        let vad = VadDetector::new(0.7);
        // THEN: the stored threshold matches
        assert!((vad.threshold - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn vad_detector_from_env_uses_default_when_var_absent() {
        // GIVEN: env var not set
        std::env::remove_var("AXTERMINATOR_VAD_THRESHOLD");
        // WHEN: constructed from env
        let vad = VadDetector::from_env();
        // THEN: threshold is the default 0.5
        assert!(
            (vad.threshold - DEFAULT_THRESHOLD).abs() < f32::EPSILON,
            "expected {DEFAULT_THRESHOLD}, got {}",
            vad.threshold
        );
    }

    #[test]
    fn vad_detector_from_env_reads_custom_threshold() {
        // GIVEN: threshold override
        std::env::set_var("AXTERMINATOR_VAD_THRESHOLD", "0.3");
        let vad = VadDetector::from_env();
        // THEN: threshold reflects env var
        assert!((vad.threshold - 0.3).abs() < f32::EPSILON);
        std::env::remove_var("AXTERMINATOR_VAD_THRESHOLD");
    }

    #[test]
    fn vad_detector_clone_preserves_threshold() {
        // GIVEN: a detector
        let vad = VadDetector::new(0.6);
        // WHEN: cloned
        let cloned = vad.clone();
        // THEN: clone has the same threshold
        assert!((cloned.threshold - 0.6).abs() < f32::EPSILON);
    }

    // -----------------------------------------------------------------------
    // has_speech — no model (graceful degradation path)
    // -----------------------------------------------------------------------

    #[test]
    fn has_speech_empty_samples_returns_false_without_model() {
        // GIVEN: empty audio slice
        let vad = VadDetector::new(0.5);
        // WHEN: queried with zero samples
        // THEN: returns false immediately (no model load attempted)
        assert_eq!(vad.has_speech(&[], 16_000).unwrap_or(true), false);
    }

    #[test]
    fn has_speech_silence_passes_through_when_model_absent() {
        // GIVEN: no VAD model (point to guaranteed non-existent path)
        std::env::set_var("AXTERMINATOR_VAD_MODEL", "/nonexistent_vad_model.onnx");
        let vad = VadDetector::new(0.5);
        let silence = vec![0.0f32; 16_000];
        // WHEN: queried with silence and no model on disk
        let result = vad.has_speech(&silence, 16_000);
        // THEN: returns true (pass-through — no gate applied, no error)
        assert_eq!(result.unwrap_or(false), true, "should pass through when model absent");
        std::env::remove_var("AXTERMINATOR_VAD_MODEL");
    }

    #[test]
    fn has_speech_sine_wave_passes_through_when_model_absent() {
        // GIVEN: 440 Hz sine wave (plausible speech-frequency content)
        std::env::set_var("AXTERMINATOR_VAD_MODEL", "/nonexistent_vad_model.onnx");
        let vad = VadDetector::new(0.5);
        let sine: Vec<f32> = (0..16_000)
            .map(|i| (i as f32 * 2.0 * std::f32::consts::PI * 440.0 / 16_000.0).sin())
            .collect();
        // WHEN: queried — model missing, result is pass-through
        let result = vad.has_speech(&sine, 16_000);
        // THEN: does not panic and returns Ok
        assert!(result.is_ok(), "should not error when model absent");
        std::env::remove_var("AXTERMINATOR_VAD_MODEL");
    }

    // -----------------------------------------------------------------------
    // Model path resolution
    // -----------------------------------------------------------------------

    #[test]
    fn vad_model_path_uses_env_override() {
        // GIVEN: custom model path via env
        std::env::set_var("AXTERMINATOR_VAD_MODEL", "/custom/silero.onnx");
        let path = vad_model_path().unwrap();
        // THEN: env path is returned verbatim
        assert_eq!(path, PathBuf::from("/custom/silero.onnx"));
        std::env::remove_var("AXTERMINATOR_VAD_MODEL");
    }

    #[test]
    fn vad_model_path_default_ends_with_expected_filename() {
        // GIVEN: no env override
        std::env::remove_var("AXTERMINATOR_VAD_MODEL");
        let path = vad_model_path().unwrap();
        // THEN: path ends with the standard model filename
        assert!(
            path.ends_with(".axterminator/models/silero_vad.onnx"),
            "unexpected default path: {path:?}"
        );
    }

    #[test]
    fn model_file_present_returns_bool_without_panic() {
        // GIVEN: any environment (model may or may not exist)
        // THEN: function returns without panic
        let _ = model_file_present();
    }
}
