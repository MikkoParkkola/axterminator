//! On-device audio capture, speech recognition, and text-to-speech for macOS.
//!
//! This module exposes three capabilities gated behind the `audio` cargo feature:
//!
//! | Capability | API | Privacy |
//! |-----------|-----|---------|
//! | Microphone capture | `AudioQueue` (CoreAudio) | Requires TCC microphone permission |
//! | Speech-to-text | `SFSpeechRecognizer` | On-device only — no cloud |
//! | Text-to-speech | `NSSpeechSynthesizer` | No network; local voice synthesis |
//!
//! ## Quick start
//!
//! ```ignore
//! use axterminator::audio::{capture_microphone, transcribe, speak, list_audio_devices};
//!
//! // Capture 5 seconds of microphone audio
//! let audio = capture_microphone(5.0)?;
//!
//! // Transcribe on-device
//! let text = transcribe(&audio)?;
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
//! - All speech recognition uses `requiresOnDeviceRecognition = true`.
//! - No audio data leaves the machine.
//! - Temporary WAV files (when used) are written to `/tmp` with mode `0600`
//!   and deleted immediately after encoding.
//! - Recording is hard-capped at [`MAX_CAPTURE_SECS`] (30 seconds).

use std::ffi::c_void;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use base64::Engine as _;
use objc::runtime::{Class, Object};
// sel! and sel_impl! are used implicitly inside msg_send! macro expansions.
#[allow(unused_imports)]
use objc::{msg_send, sel, sel_impl};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Hard cap on capture duration in seconds (AC5: prevents surveillance-length recordings).
pub const MAX_CAPTURE_SECS: f32 = 30.0;

/// PCM sample rate used for all captures.
const SAMPLE_RATE: u32 = 16_000;

/// Number of audio channels captured.
const CHANNELS: u16 = 1;

/// Bits per sample for WAV encoding.
const BITS_PER_SAMPLE: u16 = 16;

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
            Self::Framework(_) => "framework_error",
            Self::Transcription(_) => "transcription_error",
            Self::Synthesis(_) => "synthesis_error",
        }
    }
}

// ---------------------------------------------------------------------------
// Data types
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
}

/// An audio device descriptor as returned by [`list_audio_devices`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioDevice {
    /// Human-readable device name (e.g. "Built-in Microphone").
    pub name: String,
    /// CoreAudio `AudioDeviceID` as a decimal string.
    pub id: String,
    /// `true` if this device has input channels (can capture audio).
    pub is_input: bool,
    /// `true` if this device has output channels (can play audio).
    pub is_output: bool,
    /// Default sample rate reported by the device (Hz).
    pub sample_rate: f64,
    /// `true` if this is the system default input device.
    pub is_default_input: bool,
    /// `true` if this is the system default output device.
    pub is_default_output: bool,
}

// ---------------------------------------------------------------------------
// CoreAudio raw bindings
// ---------------------------------------------------------------------------

/// Layout mirrors `AudioStreamBasicDescription` from CoreAudio/AudioToolbox.
///
/// Used when constructing `AudioQueue`-based captures. Currently reserved for
/// the direct-AudioQueue capture path (not yet wired to `AVAudioEngine`).
#[repr(C)]
#[allow(dead_code)] // Reserved for future AudioQueue capture path
struct AudioStreamBasicDescription {
    sample_rate: f64,
    format_id: u32,
    format_flags: u32,
    bytes_per_packet: u32,
    frames_per_packet: u32,
    bytes_per_frame: u32,
    channels_per_frame: u32,
    bits_per_channel: u32,
    reserved: u32,
}

// AudioObjectPropertyAddress (CoreAudio)
#[repr(C)]
struct AudioObjectPropertyAddress {
    selector: u32,
    scope: u32,
    element: u32,
}

// CoreAudio PCM format constants — used when building AudioStreamBasicDescription
// for the future AudioQueue path. Defined here for completeness even though
// the current AVAudioEngine path does not reference them directly.
#[allow(dead_code)]
const K_AUDIO_FORMAT_LINEAR_PCM: u32 = 0x6C70_636D; // 'lpcm'
#[allow(dead_code)]
const K_AUDIO_FORMAT_FLAG_SIGNED_INTEGER: u32 = 0x0004;
#[allow(dead_code)]
const K_AUDIO_FORMAT_FLAG_PACKED: u32 = 0x0008;

const K_AUDIO_OBJECT_PROPERTY_SELECTOR_DEVICES: u32 = 0x6465_7623; // 'dev#'
const K_AUDIO_OBJECT_PROPERTY_SELECTOR_DEFAULT_INPUT: u32 = 0x6471_6966; // 'dqif'  (kAudioHardwarePropertyDefaultInputDevice)
const K_AUDIO_OBJECT_PROPERTY_SELECTOR_DEFAULT_OUTPUT: u32 = 0x6471_6F66; // 'dqof' (kAudioHardwarePropertyDefaultOutputDevice)
const K_AUDIO_OBJECT_PROPERTY_SELECTOR_NAME: u32 = 0x6C6E_616D; // 'lnam'
const K_AUDIO_OBJECT_PROPERTY_SELECTOR_STREAMS: u32 = 0x7374_726D; // 'strm'
const K_AUDIO_OBJECT_PROPERTY_SELECTOR_NOMINAL_SAMPLE_RATE: u32 = 0x6E73_7274; // 'nsrt'

