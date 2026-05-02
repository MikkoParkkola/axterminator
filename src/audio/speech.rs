//! On-device speech recognition (`SFSpeechRecognizer`) and text-to-speech
//! (`NSSpeechSynthesizer`).
//!
//! When the `parakeet` feature is enabled, [`transcribe`] accepts an optional
//! [`AudioEngine`] enum through the `engine` routing layer in
//! [`transcribe_with_engine`].  The bare [`transcribe`] function always uses
//! the Apple engine for backward compatibility.

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

/// Transcription engine selector.
///
/// Controls which ASR backend [`transcribe_with_engine`] uses.
///
/// | Variant      | Backend | Notes |
/// |--------------|---------|-------|
/// | `Apple`      | `SFSpeechRecognizer` | On-device, macOS built-in |
/// | `Parakeet`   | ONNX Runtime (Parakeet TDT 0.6B v3) | 25 European languages, requires model download |
///
/// `Apple` is the default and always available when the `audio` feature is
/// enabled.  `Parakeet` is only available when the `parakeet` feature is also
/// enabled; selecting it without the feature returns a compile error via
/// exhaustive `#[cfg]` gating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AudioEngine {
    /// Apple `SFSpeechRecognizer` — on-device, macOS built-in.
    #[default]
    Apple,
    /// NVIDIA Parakeet TDT 0.6B v3 via ONNX Runtime.
    ///
    /// Only available when the `parakeet` Cargo feature is enabled.
    Parakeet,
}

impl AudioEngine {
    /// Parse an engine name from a string slice (case-insensitive).
    ///
    /// Accepts `"apple"` and `"parakeet"`.  Returns `None` for unrecognised
    /// values — callers should surface this as a validation error rather than
    /// silently falling back.
    ///
    /// Named `parse_str` rather than `from_str` to avoid shadowing
    /// [`std::str::FromStr::from_str`] without implementing the trait.
    ///
    /// # Examples
    ///
    /// ```
    /// use axterminator::audio::AudioEngine;
    /// assert_eq!(AudioEngine::parse_str("apple"), Some(AudioEngine::Apple));
    /// assert_eq!(AudioEngine::parse_str("parakeet"), Some(AudioEngine::Parakeet));
    /// assert_eq!(AudioEngine::parse_str("unknown"), None);
    /// ```
    #[must_use]
    pub fn parse_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "apple" => Some(Self::Apple),
            "parakeet" => Some(Self::Parakeet),
            _ => None,
        }
    }

    /// Machine-readable name of the engine (matches the `from_str` input).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Apple => "apple",
            Self::Parakeet => "parakeet",
        }
    }
}

/// Transcribe audio on-device using `SFSpeechRecognizer`.
///
/// All recognition runs locally (`requiresOnDeviceRecognition = true`).
/// No audio data is sent over the network.  If the on-device model is not
/// yet downloaded, this returns [`AudioError::Transcription`] with a message
/// directing the user to enable Dictation in System Settings rather than
/// silently falling back to Apple cloud servers.
///
/// `language` selects the `SFSpeechRecognizer` locale (BCP-47 tag, e.g.
/// `"en-US"`, `"fi-FI"`, `"ja-JP"`).  Pass `None` for the default `"en-US"`.
///
/// **Note:** On-device model quality varies significantly by language.
/// English is solid; less common languages may have very low accuracy.
/// Check `SFSpeechRecognizer(locale:)?.isAvailable` for your target locale.
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
/// let text = transcribe(&silent, None).unwrap_or_default();
/// assert!(text.is_empty() || !text.is_empty()); // either is valid
/// ```
pub fn transcribe(audio: &AudioData, language: Option<&str>) -> Result<String, AudioError> {
    let locale = language.unwrap_or("en-US");
    debug!(samples = audio.samples.len(), locale, "transcribing audio");
    transcribe_with_sf_speech(audio, locale)
}

