//! On-device audio capture, speech recognition, and text-to-speech for macOS.
//!
//! This module exposes three capabilities gated behind the `audio` cargo feature:
//!
//! | Capability | API | Privacy |
//! |-----------|-----|---------|
//! | Microphone capture | `AVAudioEngine` (CoreAudio) | Requires TCC microphone permission |
//! | System audio capture | `ScreenCaptureKit` (macOS 14+) | No Screen Recording permission needed |
//! | Speech-to-text | `SFSpeechRecognizer` | On-device only — no cloud |
//! | Text-to-speech | `NSSpeechSynthesizer` | No network; local voice synthesis |
//!
//! ## Quick start
//!
//! ```ignore
//! use axterminator::audio::{capture_microphone, capture_system_audio, transcribe, speak, list_audio_devices};
//!
//! // Capture 5 seconds of microphone audio
//! let audio = capture_microphone(5.0)?;
//!
//! // Capture system audio (macOS 14+: no Screen Recording permission)
//! let sys_audio = capture_system_audio(3.0)?;
//!
//! // Transcribe on-device (supports multiple languages)
//! let text = transcribe(&audio, None)?;            // English (default)
//! let fi = transcribe(&audio, Some("fi-FI"))?;     // Finnish
//!
//! // Speak a line
//! speak("Verification complete")?;
//!
//! // Enumerate devices
//! let devices = list_audio_devices();
//! ```
//!
//! ## Feature flag
//!
//! Add `--features audio` to enable. Absent the flag the module does not compile,
//! preventing unwanted framework linkage and TCC permission dialogs.
//!
//! ## Permissions
//!
//! Microphone capture requires `com.apple.privacy.microphone` TCC consent.
//! The first call to [`capture_microphone`] or [`capture_system_audio`] will
//! trigger the system permission dialog when not yet granted. If the user denies
//! access, the call returns `Err(AudioError::PermissionDenied)`.
//!
//! ## Security
//!
//! - Speech recognition prefers on-device models (server fallback if model not downloaded).
//! - No audio data leaves the machine (when on-device model is available).
//! - Temporary WAV files (when used) are written to `/tmp` with mode `0600`
//!   and deleted immediately after encoding.
//! - Recording duration must stay within [`MIN_CAPTURE_SECS`]..=[`MAX_CAPTURE_SECS`].

use base64::Engine as _;

// ---------------------------------------------------------------------------
// Sub-modules (private implementation)
// ---------------------------------------------------------------------------

mod capture;
mod devices;
mod ffi;
#[cfg(feature = "parakeet")]
pub mod parakeet;
mod sck_capture;
mod speech;
#[cfg(feature = "vad")]
pub mod vad;

// ---------------------------------------------------------------------------
// Public re-exports
// ---------------------------------------------------------------------------

pub use capture::{
    capture_microphone, capture_microphone_with_metadata, capture_system_audio,
    capture_system_audio_with_metadata, validate_duration, AudioCaptureBackend, AudioCaptureSource,
    CapturedAudio,
};
pub use devices::{check_microphone_permission, list_audio_devices, AudioDevice};
pub use speech::{speak, transcribe, transcribe_with_engine, AudioEngine};
#[cfg(feature = "vad")]
pub use vad::{VadDetector, model_file_present as vad_model_present};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Minimum capture duration in seconds.
pub const MIN_CAPTURE_SECS: f32 = 0.1;

/// Hard cap on capture duration in seconds (prevents surveillance-length recordings).
pub const MAX_CAPTURE_SECS: f32 = 30.0;

/// PCM sample rate used for all captures.
pub(crate) const SAMPLE_RATE: u32 = 16_000;

/// Number of audio channels captured.
pub(crate) const CHANNELS: u16 = 1;