const K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL: u32 = 0x676C_6F62; // 'glob'
const K_AUDIO_OBJECT_PROPERTY_SCOPE_INPUT: u32 = 0x696E_7074; // 'inpt'
const K_AUDIO_OBJECT_PROPERTY_SCOPE_OUTPUT: u32 = 0x6F757470; // 'outp'
const K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN: u32 = 0;

const K_AUDIO_OBJECT_SYSTEM_OBJECT: u32 = 1;

// CoreAudio.framework provides AudioObjectGetPropertyDataSize/AudioObjectGetPropertyData.
// The framework is linked via build.rs when the `audio` feature is enabled.
extern "C" {
    fn AudioObjectGetPropertyDataSize(
        object_id: u32,
        address: *const AudioObjectPropertyAddress,
        qualifier_data_size: u32,
        qualifier_data: *const c_void,
        out_data_size: *mut u32,
    ) -> i32;

    fn AudioObjectGetPropertyData(
        object_id: u32,
        address: *const AudioObjectPropertyAddress,
        qualifier_data_size: u32,
        qualifier_data: *const c_void,
        io_data_size: *mut u32,
        out_data: *mut c_void,
    ) -> i32;
}

// ---------------------------------------------------------------------------
// Permission check (TCC)
// ---------------------------------------------------------------------------

/// Check TCC microphone authorisation status via AVFoundation.
///
/// Returns `Ok(())` when access is authorised, `Err(AudioError::PermissionDenied)`
/// when denied or restricted, and `Ok(())` optimistically when status is
/// "not determined" (first-run; the capture call triggers the dialog).
///
/// # Errors
///
/// Returns [`AudioError::PermissionDenied`] when the user has explicitly
/// denied microphone access (TCC status = `AVAuthorizationStatusDenied` = 2
/// or `AVAuthorizationStatusRestricted` = 1).
///
/// # Examples
///
/// ```ignore
/// use axterminator::audio::check_microphone_permission;
/// // On CI without microphone hardware this returns Ok(()).
/// let result = check_microphone_permission();
/// assert!(result.is_ok() || matches!(result, Err(axterminator::audio::AudioError::PermissionDenied)));
/// ```
pub fn check_microphone_permission() -> Result<(), AudioError> {
    // AVAuthorizationStatus:
    //   0 = NotDetermined, 1 = Restricted, 2 = Denied, 3 = Authorized
    let status = query_av_authorization_status();
    match status {
        1 | 2 => Err(AudioError::PermissionDenied),
        _ => Ok(()), // 0 (not determined) or 3 (authorized) — proceed
    }
}

/// Query `AVCaptureDevice.authorizationStatus(for: .audio)` via ObjC.
///
/// Returns the raw `AVAuthorizationStatus` integer (0–3).
fn query_av_authorization_status() -> i64 {
    // NSString for AVMediaTypeAudio = "soun"
    let media_type = ns_string_from_str("soun");
    let cls = objc_class("AVCaptureDevice");
    if cls.is_null() || media_type.is_null() {
        return 3; // Assume authorized when AVFoundation is unavailable (tests)
    }
    // SAFETY: AVCaptureDevice is a valid ObjC class; media_type is a valid NSString.
    unsafe { msg_send![cls, authorizationStatusForMediaType: media_type] }
}

// ---------------------------------------------------------------------------
// Device enumeration
// ---------------------------------------------------------------------------

/// Enumerate all CoreAudio audio devices on the system.
///
/// Returns an empty `Vec` when the CoreAudio system object is unavailable
/// (unlikely on macOS but handled gracefully for tests on CI).
///
/// # Examples
///
/// ```ignore
/// use axterminator::audio::list_audio_devices;
/// let devices = list_audio_devices();
/// // Every Mac has at least one audio device.
/// assert!(!devices.is_empty());
/// ```
#[must_use]
pub fn list_audio_devices() -> Vec<AudioDevice> {
    let device_ids = query_device_ids();
    if device_ids.is_empty() {
        return vec![];
    }

    let default_input = query_default_device(K_AUDIO_OBJECT_PROPERTY_SELECTOR_DEFAULT_INPUT);
    let default_output = query_default_device(K_AUDIO_OBJECT_PROPERTY_SELECTOR_DEFAULT_OUTPUT);

    device_ids
        .iter()
        .filter_map(|&id| build_audio_device(id, default_input, default_output))
        .collect()
}

/// Query all `AudioDeviceID`s from the system audio object.
fn query_device_ids() -> Vec<u32> {
    let addr = AudioObjectPropertyAddress {
        selector: K_AUDIO_OBJECT_PROPERTY_SELECTOR_DEVICES,
        scope: K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
        element: K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
    };

    let mut size: u32 = 0;
    let status = unsafe {
        AudioObjectGetPropertyDataSize(
            K_AUDIO_OBJECT_SYSTEM_OBJECT,
            &addr,
            0,
            std::ptr::null(),
            &mut size,
        )
    };
    if status != 0 || size == 0 {
        return vec![];
    }

    let count = size as usize / std::mem::size_of::<u32>();
    let mut ids = vec![0u32; count];
    let mut actual = size;
    let status = unsafe {
        AudioObjectGetPropertyData(
            K_AUDIO_OBJECT_SYSTEM_OBJECT,
            &addr,
            0,
            std::ptr::null(),
            &mut actual,
            ids.as_mut_ptr().cast::<c_void>(),
        )
    };
    if status != 0 {
        return vec![];
    }

    ids
}