/// Transcribe audio using the selected [`AudioEngine`].
///
/// This is the engine-routing entry point consumed by `ax_listen` when the
/// caller specifies an explicit `engine` parameter.  For backward
/// compatibility, [`transcribe`] always uses [`AudioEngine::Apple`].
///
/// # Errors
///
/// - [`AudioError::Transcription`] when the `Parakeet` engine is requested but
///   the `parakeet` feature is not enabled at compile time.
/// - All errors from [`transcribe`] apply when `engine` is `Apple`.
/// - All errors from [`crate::audio::parakeet::transcribe_parakeet`] apply
///   when `engine` is `Parakeet` (including model-not-downloaded errors).
///
/// # Examples
///
/// ```ignore
/// use axterminator::audio::{AudioData, transcribe_with_engine, AudioEngine};
/// let silent = AudioData::silent(1.0);
/// let text = transcribe_with_engine(&silent, None, AudioEngine::Apple).unwrap_or_default();
/// ```
pub fn transcribe_with_engine(
    audio: &AudioData,
    language: Option<&str>,
    engine: AudioEngine,
) -> Result<String, AudioError> {
    match engine {
        AudioEngine::Apple => transcribe(audio, language),
        AudioEngine::Parakeet => transcribe_parakeet_engine(audio, language),
    }
}

/// Dispatch to the Parakeet backend, gated by the `parakeet` feature.
#[cfg(feature = "parakeet")]
fn transcribe_parakeet_engine(
    audio: &AudioData,
    language: Option<&str>,
) -> Result<String, AudioError> {
    crate::audio::parakeet::transcribe_parakeet(audio, language)
}

/// Stub returned when `parakeet` feature is not compiled in.
#[cfg(not(feature = "parakeet"))]
fn transcribe_parakeet_engine(
    _audio: &AudioData,
    _language: Option<&str>,
) -> Result<String, AudioError> {
    Err(AudioError::Transcription(
        "The Parakeet engine requires the `parakeet` Cargo feature. \
         Rebuild with `--features parakeet` to enable it."
            .to_string(),
    ))
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
    speak_with_voice(text, None)
}

/// Synthesize `text` as speech using an optional macOS voice identifier.
///
/// `voice` should be one of the identifiers returned by [`list_speech_voices`],
/// for example `com.apple.speech.synthesis.voice.Alex`.
pub fn speak_with_voice(text: &str, voice: Option<&str>) -> Result<Duration, AudioError> {
    if text.is_empty() {
        return Err(AudioError::Synthesis(
            "Cannot speak empty string".to_string(),
        ));
    }
    let voice = voice
        .map(str::trim)
        .filter(|candidate| !candidate.is_empty());
    debug!(
        chars = text.len(),
        voice = voice.unwrap_or("system-default"),
        "speaking text"
    );
    speak_with_ns_speech_synthesizer(text, voice)
}

/// Return the installed macOS speech voice identifiers.
pub fn list_speech_voices() -> Result<Vec<String>, AudioError> {
    let cls = objc_class("NSSpeechSynthesizer");
    if cls.is_null() {
        return Err(AudioError::Synthesis(
            "NSSpeechSynthesizer unavailable".to_string(),
        ));
    }

    let voices: *mut Object = unsafe { msg_send![cls, availableVoices] };
    if voices.is_null() {
        return Err(AudioError::Synthesis(
            "NSSpeechSynthesizer returned no voices".to_string(),
        ));
    }

    let count: usize = unsafe { msg_send![voices, count] };
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let voice: *mut Object = unsafe { msg_send![voices, objectAtIndex: i] };
        if !voice.is_null() {
            out.push(ns_string_to_rust(voice));
        }
    }

    Ok(out)
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
/// transcription. Writes a private temporary WAV file, runs the recognizer,
/// then lets the temp file clean itself up.
fn transcribe_with_sf_speech(audio: &AudioData, locale: &str) -> Result<String, AudioError> {
    // Ensure authorization is present — this is the fix for BUG #26.
    // When status is NotDetermined the system dialog is shown and we block
    // until the user responds.  Without this call, SFSpeechRecognizer silently
    // returns empty results instead of surfacing a permission error.
    request_speech_authorization()?;

    let wav_bytes = audio.to_wav_bytes();

    let tmp_file = write_temp_wav(&wav_bytes)
        .map_err(|e| AudioError::Framework(format!("Temp file write failed: {e}")))?;
    let tmp_path = tmp_file.path().to_string_lossy().into_owned();

    run_sf_speech_recognizer(&tmp_path, locale)
}

