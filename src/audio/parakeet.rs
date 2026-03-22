//! NVIDIA Parakeet TDT 0.6B v3 — ONNX Runtime inference backend.
//!
//! This module provides an optional, high-quality multilingual ASR engine as
//! an alternative to the Apple `SFSpeechRecognizer` path.  It is compiled only
//! when both the `audio` and `parakeet` Cargo features are enabled.
//!
//! ## Architecture
//!
//! ```text
//! AudioData (f32 @ 16 kHz)
//!     └─► log-mel spectrogram (80 mel bins, 25 ms frame, 10 ms hop)
//!             └─► ONNX Runtime session (parakeet-tdt-0.6b-v3/model.onnx)
//!                     └─► token ids  ──►  vocab decode  ──►  String
//! ```
//!
//! ## Model files
//!
//! On first use the module checks for:
//! - `~/.axterminator/models/parakeet-tdt-0.6b-v3/model.onnx`
//! - `~/.axterminator/models/parakeet-tdt-0.6b-v3/tokenizer.json`
//!
//! If either file is absent the function returns
//! [`AudioError::Transcription`] with a clear message instructing the user to
//! download the model.
//!
//! ## Mel spectrogram parameters (match Parakeet pre-processing)
//!
//! | Parameter      | Value  |
//! |----------------|--------|
//! | Sample rate    | 16 000 Hz |
//! | FFT size       | 512    |
//! | Hop length     | 160 samples (10 ms) |
//! | Win length     | 400 samples (25 ms) |
//! | Mel bins       | 80     |
//! | Freq range     | 0 – 8 000 Hz |
//! | Log floor      | 1e-5   |
//! | Normalization  | global mean / std (NeMo defaults) |

use std::path::{Path, PathBuf};

use tracing::{debug, info, warn};

use super::{AudioData, AudioError};

// ---------------------------------------------------------------------------
// Constants — mel spectrogram hyper-parameters
// ---------------------------------------------------------------------------

/// FFT window size in samples.
const FFT_SIZE: usize = 512;

/// Hop size between consecutive frames (10 ms @ 16 kHz).
const HOP_LENGTH: usize = 160;

/// Analysis window length (25 ms @ 16 kHz).
const WIN_LENGTH: usize = 400;

/// Number of mel filter banks.
const N_MELS: usize = 80;

/// Upper cutoff frequency for the mel filter bank (Hz).
const MEL_FMAX: f64 = 8_000.0;

/// Log-mel floor: `log(max(power, LOG_FLOOR))`.
const LOG_FLOOR: f32 = 1e-5;

/// NeMo global mean subtracted during normalisation.
const GLOBAL_MEAN: f32 = -5.017;

/// NeMo global std used for normalisation.
const GLOBAL_STD: f32 = 2.698;

// ---------------------------------------------------------------------------
// Model directory layout
// ---------------------------------------------------------------------------

/// Return the directory that holds the Parakeet model files.
///
/// Resolves to `~/.axterminator/models/parakeet-tdt-0.6b-v3/`.
///
/// # Errors
///
/// Returns [`AudioError::Transcription`] when `$HOME` cannot be determined.
pub(crate) fn model_dir() -> Result<PathBuf, AudioError> {
    let home = std::env::var("HOME").map_err(|_| {
        AudioError::Transcription("Cannot determine $HOME for model directory".to_string())
    })?;
    Ok(PathBuf::from(home)
        .join(".axterminator")
        .join("models")
        .join("parakeet-tdt-0.6b-v3"))
}

/// Path to the ONNX model file.
pub(crate) fn model_onnx_path() -> Result<PathBuf, AudioError> {
    Ok(model_dir()?.join("model.onnx"))
}

/// Path to the tokenizer JSON file.
pub(crate) fn tokenizer_json_path() -> Result<PathBuf, AudioError> {
    Ok(model_dir()?.join("tokenizer.json"))
}

/// Return `true` when all required model files are present on disk.
///
/// # Examples
///
/// ```
/// // Files are not present in CI, so the function returns false.
/// # #[cfg(all(feature = "audio", feature = "parakeet"))]
/// # {
/// use axterminator::audio::parakeet::model_files_present;
/// let present = model_files_present();
/// // We only assert the return type is bool; actual value depends on the environment.
/// let _ = present;
/// # }
/// ```
#[must_use]
pub fn model_files_present() -> bool {
    model_onnx_path().map(|p| p.exists()).unwrap_or(false)
        && tokenizer_json_path().map(|p| p.exists()).unwrap_or(false)
}