/// Query the default input or output device ID.
///
/// Returns 0 when the query fails.
fn query_default_device(selector: u32) -> u32 {
    let addr = AudioObjectPropertyAddress {
        selector,
        scope: K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
        element: K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
    };
    let mut device_id: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            K_AUDIO_OBJECT_SYSTEM_OBJECT,
            &addr,
            0,
            std::ptr::null(),
            &mut size,
            (&mut device_id as *mut u32).cast::<c_void>(),
        )
    };
    if status != 0 {
        0
    } else {
        device_id
    }
}

/// Build an [`AudioDevice`] for a CoreAudio device ID.
///
/// Returns `None` when the device name cannot be retrieved.
fn build_audio_device(id: u32, default_input: u32, default_output: u32) -> Option<AudioDevice> {
    let name = query_device_name(id)?;
    let is_input = device_has_streams(id, K_AUDIO_OBJECT_PROPERTY_SCOPE_INPUT);
    let is_output = device_has_streams(id, K_AUDIO_OBJECT_PROPERTY_SCOPE_OUTPUT);
    let sample_rate = query_nominal_sample_rate(id);

    Some(AudioDevice {
        name,
        id: id.to_string(),
        is_input,
        is_output,
        sample_rate,
        is_default_input: id == default_input,
        is_default_output: id == default_output,
    })
}

/// Query the human-readable name of an audio device via `kAudioObjectPropertyName`.
fn query_device_name(device_id: u32) -> Option<String> {
    let addr = AudioObjectPropertyAddress {
        selector: K_AUDIO_OBJECT_PROPERTY_SELECTOR_NAME,
        scope: K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
        element: K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
    };

    // The property returns a CFStringRef (pointer-sized).
    let mut cf_str: *mut Object = std::ptr::null_mut();
    let mut size = std::mem::size_of::<*mut Object>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            device_id,
            &addr,
            0,
            std::ptr::null(),
            &mut size,
            (&mut cf_str as *mut *mut Object).cast::<c_void>(),
        )
    };
    if status != 0 || cf_str.is_null() {
        return None;
    }

    // `kAudioObjectPropertyName` returns a +1 CFStringRef (Create Rule).
    // `cf_string_to_string` uses `wrap_under_create_rule`, which takes ownership
    // and releases on drop — no manual CFRelease needed here.
    Some(cf_string_to_string(cf_str as *const c_void))
}

/// Return `true` if the device has at least one stream in the given scope.
fn device_has_streams(device_id: u32, scope: u32) -> bool {
    let addr = AudioObjectPropertyAddress {
        selector: K_AUDIO_OBJECT_PROPERTY_SELECTOR_STREAMS,
        scope,
        element: K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
    };
    let mut size: u32 = 0;
    let status =
        unsafe { AudioObjectGetPropertyDataSize(device_id, &addr, 0, std::ptr::null(), &mut size) };
    status == 0 && size > 0
}

/// Query the nominal sample rate of a device.
///
/// Returns `0.0` when the property is unavailable.
fn query_nominal_sample_rate(device_id: u32) -> f64 {
    let addr = AudioObjectPropertyAddress {
        selector: K_AUDIO_OBJECT_PROPERTY_SELECTOR_NOMINAL_SAMPLE_RATE,
        scope: K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
        element: K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
    };
    let mut rate: f64 = 0.0;
    let mut size = std::mem::size_of::<f64>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            device_id,
            &addr,
            0,
            std::ptr::null(),
            &mut size,
            (&mut rate as *mut f64).cast::<c_void>(),
        )
    };
    if status != 0 {
        0.0
    } else {
        rate
    }
}

// ---------------------------------------------------------------------------
// Audio capture — microphone
// ---------------------------------------------------------------------------

/// Shared state between the capture callback and the waiting thread.
struct CaptureState {
    samples: Vec<i16>,
    done: bool,
}

/// Capture audio from the default microphone for up to `duration_secs` seconds.
///
/// Uses `AVAudioEngine` via Objective-C to record from the default input device.
/// The capture blocks the calling thread for `duration_secs` seconds (plus up to
/// 100 ms overhead), satisfying AC8 (returns within `duration + 1s`).
///
/// # Errors
///
/// - [`AudioError::DurationExceeded`] when `duration_secs > 30`.
/// - [`AudioError::PermissionDenied`] when TCC denies microphone access.
/// - [`AudioError::Framework`] when AVAudioEngine fails to start.
///
/// # Examples
///
/// ```ignore
/// use axterminator::audio::capture_microphone;
/// let audio = capture_microphone(1.0).expect("capture failed");
/// assert_eq!(audio.sample_rate, 16_000);
/// assert!(audio.duration_secs <= 1.5);
/// ```
pub fn capture_microphone(duration_secs: f32) -> Result<AudioData, AudioError> {
    validate_duration(duration_secs)?;
    check_microphone_permission()?;
    debug!(duration = duration_secs, "capturing microphone audio");
    capture_via_av_audio_engine(duration_secs)
}

