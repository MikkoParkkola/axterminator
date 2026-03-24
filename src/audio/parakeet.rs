//! NVIDIA Parakeet TDT 0.6B v3 — ONNX Runtime inference backend.
//!
//! This module provides an optional, high-quality multilingual ASR engine as
//! an alternative to the Apple `SFSpeechRecognizer` path.  It is compiled only
//! when both the `audio` and `parakeet` Cargo features are enabled.
//!
//! ## Architecture
//!
//! Parakeet TDT is a Token-and-Duration Transducer (TDT) model with three
//! ONNX components:
//!
//! ```text
//! AudioData (f32 @ 16 kHz)
//!     └─► nemo128.onnx (mel-spectrogram preprocessor, 128-dim features)
//!             └─► encoder-model.onnx (Conformer encoder → 1024-dim, /8 subsampling)
//!                     └─► decoder_joint-model.onnx (LSTM decoder + joiner)
//!                             └─► TDT greedy decode → token IDs → vocab decode → String
//! ```
//!
//! ## Model files
//!
//! On first use the module checks for:
//! - `~/.axterminator/models/parakeet-tdt-0.6b-v3/nemo128.onnx`       (preprocessor)
//! - `~/.axterminator/models/parakeet-tdt-0.6b-v3/encoder-model.onnx` (+ `.data` sidecar)
//! - `~/.axterminator/models/parakeet-tdt-0.6b-v3/decoder_joint-model.onnx`
//! - `~/.axterminator/models/parakeet-tdt-0.6b-v3/vocab.txt`
//!
//! If any file is absent the function returns [`AudioError::Transcription`]
//! with a clear message instructing the user to download the model.
//!
//! Download command:
//! ```text
//! pip install huggingface_hub
//! huggingface-cli download istupakov/parakeet-tdt-0.6b-v3-onnx \
//!   encoder-model.onnx encoder-model.onnx.data decoder_joint-model.onnx \
//!   nemo128.onnx vocab.txt config.json \
//!   --local-dir ~/.axterminator/models/parakeet-tdt-0.6b-v3
//! ```
//!
//! ## TDT Decoding
//!
//! The decoder_joint model outputs logits of size `vocab_size + num_durations`
//! (8193 + 5 = 8198).  At each step:
//! - Token logits (0..8192) select the vocabulary token (8192 = blank).
//! - Duration logits (0..4) select how many encoder frames to skip.
//! - If token is blank: advance by max(1, duration).
//! - If token is non-blank: emit token, update decoder state.
//!   If duration > 0, also advance encoder position.

use std::path::{Path, PathBuf};

use tracing::{debug, info, warn};

use super::{AudioData, AudioError};

// ---------------------------------------------------------------------------
// Constants — TDT model parameters
// ---------------------------------------------------------------------------

/// Vocabulary size (tokens 0..8192, where 8192 = blank).
const VOCAB_SIZE: usize = 8193;

/// Blank token ID in the TDT vocabulary.
const BLANK_TOKEN: i32 = 8192;

/// Number of TDT duration classes (durations 0..4).
const NUM_DURATIONS: usize = 5;

/// LSTM hidden dimension in the prediction network.
const PRED_HIDDEN_DIM: usize = 640;

/// Encoder output dimension.
const ENCODER_DIM: usize = 1024;

/// Maximum symbols emitted per encoder frame (prevents infinite loops).
const MAX_SYMBOLS_PER_STEP: usize = 10;

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

/// Path to the mel-spectrogram preprocessor ONNX model.
pub(crate) fn preprocessor_path() -> Result<PathBuf, AudioError> {
    Ok(model_dir()?.join("nemo128.onnx"))
}

/// Path to the Conformer encoder ONNX model.
pub(crate) fn encoder_path() -> Result<PathBuf, AudioError> {
    Ok(model_dir()?.join("encoder-model.onnx"))
}

/// Path to the decoder+joiner ONNX model.
pub(crate) fn decoder_joint_path() -> Result<PathBuf, AudioError> {
    Ok(model_dir()?.join("decoder_joint-model.onnx"))
}

/// Path to the vocabulary text file.
pub(crate) fn vocab_path() -> Result<PathBuf, AudioError> {
    Ok(model_dir()?.join("vocab.txt"))
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
    preprocessor_path().map(|p| p.exists()).unwrap_or(false)
        && encoder_path().map(|p| p.exists()).unwrap_or(false)
        && decoder_joint_path().map(|p| p.exists()).unwrap_or(false)
        && vocab_path().map(|p| p.exists()).unwrap_or(false)
}