/// Bits per sample for WAV encoding.
pub(crate) const BITS_PER_SAMPLE: u16 = 16;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can arise from audio operations.
///
/// Each variant maps to a machine-readable error `code` in MCP responses so
/// clients can branch on the specific failure cause without string matching.
#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    /// Microphone or audio permission denied by TCC.
    ///
    /// Direct users to: **System Settings > Privacy & Security > Microphone**.
    #[error(
        "Microphone access denied. Enable it at System Settings > Privacy & Security > Microphone."
    )]
    PermissionDenied,

    /// Requested duration exceeds [`MAX_CAPTURE_SECS`].
    #[error("Duration {requested}s exceeds maximum allowed {max}s")]
    DurationExceeded { requested: f32, max: f32 },

    /// Requested duration is missing, non-finite, or shorter than the minimum.
    #[error("Duration {requested}s must be finite and at least {min}s")]
    InvalidDuration { requested: f32, min: f32 },

    /// An underlying CoreAudio or Objective-C framework call failed.
    #[error("Audio framework error: {0}")]
    Framework(String),

    /// Speech recognition failed or produced no result.
    #[error("Transcription failed: {0}")]
    Transcription(String),

    /// Text-to-speech synthesis failed.
    #[error("Speech synthesis failed: {0}")]
    Synthesis(String),
}

impl AudioError {
    /// Machine-readable error code forwarded to MCP clients.
    ///
    /// # Examples
    ///
    /// ```
    /// use axterminator::audio::AudioError;
    /// assert_eq!(AudioError::PermissionDenied.code(), "microphone_denied");
    /// assert_eq!(
    ///     AudioError::DurationExceeded { requested: 60.0, max: 30.0 }.code(),
    ///     "duration_exceeded"
    /// );
    /// ```
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::PermissionDenied => "microphone_denied",
            Self::DurationExceeded { .. } => "duration_exceeded",
            Self::InvalidDuration { .. } => "invalid_duration",
            Self::Framework(_) => "framework_error",
            Self::Transcription(_) => "transcription_error",
            Self::Synthesis(_) => "synthesis_error",
        }
    }
}

// ---------------------------------------------------------------------------
// AudioData
// ---------------------------------------------------------------------------

/// Raw captured audio data in normalised float PCM format.
///
/// Samples are in the range `[-1.0, 1.0]`. Use [`AudioData::to_wav_bytes`]
/// to encode as a standard 16-bit PCM WAV for transmission or analysis.
#[derive(Debug, Clone)]
pub struct AudioData {
    /// Interleaved float PCM samples (normalised to −1.0…1.0).
    pub samples: Vec<f32>,
    /// Sample rate in Hz (always [`SAMPLE_RATE`] = 16 000).
    pub sample_rate: u32,
    /// Number of channels (always 1 — mono).
    pub channels: u16,
    /// Actual captured duration in seconds.
    pub duration_secs: f32,
}

impl AudioData {
    /// Create a silent (zero-filled) buffer of `duration_secs`.
    ///
    /// Useful for testing pipelines without real hardware.
    ///
    /// # Examples
    ///
    /// ```
    /// use axterminator::audio::AudioData;
    /// let silent = AudioData::silent(1.0);
    /// assert_eq!(silent.sample_rate, 16_000);
    /// assert_eq!(silent.channels, 1);
    /// assert!((silent.duration_secs - 1.0).abs() < 0.01);
    /// ```
    #[must_use]
    pub fn silent(duration_secs: f32) -> Self {
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let n_samples = (f64::from(duration_secs) * f64::from(SAMPLE_RATE)) as usize;
        Self {
            samples: vec![0.0f32; n_samples],
            sample_rate: SAMPLE_RATE,
            channels: CHANNELS,
            duration_secs,
        }
    }

    /// Encode as a base64-encoded WAV string (PCM 16-bit, mono, 16 kHz).
    ///
    /// The returned string is suitable for embedding directly in a JSON response.
    ///
    /// # Examples
    ///
    /// ```
    /// use axterminator::audio::AudioData;
    /// let data = AudioData::silent(0.1);
    /// let b64 = data.to_wav_base64();
    /// assert!(!b64.is_empty());
    /// // WAV magic bytes encode to "UklG" in base64
    /// assert!(b64.starts_with("UklG"), "expected RIFF header: {b64}");
    /// ```
    #[must_use]
    pub fn to_wav_base64(&self) -> String {
        let bytes = self.to_wav_bytes();
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    }