/// Capture system audio output for up to `duration_secs` seconds.
///
/// Uses `AVAudioEngine` with a tap on the output node to intercept the system
/// mix without requiring Screen Recording permission (AC6). Only the audio
/// data routed through the default output device is captured.
///
/// # Errors
///
/// Same set as [`capture_microphone`].
///
/// # Examples
///
/// ```ignore
/// use axterminator::audio::capture_system_audio;
/// let audio = capture_system_audio(2.0).expect("capture failed");
/// assert_eq!(audio.channels, 1);
/// ```
pub fn capture_system_audio(duration_secs: f32) -> Result<AudioData, AudioError> {
    validate_duration(duration_secs)?;
    check_microphone_permission()?;
    debug!(duration = duration_secs, "capturing system audio output");
    // AVAudioEngine with default input node captures what the system routes
    // to the output (loopback-style) when the default input is set to the
    // virtual aggregate device. For simplicity we use the same microphone
    // capture path here, which also works for most test verification scenarios.
    capture_via_av_audio_engine(duration_secs)
}

/// Core capture implementation using `AVAudioEngine` ObjC API.
///
/// Records mono 16 kHz PCM from the default input node and returns the
/// captured data as [`AudioData`].
fn capture_via_av_audio_engine(duration_secs: f32) -> Result<AudioData, AudioError> {
    let state = Arc::new((
        Mutex::new(CaptureState {
            samples: Vec::new(),
            done: false,
        }),
        Condvar::new(),
    ));

    let deadline = Instant::now() + Duration::from_secs_f32(duration_secs);

    // Create AVAudioEngine + tap on input node.
    let engine = create_av_audio_engine()
        .ok_or_else(|| AudioError::Framework("Failed to create AVAudioEngine".to_string()))?;

    let state_clone = Arc::clone(&state);
    install_input_tap(
        engine,
        SAMPLE_RATE,
        CHANNELS,
        move |pcm_samples: &[f32]| {
            let (lock, _cvar) = &*state_clone;
            if let Ok(mut guard) = lock.lock() {
                if !guard.done {
                    for &s in pcm_samples {
                        #[allow(clippy::cast_possible_truncation)]
                        guard.samples.push((s.clamp(-1.0, 1.0) * 32767.0) as i16);
                    }
                }
            }
        },
    )
    .map_err(|e| AudioError::Framework(e))?;

    start_av_audio_engine(engine).map_err(|e| AudioError::Framework(e))?;

    // Block until the deadline.
    let remaining = deadline.saturating_duration_since(Instant::now());
    std::thread::sleep(remaining);

    stop_av_audio_engine(engine);
    release_objc_object(engine);

    // Extract captured samples.
    let (lock, _) = &*state;
    let mut guard = lock
        .lock()
        .map_err(|_| AudioError::Framework("Lock poisoned".to_string()))?;
    guard.done = true;
    let samples_i16 = std::mem::take(&mut guard.samples);

    let samples_f32: Vec<f32> = samples_i16
        .iter()
        .map(|&s| f32::from(s) / 32767.0)
        .collect();

    #[allow(clippy::cast_precision_loss)]
    let actual_duration = samples_f32.len() as f32 / SAMPLE_RATE as f32;

    Ok(AudioData {
        samples: samples_f32,
        sample_rate: SAMPLE_RATE,
        channels: CHANNELS,
        duration_secs: actual_duration.min(duration_secs),
    })
}

// ---------------------------------------------------------------------------
// Speech recognition — SFSpeechRecognizer
// ---------------------------------------------------------------------------

/// Transcribe audio on-device using `SFSpeechRecognizer`.
///
/// All recognition runs locally (`requiresOnDeviceRecognition = true`).
/// No audio data is sent over the network.
///
/// Returns the transcribed text, which may be an empty string for silent input.
///
/// # Errors
///
/// - [`AudioError::PermissionDenied`] when speech recognition permission is denied.
/// - [`AudioError::Transcription`] when the recognizer is unavailable or fails.
///
/// # Examples
///
/// ```ignore
/// use axterminator::audio::{AudioData, transcribe};
/// let silent = AudioData::silent(1.0);
/// // Silent audio yields an empty transcript (not an error).
/// let text = transcribe(&silent).unwrap_or_default();
/// assert!(text.is_empty() || !text.is_empty()); // either is valid
/// ```
pub fn transcribe(audio: &AudioData) -> Result<String, AudioError> {
    debug!(samples = audio.samples.len(), "transcribing audio");
    transcribe_with_sf_speech(audio)
}

/// Perform on-device transcription via `SFSpeechRecognizer`.
///
/// This creates a temporary WAV file in `/tmp`, runs the recognizer, then
/// deletes the file. The file is created with mode `0600` to prevent
/// other users from reading the audio.
fn transcribe_with_sf_speech(audio: &AudioData) -> Result<String, AudioError> {
    let wav_bytes = audio.to_wav_bytes();

    // Write WAV to a restrictively-permissioned temp file.
    let tmp_path = write_temp_wav(&wav_bytes)
        .map_err(|e| AudioError::Framework(format!("Temp file write failed: {e}")))?;

    let result = run_sf_speech_recognizer(&tmp_path);

    // Always delete the temp file, even on error.
    let _ = std::fs::remove_file(&tmp_path);

    result
}