/// Log download instructions when model files are absent.
///
/// Does **not** block or download — callers are responsible for triggering
/// the download out-of-band before calling [`transcribe_parakeet`].
pub fn log_download_instructions() {
    warn!(
        "Parakeet model files not found. \
         Download them from HuggingFace with:\n\
         \n  pip install huggingface_hub\n\
         \n  huggingface-cli download istupakov/parakeet-tdt-0.6b-v3-onnx \\\n\
         \n    encoder-model.onnx encoder-model.onnx.data \\\n\
         \n    decoder_joint-model.onnx nemo128.onnx vocab.txt config.json \\\n\
         \n    --local-dir ~/.axterminator/models/parakeet-tdt-0.6b-v3\n"
    );
}

// ---------------------------------------------------------------------------
// Public transcription entry point
// ---------------------------------------------------------------------------

/// Transcribe `audio` using the ONNX Parakeet TDT model.
///
/// Requires all model files to be present under
/// `~/.axterminator/models/parakeet-tdt-0.6b-v3/`.  When the files are
/// absent this function returns a descriptive error — it never attempts
/// a network download.
///
/// The pipeline runs three ONNX models in sequence:
/// 1. `nemo128.onnx` — mel-spectrogram preprocessor (waveform → 128-dim features)
/// 2. `encoder-model.onnx` — Conformer encoder (features → 1024-dim encodings)
/// 3. `decoder_joint-model.onnx` — LSTM decoder + joiner (TDT greedy decoding)
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
    let preproc_path = preprocessor_path()?;
    let enc_path = encoder_path()?;
    let dec_path = decoder_joint_path()?;
    let voc_path = vocab_path()?;

    validate_model_files(&preproc_path, &enc_path, &dec_path, &voc_path)?;

    debug!(
        samples = audio.samples.len(),
        sample_rate = audio.sample_rate,
        "running Parakeet TDT inference pipeline"
    );

    // Step 1: Mel-spectrogram preprocessing
    let (features, feature_len) = run_preprocessor(&audio.samples, &preproc_path)?;
    debug!(feature_len, "preprocessor complete");

    // Step 2: Conformer encoder
    let (enc_out, enc_len) = run_encoder(&features, feature_len, &enc_path)?;
    debug!(enc_len, "encoder complete");

    // Step 3: TDT greedy decoding
    let token_ids = run_tdt_greedy_decode(&enc_out, enc_len, &dec_path)?;
    debug!(n_tokens = token_ids.len(), "TDT decode complete");

    // Step 4: Vocabulary lookup
    let transcript = decode_token_ids(&token_ids, &voc_path)?;

    info!(
        transcript = transcript.as_str(),
        "Parakeet transcription complete"
    );
    Ok(transcript)
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

