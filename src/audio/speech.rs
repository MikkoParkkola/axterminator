//! On-device speech recognition (`SFSpeechRecognizer`) and text-to-speech
//! (`NSSpeechSynthesizer`).

use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use objc::runtime::Object;
#[allow(unused_imports)]
use objc::{msg_send, sel, sel_impl};
use tracing::{debug, warn};

use super::ffi::{ns_string_from_str, ns_string_to_rust, objc_class, release_objc_object};
use super::{AudioData, AudioError};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Transcribe audio on-device using `SFSpeechRecognizer`.
///
/// All recognition runs locally (`requiresOnDeviceRecognition = true`).
/// No audio data is sent over the network.
///
/// Returns the transcribed text, which may be empty for silent input.
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
/// let text = transcribe(&silent).unwrap_or_default();
/// assert!(text.is_empty() || !text.is_empty()); // either is valid
/// ```
pub fn transcribe(audio: &AudioData) -> Result<String, AudioError> {
    debug!(samples = audio.samples.len(), "transcribing audio");
    transcribe_with_sf_speech(audio)
}

/// Synthesize `text` as speech and play it through the default audio output.
///
/// Blocks until synthesis completes. Uses `NSSpeechSynthesizer` with the
/// system default voice. Returns the elapsed synthesis duration.
///
/// # Errors
///
/// - [`AudioError::Synthesis`] when `NSSpeechSynthesizer` is unavailable or fails.
/// - [`AudioError::Synthesis`] when `text` is empty.
///
/// # Examples
///
/// ```ignore
/// use axterminator::audio::speak;
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

// ---------------------------------------------------------------------------
// Speech recognition internals
// ---------------------------------------------------------------------------

/// Perform on-device transcription via `SFSpeechRecognizer`.
///
/// Writes a temporary WAV file in `/tmp`, runs the recognizer, then deletes it.
fn transcribe_with_sf_speech(audio: &AudioData) -> Result<String, AudioError> {
    let wav_bytes = audio.to_wav_bytes();

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
/// same process.
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

    let url = nsurl_from_path(wav_path).ok_or_else(|| {
        AudioError::Transcription(format!("Cannot create NSURL for: {wav_path}"))
    })?;

    let request = create_sf_speech_url_recognition_request(url).ok_or_else(|| {
        AudioError::Transcription("Failed to create recognition request".to_string())
    })?;

    set_requires_on_device_recognition(request, true);

    let result_holder: Arc<Mutex<Option<Result<String, AudioError>>>> =
        Arc::new(Mutex::new(None));
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

/// Start an async recognition task; invoke `callback` when the final result arrives.
fn recognize_async(
    recognizer: *mut Object,
    request: *mut Object,
    callback: impl Fn(Option<String>, Option<String>) + Send + 'static,
) {
    let cb = Arc::new(Mutex::new(callback));
    let task_block =
        block::ConcreteBlock::new(move |result: *mut Object, error: *mut Object| {
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

/// Create `SFSpeechRecognizer` for `en-US`.
fn create_sf_speech_recognizer() -> Option<*mut Object> {
    let cls = objc_class("SFSpeechRecognizer");
    if cls.is_null() {
        return None;
    }
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

// ---------------------------------------------------------------------------
// TTS internals
// ---------------------------------------------------------------------------

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
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{encode_wav_pcm16, CHANNELS, SAMPLE_RATE};

    #[test]
    fn speak_empty_text_returns_synthesis_error() {
        let err = speak("").unwrap_err();
        assert_eq!(err.code(), "synthesis_error");
    }

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
}