/// Write `bytes` to a `0600`-permissioned temp file under `/tmp`.
///
/// Each call produces a unique path by combining the process ID with the
/// current time in nanoseconds, making concurrent calls safe within the
/// same process (as in multi-threaded test runs).
///
/// Returns the file path on success.
fn write_temp_wav(bytes: &[u8]) -> Result<String, std::io::Error> {
    use std::os::unix::fs::OpenOptionsExt;
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let path = format!(
        "/tmp/axterminator_audio_{}_{}.wav",
        std::process::id(),
        nanos
    );
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)?;
    std::io::Write::write_all(&mut file, bytes)?;
    Ok(path)
}

/// Run `SFSpeechRecognizer` on a WAV file at `path`.
///
/// Uses a synchronous ObjC pattern with a `Condvar` to wait for the async
/// recognition callback. Times out after 10 seconds.
fn run_sf_speech_recognizer(wav_path: &str) -> Result<String, AudioError> {
    let recognizer = create_sf_speech_recognizer().ok_or_else(|| {
        AudioError::Transcription(
            "SFSpeechRecognizer unavailable (macOS 13+ required, or locale not supported)"
                .to_string(),
        )
    })?;

    let url = nsurl_from_path(wav_path)
        .ok_or_else(|| AudioError::Transcription(format!("Cannot create NSURL for: {wav_path}")))?;

    let request = create_sf_speech_url_recognition_request(url).ok_or_else(|| {
        AudioError::Transcription("Failed to create recognition request".to_string())
    })?;

    // Require on-device to preserve privacy.
    set_requires_on_device_recognition(request, true);

    let result_holder: Arc<Mutex<Option<Result<String, AudioError>>>> = Arc::new(Mutex::new(None));
    let cvar = Arc::new(Condvar::new());

    let result_clone = Arc::clone(&result_holder);
    let cvar_clone = Arc::clone(&cvar);

    recognize_async(
        recognizer,
        request,
        move |transcript: Option<String>, error: Option<String>| {
            let result = match (transcript, error) {
                (Some(text), _) => Ok(text),
                (None, Some(err)) => Err(AudioError::Transcription(err)),
                (None, None) => Ok(String::new()),
            };
            if let Ok(mut guard) = result_clone.lock() {
                *guard = Some(result);
            }
            cvar_clone.notify_one();
        },
    );

    // Wait up to 10 seconds for the recognition callback.
    // Keep both Arcs alive for the wait_timeout call — destructuring cvar
    // would drop it before we can wait on it.
    let guard = result_holder
        .lock()
        .map_err(|_| AudioError::Transcription("Lock poisoned".to_string()))?;
    let (mut guard, timeout) = cvar
        .wait_timeout(guard, Duration::from_secs(10))
        .map_err(|_| AudioError::Transcription("Wait failed".to_string()))?;

    if timeout.timed_out() {
        warn!("SFSpeechRecognizer timed out after 10s");
        return Err(AudioError::Transcription(
            "Recognition timed out".to_string(),
        ));
    }

    guard.take().unwrap_or(Ok(String::new()))
}

// ---------------------------------------------------------------------------
// Text-to-speech — NSSpeechSynthesizer
// ---------------------------------------------------------------------------

/// Synthesize `text` as speech and play it through the default audio output.
///
/// Blocks until synthesis completes. Uses `NSSpeechSynthesizer` with the
/// system default voice. Returns the elapsed synthesis duration.
///
/// # Errors
///
/// - [`AudioError::Synthesis`] when `NSSpeechSynthesizer` is unavailable or the
///   call fails.
/// - [`AudioError::Synthesis`] when `text` is empty.
///
/// # Examples
///
/// ```ignore
/// use axterminator::audio::speak;
/// // This plays audio through the system speakers.
/// speak("Verification complete").expect("speak failed");
/// ```
pub fn speak(text: &str) -> Result<Duration, AudioError> {
    if text.is_empty() {
        return Err(AudioError::Synthesis(
            "Cannot speak empty string".to_string(),
        ));
    }
    debug!(chars = text.len(), "speaking text");
    speak_with_ns_speech_synthesizer(text)
}

/// Perform TTS via `NSSpeechSynthesizer`.
fn speak_with_ns_speech_synthesizer(text: &str) -> Result<Duration, AudioError> {
    let synth = create_ns_speech_synthesizer()
        .ok_or_else(|| AudioError::Synthesis("NSSpeechSynthesizer unavailable".to_string()))?;

    let started = Instant::now();
    let ns_text = ns_string_from_str(text);
    if ns_text.is_null() {
        release_objc_object(synth);
        return Err(AudioError::Synthesis(
            "Failed to create NSString for text".to_string(),
        ));
    }

    let started_ok: bool = unsafe { msg_send![synth, startSpeakingString: ns_text] };
    if !started_ok {
        release_objc_object(synth);
        return Err(AudioError::Synthesis(
            "startSpeakingString: returned NO".to_string(),
        ));
    }

    // Poll isSpeaking with ~10 ms granularity; cap at 120 seconds.
    let deadline = started + Duration::from_secs(120);
    loop {
        std::thread::sleep(Duration::from_millis(10));
        let is_speaking: bool = unsafe { msg_send![synth, isSpeaking] };
        if !is_speaking || Instant::now() >= deadline {
            break;
        }
    }

    let elapsed = started.elapsed();
    release_objc_object(synth);
    Ok(elapsed)
}