fn validate_model_files(
    preproc: &Path,
    encoder: &Path,
    decoder: &Path,
    vocab: &Path,
) -> Result<(), AudioError> {
    let missing: Vec<&str> = [
        (preproc, "nemo128.onnx"),
        (encoder, "encoder-model.onnx"),
        (decoder, "decoder_joint-model.onnx"),
        (vocab, "vocab.txt"),
    ]
    .iter()
    .filter(|(p, _)| !p.exists())
    .map(|(_, name)| *name)
    .collect();

    if !missing.is_empty() {
        log_download_instructions();
        return Err(AudioError::Transcription(format!(
            "Parakeet model files not downloaded (missing: {}). \
             Run `huggingface-cli download istupakov/parakeet-tdt-0.6b-v3-onnx \
             encoder-model.onnx encoder-model.onnx.data decoder_joint-model.onnx \
             nemo128.onnx vocab.txt config.json \
             --local-dir ~/.axterminator/models/parakeet-tdt-0.6b-v3` to install them.",
            missing.join(", ")
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// ONNX Runtime — Preprocessor (nemo128.onnx)
// ---------------------------------------------------------------------------

/// Run the mel-spectrogram preprocessor on raw waveform samples.
///
/// Takes f32 samples at 16 kHz and returns 128-dim features with shape
/// `[1, 128, T]` as a flat `Vec<f32>` plus the number of time frames `T`.
fn run_preprocessor(samples: &[f32], onnx_path: &Path) -> Result<(Vec<f32>, usize), AudioError> {
    use ort::session::Session;
    use ort::value::Tensor;

    let mut session = Session::builder()
        .map_err(|e| {
            AudioError::Transcription(format!("Preprocessor session builder failed: {e}"))
        })?
        .commit_from_file(onnx_path)
        .map_err(|e| {
            AudioError::Transcription(format!(
                "Failed to load preprocessor from {onnx_path:?}: {e}"
            ))
        })?;

    let n_samples = samples.len();
    let waveform = Tensor::<f32>::from_array(([1, n_samples], samples.to_vec().into_boxed_slice()))
        .map_err(|e| AudioError::Transcription(format!("Failed to create waveform tensor: {e}")))?;

    #[allow(clippy::cast_possible_truncation)]
    let lengths = Tensor::<i64>::from_array(([1], vec![n_samples as i64].into_boxed_slice()))
        .map_err(|e| AudioError::Transcription(format!("Failed to create lengths tensor: {e}")))?;

    let outputs = session
        .run(ort::inputs![waveform, lengths])
        .map_err(|e| AudioError::Transcription(format!("Preprocessor inference failed: {e}")))?;

    // Output 0: features [1, 128, T], Output 1: feature_lengths [1]
    let features_val = outputs
        .get("features")
        .or_else(|| {
            if outputs.len() > 0 {
                Some(&outputs[0])
            } else {
                None
            }
        })
        .ok_or_else(|| AudioError::Transcription("Preprocessor produced no outputs".to_string()))?;

    let (shape, data) = features_val
        .try_extract_tensor::<f32>()
        .map_err(|e| AudioError::Transcription(format!("Failed to extract features: {e}")))?;

    let feature_len = if shape.len() >= 3 {
        shape[2] as usize
    } else {
        data.len() / 128
    };

    Ok((data.to_vec(), feature_len))
}

// ---------------------------------------------------------------------------
// ONNX Runtime — Encoder (encoder-model.onnx)
// ---------------------------------------------------------------------------

/// Run the Conformer encoder on preprocessed features.
///
/// Takes features of shape `[1, 128, T_features]` and returns encoder
/// outputs of shape `[1, 1024, T_enc]` (T_enc = T_features / 8 due to
/// subsampling) as a flat `Vec<f32>` plus the number of encoder frames.
fn run_encoder(
    features: &[f32],
    feature_len: usize,
    onnx_path: &Path,
) -> Result<(Vec<f32>, usize), AudioError> {
    use ort::session::Session;
    use ort::value::Tensor;

    let mut session = Session::builder()
        .map_err(|e| AudioError::Transcription(format!("Encoder session builder failed: {e}")))?
        .commit_from_file(onnx_path)
        .map_err(|e| {
            AudioError::Transcription(format!("Failed to load encoder from {onnx_path:?}: {e}"))
        })?;

    let shape: [usize; 3] = [1, 128, feature_len];
    let input_tensor = Tensor::<f32>::from_array((shape, features.to_vec().into_boxed_slice()))
        .map_err(|e| AudioError::Transcription(format!("Failed to create encoder input: {e}")))?;

    #[allow(clippy::cast_possible_truncation)]
    let lengths = Tensor::<i64>::from_array(([1], vec![feature_len as i64].into_boxed_slice()))
        .map_err(|e| {
            AudioError::Transcription(format!("Failed to create encoder lengths tensor: {e}"))
        })?;

    let outputs = session
        .run(ort::inputs![input_tensor, lengths])
        .map_err(|e| AudioError::Transcription(format!("Encoder inference failed: {e}")))?;

    // Output 0: encoded [1, 1024, T_enc], Output 1: encoded_lengths [1]
    let enc_val = outputs
        .get("outputs")
        .or_else(|| {
            if outputs.len() > 0 {
                Some(&outputs[0])
            } else {
                None
            }
        })
        .ok_or_else(|| AudioError::Transcription("Encoder produced no outputs".to_string()))?;

    let (shape, data) = enc_val.try_extract_tensor::<f32>().map_err(|e| {
        AudioError::Transcription(format!("Failed to extract encoder outputs: {e}"))
    })?;

    // Extract encoded_lengths from second output
    let enc_len = if let Some(len_val) = outputs.get("encoded_lengths").or_else(|| {
        if outputs.len() > 1 {
            Some(&outputs[1])
        } else {
            None
        }
    }) {
        let (_, len_data) = len_val.try_extract_tensor::<i64>().map_err(|e| {
            AudioError::Transcription(format!("Failed to extract encoded_lengths: {e}"))
        })?;
        len_data[0] as usize
    } else {
        // Fall back to shape-based calculation
        if shape.len() >= 3 {
            shape[2] as usize
        } else {
            data.len() / ENCODER_DIM
        }
    };

    Ok((data.to_vec(), enc_len))
}

// ---------------------------------------------------------------------------
// ONNX Runtime — TDT Greedy Decode (decoder_joint-model.onnx)
// ---------------------------------------------------------------------------

/// Run TDT greedy decoding over encoder outputs.
///
/// The decoder_joint model outputs logits of shape `[1, 1, 1, 8198]` where:
/// - Indices 0..8192 are vocabulary token logits (8192 = blank)
/// - Indices 8193..8197 are duration logits (durations 0..4)
///
/// At each step the decoder selects:
/// 1. The highest-scoring token from the vocabulary logits
/// 2. The highest-scoring duration from the duration logits
///
/// If the token is blank, the encoder advances by `max(1, duration)` frames.
/// If the token is non-blank, it is emitted and the decoder state updates.
/// If the duration is > 0, the encoder also advances.
fn run_tdt_greedy_decode(
    enc_out: &[f32],
    enc_len: usize,
    onnx_path: &Path,
) -> Result<Vec<i32>, AudioError> {
    use ort::session::Session;
    use ort::value::Tensor;

    let mut session = Session::builder()
        .map_err(|e| AudioError::Transcription(format!("Decoder session builder failed: {e}")))?
        .commit_from_file(onnx_path)
        .map_err(|e| {
            AudioError::Transcription(format!("Failed to load decoder from {onnx_path:?}: {e}"))
        })?;

    let mut tokens: Vec<i32> = Vec::new();
    let mut last_token = BLANK_TOKEN;
    let mut state1 = vec![0.0f32; 2 * PRED_HIDDEN_DIM]; // [2, 1, 640] flattened
    let mut state2 = vec![0.0f32; 2 * PRED_HIDDEN_DIM];
    let mut t_idx: usize = 0;

    while t_idx < enc_len {
        let mut symbols_this_step = 0;

        loop {
            if symbols_this_step >= MAX_SYMBOLS_PER_STEP {
                t_idx += 1;
                break;
            }

            // Extract single encoder frame: [1, 1024, 1]
            let frame_start = t_idx * ENCODER_DIM;
            let frame_end = frame_start + ENCODER_DIM;
            if frame_end > enc_out.len() {
                t_idx = enc_len; // safety
                break;
            }
            let frame_data: Vec<f32> = enc_out[frame_start..frame_end].to_vec();

            let enc_frame = Tensor::<f32>::from_array((
                [1usize, ENCODER_DIM, 1],
                frame_data.into_boxed_slice(),
            ))
            .map_err(|e| {
                AudioError::Transcription(format!("Failed to create encoder frame: {e}"))
            })?;

            let targets =
                Tensor::<i32>::from_array(([1usize, 1], vec![last_token].into_boxed_slice()))
                    .map_err(|e| {
                        AudioError::Transcription(format!("Failed to create targets tensor: {e}"))
                    })?;

            let target_length = Tensor::<i32>::from_array((
                [1usize],
                vec![1i32].into_boxed_slice(),
            ))
            .map_err(|e| {
                AudioError::Transcription(format!("Failed to create target_length tensor: {e}"))
            })?;

            let state1_tensor = Tensor::<f32>::from_array((
                [2usize, 1, PRED_HIDDEN_DIM],
                state1.clone().into_boxed_slice(),
            ))
            .map_err(|e| {
                AudioError::Transcription(format!("Failed to create state1 tensor: {e}"))
            })?;

            let state2_tensor = Tensor::<f32>::from_array((
                [2usize, 1, PRED_HIDDEN_DIM],
                state2.clone().into_boxed_slice(),
            ))
            .map_err(|e| {
                AudioError::Transcription(format!("Failed to create state2 tensor: {e}"))
            })?;

            let outputs = session
                .run(ort::inputs![
                    enc_frame,
                    targets,
                    target_length,
                    state1_tensor,
                    state2_tensor
                ])
                .map_err(|e| AudioError::Transcription(format!("Decoder inference failed: {e}")))?;

            // Output 0: logits [1, 1, 1, 8198]
            let logits_val = &outputs[0];
            let (_shape, logits) = logits_val.try_extract_tensor::<f32>().map_err(|e| {
                AudioError::Transcription(format!("Failed to extract decoder logits: {e}"))
            })?;

            // Split logits into token scores and duration scores
            let token_logits = &logits[..VOCAB_SIZE];
            let dur_logits = &logits[VOCAB_SIZE..VOCAB_SIZE + NUM_DURATIONS];

            let token_id = argmax_f32(token_logits) as i32;
            let dur_id = argmax_f32(dur_logits);

            if token_id == BLANK_TOKEN {
                // Blank: advance encoder by at least 1 frame
                let advance = dur_id.max(1);
                t_idx += advance;
                break;
            }

            // Non-blank: emit token and update decoder state
            tokens.push(token_id);
            last_token = token_id;
            symbols_this_step += 1;

            // Update LSTM states from decoder output
            if let Some(s1_val) = outputs.get("output_states_1").or_else(|| {
                if outputs.len() > 2 {
                    Some(&outputs[2])
                } else {
                    None
                }
            }) {
                if let Ok((_, s1_data)) = s1_val.try_extract_tensor::<f32>() {
                    state1 = s1_data.to_vec();
                }
            }
            if let Some(s2_val) = outputs.get("output_states_2").or_else(|| {
                if outputs.len() > 3 {
                    Some(&outputs[3])
                } else {
                    None
                }
            }) {
                if let Ok((_, s2_data)) = s2_val.try_extract_tensor::<f32>() {
                    state2 = s2_data.to_vec();
                }
            }

            if dur_id > 0 {
                // Duration > 0: also advance encoder position
                t_idx += dur_id;
                break;
            }
            // Duration == 0: stay on same frame, try to emit more tokens
        }
    }

    Ok(tokens)
}

/// Return the index of the maximum value in `data`.
fn argmax_f32(data: &[f32]) -> usize {
    data.iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(i, _)| i)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Vocabulary decode
// ---------------------------------------------------------------------------

/// Decode token IDs to a UTF-8 string using `vocab.txt`.
///
/// The vocabulary file uses the NeMo format: one line per token, each line
/// containing `<token> <index>`.  Special tokens (`<blank>`, `<unk>`, etc.)
/// are filtered out during decoding.
///
/// SentencePiece word-boundary markers (`\u{2581}`) are normalised to ASCII
/// spaces.
///
/// # Errors
///
/// Returns [`AudioError::Transcription`] when the vocabulary file cannot be
/// parsed or a token ID is out of range.
fn decode_token_ids(token_ids: &[i32], vocab_path: &Path) -> Result<String, AudioError> {
    let vocab = load_vocab(vocab_path)?;
    let text = token_ids
        .iter()
        .filter_map(|&id| {
            let idx = id as usize;
            vocab.get(idx).filter(|tok| !is_special_token(tok))
        })
        .fold(String::new(), |mut acc, piece| {
            acc.push_str(piece);
            acc
        });

    // Normalise SentencePiece word-boundary marker to ASCII space.
    let normalised = text.replace('\u{2581}', " ").trim().to_string();
    Ok(normalised)
}

/// Check whether a token is a special/control token that should be filtered
/// from the output text.
fn is_special_token(token: &str) -> bool {
    token.starts_with('<') && token.ends_with('>')
}

/// Load vocabulary from `vocab.txt` into an index-addressed `Vec<String>`.
///
/// Supports the NeMo format (`<token> <index>` per line) and also the
/// HuggingFace `tokenizer.json` format for backward compatibility.
///
/// # Errors
///
/// Returns [`AudioError::Transcription`] on I/O or parse failure.
fn load_vocab(vocab_path: &Path) -> Result<Vec<String>, AudioError> {
    let content = std::fs::read_to_string(vocab_path)
        .map_err(|e| AudioError::Transcription(format!("Cannot read vocab file: {e}")))?;

    // NeMo vocab.txt format: "<token> <index>" per line
    if vocab_path.extension().is_some_and(|ext| ext == "txt") {
        return load_vocab_nemo_txt(&content);
    }

    // Try JSON formats (tokenizer.json backward compatibility)
    load_vocab_json(&content)
}

/// Parse NeMo `vocab.txt` format: each line is `<token> <index>`.
fn load_vocab_nemo_txt(content: &str) -> Result<Vec<String>, AudioError> {
    let mut entries: Vec<(usize, String)> = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Split on last space to get token and index
        if let Some(last_space) = line.rfind(' ') {
            let token = &line[..last_space];
            let idx_str = &line[last_space + 1..];
            if let Ok(idx) = idx_str.parse::<usize>() {
                entries.push((idx, token.to_string()));
            }
        }
    }

    if entries.is_empty() {
        return Err(AudioError::Transcription(
            "vocab.txt is empty or has unrecognised format".to_string(),
        ));
    }

    let max_id = entries.iter().map(|(id, _)| *id).max().unwrap_or(0);
    let mut vocab = vec![String::new(); max_id + 1];
    for (id, token) in entries {
        vocab[id] = token;
    }
    Ok(vocab)
}

/// Parse vocabulary from JSON formats (backward compatibility with tokenizer.json).
fn load_vocab_json(content: &str) -> Result<Vec<String>, AudioError> {
    let root: serde_json::Value = serde_json::from_str(content)
        .map_err(|e| AudioError::Transcription(format!("vocab file parse error: {e}")))?;

    // HuggingFace fast-tokenizer format: {"model": {"vocab": {"token": id, ...}}}
    if let Some(vocab_map) = root.pointer("/model/vocab").and_then(|v| v.as_object()) {
        return build_vocab_from_map(vocab_map);
    }

    // Flat array: [token0, token1, ...]
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
        "vocab file has an unrecognised format".to_string(),
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
// Backward-compatible public API for mel spectrogram (used by tests)
// ---------------------------------------------------------------------------

/// Compute the log-mel spectrogram for `samples` at 16 kHz.
///
/// Returns a flat `Vec<f32>` in row-major order with logical shape
/// `[N_MELS, n_frames]`, normalised with the NeMo global mean/std.
///
/// Note: The TDT pipeline uses `nemo128.onnx` for preprocessing instead of
/// this function.  This is retained for testing and backward compatibility.
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
    const N_MELS: usize = 80;
    const FFT_SIZE: usize = 512;
    const HOP_LENGTH: usize = 160;
    const WIN_LENGTH: usize = 400;
    const MEL_FMAX: f64 = 8_000.0;
    const LOG_FLOOR: f32 = 1e-5;
    const GLOBAL_MEAN: f32 = -5.017;
    const GLOBAL_STD: f32 = 2.698;

    let hann = hann_window(WIN_LENGTH);
    let mel_fb = mel_filterbank(FFT_SIZE, N_MELS, 16_000.0, 0.0, MEL_FMAX);
    let frames = frame_and_window(samples, &hann, WIN_LENGTH, HOP_LENGTH, FFT_SIZE);
    let n_frames = frames.len();

    let mut out = vec![0.0f32; N_MELS * n_frames];

    for (frame_idx, frame) in frames.iter().enumerate() {
        let power = compute_power_spectrum(frame);
        apply_mel_filterbank_frame(&power, &mel_fb, &mut out, frame_idx, N_MELS);
    }

    for v in out.iter_mut() {
        *v = v.max(LOG_FLOOR).ln();
        *v = (*v - GLOBAL_MEAN) / GLOBAL_STD;
    }
    out
}

// ---------------------------------------------------------------------------
// DSP helpers (retained for backward compatibility and tests)
// ---------------------------------------------------------------------------

/// Split `samples` into overlapping windowed frames.
fn frame_and_window(
    samples: &[f32],
    window: &[f32],
    win_length: usize,
    hop_length: usize,
    fft_size: usize,
) -> Vec<Vec<f32>> {
    let n_samples = samples.len();
    let pad = win_length / 2;
    let padded_len = n_samples + 2 * pad;

    let mut padded = vec![0.0f32; padded_len];
    padded[pad..pad + n_samples].copy_from_slice(samples);

    let n_frames = (padded_len.saturating_sub(win_length)) / hop_length + 1;
    let mut frames = Vec::with_capacity(n_frames);

    for i in 0..n_frames {
        let start = i * hop_length;
        let end = (start + win_length).min(padded_len);
        let mut frame = vec![0.0f32; fft_size];
        for (j, &s) in padded[start..end].iter().enumerate() {
            frame[j] = s * window[j];
        }
        frames.push(frame);
    }
    frames
}

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

/// Compute the power spectrum `|FFT(frame)|^2` of one windowed frame.
fn compute_power_spectrum(frame: &[f32]) -> Vec<f32> {
    let n = frame.len();
    let mut re: Vec<f64> = frame.iter().map(|&s| f64::from(s)).collect();
    let mut im: Vec<f64> = vec![0.0; n];

    fft_inplace(&mut re, &mut im, n);

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
fn fft_inplace(re: &mut [f64], im: &mut [f64], n: usize) {
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

/// Build a mel filter bank matrix of shape `[n_mels, n_fft/2+1]`.
fn mel_filterbank(n_fft: usize, n_mels: usize, sr: f64, fmin: f64, fmax: f64) -> Vec<Vec<f32>> {
    let n_freqs = n_fft / 2 + 1;
    let mel_min = hz_to_mel(fmin);
    let mel_max = hz_to_mel(fmax);

    let mel_points: Vec<f64> = (0..=n_mels + 1)
        .map(|i| {
            #[allow(clippy::cast_precision_loss)]
            {
                mel_min + (mel_max - mel_min) * i as f64 / (n_mels + 1) as f64
            }
        })
        .collect();
    let hz_points: Vec<f64> = mel_points.iter().map(|&m| mel_to_hz(m)).collect();

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
fn apply_mel_filterbank_frame(
    power: &[f32],
    fb: &[Vec<f32>],
    out: &mut [f32],
    frame_idx: usize,
    n_mels: usize,
) {
    let n_frames = out.len() / n_mels;
    for (m, filter) in fb.iter().enumerate() {
        let energy: f32 = filter.iter().zip(power).map(|(&w, &p)| w * p).sum();
        out[m * n_frames + frame_idx] = energy;
    }
}

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
        for &hz in &[100.0_f64, 500.0, 1000.0, 4000.0, 8000.0] {
            let recovered = mel_to_hz(hz_to_mel(hz));
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
        let n = 512;
        let mut re: Vec<f64> = vec![1.0; n];
        let mut im: Vec<f64> = vec![0.0; n];
        fft_inplace(&mut re, &mut im, n);
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
        let n = 512;
        let mut re: Vec<f64> = vec![0.0; n];
        let mut im: Vec<f64> = vec![0.0; n];
        re[0] = 1.0;
        fft_inplace(&mut re, &mut im, n);
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
        let fb = mel_filterbank(512, 80, 16_000.0, 0.0, 8_000.0);
        assert_eq!(fb.len(), 80, "expected 80 rows");
        for row in &fb {
            assert_eq!(row.len(), 257, "each row should have 257 bins");
        }
    }

    #[test]
    fn mel_filterbank_rows_are_non_negative() {
        let fb = mel_filterbank(512, 80, 16_000.0, 0.0, 8_000.0);
        for (m, row) in fb.iter().enumerate() {
            for (k, &v) in row.iter().enumerate() {
                assert!(v >= 0.0, "filter bank [{m}][{k}] = {v} is negative");
            }
        }
    }

    #[test]
    fn mel_filterbank_each_filter_has_positive_area() {
        let fb = mel_filterbank(512, 80, 16_000.0, 0.0, 8_000.0);
        for (m, row) in fb.iter().enumerate() {
            let area: f32 = row.iter().sum();
            assert!(area > 0.0, "filter {m} has zero area");
        }
    }

    // -----------------------------------------------------------------------
    // compute_log_mel_spectrogram
    // -----------------------------------------------------------------------

    #[test]
    fn log_mel_spectrogram_silence_has_expected_frame_count() {
        let silence = vec![0.0f32; 16_000];
        let mel = compute_log_mel_spectrogram(&silence);
        assert_eq!(
            mel.len() % 80,
            0,
            "mel length {} is not a multiple of 80",
            mel.len()
        );
    }

    #[test]
    fn log_mel_spectrogram_silence_values_are_normalised() {
        let log_floor: f32 = 1e-5;
        let global_mean: f32 = -5.017;
        let global_std: f32 = 2.698;
        let silence = vec![0.0f32; 16_000];
        let mel = compute_log_mel_spectrogram(&silence);
        let expected = (log_floor.ln() - global_mean) / global_std;
        for &v in &mel {
            assert!(
                (v - expected).abs() < 0.01,
                "silence bin should be {expected:.4}, got {v:.4}"
            );
        }
    }

    #[test]
    fn log_mel_spectrogram_empty_input_produces_at_least_one_frame() {
        let mel = compute_log_mel_spectrogram(&[]);
        assert!(!mel.is_empty(), "empty input should still produce frames");
    }

    // -----------------------------------------------------------------------
    // frame_and_window
    // -----------------------------------------------------------------------

    #[test]
    fn frame_and_window_one_second_produces_correct_count() {
        let window = hann_window(400);
        let samples = vec![0.0f32; 16_000];
        let frames = frame_and_window(&samples, &window, 400, 160, 512);
        assert!(!frames.is_empty(), "should produce frames for 1s audio");
    }

    #[test]
    fn frame_and_window_each_frame_has_fft_size() {
        let window = hann_window(400);
        let samples = vec![0.5f32; 800];
        let frames = frame_and_window(&samples, &window, 400, 160, 512);
        for (i, frame) in frames.iter().enumerate() {
            assert_eq!(frame.len(), 512, "frame {i} length {} != 512", frame.len());
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
        let result = std::panic::catch_unwind(model_files_present);
        assert!(result.is_ok(), "model_files_present should never panic");
    }

    // -----------------------------------------------------------------------
    // load_vocab (unit tests with synthetic data)
    // -----------------------------------------------------------------------

    #[test]
    fn load_vocab_parses_nemo_txt_format() {
        use std::io::Write as _;
        let content = "<unk> 0\n<blk> 1\nhello 2\nworld 3\n";
        let mut tmp = tempfile::Builder::new().suffix(".txt").tempfile().unwrap();
        tmp.write_all(content.as_bytes()).unwrap();
        let vocab = load_vocab(tmp.path()).unwrap();
        assert_eq!(vocab.len(), 4);
        assert_eq!(vocab[0], "<unk>");
        assert_eq!(vocab[2], "hello");
        assert_eq!(vocab[3], "world");
    }

    #[test]
    fn load_vocab_parses_huggingface_json_format() {
        use std::io::Write as _;
        let json = r#"{"model":{"vocab":{"<blank>":0,"a":1,"b":2}}}"#;
        let mut tmp = tempfile::Builder::new().suffix(".json").tempfile().unwrap();
        tmp.write_all(json.as_bytes()).unwrap();
        let vocab = load_vocab(tmp.path()).unwrap();
        assert_eq!(vocab.len(), 3);
        assert_eq!(vocab[1], "a");
        assert_eq!(vocab[2], "b");
    }

    #[test]
    fn load_vocab_parses_flat_array_json_format() {
        use std::io::Write as _;
        let json = r#"["<blank>","hello","world"]"#;
        let mut tmp = tempfile::Builder::new().suffix(".json").tempfile().unwrap();
        tmp.write_all(json.as_bytes()).unwrap();
        let vocab = load_vocab(tmp.path()).unwrap();
        assert_eq!(vocab.len(), 3);
        assert_eq!(vocab[0], "<blank>");
        assert_eq!(vocab[2], "world");
    }

    #[test]
    fn load_vocab_returns_error_on_missing_file() {
        let result = load_vocab(Path::new("/nonexistent/path/vocab.txt"));
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
        let content = "\u{2581}hello 0\n\u{2581}world 1\n";
        let mut tmp = tempfile::Builder::new().suffix(".txt").tempfile().unwrap();
        tmp.write_all(content.as_bytes()).unwrap();
        let result = decode_token_ids(&[0, 1], tmp.path()).unwrap();
        assert_eq!(result, "hello world");
    }

    #[test]
    fn decode_token_ids_empty_input_returns_empty_string() {
        use std::io::Write as _;
        let content = "a 0\n";
        let mut tmp = tempfile::Builder::new().suffix(".txt").tempfile().unwrap();
        tmp.write_all(content.as_bytes()).unwrap();
        let result = decode_token_ids(&[], tmp.path()).unwrap();
        assert_eq!(result, "");
    }

    // -----------------------------------------------------------------------
    // argmax_f32
    // -----------------------------------------------------------------------

    #[test]
    fn argmax_f32_finds_correct_index() {
        assert_eq!(argmax_f32(&[1.0, 3.0, 2.0]), 1);
        assert_eq!(argmax_f32(&[5.0, 1.0, 2.0]), 0);
        assert_eq!(argmax_f32(&[1.0, 2.0, 7.0]), 2);
    }

    #[test]
    fn argmax_f32_empty_returns_zero() {
        assert_eq!(argmax_f32(&[]), 0);
    }

    // -----------------------------------------------------------------------
    // is_special_token
    // -----------------------------------------------------------------------

    #[test]
    fn is_special_token_detects_angle_brackets() {
        assert!(is_special_token("<blank>"));
        assert!(is_special_token("<unk>"));
        assert!(is_special_token("<blk>"));
        assert!(!is_special_token("hello"));
        assert!(!is_special_token("<partial"));
        assert!(!is_special_token("partial>"));
    }
}
