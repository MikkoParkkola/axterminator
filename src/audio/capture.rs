//! Microphone and system audio capture via `AVAudioEngine`.

use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use objc::runtime::Object;
#[allow(unused_imports)]
use objc::{msg_send, sel, sel_impl};
use tracing::debug;

use super::devices::check_microphone_permission;
use super::ffi::{ns_string_to_rust, objc_class, release_objc_object};
use super::{AudioData, AudioError, CHANNELS, MAX_CAPTURE_SECS, MIN_CAPTURE_SECS, SAMPLE_RATE};

/// Requested or effective audio source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioCaptureSource {
    Microphone,
    System,
}

impl AudioCaptureSource {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Microphone => "microphone",
            Self::System => "system",
        }
    }
}

/// Concrete backend used to satisfy an audio capture request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioCaptureBackend {
    AvAudioEngineInput,
    AvAudioEngineInputFallback,
    ScreenCaptureKitAudioOnly,
}

impl AudioCaptureBackend {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AvAudioEngineInput => "av_audio_engine_input",
            Self::AvAudioEngineInputFallback => "av_audio_engine_input_fallback",
            Self::ScreenCaptureKitAudioOnly => "screen_capturekit_audio_only",
        }
    }
}

/// Audio plus truthful metadata about what backend/source actually produced it.
#[derive(Debug, Clone)]
pub struct CapturedAudio {
    pub audio: AudioData,
    pub source_used: AudioCaptureSource,
    pub capture_backend: AudioCaptureBackend,
}

// ---------------------------------------------------------------------------
// Internal state
// ---------------------------------------------------------------------------

/// Shared state between the AVAudioEngine tap callback and the waiting thread.
pub(super) struct CaptureState {
    pub(super) samples: Vec<i16>,
    pub(super) done: bool,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Validate that `duration_secs` stays within the supported capture range.
///
/// # Errors
///
/// Returns [`AudioError::InvalidDuration`] when `duration_secs` is non-finite or
/// below [`MIN_CAPTURE_SECS`], and [`AudioError::DurationExceeded`] when it
/// exceeds [`MAX_CAPTURE_SECS`].
///
/// # Examples
///
/// ```
/// use axterminator::audio::{validate_duration, AudioError, MAX_CAPTURE_SECS};
///
/// assert!(validate_duration(5.0).is_ok());
/// assert!(validate_duration(MIN_CAPTURE_SECS).is_ok());
/// assert!(validate_duration(MAX_CAPTURE_SECS).is_ok());
/// let err = validate_duration(31.0).unwrap_err();
/// assert_eq!(err.code(), "duration_exceeded");
/// ```
pub fn validate_duration(duration_secs: f32) -> Result<(), AudioError> {
    if !duration_secs.is_finite() || duration_secs < MIN_CAPTURE_SECS {
        Err(AudioError::InvalidDuration {
            requested: duration_secs,
            min: MIN_CAPTURE_SECS,
        })
    } else if duration_secs > MAX_CAPTURE_SECS {
        Err(AudioError::DurationExceeded {
            requested: duration_secs,
            max: MAX_CAPTURE_SECS,
        })
    } else {
        Ok(())
    }
}

/// Capture audio from the default microphone for up to `duration_secs` seconds.
///
/// Uses `AVAudioEngine` via Objective-C to record from the default input device.
/// The capture blocks the calling thread for `duration_secs` seconds (plus up to
/// 100 ms overhead).
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
    Ok(capture_microphone_with_metadata(duration_secs)?.audio)
}

/// Capture microphone audio and report the concrete backend/source used.
pub fn capture_microphone_with_metadata(duration_secs: f32) -> Result<CapturedAudio, AudioError> {
    validate_duration(duration_secs)?;
    check_microphone_permission()?;
    debug!(duration = duration_secs, "capturing microphone audio");
    Ok(CapturedAudio {
        audio: capture_via_av_audio_engine(duration_secs)?,
        source_used: AudioCaptureSource::Microphone,
        capture_backend: AudioCaptureBackend::AvAudioEngineInput,
    })
}

/// Capture system audio output for up to `duration_secs` seconds.
///
/// On macOS 14+, uses ScreenCaptureKit in audio-only mode (`width=0, height=0`)
/// which does **not** require Screen Recording TCC permission — a significantly
/// better UX than the AVAudioEngine fallback.
///
/// On macOS 13 and earlier (or if SCK fails), falls back to `AVAudioEngine`
/// input-node tap, which requires microphone TCC permission.
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
    Ok(capture_system_audio_with_metadata(duration_secs)?.audio)
}

/// Capture system audio and report the concrete backend/source used.
pub fn capture_system_audio_with_metadata(duration_secs: f32) -> Result<CapturedAudio, AudioError> {
    validate_duration(duration_secs)?;

    // Prefer ScreenCaptureKit on macOS 14+ (no Screen Recording permission needed).
    if super::sck_capture::sck_available() {
        debug!(
            duration = duration_secs,
            "attempting SCK audio-only capture (macOS 14+)"
        );
        match super::sck_capture::capture_system_audio_sck(duration_secs) {
            Ok(audio) => {
                return Ok(CapturedAudio {
                    audio,
                    source_used: AudioCaptureSource::System,
                    capture_backend: AudioCaptureBackend::ScreenCaptureKitAudioOnly,
                });
            }
            Err(e) => {
                tracing::warn!(error = %e, "SCK capture failed, falling back to AVAudioEngine");
            }
        }
    }

    // Fallback: AVAudioEngine (requires microphone permission).
    check_microphone_permission()?;
    debug!(
        duration = duration_secs,
        "capturing system audio via AVAudioEngine (fallback)"
    );
    Ok(CapturedAudio {
        audio: capture_via_av_audio_engine(duration_secs)?,
        source_used: AudioCaptureSource::Microphone,
        capture_backend: AudioCaptureBackend::AvAudioEngineInputFallback,
    })
}