    /// Encode audio samples as a 16-bit PCM WAV byte vector.
    ///
    /// The WAV header is always 44 bytes (standard PCM, no extension chunks).
    ///
    /// # Examples
    ///
    /// ```
    /// use axterminator::audio::AudioData;
    /// let data = AudioData::silent(0.0);
    /// let bytes = data.to_wav_bytes();
    /// // Minimum WAV = 44-byte header + 0 data bytes
    /// assert_eq!(bytes.len(), 44);
    /// assert_eq!(&bytes[0..4], b"RIFF");
    /// assert_eq!(&bytes[8..12], b"WAVE");
    /// ```
    #[must_use]
    pub fn to_wav_bytes(&self) -> Vec<u8> {
        encode_wav_pcm16(&self.samples, self.sample_rate, self.channels)
    }

    /// Duration of the audio data in milliseconds.
    ///
    /// # Examples
    ///
    /// ```
    /// use axterminator::audio::AudioData;
    /// let data = AudioData::silent(1.5);
    /// assert_eq!(data.duration_ms(), 1500);
    /// ```
    #[must_use]
    pub fn duration_ms(&self) -> u64 {
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        {
            (f64::from(self.duration_secs) * 1000.0) as u64
        }
    }

    /// Size of the WAV payload in bytes (header + PCM data).
    ///
    /// Useful for estimating MCP response payload size. The base64 encoding
    /// inflates this by ~33%, so the JSON field will be approximately
    /// `size_bytes * 4 / 3`.
    ///
    /// # Examples
    ///
    /// ```
    /// use axterminator::audio::AudioData;
    /// let data = AudioData::silent(1.0);
    /// // 16 kHz × 1 ch × 2 bytes/sample + 44 byte header = 32044
    /// assert_eq!(data.wav_size_bytes(), 32044);
    /// ```
    #[must_use]
    pub fn wav_size_bytes(&self) -> usize {
        44 + self.samples.len() * (BITS_PER_SAMPLE as usize / 8)
    }