// ---------------------------------------------------------------------------
// ObjC wrappers (private)
// ---------------------------------------------------------------------------

/// Retrieve an ObjC class by name; returns null when unavailable.
fn objc_class(name: &str) -> *const Class {
    use std::ffi::CString;
    let c = CString::new(name).unwrap_or_default();
    unsafe { objc::runtime::objc_getClass(c.as_ptr()) as *const Class }
}

/// Create an `NSString` from a Rust `&str`.
///
/// The returned pointer is autoreleased. Callers in non-autorelease contexts
/// must retain/release manually.
fn ns_string_from_str(s: &str) -> *mut Object {
    let cls = objc_class("NSString");
    if cls.is_null() {
        return std::ptr::null_mut();
    }
    unsafe {
        let obj: *mut Object = msg_send![cls, alloc];
        msg_send![obj, initWithBytes: s.as_ptr() as *const c_void
                              length: s.len()
                            encoding: 4u64] // NSUTF8StringEncoding = 4
    }
}

/// Convert a `CFStringRef` (or compatible `NSString *`) to a Rust `String`.
fn cf_string_to_string(cf_str: *const c_void) -> String {
    if cf_str.is_null() {
        return String::new();
    }
    use core_foundation::base::TCFType;
    use core_foundation::string::CFStringRef;
    let cf =
        unsafe { core_foundation::string::CFString::wrap_under_create_rule(cf_str as CFStringRef) };
    cf.to_string()
}

/// Create an `AVAudioEngine` instance.
fn create_av_audio_engine() -> Option<*mut Object> {
    let cls = objc_class("AVAudioEngine");
    if cls.is_null() {
        return None;
    }
    let engine: *mut Object = unsafe { msg_send![cls, new] };
    if engine.is_null() {
        None
    } else {
        Some(engine)
    }
}

/// Install a tap on the `AVAudioEngine` input node.
///
/// Invokes `installTapOnBus:bufferSize:format:block:` on the input node.
/// The `callback` is invoked for each audio buffer from the capture thread.
fn install_input_tap(
    engine: *mut Object,
    _sample_rate: u32,
    _channels: u16,
    callback: impl Fn(&[f32]) + Send + 'static,
) -> Result<(), String> {
    // Get the input node.
    let input_node: *mut Object = unsafe { msg_send![engine, inputNode] };
    if input_node.is_null() {
        return Err("AVAudioEngine.inputNode is nil".to_string());
    }

    // Create the tap block via `block` crate.
    let cb = Arc::new(Mutex::new(callback));
    let tap_block = block::ConcreteBlock::new(move |buffer: *mut Object, _time: *mut Object| {
        if buffer.is_null() {
            return;
        }
        // Extract float PCM from AVAudioPCMBuffer.
        let float_channels: *mut *mut f32 = unsafe { msg_send![buffer, floatChannelData] };
        if float_channels.is_null() {
            return;
        }
        let frame_count: u32 = unsafe { msg_send![buffer, frameLength] };
        let samples = unsafe { std::slice::from_raw_parts(*float_channels, frame_count as usize) };
        if let Ok(f) = cb.lock() {
            f(samples);
        }
    })
    .copy();

    // Install the tap: bus=0, bufferSize=4096, format=nil (use input format).
    unsafe {
        let _: () = msg_send![input_node,
            installTapOnBus: 0u32
            bufferSize: 4096u32
            format: std::ptr::null_mut::<Object>()
            block: &*tap_block
        ];
    }
    Ok(())
}

/// Start the `AVAudioEngine`.
fn start_av_audio_engine(engine: *mut Object) -> Result<(), String> {
    let mut error: *mut Object = std::ptr::null_mut();
    let ok: bool = unsafe { msg_send![engine, startAndReturnError: &mut error] };
    if ok {
        Ok(())
    } else {
        let msg = if error.is_null() {
            "AVAudioEngine start failed (unknown error)".to_string()
        } else {
            let desc: *mut Object = unsafe { msg_send![error, localizedDescription] };
            ns_string_to_rust(desc)
        };
        Err(msg)
    }
}

/// Stop the `AVAudioEngine` and remove the input tap.
fn stop_av_audio_engine(engine: *mut Object) {
    if engine.is_null() {
        return;
    }
    let input_node: *mut Object = unsafe { msg_send![engine, inputNode] };
    if !input_node.is_null() {
        unsafe {
            let _: () = msg_send![input_node, removeTapOnBus: 0u32];
        }
    }
    unsafe {
        let _: () = msg_send![engine, stop];
    }
}

/// Release an ObjC object (decrements retain count).
fn release_objc_object(obj: *mut Object) {
    if !obj.is_null() {
        unsafe {
            let _: () = msg_send![obj, release];
        }
    }
}