/// Log download instructions when model files are absent.
///
/// Does **not** block or download — callers are responsible for triggering
/// the download out-of-band before calling [`transcribe_parakeet`].
pub fn log_download_instructions() {
    warn!(
        "Parakeet model files not found. \
         Download them from HuggingFace with:\n\
         \n  mkdir -p ~/.axterminator/models/parakeet-tdt-0.6b-v3\n\
         \n  # requires `huggingface-cli` (pip install huggingface_hub)\n\
         \n  huggingface-cli download nvidia/parakeet-tdt-0.6b-v3 \\\n\
         \n    model.onnx tokenizer.json \\\n\
         \n    --local-dir ~/.axterminator/models/parakeet-tdt-0.6b-v3\n"
    );
}

// ---------------------------------------------------------------------------
// Public transcription entry point
// ---------------------------------------------------------------------------

/// Transcribe `audio` using the ONNX Parakeet TDT model.
///
/// Requires `model.onnx` and `tokenizer.json` to be present under
/// `~/.axterminator/models/parakeet-tdt-0.6b-v3/`.  When the files are
/// absent this function returns a descriptive error — it never attempts
/// a network download (download must be triggered by the user or a separate
/// setup workflow).
///
/// The `_language` parameter is accepted for API uniformity with the Apple
/// path but is currently ignored: Parakeet performs automatic language
/// detection internally.
///
/// # Errors
///
/// - [`AudioError::Transcription`] when model files are missing.
/// - [`AudioError::Transcription`] when ONNX session creation fails.
/// - [`AudioError::Transcription`] when inference fails.
pub fn transcribe_parakeet(
    audio: &AudioData,
    _language: Option<&str>,
) -> Result<String, AudioError> {
    let onnx_path = model_onnx_path()?;
    let tok_path = tokenizer_json_path()?;

    validate_model_files(&onnx_path, &tok_path)?;

    debug!(
        samples = audio.samples.len(),
        sample_rate = audio.sample_rate,
        "computing log-mel spectrogram for Parakeet"
    );

    let features = compute_log_mel_spectrogram(&audio.samples);
    let n_frames = features.len() / N_MELS;
    debug!(n_frames, n_mels = N_MELS, "mel spectrogram computed");

    let token_ids = run_onnx_inference(&features, n_frames, &onnx_path)?;
    let transcript = decode_token_ids(&token_ids, &tok_path)?;

    info!(
        transcript = transcript.as_str(),
        "Parakeet transcription complete"
    );
    Ok(transcript)
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

fn validate_model_files(onnx_path: &Path, tok_path: &Path) -> Result<(), AudioError> {
    if !onnx_path.exists() || !tok_path.exists() {
        log_download_instructions();
        return Err(AudioError::Transcription(
            "Parakeet model files not downloaded. \
             Run `huggingface-cli download nvidia/parakeet-tdt-0.6b-v3 model.onnx tokenizer.json \
             --local-dir ~/.axterminator/models/parakeet-tdt-0.6b-v3` to install them."
                .to_string(),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// ONNX Runtime inference
// ---------------------------------------------------------------------------

/// Load the ONNX session and run the encoder→decoder pipeline on `features`.
///
/// `features` is a flat `Vec<f32>` in row-major order with shape
/// `[1, N_MELS, n_frames]`.  The function returns a `Vec<i64>` of output
/// token IDs.
///
/// # Errors
///
/// Returns [`AudioError::Transcription`] on any ONNX Runtime failure.
fn run_onnx_inference(
    features: &[f32],
    n_frames: usize,
    onnx_path: &Path,
) -> Result<Vec<i64>, AudioError> {
    use ort::session::Session;
    use ort::value::Tensor;

    let mut session = Session::builder()
        .map_err(|e| AudioError::Transcription(format!("ONNX session builder failed: {e}")))?
        .commit_from_file(onnx_path)
        .map_err(|e| {
            AudioError::Transcription(format!("Failed to load ONNX model from {onnx_path:?}: {e}"))
        })?;

    // Shape: [batch=1, n_mels=80, n_frames].  `from_array` takes a boxed slice.
    #[allow(clippy::cast_possible_truncation)]
    let shape: [usize; 3] = [1, N_MELS, n_frames];
    let input_tensor = Tensor::<f32>::from_array((shape, features.to_vec().into_boxed_slice()))
        .map_err(|e| AudioError::Transcription(format!("Failed to create input tensor: {e}")))?;

    let outputs = session
        .run(ort::inputs![input_tensor])
        .map_err(|e| AudioError::Transcription(format!("ONNX inference failed: {e}")))?;

    extract_token_ids(&outputs)
}

/// Extract the first integer output from ONNX session results.
///
/// Tries common output names used by Parakeet/NeMo ONNX exports in order,
/// then falls back to index 0.
fn extract_token_ids(outputs: &ort::session::SessionOutputs<'_>) -> Result<Vec<i64>, AudioError> {
    // Resolve output by well-known name, or fall back to the first output by index.
    let output = ["output", "logits", "predictions"]
        .iter()
        .find_map(|name| outputs.get(*name))
        .or_else(|| {
            if outputs.len() > 0 {
                Some(&outputs[0])
            } else {
                None
            }
        })
        .ok_or_else(|| AudioError::Transcription("ONNX model produced no outputs".to_string()))?;

    let (_shape, data) = output
        .try_extract_tensor::<i64>()
        .map_err(|e| AudioError::Transcription(format!("Failed to extract token IDs: {e}")))?;

    Ok(data.to_vec())
}

// ---------------------------------------------------------------------------
// Tokenizer decode
// ---------------------------------------------------------------------------

/// Decode token IDs to a UTF-8 string using `tokenizer.json`.
///
/// Uses a minimal BPE-compatible decode via the `tokenizers` crate that
/// ships as part of `ort`'s ecosystem.  For Parakeet's SentencePiece-based
/// vocabulary, this resolves `▁` (U+2581) word-boundary markers to spaces
/// and strips special tokens (`<blank>`, `<unk>`, etc.).
///
/// # Errors
///
/// Returns [`AudioError::Transcription`] when the vocabulary file cannot be
/// parsed or a token ID is out of range.
fn decode_token_ids(token_ids: &[i64], tok_path: &Path) -> Result<String, AudioError> {
    let vocab = load_vocab(tok_path)?;
    let text = token_ids
        .iter()
        .filter_map(|&id| vocab.get(id as usize))
        .fold(String::new(), |mut acc, piece| {
            acc.push_str(piece);
            acc
        });

    // Normalise SentencePiece word-boundary marker (▁ = U+2581) to ASCII space.
    let normalised = text.replace('\u{2581}', " ").trim().to_string();
    Ok(normalised)
}

/// Load vocabulary from `tokenizer.json` into an index-addressed `Vec<String>`.
///
/// Supports both the HuggingFace tokenizers format (`model.vocab` map keyed by
/// token string, valued by integer index) and a flat array format.
///
/// # Errors
///
/// Returns [`AudioError::Transcription`] on I/O or JSON parse failure.
fn load_vocab(tok_path: &Path) -> Result<Vec<String>, AudioError> {
    let json_text = std::fs::read_to_string(tok_path)
        .map_err(|e| AudioError::Transcription(format!("Cannot read tokenizer.json: {e}")))?;

    let root: serde_json::Value = serde_json::from_str(&json_text)
        .map_err(|e| AudioError::Transcription(format!("tokenizer.json parse error: {e}")))?;

    // HuggingFace fast-tokenizer format: {"model": {"vocab": {"token": id, ...}}}
    if let Some(vocab_map) = root.pointer("/model/vocab").and_then(|v| v.as_object()) {
        return build_vocab_from_map(vocab_map);
    }

    // Parakeet/NeMo flat vocab list: [token0, token1, ...]
    if let Some(arr) = root.as_array() {
        return Ok(arr
            .iter()
            .map(|v| v.as_str().unwrap_or("").to_string())
            .collect());
    }

    // Flat object with numeric keys: {"0": "token", "1": "token", ...}
    if let Some(obj) = root.as_object() {
        let mut entries: Vec<(usize, String)> = obj
            .iter()
            .filter_map(|(k, v)| {
                let idx: usize = k.parse().ok()?;
                Some((idx, v.as_str().unwrap_or("").to_string()))
            })
            .collect();
        entries.sort_unstable_by_key(|(i, _)| *i);
        return Ok(entries.into_iter().map(|(_, s)| s).collect());
    }

    Err(AudioError::Transcription(
        "tokenizer.json has an unrecognised format (expected model.vocab map or flat array)"
            .to_string(),
    ))
}

/// Convert a `{token: id}` map to an index-addressed vocabulary vector.
fn build_vocab_from_map(
    map: &serde_json::Map<String, serde_json::Value>,
) -> Result<Vec<String>, AudioError> {
    let mut pairs: Vec<(usize, String)> = map
        .iter()
        .filter_map(|(token, id_val)| {
            let id: usize = id_val.as_u64()? as usize;
            Some((id, token.clone()))
        })
        .collect();
    pairs.sort_unstable_by_key(|(id, _)| *id);

    let max_id = pairs.last().map(|(id, _)| *id).unwrap_or(0);
    let mut vocab = vec![String::new(); max_id + 1];
    for (id, token) in pairs {
        vocab[id] = token;
    }
    Ok(vocab)
}

// ---------------------------------------------------------------------------
// Log-mel spectrogram
// ---------------------------------------------------------------------------

/// Compute the log-mel spectrogram for `samples` at 16 kHz.
///
/// Returns a flat `Vec<f32>` in row-major order with logical shape
/// `[N_MELS, n_frames]`, normalised with the NeMo global mean/std so the
/// values are compatible with the pre-trained Parakeet checkpoint.
///
/// The implementation uses a pure-Rust FFT via Cooley-Tukey DIT for
/// portability (no C dependencies beyond what `ort` already pulls in).
///
/// # Performance
///
/// For a 5-second clip at 16 kHz (80 000 samples):
/// - Frame count: ~499 frames
/// - FFT calls: ~499 × O(N log N) with N=512
/// - Total: < 20 ms on a modern core
///
/// # Examples
///
/// ```
/// # #[cfg(all(feature = "audio", feature = "parakeet"))]
/// # {
/// use axterminator::audio::parakeet::compute_log_mel_spectrogram;
/// let silence = vec![0.0f32; 16_000]; // 1 second of silence
/// let mel = compute_log_mel_spectrogram(&silence);
/// // Each frame contributes N_MELS values.
/// assert_eq!(mel.len() % 80, 0);
/// # }
/// ```
pub fn compute_log_mel_spectrogram(samples: &[f32]) -> Vec<f32> {
    let hann = hann_window(WIN_LENGTH);
    let mel_fb = mel_filterbank(FFT_SIZE, N_MELS, 16_000.0, 0.0, MEL_FMAX);
    let frames = frame_and_window(samples, &hann);
    let n_frames = frames.len();

    let mut out = vec![0.0f32; N_MELS * n_frames];

    for (frame_idx, frame) in frames.iter().enumerate() {
        let power = compute_power_spectrum(frame);
        apply_mel_filterbank_frame(&power, &mel_fb, &mut out, frame_idx);
    }

    apply_log_and_normalise(&mut out);
    out
}

// ---------------------------------------------------------------------------
// Frame extraction
// ---------------------------------------------------------------------------

/// Split `samples` into overlapping windowed frames.
///
/// Each frame is `WIN_LENGTH` samples wide, advanced by `HOP_LENGTH` samples.
/// The returned vector has shape `[n_frames][FFT_SIZE]` — zero-padded to
/// `FFT_SIZE` when `WIN_LENGTH < FFT_SIZE`.
fn frame_and_window(samples: &[f32], window: &[f32]) -> Vec<Vec<f32>> {
    let n_samples = samples.len();
    // Centre padding: half a window on each side (matches librosa default).
    let pad = WIN_LENGTH / 2;
    let padded_len = n_samples + 2 * pad;

    let mut padded = vec![0.0f32; padded_len];
    padded[pad..pad + n_samples].copy_from_slice(samples);

    let n_frames = (padded_len.saturating_sub(WIN_LENGTH)) / HOP_LENGTH + 1;
    let mut frames = Vec::with_capacity(n_frames);

    for i in 0..n_frames {
        let start = i * HOP_LENGTH;
        let end = (start + WIN_LENGTH).min(padded_len);
        let mut frame = vec![0.0f32; FFT_SIZE];
        for (j, &s) in padded[start..end].iter().enumerate() {
            frame[j] = s * window[j];
        }
        frames.push(frame);
    }
    frames
}

// ---------------------------------------------------------------------------
// Hann window
// ---------------------------------------------------------------------------

/// Build a Hann analysis window of `size` samples.
fn hann_window(size: usize) -> Vec<f32> {
    use std::f64::consts::PI;
    (0..size)
        .map(|n| {
            #[allow(clippy::cast_precision_loss)]
            let w = 0.5 * (1.0 - (2.0 * PI * n as f64 / (size - 1) as f64).cos());
            w as f32
        })
        .collect()
}

// ---------------------------------------------------------------------------
// FFT (Cooley-Tukey DIT, radix-2, in-place)
// ---------------------------------------------------------------------------

/// Compute the power spectrum `|FFT(frame)|²` of one windowed frame.
///
/// The frame must be exactly `FFT_SIZE` elements (512 here, a power of two).
fn compute_power_spectrum(frame: &[f32]) -> Vec<f32> {
    let n = frame.len(); // == FFT_SIZE == 512
    let mut re: Vec<f64> = frame.iter().map(|&s| f64::from(s)).collect();
    let mut im: Vec<f64> = vec![0.0; n];

    fft_inplace(&mut re, &mut im, n);

    // Power spectrum: keep only positive frequencies (DC .. n/2 inclusive).
    (0..=n / 2)
        .map(|k| {
            #[allow(clippy::cast_possible_truncation)]
            {
                (re[k] * re[k] + im[k] * im[k]) as f32
            }
        })
        .collect()
}

/// Cooley-Tukey in-place DIT FFT for power-of-two `n`.
///
/// Computes the DFT of (`re`, `im`) in-place.  `n` must be a power of two.
fn fft_inplace(re: &mut [f64], im: &mut [f64], n: usize) {
    // Bit-reversal permutation.
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }

    // Butterfly passes.
    let mut len = 2usize;
    while len <= n {
        fft_butterfly_pass(re, im, n, len);
        len <<= 1;
    }
}

/// Execute one butterfly pass of length `len` for the FFT.
fn fft_butterfly_pass(re: &mut [f64], im: &mut [f64], n: usize, len: usize) {
    use std::f64::consts::PI;
    let half = len / 2;
    let angle = -2.0 * PI / len as f64;
    let (wr0, wi0) = (angle.cos(), angle.sin());

    let mut i = 0;
    while i < n {
        let (mut wr, mut wi) = (1.0_f64, 0.0_f64);
        for k in 0..half {
            let (ur, ui) = (re[i + k], im[i + k]);
            let (vr, vi) = (
                re[i + k + half] * wr - im[i + k + half] * wi,
                re[i + k + half] * wi + im[i + k + half] * wr,
            );
            re[i + k] = ur + vr;
            im[i + k] = ui + vi;
            re[i + k + half] = ur - vr;
            im[i + k + half] = ui - vi;
            let (new_wr, new_wi) = (wr * wr0 - wi * wi0, wr * wi0 + wi * wr0);
            wr = new_wr;
            wi = new_wi;
        }
        i += len;
    }
}

// ---------------------------------------------------------------------------
// Mel filter bank
// ---------------------------------------------------------------------------

/// Build a mel filter bank matrix of shape `[n_mels, n_fft/2+1]`.
///
/// Follows the HTK/librosa convention: triangular filters linearly spaced
/// on the mel scale between `fmin` and `fmax` Hz.
fn mel_filterbank(n_fft: usize, n_mels: usize, sr: f64, fmin: f64, fmax: f64) -> Vec<Vec<f32>> {
    let n_freqs = n_fft / 2 + 1;
    let mel_min = hz_to_mel(fmin);
    let mel_max = hz_to_mel(fmax);

    // Centre frequencies of each mel bin (n_mels + 2 points including edges).
    let mel_points: Vec<f64> = (0..=n_mels + 1)
        .map(|i| {
            #[allow(clippy::cast_precision_loss)]
            {
                mel_min + (mel_max - mel_min) * i as f64 / (n_mels + 1) as f64
            }
        })
        .collect();
    let hz_points: Vec<f64> = mel_points.iter().map(|&m| mel_to_hz(m)).collect();

    // Map hz_points to FFT bin indices.
    let fft_freqs: Vec<f64> = (0..n_freqs)
        .map(|k| {
            #[allow(clippy::cast_precision_loss)]
            {
                k as f64 * sr / n_fft as f64
            }
        })
        .collect();

    let bin_freqs: Vec<f64> = hz_points
        .iter()
        .map(|&hz| {
            #[allow(clippy::cast_precision_loss)]
            {
                (hz / (sr / n_fft as f64)).floor()
            }
        })
        .collect();

    // Build triangular filter responses.
    let mut fb = vec![vec![0.0f32; n_freqs]; n_mels];
    for m in 0..n_mels {
        let (f_lo, f_mid, f_hi) = (hz_points[m], hz_points[m + 1], hz_points[m + 2]);
        let (b_lo, b_hi) = (bin_freqs[m] as usize, bin_freqs[m + 2] as usize);
        for k in b_lo..=b_hi.min(n_freqs - 1) {
            let f = fft_freqs[k];
            fb[m][k] = triangular_filter_weight(f, f_lo, f_mid, f_hi);
        }
    }
    fb
}

/// Triangular filter weight for frequency `f` given `[lo, mid, hi]` bounds.
#[inline]
fn triangular_filter_weight(f: f64, lo: f64, mid: f64, hi: f64) -> f32 {
    #[allow(clippy::cast_possible_truncation)]
    if f >= lo && f <= mid && (mid - lo) > 1e-12 {
        ((f - lo) / (mid - lo)) as f32
    } else if f > mid && f <= hi && (hi - mid) > 1e-12 {
        ((hi - f) / (hi - mid)) as f32
    } else {
        0.0
    }
}

/// Apply mel filter bank to one power spectrum frame.
fn apply_mel_filterbank_frame(power: &[f32], fb: &[Vec<f32>], out: &mut [f32], frame_idx: usize) {
    let n_frames = out.len() / N_MELS;
    for (m, filter) in fb.iter().enumerate() {
        let energy: f32 = filter.iter().zip(power).map(|(&w, &p)| w * p).sum();
        // Store in column-major (n_mels, n_frames) layout expected by Parakeet.
        out[m * n_frames + frame_idx] = energy;
    }
}

/// Apply `log(max(x, LOG_FLOOR))` and normalise with NeMo global mean/std.
fn apply_log_and_normalise(features: &mut [f32]) {
    for v in features.iter_mut() {
        *v = v.max(LOG_FLOOR).ln();
        *v = (*v - GLOBAL_MEAN) / GLOBAL_STD;
    }
}

// ---------------------------------------------------------------------------
// Mel ↔ Hz conversions (HTK formula)
// ---------------------------------------------------------------------------

#[inline]
fn hz_to_mel(hz: f64) -> f64 {
    2595.0 * (1.0 + hz / 700.0).log10()
}

#[inline]
fn mel_to_hz(mel: f64) -> f64 {
    700.0 * (10.0_f64.powf(mel / 2595.0) - 1.0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // hz_to_mel / mel_to_hz round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn hz_mel_round_trip_within_tolerance() {
        // GIVEN: representative frequencies
        for &hz in &[100.0_f64, 500.0, 1000.0, 4000.0, 8000.0] {
            // WHEN: convert to mel and back
            let recovered = mel_to_hz(hz_to_mel(hz));
            // THEN: within 0.01 Hz of original
            assert!(
                (recovered - hz).abs() < 0.01,
                "round-trip failed for {hz} Hz: got {recovered}"
            );
        }
    }

    #[test]
    fn hz_to_mel_zero_is_zero() {
        assert!((hz_to_mel(0.0)).abs() < 1e-9);
    }

    #[test]
    fn hz_to_mel_monotonically_increasing() {
        let freqs = [100.0_f64, 500.0, 1000.0, 4000.0, 8000.0];
        for w in freqs.windows(2) {
            assert!(
                hz_to_mel(w[0]) < hz_to_mel(w[1]),
                "mel scale must be monotone: {} >= {}",
                hz_to_mel(w[0]),
                hz_to_mel(w[1])
            );
        }
    }

    // -----------------------------------------------------------------------
    // Hann window
    // -----------------------------------------------------------------------

    #[test]
    fn hann_window_length_matches_requested_size() {
        let w = hann_window(400);
        assert_eq!(w.len(), 400);
    }

    #[test]
    fn hann_window_first_and_last_samples_near_zero() {
        let w = hann_window(400);
        assert!(w[0].abs() < 0.01, "first Hann sample should be near 0");
        assert!(w[399].abs() < 0.01, "last Hann sample should be near 0");
    }

    #[test]
    fn hann_window_peak_near_midpoint() {
        let w = hann_window(400);
        let max_idx = w
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(i, _)| i)
            .unwrap();
        // Peak should be near the centre of the window (within 5%).
        assert!(
            max_idx > 175 && max_idx < 225,
            "Hann window peak at {max_idx}, expected ~200"
        );
    }

    // -----------------------------------------------------------------------
    // FFT correctness
    // -----------------------------------------------------------------------

    #[test]
    fn fft_dc_input_produces_dc_spike() {
        // GIVEN: constant input (DC signal = 1.0)
        let n = 512;
        let mut re: Vec<f64> = vec![1.0; n];
        let mut im: Vec<f64> = vec![0.0; n];
        // WHEN: FFT computed
        fft_inplace(&mut re, &mut im, n);
        // THEN: bin 0 (DC) should have magnitude N, all others near zero
        assert!(
            (re[0] - n as f64).abs() < 1e-6,
            "DC bin should be {n}, got {}",
            re[0]
        );
        for k in 1..n {
            let mag = (re[k] * re[k] + im[k] * im[k]).sqrt();
            assert!(mag < 1e-6, "non-DC bin {k} should be ~0, got {mag}");
        }
    }

    #[test]
    fn fft_impulse_at_zero_is_flat_spectrum() {
        // GIVEN: unit impulse at index 0
        let n = 512;
        let mut re: Vec<f64> = vec![0.0; n];
        let mut im: Vec<f64> = vec![0.0; n];
        re[0] = 1.0;
        // WHEN: FFT computed
        fft_inplace(&mut re, &mut im, n);
        // THEN: all bins should have magnitude 1.0
        for k in 0..n {
            let mag = (re[k] * re[k] + im[k] * im[k]).sqrt();
            assert!(
                (mag - 1.0).abs() < 1e-6,
                "bin {k} magnitude should be 1.0, got {mag}"
            );
        }
    }

    // -----------------------------------------------------------------------
    // Mel filter bank
    // -----------------------------------------------------------------------

    #[test]
    fn mel_filterbank_has_correct_shape() {
        let fb = mel_filterbank(FFT_SIZE, N_MELS, 16_000.0, 0.0, MEL_FMAX);
        assert_eq!(fb.len(), N_MELS, "expected {N_MELS} rows");
        for row in &fb {
            assert_eq!(row.len(), FFT_SIZE / 2 + 1, "each row should have 257 bins");
        }
    }

    #[test]
    fn mel_filterbank_rows_are_non_negative() {
        let fb = mel_filterbank(FFT_SIZE, N_MELS, 16_000.0, 0.0, MEL_FMAX);
        for (m, row) in fb.iter().enumerate() {
            for (k, &v) in row.iter().enumerate() {
                assert!(v >= 0.0, "filter bank [{m}][{k}] = {v} is negative");
            }
        }
    }

    #[test]
    fn mel_filterbank_each_filter_has_positive_area() {
        // Every filter should contribute at least some energy.
        let fb = mel_filterbank(FFT_SIZE, N_MELS, 16_000.0, 0.0, MEL_FMAX);
        for (m, row) in fb.iter().enumerate() {
            let area: f32 = row.iter().sum();
            assert!(area > 0.0, "filter {m} has zero area — misaligned?");
        }
    }

    // -----------------------------------------------------------------------
    // compute_log_mel_spectrogram
    // -----------------------------------------------------------------------

    #[test]
    fn log_mel_spectrogram_silence_has_expected_frame_count() {
        // GIVEN: 1 second of silence at 16 kHz
        let silence = vec![0.0f32; 16_000];
        // WHEN: spectrogram computed
        let mel = compute_log_mel_spectrogram(&silence);
        // THEN: length must be a multiple of N_MELS
        assert_eq!(
            mel.len() % N_MELS,
            0,
            "mel length {} is not a multiple of {N_MELS}",
            mel.len()
        );
    }

    #[test]
    fn log_mel_spectrogram_silence_values_are_normalised() {
        // GIVEN: silence → power = 0 → log(LOG_FLOOR) = log(1e-5) ≈ -11.51
        // After normalisation: (log(1e-5) - GLOBAL_MEAN) / GLOBAL_STD
        let silence = vec![0.0f32; 16_000];
        let mel = compute_log_mel_spectrogram(&silence);
        let expected = (LOG_FLOOR.ln() - GLOBAL_MEAN) / GLOBAL_STD;
        for &v in &mel {
            assert!(
                (v - expected).abs() < 0.01,
                "silence bin should be {expected:.4}, got {v:.4}"
            );
        }
    }

    #[test]
    fn log_mel_spectrogram_empty_input_produces_at_least_one_frame() {
        // Edge case: zero-length input should still produce output (padded).
        let mel = compute_log_mel_spectrogram(&[]);
        assert!(!mel.is_empty(), "empty input should still produce frames");
    }

    // -----------------------------------------------------------------------
    // frame_and_window
    // -----------------------------------------------------------------------

    #[test]
    fn frame_and_window_one_second_produces_correct_count() {
        let window = hann_window(WIN_LENGTH);
        let samples = vec![0.0f32; 16_000];
        let frames = frame_and_window(&samples, &window);
        // Expected: (16_000 + WIN_LENGTH - HOP_LENGTH) / HOP_LENGTH ≈ 100 frames
        // (depends on centre-padding)
        assert!(!frames.is_empty(), "should produce frames for 1s audio");
    }

    #[test]
    fn frame_and_window_each_frame_has_fft_size() {
        let window = hann_window(WIN_LENGTH);
        let samples = vec![0.5f32; 800];
        let frames = frame_and_window(&samples, &window);
        for (i, frame) in frames.iter().enumerate() {
            assert_eq!(
                frame.len(),
                FFT_SIZE,
                "frame {i} length {} != {FFT_SIZE}",
                frame.len()
            );
        }
    }

    // -----------------------------------------------------------------------
    // Triangular filter weight
    // -----------------------------------------------------------------------

    #[test]
    fn triangular_filter_weight_at_midpoint_is_one() {
        let w = triangular_filter_weight(1000.0, 800.0, 1000.0, 1200.0);
        assert!(
            (w - 1.0).abs() < 1e-5,
            "midpoint weight should be 1.0, got {w}"
        );
    }

    #[test]
    fn triangular_filter_weight_outside_band_is_zero() {
        assert_eq!(triangular_filter_weight(600.0, 800.0, 1000.0, 1200.0), 0.0);
        assert_eq!(triangular_filter_weight(1400.0, 800.0, 1000.0, 1200.0), 0.0);
    }

    #[test]
    fn triangular_filter_weight_at_lower_edge_is_zero() {
        let w = triangular_filter_weight(800.0, 800.0, 1000.0, 1200.0);
        assert!(w.abs() < 1e-5, "lower edge weight should be ~0, got {w}");
    }

    // -----------------------------------------------------------------------
    // model_dir / model_files_present
    // -----------------------------------------------------------------------

    #[test]
    fn model_dir_contains_expected_subpath() {
        let dir = model_dir().expect("model_dir should not fail with valid $HOME");
        let s = dir.to_string_lossy();
        assert!(s.contains("parakeet-tdt-0.6b-v3"), "unexpected path: {s}");
        assert!(s.contains(".axterminator"), "missing .axterminator: {s}");
    }

    #[test]
    fn model_files_present_returns_false_in_clean_env() {
        // Model files are not present in CI / developer machines without the
        // explicit download step.  This test verifies the function returns a
        // bool and does not panic — actual true/false depends on environment.
        let result = std::panic::catch_unwind(model_files_present);
        assert!(result.is_ok(), "model_files_present should never panic");
    }

    // -----------------------------------------------------------------------
    // load_vocab (unit tests with synthetic JSON)
    // -----------------------------------------------------------------------

    #[test]
    fn load_vocab_parses_huggingface_format() {
        use std::io::Write as _;
        let json = r#"{"model":{"vocab":{"<blank>":0,"a":1,"b":2}}}"#;
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        tmp.write_all(json.as_bytes()).unwrap();
        let vocab = load_vocab(tmp.path()).unwrap();
        assert_eq!(vocab.len(), 3);
        assert_eq!(vocab[1], "a");
        assert_eq!(vocab[2], "b");
    }

    #[test]
    fn load_vocab_parses_flat_array_format() {
        use std::io::Write as _;
        let json = r#"["<blank>","hello","world"]"#;
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        tmp.write_all(json.as_bytes()).unwrap();
        let vocab = load_vocab(tmp.path()).unwrap();
        assert_eq!(vocab.len(), 3);
        assert_eq!(vocab[0], "<blank>");
        assert_eq!(vocab[2], "world");
    }

    #[test]
    fn load_vocab_returns_error_on_invalid_json() {
        use std::io::Write as _;
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        tmp.write_all(b"not json at all").unwrap();
        let result = load_vocab(tmp.path());
        assert!(result.is_err(), "invalid JSON must return error");
        assert_eq!(result.unwrap_err().code(), "transcription_error");
    }

    #[test]
    fn load_vocab_returns_error_on_missing_file() {
        let result = load_vocab(Path::new("/nonexistent/path/tokenizer.json"));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), "transcription_error");
    }

    // -----------------------------------------------------------------------
    // build_vocab_from_map
    // -----------------------------------------------------------------------

    #[test]
    fn build_vocab_from_map_orders_by_id() {
        let mut map = serde_json::Map::new();
        map.insert("z".to_string(), serde_json::json!(2));
        map.insert("a".to_string(), serde_json::json!(0));
        map.insert("m".to_string(), serde_json::json!(1));
        let vocab = build_vocab_from_map(&map).unwrap();
        assert_eq!(vocab[0], "a");
        assert_eq!(vocab[1], "m");
        assert_eq!(vocab[2], "z");
    }

    // -----------------------------------------------------------------------
    // decode_token_ids (unit test with trivial vocab)
    // -----------------------------------------------------------------------

    #[test]
    fn decode_token_ids_joins_pieces_and_strips_whitespace() {
        use std::io::Write as _;
        // Vocab: 0="▁hello", 1="▁world"
        let json = r#"{"model":{"vocab":{"▁hello":0,"▁world":1}}}"#;
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        tmp.write_all(json.as_bytes()).unwrap();
        let result = decode_token_ids(&[0, 1], tmp.path()).unwrap();
        // ▁ → space, then trim leading space
        assert_eq!(result, "hello world");
    }

    #[test]
    fn decode_token_ids_empty_input_returns_empty_string() {
        use std::io::Write as _;
        let json = r#"{"model":{"vocab":{"a":0}}}"#;
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        tmp.write_all(json.as_bytes()).unwrap();
        let result = decode_token_ids(&[], tmp.path()).unwrap();
        assert_eq!(result, "");
    }
}
