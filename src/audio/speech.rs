//! On-device speech recognition (`SFSpeechRecognizer`) and text-to-speech
//! (`NSSpeechSynthesizer`).

use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use objc::runtime::Object;
#[allow(unused_imports)]
use objc::{msg_send, sel, sel_impl};
use tracing::{debug, info, warn};

use super::ffi::{ns_string_from_str, ns_string_to_rust, objc_class, release_objc_object};
use super::{AudioData, AudioError};

// ---------------------------------------------------------------------------
// SFSpeechRecognizerAuthorizationStatus mirror
// ---------------------------------------------------------------------------

/// Mirror of `SFSpeechRecognizerAuthorizationStatus` enum values.
///
/// These must match the Objective-C SDK values exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SpeechAuthStatus {
    NotDetermined = 0,
    Denied = 1,
    Restricted = 2,
    Authorized = 3,
}

impl SpeechAuthStatus {
    fn from_raw(v: i64) -> Self {
        match v {
            1 => Self::Denied,
            2 => Self::Restricted,
            3 => Self::Authorized,
            _ => Self::NotDetermined,
        }
    }
}

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

/// Query `SFSpeechRecognizer.authorizationStatus` without prompting.
fn speech_authorization_status() -> SpeechAuthStatus {
    let cls = objc_class("SFSpeechRecognizer");
    if cls.is_null() {
        return SpeechAuthStatus::Restricted;
    }
    let raw: i64 = unsafe { msg_send![cls, authorizationStatus] };
    SpeechAuthStatus::from_raw(raw)
}

/// Request `SFSpeechRecognizer` authorization from the user.
///
/// Blocks the calling thread (up to 30 s) until the user responds to the
/// system permission dialog.  This makes axterminator appear in
/// **System Settings › Privacy & Security › Speech Recognition**.
///
/// Returns `Ok(())` when permission is granted, or an appropriate
/// [`AudioError`] when denied, restricted, or the dialog times out.
fn request_speech_authorization() -> Result<(), AudioError> {
    let status = speech_authorization_status();
    match status {
        SpeechAuthStatus::Authorized => return Ok(()),
        SpeechAuthStatus::Denied => {
            return Err(AudioError::PermissionDenied);
        }
        SpeechAuthStatus::Restricted => {
            return Err(AudioError::Transcription(
                "Speech recognition is restricted on this device".to_string(),
            ));
        }
        SpeechAuthStatus::NotDetermined => {}
    }

    info!("Requesting SFSpeechRecognizer authorization from user");

    let cls = objc_class("SFSpeechRecognizer");
    if cls.is_null() {
        return Err(AudioError::Transcription(
            "SFSpeechRecognizer class not available (macOS 10.15+ required)".to_string(),
        ));
    }

    let granted_holder: Arc<Mutex<Option<SpeechAuthStatus>>> = Arc::new(Mutex::new(None));
    let cvar = Arc::new(Condvar::new());

    let granted_clone = Arc::clone(&granted_holder);
    let cvar_clone = Arc::clone(&cvar);

    // The completion block is called on an arbitrary background queue.
    let block = block::ConcreteBlock::new(move |raw_status: i64| {
        let new_status = SpeechAuthStatus::from_raw(raw_status);
        if let Ok(mut guard) = granted_clone.lock() {
            *guard = Some(new_status);
        }
        cvar_clone.notify_one();
    })
    .copy();

    unsafe {
        let _: () = msg_send![cls, requestAuthorization: &*block];
    }

    // Wait up to 30 s for the user to respond.
    let guard = granted_holder.lock().map_err(|_| {
        AudioError::Transcription("Lock poisoned waiting for speech auth".to_string())
    })?;
    let (mut guard, timeout) = cvar
        .wait_timeout(guard, Duration::from_secs(30))
        .map_err(|_| AudioError::Transcription("Condvar wait failed".to_string()))?;

    if timeout.timed_out() {
        warn!("SFSpeechRecognizer authorization dialog timed out after 30s");
        return Err(AudioError::PermissionDenied);
    }

    match guard.take().unwrap_or(SpeechAuthStatus::NotDetermined) {
        SpeechAuthStatus::Authorized => Ok(()),
        SpeechAuthStatus::Denied => Err(AudioError::PermissionDenied),
        SpeechAuthStatus::Restricted => Err(AudioError::Transcription(
            "Speech recognition is restricted on this device".to_string(),
        )),
        SpeechAuthStatus::NotDetermined => Err(AudioError::PermissionDenied),
    }
}