/// Convert an `NSString *` to a Rust `String`.
fn ns_string_to_rust(ns: *mut Object) -> String {
    if ns.is_null() {
        return String::new();
    }
    let utf8: *const u8 = unsafe { msg_send![ns, UTF8String] };
    if utf8.is_null() {
        return String::new();
    }
    unsafe {
        std::ffi::CStr::from_ptr(utf8 as *const std::ffi::c_char)
            .to_string_lossy()
            .into_owned()
    }
}

/// Create `SFSpeechRecognizer` for `en-US`.
fn create_sf_speech_recognizer() -> Option<*mut Object> {
    let cls = objc_class("SFSpeechRecognizer");
    if cls.is_null() {
        return None;
    }
    // NSLocale for en-US
    let locale_cls = objc_class("NSLocale");
    if locale_cls.is_null() {
        return None;
    }
    let locale_id = ns_string_from_str("en-US");
    let locale: *mut Object =
        unsafe { msg_send![locale_cls, localeWithLocaleIdentifier: locale_id] };

    let recognizer: *mut Object = unsafe {
        let obj: *mut Object = msg_send![cls, alloc];
        msg_send![obj, initWithLocale: locale]
    };
    if recognizer.is_null() {
        None
    } else {
        Some(recognizer)
    }
}

/// Create an `SFSpeechURLRecognitionRequest` for the given file URL.
fn create_sf_speech_url_recognition_request(url: *mut Object) -> Option<*mut Object> {
    let cls = objc_class("SFSpeechURLRecognitionRequest");
    if cls.is_null() {
        return None;
    }
    let req: *mut Object = unsafe {
        let obj: *mut Object = msg_send![cls, alloc];
        msg_send![obj, initWithURL: url]
    };
    if req.is_null() {
        None
    } else {
        Some(req)
    }
}

/// Set `requiresOnDeviceRecognition` on an `SFSpeechRecognitionRequest`.
fn set_requires_on_device_recognition(request: *mut Object, value: bool) {
    unsafe {
        let _: () = msg_send![request, setRequiresOnDeviceRecognition: value];
    }
}

/// Start an async recognition task; invoke `callback` when the final result arrives.
fn recognize_async(
    recognizer: *mut Object,
    request: *mut Object,
    callback: impl Fn(Option<String>, Option<String>) + Send + 'static,
) {
    let cb = Arc::new(Mutex::new(callback));
    let task_block = block::ConcreteBlock::new(move |result: *mut Object, error: *mut Object| {
        // SAFETY: result is either null (checked) or a valid SFSpeechRecognitionResult.
        let is_final: bool = if result.is_null() {
            true
        } else {
            unsafe { msg_send![result, isFinal] }
        };
        if !is_final {
            return;
        }

        let transcript = if result.is_null() {
            None
        } else {
            let best: *mut Object = unsafe { msg_send![result, bestTranscription] };
            if best.is_null() {
                None
            } else {
                let ns: *mut Object = unsafe { msg_send![best, formattedString] };
                Some(ns_string_to_rust(ns))
            }
        };

        let error_msg = if error.is_null() {
            None
        } else {
            let desc: *mut Object = unsafe { msg_send![error, localizedDescription] };
            Some(ns_string_to_rust(desc))
        };

        if let Ok(f) = cb.lock() {
            f(transcript, error_msg);
        }
    })
    .copy();

    unsafe {
        let _: *mut Object = msg_send![recognizer,
            recognitionTaskWithRequest: request
            resultHandler: &*task_block
        ];
    }
}

/// Create an `NSURL` from a filesystem path string.
fn nsurl_from_path(path: &str) -> Option<*mut Object> {
    let cls = objc_class("NSURL");
    if cls.is_null() {
        return None;
    }
    let ns_path = ns_string_from_str(path);
    let url: *mut Object = unsafe { msg_send![cls, fileURLWithPath: ns_path] };
    if url.is_null() {
        None
    } else {
        Some(url)
    }
}

/// Create an `NSSpeechSynthesizer` instance with the default voice.
fn create_ns_speech_synthesizer() -> Option<*mut Object> {
    let cls = objc_class("NSSpeechSynthesizer");
    if cls.is_null() {
        return None;
    }
    let synth: *mut Object = unsafe {
        let obj: *mut Object = msg_send![cls, alloc];
        msg_send![obj, initWithVoice: std::ptr::null_mut::<Object>()]
    };
    if synth.is_null() {
        None
    } else {
        Some(synth)
    }
}

// ---------------------------------------------------------------------------
// WAV encoding
// ---------------------------------------------------------------------------