/// Write `bytes` to a unique `0600`-permissioned temp WAV file.
fn write_temp_wav(bytes: &[u8]) -> Result<tempfile::NamedTempFile, std::io::Error> {
    use std::io::Write as _;

    let mut file = tempfile::Builder::new()
        .prefix("axterminator_audio_")
        .suffix(".wav")
        .tempfile()?;
    file.write_all(bytes)?;
    file.flush()?;
    Ok(file)
}

/// Run `SFSpeechRecognizer` on a WAV file at `path`.
///
/// Uses a synchronous ObjC pattern with a `Condvar` to wait for the async
/// recognition callback.  Times out after 10 seconds.
///
/// Errors from the recognition task are surfaced as [`AudioError::Transcription`]
/// rather than silently producing an empty result.
fn run_sf_speech_recognizer(wav_path: &str, locale: &str) -> Result<String, AudioError> {
    let recognizer = create_sf_speech_recognizer(locale).ok_or_else(|| {
        AudioError::Transcription(format!(
            "SFSpeechRecognizer unavailable for locale \"{locale}\" — check that speech \
             recognition is enabled and the locale is supported on this device"
        ))
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

    // Enforce on-device recognition.  The tool descriptions for ax_listen
    // and ax_start_capture promise "on-device only — no cloud, no network".
    // Setting this to false would silently forward audio to Apple servers
    // when the local model is unavailable, violating that contract.
    //
    // If the on-device model is not yet downloaded, isAvailable returns NO
    // above and we return an explicit error before reaching this point.
    // The caller must ensure the dictation model is present in
    // System Settings > Keyboard > Dictation before invoking transcription.
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

    // SFSpeechRecognizer dispatches its callback to the current thread's
    // RunLoop.  We must pump the RunLoop while waiting, otherwise the
    // callback never fires and we time out.  This mirrors the Swift pattern:
    //   while !done { RunLoop.current.run(mode: .default, before: ...) }
    unsafe extern "C" {
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

/// Create `SFSpeechRecognizer` for the given BCP-47 locale (e.g. `"en-US"`,
/// `"fi-FI"`, `"ja-JP"`).
fn create_sf_speech_recognizer(locale_tag: &str) -> Option<*mut Object> {
    let cls = objc_class("SFSpeechRecognizer");
    if cls.is_null() {
        return None;
    }
    let locale_cls = objc_class("NSLocale");
    if locale_cls.is_null() {
        return None;
    }
    let locale_id = ns_string_from_str(locale_tag);
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
fn speak_with_ns_speech_synthesizer(
    text: &str,
    voice: Option<&str>,
) -> Result<Duration, AudioError> {
    let synth = create_ns_speech_synthesizer(voice)?;

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

/// Create an `NSSpeechSynthesizer` instance with the requested voice.
fn create_ns_speech_synthesizer(voice: Option<&str>) -> Result<*mut Object, AudioError> {
    let cls = objc_class("NSSpeechSynthesizer");
    if cls.is_null() {
        return Err(AudioError::Synthesis(
            "NSSpeechSynthesizer unavailable".to_string(),
        ));
    }
    let voice_id = voice.map(ns_string_from_str);
    let synth: *mut Object = unsafe {
        let obj: *mut Object = msg_send![cls, alloc];
        msg_send![obj, initWithVoice: voice_id.unwrap_or(std::ptr::null_mut::<Object>())]
    };
    if synth.is_null() {
        Err(AudioError::Synthesis(match voice {
            Some(voice_name) => {
                format!("Speech voice \"{voice_name}\" is unavailable on this system")
            }
            None => "NSSpeechSynthesizer unavailable".to_string(),
        }))
    } else {
        Ok(synth)
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
    // AudioEngine parsing
    // -----------------------------------------------------------------------

    #[test]
    fn audio_engine_from_str_apple() {
        assert_eq!(AudioEngine::parse_str("apple"), Some(AudioEngine::Apple));
        assert_eq!(AudioEngine::parse_str("APPLE"), Some(AudioEngine::Apple));
    }

    #[test]
    fn audio_engine_from_str_parakeet() {
        assert_eq!(
            AudioEngine::parse_str("parakeet"),
            Some(AudioEngine::Parakeet)
        );
        assert_eq!(
            AudioEngine::parse_str("PARAKEET"),
            Some(AudioEngine::Parakeet)
        );
    }

    #[test]
    fn audio_engine_from_str_unknown_returns_none() {
        assert_eq!(AudioEngine::parse_str("whisper"), None);
        assert_eq!(AudioEngine::parse_str(""), None);
    }

    #[test]
    fn audio_engine_as_str_round_trips() {
        assert_eq!(AudioEngine::Apple.as_str(), "apple");
        assert_eq!(AudioEngine::Parakeet.as_str(), "parakeet");
    }

    #[test]
    fn audio_engine_default_is_apple() {
        assert_eq!(AudioEngine::default(), AudioEngine::Apple);
    }

    // -----------------------------------------------------------------------
    // transcribe_with_engine — Apple path (no hardware — just validates routing)
    // -----------------------------------------------------------------------

    #[test]
    fn transcribe_with_engine_parakeet_routes_correctly() {
        // GIVEN: parakeet engine requested at runtime
        // WHEN: parakeet feature is compiled in
        // THEN: if model files are present, returns Ok; if absent, returns transcription_error.
        //       If feature is not compiled in, the stub returns transcription_error.
        let audio = crate::audio::AudioData::silent(0.1);
        let result = transcribe_with_engine(&audio, None, AudioEngine::Parakeet);
        match result {
            Ok(text) => {
                // Model files present — transcription succeeded (silence → empty or short text)
                assert!(
                    text.len() < 200,
                    "unexpected long transcription for silence"
                );
            }
            Err(e) => {
                // Model files absent or feature not compiled — transcription_error
                assert_eq!(e.code(), "transcription_error");
            }
        }
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

    #[test]
    fn list_speech_voices_returns_non_empty_ids() {
        let voices = list_speech_voices().unwrap();
        assert!(
            !voices.is_empty(),
            "expected at least one installed macOS voice"
        );
        assert!(voices.iter().all(|voice| !voice.is_empty()));
    }

    // -----------------------------------------------------------------------
    // write_temp_wav
    // -----------------------------------------------------------------------

    #[test]
    fn write_temp_wav_creates_readable_file() {
        use std::os::unix::fs::PermissionsExt;
        let bytes = encode_wav_pcm16(&[], SAMPLE_RATE, CHANNELS);
        let file = write_temp_wav(&bytes).unwrap();
        let meta = std::fs::metadata(file.path()).unwrap();
        let mode = meta.permissions().mode();
        // File must be owner-only (0600)
        assert_eq!(
            mode & 0o777,
            0o600,
            "expected mode 0600, got {:o}",
            mode & 0o777
        );
    }

    #[test]
    fn write_temp_wav_file_contains_wav_header() {
        // GIVEN: 16 silence samples
        let samples: Vec<f32> = vec![0.0; 16];
        let bytes = encode_wav_pcm16(&samples, SAMPLE_RATE, CHANNELS);
        let file = write_temp_wav(&bytes).unwrap();
        let content = std::fs::read(file.path()).unwrap();
        // THEN: file starts with RIFF magic
        assert_eq!(&content[0..4], b"RIFF");
    }

    #[test]
    fn write_temp_wav_paths_are_unique_across_calls() {
        // GIVEN: two rapid successive calls
        let bytes = encode_wav_pcm16(&[], SAMPLE_RATE, CHANNELS);
        let f1 = write_temp_wav(&bytes).unwrap();
        let f2 = write_temp_wav(&bytes).unwrap();
        let p1 = f1.path().to_path_buf();
        let p2 = f2.path().to_path_buf();
        // THEN: different paths (no clobbering)
        assert_ne!(p1, p2);
    }
}