    /// Split audio into chunks of at most `max_secs` seconds each.
    ///
    /// Returns a `Vec` of `AudioData` segments. The last chunk may be shorter
    /// than `max_secs`. If the audio is already shorter than `max_secs`,
    /// returns a single-element vec containing a clone of `self`.
    ///
    /// # Examples
    ///
    /// ```
    /// use axterminator::audio::AudioData;
    /// let data = AudioData::silent(10.0);
    /// let chunks = data.into_chunks(3.0);
    /// assert_eq!(chunks.len(), 4); // 3 + 3 + 3 + 1 seconds
    /// assert!((chunks[0].duration_secs - 3.0).abs() < 0.01);
    /// assert!((chunks[3].duration_secs - 1.0).abs() < 0.01);
    /// ```
    #[must_use]
    pub fn into_chunks(&self, max_secs: f32) -> Vec<AudioData> {
        if max_secs <= 0.0 || self.duration_secs <= max_secs {
            return vec![self.clone()];
        }

        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let samples_per_chunk = (f64::from(max_secs) * f64::from(self.sample_rate)) as usize;
        if samples_per_chunk == 0 {
            return vec![self.clone()];
        }

        self.samples
            .chunks(samples_per_chunk)
            .map(|chunk| {
                #[allow(clippy::cast_precision_loss)]
                let dur = chunk.len() as f32 / self.sample_rate as f32;
                AudioData {
                    samples: chunk.to_vec(),
                    sample_rate: self.sample_rate,
                    channels: self.channels,
                    duration_secs: dur,
                }
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// WAV encoding (pub(crate) so speech.rs tests can reach it)
// ---------------------------------------------------------------------------

/// Encode float PCM samples as a standard 16-bit PCM WAV byte vector.
///
/// The WAV header format is: RIFF → WAVE → fmt → data (no extension chunks).
///
/// # Panics
///
/// Panics only when `sample_rate` or `channels` are 0 (invalid WAV). Both
/// values are always constants in this module.
pub(crate) fn encode_wav_pcm16(samples: &[f32], sample_rate: u32, channels: u16) -> Vec<u8> {
    let bytes_per_sample: u16 = BITS_PER_SAMPLE / 8;
    #[allow(clippy::cast_possible_truncation)]
    let data_len = (samples.len() * bytes_per_sample as usize) as u32;
    let fmt_chunk_size: u32 = 16;
    let riff_size = 4 + (8 + fmt_chunk_size) + (8 + data_len);
    let byte_rate = sample_rate * u32::from(channels) * u32::from(bytes_per_sample);
    let block_align = channels * bytes_per_sample;

    let mut out = Vec::with_capacity(44 + data_len as usize);

    // RIFF header
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&riff_size.to_le_bytes());
    out.extend_from_slice(b"WAVE");

    // fmt chunk
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&fmt_chunk_size.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM = 1
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&BITS_PER_SAMPLE.to_le_bytes());

    // data chunk
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for &s in samples {
        #[allow(clippy::cast_possible_truncation)]
        let pcm = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        out.extend_from_slice(&pcm.to_le_bytes());
    }

    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // AudioError codes
    // -----------------------------------------------------------------------

    #[test]
    fn audio_error_permission_denied_code() {
        assert_eq!(AudioError::PermissionDenied.code(), "microphone_denied");
    }

    #[test]
    fn audio_error_duration_exceeded_code() {
        let e = AudioError::DurationExceeded {
            requested: 60.0,
            max: 30.0,
        };
        assert_eq!(e.code(), "duration_exceeded");
    }

    #[test]
    fn audio_error_invalid_duration_code() {
        let e = AudioError::InvalidDuration {
            requested: 0.0,
            min: 0.1,
        };
        assert_eq!(e.code(), "invalid_duration");
    }

    #[test]
    fn audio_error_framework_code() {
        let e = AudioError::Framework("oops".to_string());
        assert_eq!(e.code(), "framework_error");
    }

    #[test]
    fn audio_error_transcription_code() {
        let e = AudioError::Transcription("failed".to_string());
        assert_eq!(e.code(), "transcription_error");
    }

    #[test]
    fn audio_error_synthesis_code() {
        let e = AudioError::Synthesis("failed".to_string());
        assert_eq!(e.code(), "synthesis_error");
    }

    #[test]
    fn audio_error_display_includes_message() {
        let e = AudioError::Framework("bad call".to_string());
        assert!(e.to_string().contains("bad call"));
    }

    // -----------------------------------------------------------------------
    // AudioData
    // -----------------------------------------------------------------------

    #[test]
    fn audio_data_silent_has_correct_sample_count() {
        // GIVEN: 1 second of silence at 16 kHz
        let data = AudioData::silent(1.0);
        // THEN: sample count matches sample_rate
        assert_eq!(data.samples.len(), SAMPLE_RATE as usize);
        assert_eq!(data.sample_rate, SAMPLE_RATE);
        assert_eq!(data.channels, CHANNELS);
    }

    #[test]
    fn audio_data_silent_all_samples_are_zero() {
        let data = AudioData::silent(0.5);
        assert!(data.samples.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn audio_data_duration_ms_converts_correctly() {
        let data = AudioData::silent(1.5);
        assert_eq!(data.duration_ms(), 1500);
    }

    #[test]
    fn audio_data_duration_ms_rounds_down() {
        let data = AudioData::silent(1.001);
        assert!(data.duration_ms() >= 1000);
    }

    // -----------------------------------------------------------------------
    // WAV size
    // -----------------------------------------------------------------------

    #[test]
    fn wav_size_bytes_empty() {
        let data = AudioData::silent(0.0);
        // Header only, no data.
        assert_eq!(data.wav_size_bytes(), 44);
    }

    #[test]
    fn wav_size_bytes_one_second() {
        let data = AudioData::silent(1.0);
        // 16 kHz × 1 ch × 2 bytes/sample + 44 header = 32044
        assert_eq!(data.wav_size_bytes(), 32044);
    }

    #[test]
    fn wav_size_bytes_matches_actual_wav() {
        let data = AudioData::silent(0.5);
        assert_eq!(data.wav_size_bytes(), data.to_wav_bytes().len());
    }

    // -----------------------------------------------------------------------
    // Chunking
    // -----------------------------------------------------------------------

    #[test]
    fn into_chunks_short_audio_returns_single_chunk() {
        let data = AudioData::silent(2.0);
        let chunks = data.into_chunks(5.0);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].samples.len(), data.samples.len());
    }

    #[test]
    fn into_chunks_splits_evenly() {
        let data = AudioData::silent(10.0);
        let chunks = data.into_chunks(5.0);
        assert_eq!(chunks.len(), 2);
        assert!((chunks[0].duration_secs - 5.0).abs() < 0.01);
        assert!((chunks[1].duration_secs - 5.0).abs() < 0.01);
    }

    #[test]
    fn into_chunks_last_chunk_shorter() {
        let data = AudioData::silent(7.0);
        let chunks = data.into_chunks(3.0);
        // 3 + 3 + 1 = 7
        assert_eq!(chunks.len(), 3);
        assert!((chunks[2].duration_secs - 1.0).abs() < 0.01);
    }

    #[test]
    fn into_chunks_preserves_total_samples() {
        let data = AudioData::silent(10.0);
        let chunks = data.into_chunks(3.0);
        let total: usize = chunks.iter().map(|c| c.samples.len()).sum();
        assert_eq!(total, data.samples.len());
    }

    #[test]
    fn into_chunks_zero_max_returns_single() {
        let data = AudioData::silent(5.0);
        let chunks = data.into_chunks(0.0);
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn into_chunks_negative_max_returns_single() {
        let data = AudioData::silent(5.0);
        let chunks = data.into_chunks(-1.0);
        assert_eq!(chunks.len(), 1);
    }

    // -----------------------------------------------------------------------
    // WAV encoding
    // -----------------------------------------------------------------------

    #[test]
    fn encode_wav_pcm16_minimum_header_is_44_bytes() {
        let bytes = encode_wav_pcm16(&[], SAMPLE_RATE, CHANNELS);
        assert_eq!(bytes.len(), 44);
    }

    #[test]
    fn encode_wav_pcm16_riff_magic() {
        let bytes = encode_wav_pcm16(&[], SAMPLE_RATE, CHANNELS);
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(&bytes[12..16], b"fmt ");
        assert_eq!(&bytes[36..40], b"data");
    }

    #[test]
    fn encode_wav_pcm16_data_length_matches_sample_count() {
        let samples: Vec<f32> = vec![0.5; 100];
        let bytes = encode_wav_pcm16(&samples, SAMPLE_RATE, CHANNELS);
        let data_len = u32::from_le_bytes(bytes[40..44].try_into().unwrap());
        assert_eq!(data_len, 200);
        assert_eq!(bytes.len(), 244);
    }

    #[test]
    fn audio_data_to_wav_base64_starts_with_riff() {
        let data = AudioData::silent(0.0);
        let b64 = data.to_wav_base64();
        // "RIFF" in base64 = "UklG"
        assert!(
            b64.starts_with("UklG"),
            "expected RIFF magic in base64: {b64}"
        );
    }

    #[test]
    fn audio_data_to_wav_base64_round_trips() {
        let data = AudioData::silent(0.0);
        let b64 = data.to_wav_base64();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&b64)
            .unwrap();
        assert_eq!(&decoded[0..4], b"RIFF");
    }

    #[test]
    fn audio_data_to_wav_bytes_matches_direct_encoding() {
        let data = AudioData::silent(0.1);
        let via_method = data.to_wav_bytes();
        let direct = encode_wav_pcm16(&data.samples, data.sample_rate, data.channels);
        assert_eq!(via_method, direct);
    }
}