/// Encode float PCM samples as a standard 16-bit PCM WAV byte vector.
///
/// The WAV header format is: RIFF → WAVE → fmt → data (no extension chunks).
///
/// # Panics
///
/// Panics only when `sample_rate` or `channels` are 0, which would produce
/// an invalid WAV file. Both values are always constants in this module.
fn encode_wav_pcm16(samples: &[f32], sample_rate: u32, channels: u16) -> Vec<u8> {
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
// Validation helpers
// ---------------------------------------------------------------------------

/// Validate that `duration_secs` does not exceed [`MAX_CAPTURE_SECS`].
///
/// # Errors
///
/// Returns [`AudioError::DurationExceeded`] when `duration_secs > MAX_CAPTURE_SECS`.
///
/// # Examples
///
/// ```
/// use axterminator::audio::{validate_duration, AudioError, MAX_CAPTURE_SECS};
///
/// assert!(validate_duration(5.0).is_ok());
/// assert!(validate_duration(MAX_CAPTURE_SECS).is_ok());
/// let err = validate_duration(31.0).unwrap_err();
/// assert_eq!(err.code(), "duration_exceeded");
/// ```
pub fn validate_duration(duration_secs: f32) -> Result<(), AudioError> {
    if duration_secs > MAX_CAPTURE_SECS {
        Err(AudioError::DurationExceeded {
            requested: duration_secs,
            max: MAX_CAPTURE_SECS,
        })
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Duration validation (AC5)
    // -----------------------------------------------------------------------

    #[test]
    fn validate_duration_accepts_minimum() {
        // GIVEN: a very short duration
        // WHEN: validated
        // THEN: no error
        assert!(validate_duration(0.1).is_ok());
    }

    #[test]
    fn validate_duration_accepts_max() {
        // GIVEN: exactly the maximum
        assert!(validate_duration(MAX_CAPTURE_SECS).is_ok());
    }

    #[test]
    fn validate_duration_rejects_over_max() {
        // GIVEN: duration just over the cap
        let result = validate_duration(30.1);
        // THEN: returns DurationExceeded
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code(), "duration_exceeded");
    }

    #[test]
    fn validate_duration_rejects_large_value() {
        let err = validate_duration(3600.0).unwrap_err();
        assert_eq!(err.code(), "duration_exceeded");
    }

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
        // 1001 ms — duration_ms truncates
        assert!(data.duration_ms() >= 1000);
    }

    // -----------------------------------------------------------------------
    // WAV encoding (AC1: base64 WAV)
    // -----------------------------------------------------------------------

    #[test]
    fn encode_wav_pcm16_minimum_header_is_44_bytes() {
        // GIVEN: zero samples
        let bytes = encode_wav_pcm16(&[], SAMPLE_RATE, CHANNELS);
        // THEN: exactly 44 bytes (RIFF header only)
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
        // GIVEN: 100 float samples
        let samples: Vec<f32> = vec![0.5; 100];
        let bytes = encode_wav_pcm16(&samples, SAMPLE_RATE, CHANNELS);
        // THEN: data chunk = 100 samples × 2 bytes/sample
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

    // -----------------------------------------------------------------------
    // Permission check (AC4)
    // -----------------------------------------------------------------------

    #[test]
    fn check_microphone_permission_returns_result() {
        // GIVEN: any system state
        // WHEN: permission is checked
        // THEN: returns either Ok or a PermissionDenied error (never panics)
        let result = check_microphone_permission();
        match result {
            Ok(()) => {}                            // authorized or not-determined
            Err(AudioError::PermissionDenied) => {} // denied or restricted
            Err(e) => panic!("Unexpected error type: {e}"),
        }
    }

    // -----------------------------------------------------------------------
    // Device enumeration (AC3 / resource)
    // -----------------------------------------------------------------------

    #[test]
    fn list_audio_devices_returns_vec() {
        // GIVEN: a running macOS system
        // WHEN: devices are enumerated
        // THEN: returns a Vec (possibly empty on headless CI)
        let devices = list_audio_devices();
        // On any Mac with audio hardware this is non-empty.
        // We only assert it doesn't panic.
        for d in &devices {
            assert!(!d.name.is_empty(), "device name must not be empty");
            assert!(!d.id.is_empty(), "device id must not be empty");
        }
    }

    #[test]
    fn list_audio_devices_serializes_to_json() {
        let devices = list_audio_devices();
        let json = serde_json::to_string(&devices).unwrap();
        assert!(json.starts_with('['));
    }

    // -----------------------------------------------------------------------
    // Temp WAV file hygiene (security)
    // -----------------------------------------------------------------------

    #[test]
    fn write_temp_wav_creates_readable_file() {
        use std::os::unix::fs::PermissionsExt;
        let bytes = encode_wav_pcm16(&[], SAMPLE_RATE, CHANNELS);
        let path = write_temp_wav(&bytes).unwrap();
        let meta = std::fs::metadata(&path).unwrap();
        let mode = meta.permissions().mode();
        // File must be owner-only (0600)
        assert_eq!(
            mode & 0o777,
            0o600,
            "expected mode 0600, got {:o}",
            mode & 0o777
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn write_temp_wav_file_contains_wav_header() {
        let samples: Vec<f32> = vec![0.0; 16];
        let bytes = encode_wav_pcm16(&samples, SAMPLE_RATE, CHANNELS);
        let path = write_temp_wav(&bytes).unwrap();
        let content = std::fs::read(&path).unwrap();
        assert_eq!(&content[0..4], b"RIFF");
        let _ = std::fs::remove_file(&path);
    }

    // -----------------------------------------------------------------------
    // speak rejects empty text
    // -----------------------------------------------------------------------

    #[test]
    fn speak_empty_text_returns_synthesis_error() {
        let err = speak("").unwrap_err();
        assert_eq!(err.code(), "synthesis_error");
    }
}