// ---------------------------------------------------------------------------
// Private implementation
// ---------------------------------------------------------------------------

/// Core capture implementation using `AVAudioEngine` ObjC API.
///
/// Captures at the input device's native sample rate (typically 48 kHz)
/// to avoid format mismatches and downsampling artifacts. The WAV header
/// reflects the true sample rate so SFSpeechRecognizer processes it correctly.
fn capture_via_av_audio_engine(duration_secs: f32) -> Result<AudioData, AudioError> {
    let state = Arc::new((
        Mutex::new(CaptureState {
            samples: Vec::new(),
            done: false,
        }),
        Condvar::new(),
    ));

    let deadline = Instant::now() + Duration::from_secs_f32(duration_secs);

    let engine = create_av_audio_engine()
        .ok_or_else(|| AudioError::Framework("Failed to create AVAudioEngine".to_string()))?;

    // Query the native sample rate so we record without resampling.
    let native_rate = query_input_sample_rate(engine);

    let state_clone = Arc::clone(&state);
    install_input_tap(
        engine,
        native_rate,
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
    .map_err(AudioError::Framework)?;

    start_av_audio_engine(engine).map_err(AudioError::Framework)?;

    let remaining = deadline.saturating_duration_since(Instant::now());
    std::thread::sleep(remaining);

    stop_av_audio_engine(engine);
    release_objc_object(engine);

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
    let actual_duration = samples_f32.len() as f32 / native_rate as f32;

    Ok(AudioData {
        samples: samples_f32,
        sample_rate: native_rate,
        channels: CHANNELS,
        duration_secs: actual_duration.min(duration_secs),
    })
}

/// Query the input node's native sample rate.
fn query_input_sample_rate(engine: *mut Object) -> u32 {
    let input_node: *mut Object = unsafe { msg_send![engine, inputNode] };
    if input_node.is_null() {
        return SAMPLE_RATE;
    }
    let format: *mut Object = unsafe { msg_send![input_node, outputFormatForBus: 0u32] };
    if format.is_null() {
        return SAMPLE_RATE;
    }
    let rate: f64 = unsafe { msg_send![format, sampleRate] };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let rate_u32 = rate as u32;
    if rate_u32 == 0 {
        SAMPLE_RATE
    } else {
        rate_u32
    }
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
    let input_node: *mut Object = unsafe { msg_send![engine, inputNode] };
    if input_node.is_null() {
        return Err("AVAudioEngine.inputNode is nil".to_string());
    }

    let cb = Arc::new(Mutex::new(callback));
    let tap_block = block::ConcreteBlock::new(move |buffer: *mut Object, _time: *mut Object| {
        if buffer.is_null() {
            return;
        }
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

    // Install tap with null format = capture at device's native sample rate.
    // The caller (capture_via_av_audio_engine) queries the native rate and
    // writes the WAV header accordingly.
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
        return Ok(());
    }
    let msg = if error.is_null() {
        "AVAudioEngine start failed (unknown error)".to_string()
    } else {
        let desc: *mut Object = unsafe { msg_send![error, localizedDescription] };
        ns_string_to_rust(desc)
    };
    Err(msg)
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_duration_accepts_minimum() {
        // GIVEN: a very short duration
        assert!(validate_duration(MIN_CAPTURE_SECS).is_ok());
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

    #[test]
    fn validate_duration_rejects_zero() {
        let err = validate_duration(0.0).unwrap_err();
        assert_eq!(err.code(), "invalid_duration");
    }

    #[test]
    fn validate_duration_rejects_negative() {
        let err = validate_duration(-1.0).unwrap_err();
        assert_eq!(err.code(), "invalid_duration");
    }

    #[test]
    fn validate_duration_rejects_non_finite() {
        let err = validate_duration(f32::INFINITY).unwrap_err();
        assert_eq!(err.code(), "invalid_duration");
        let err = validate_duration(f32::NAN).unwrap_err();
        assert_eq!(err.code(), "invalid_duration");
    }

    #[test]
    fn audio_capture_source_strings_are_stable() {
        assert_eq!(AudioCaptureSource::Microphone.as_str(), "microphone");
        assert_eq!(AudioCaptureSource::System.as_str(), "system");
    }

    #[test]
    fn audio_capture_backend_strings_are_stable() {
        assert_eq!(
            AudioCaptureBackend::AvAudioEngineInput.as_str(),
            "av_audio_engine_input"
        );
        assert_eq!(
            AudioCaptureBackend::AvAudioEngineInputFallback.as_str(),
            "av_audio_engine_input_fallback"
        );
        assert_eq!(
            AudioCaptureBackend::ScreenCaptureKitAudioOnly.as_str(),
            "screen_capturekit_audio_only"
        );
    }
}