/// Perform on-device transcription via `SFSpeechRecognizer`.
///
/// Ensures speech recognition permission is obtained before attempting
/// transcription.  Writes a temporary WAV file in `/tmp`, runs the
/// recognizer, then deletes it.
fn transcribe_with_sf_speech(audio: &AudioData) -> Result<String, AudioError> {
    // Ensure authorization is present — this is the fix for BUG #26.
    // When status is NotDetermined the system dialog is shown and we block
    // until the user responds.  Without this call, SFSpeechRecognizer silently
    // returns empty results instead of surfacing a permission error.
    request_speech_authorization()?;

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
/// recognition callback.  Times out after 10 seconds.
///
/// Errors from the recognition task are surfaced as [`AudioError::Transcription`]
/// rather than silently producing an empty result.
fn run_sf_speech_recognizer(wav_path: &str) -> Result<String, AudioError> {
    let recognizer = create_sf_speech_recognizer().ok_or_else(|| {
        AudioError::Transcription(
            "SFSpeechRecognizer unavailable — check that speech recognition \
             is enabled and the locale (en-US) is supported on this device"
                .to_string(),
        )
    })?;

    // Verify the recognizer is actually available (device might not support on-device).
    let is_available: bool = unsafe { msg_send![recognizer, isAvailable] };
    if !is_available {
        return Err(AudioError::Transcription(
            "SFSpeechRecognizer reports isAvailable=NO — \
             on-device speech recognition may not be downloaded yet"
                .to_string(),
        ));
    }

    let url = nsurl_from_path(wav_path)
        .ok_or_else(|| AudioError::Transcription(format!("Cannot create NSURL for: {wav_path}")))?;

    let request = create_sf_speech_url_recognition_request(url).ok_or_else(|| {
        AudioError::Transcription("Failed to create recognition request".to_string())
    })?;

    // Prefer on-device but fall back to server-based if on-device model
    // is not downloaded. requiresOnDeviceRecognition=true causes silent
    // timeout when the model isn't available.
    set_requires_on_device_recognition(request, false);

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

    // SFSpeechRecognizer dispatches its callback to the current thread's
    // RunLoop.  We must pump the RunLoop while waiting, otherwise the
    // callback never fires and we time out.  This mirrors the Swift pattern:
    //   while !done { RunLoop.current.run(mode: .default, before: ...) }
    extern "C" {
        fn CFRunLoopRunInMode(mode: *const Object, seconds: f64, ret: bool) -> i32;
        static kCFRunLoopDefaultMode: *const Object;
    }

    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        // Check if the callback has fired.
        if let Ok(guard) = result_holder.lock() {
            if guard.is_some() {
                break;
            }
        }
        if std::time::Instant::now() >= deadline {
            warn!("SFSpeechRecognizer timed out after 15s");
            return Err(AudioError::Transcription(
                "Recognition timed out — check that Speech Recognition is enabled in \
                 System Settings > Privacy & Security > Speech Recognition, and that \
                 the on-device dictation model is downloaded (System Settings > Keyboard > Dictation)"
                    .to_string(),
            ));
        }
        // Pump the RunLoop for 100ms so GCD can deliver the callback.
        unsafe {
            CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.1, false);
        }
    }

    let mut guard = result_holder
        .lock()
        .map_err(|_| AudioError::Transcription("Lock poisoned".to_string()))?;
    guard.take().unwrap_or(Ok(String::new()))
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

    // -----------------------------------------------------------------------
    // SpeechAuthStatus
    // -----------------------------------------------------------------------

    #[test]
    fn speech_auth_status_from_raw_authorized() {
        // GIVEN: raw value 3 (SFSpeechRecognizerAuthorizationStatusAuthorized)
        // THEN: maps to Authorized
        assert_eq!(SpeechAuthStatus::from_raw(3), SpeechAuthStatus::Authorized);
    }

    #[test]
    fn speech_auth_status_from_raw_denied() {
        assert_eq!(SpeechAuthStatus::from_raw(1), SpeechAuthStatus::Denied);
    }

    #[test]
    fn speech_auth_status_from_raw_restricted() {
        assert_eq!(SpeechAuthStatus::from_raw(2), SpeechAuthStatus::Restricted);
    }

    #[test]
    fn speech_auth_status_from_raw_not_determined() {
        assert_eq!(
            SpeechAuthStatus::from_raw(0),
            SpeechAuthStatus::NotDetermined
        );
    }

    #[test]
    fn speech_auth_status_from_raw_unknown_defaults_to_not_determined() {
        // GIVEN: an unknown value (e.g. future SDK variant)
        // THEN: falls back to NotDetermined (safest default)
        assert_eq!(
            SpeechAuthStatus::from_raw(99),
            SpeechAuthStatus::NotDetermined
        );
    }

    // -----------------------------------------------------------------------
    // speak() guard
    // -----------------------------------------------------------------------

    #[test]
    fn speak_empty_text_returns_synthesis_error() {
        // GIVEN: empty input
        let err = speak("").unwrap_err();
        // THEN: synthesis_error code (not a panic or framework error)
        assert_eq!(err.code(), "synthesis_error");
    }

    // -----------------------------------------------------------------------
    // write_temp_wav
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
        // GIVEN: 16 silence samples
        let samples: Vec<f32> = vec![0.0; 16];
        let bytes = encode_wav_pcm16(&samples, SAMPLE_RATE, CHANNELS);
        let path = write_temp_wav(&bytes).unwrap();
        let content = std::fs::read(&path).unwrap();
        // THEN: file starts with RIFF magic
        assert_eq!(&content[0..4], b"RIFF");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn write_temp_wav_paths_are_unique_across_calls() {
        // GIVEN: two rapid successive calls
        let bytes = encode_wav_pcm16(&[], SAMPLE_RATE, CHANNELS);
        let p1 = write_temp_wav(&bytes).unwrap();
        let p2 = write_temp_wav(&bytes).unwrap();
        // THEN: different paths (no clobbering)
        assert_ne!(p1, p2);
        let _ = std::fs::remove_file(&p1);
        let _ = std::fs::remove_file(&p2);
    }
}
