//! Audio MCP tools (requires `audio` feature).
//!
//! | Tool | Purpose |
//! |------|---------|
//! | `ax_listen`        | Capture audio and optionally transcribe |
//! | `ax_speak`         | Synthesize and play text as speech |
//! | `ax_audio_devices` | List available audio input/output devices |
//!
//! All functions are gated behind `#[cfg(feature = "audio")]`.
//! Uses CoreAudio and SFSpeechRecognizer — on-device, no cloud.

#[cfg(feature = "audio")]
use serde_json::{json, Value};

#[cfg(feature = "audio")]
use crate::mcp::annotations;
#[cfg(feature = "audio")]
use crate::mcp::args::{
    extract_bool_field_or, extract_f64_field_or, extract_or_return, extract_required_string_field,
    extract_string_field_or,
};
#[cfg(feature = "audio")]
use crate::mcp::protocol::{Tool, ToolCallResult};

// ---------------------------------------------------------------------------
// Tool names
// ---------------------------------------------------------------------------

#[cfg(feature = "audio")]
pub(crate) const TOOL_AX_LISTEN: &str = "ax_listen";
#[cfg(feature = "audio")]
pub(crate) const TOOL_AX_SPEAK: &str = "ax_speak";
#[cfg(feature = "audio")]
pub(crate) const TOOL_AX_AUDIO_DEVICES: &str = "ax_audio_devices";

// ---------------------------------------------------------------------------
// Tool declarations
// ---------------------------------------------------------------------------

/// All audio tools registered when the `audio` feature is active.
#[cfg(feature = "audio")]
pub(crate) fn audio_tools() -> Vec<Tool> {
    vec![tool_ax_listen(), tool_ax_speak(), tool_ax_audio_devices()]
}

#[cfg(feature = "audio")]
fn tool_ax_listen() -> Tool {
    Tool {
        name: TOOL_AX_LISTEN,
        title: "Capture audio and optionally transcribe it",
        description: "Capture audio from the system (microphone or loopback output) for \
            `duration` seconds and return the raw WAV data as base64. When `transcribe` is \
            true the audio is also transcribed on-device (privacy-preserving — no cloud).\n\
            \n\
            On macOS 14+, system audio capture uses ScreenCaptureKit in audio-only mode \
            (width=0, height=0) which does NOT require Screen Recording permission.\n\
            \n\
            Sources:\n\
            - `\"microphone\"` — default input device (requires TCC microphone permission)\n\
            - `\"system\"` — system audio output loopback (macOS 14+: no Screen Recording needed)\n\
            \n\
            Transcription engines (requires `transcribe: true`):\n\
            - `\"apple\"` — Apple SFSpeechRecognizer (default, macOS 13+, any language)\n\
            - `\"parakeet\"` — NVIDIA Parakeet TDT 0.6B v3 (25 European languages, \
              ONNX Runtime, requires model download — see `~/.axterminator/models/`)\n\
            \n\
            Duration is capped at 30 seconds. The call returns within `duration + 1s`.\n\
            \n\
            For long captures, set `max_chunk_secs` to split audio into smaller segments \
            (reduces peak MCP payload size). A 30s capture at 16kHz mono = ~960KB WAV → \
            ~1.3MB base64. Chunking into 5s segments keeps each under ~220KB.\n\
            \n\
            Example: verify an error sound played\n\
            `{\"duration\": 3, \"source\": \"system\", \"transcribe\": false}`\n\
            \n\
            Example: transcribe Finnish speech with Apple engine\n\
            `{\"duration\": 10, \"transcribe\": true, \"language\": \"fi-FI\"}`\n\
            \n\
            Example: high-quality transcription with Parakeet\n\
            `{\"duration\": 10, \"transcribe\": true, \"engine\": \"parakeet\"}`",
        input_schema: json!({
            "type": "object",
            "properties": {
                "duration": {
                    "type": "number",
                    "description": "Capture length in seconds (default 5, max 30)",
                    "default": 5.0,
                    "minimum": crate::audio::MIN_CAPTURE_SECS,
                    "maximum": crate::audio::MAX_CAPTURE_SECS
                },
                "source": {
                    "type": "string",
                    "enum": ["microphone", "system"],
                    "description": "Audio source (default \"microphone\")",
                    "default": "microphone"
                },
                "transcribe": {
                    "type": "boolean",
                    "description": "When true, return a text transcript in addition to raw audio",
                    "default": false
                },
                "engine": {
                    "type": "string",
                    "enum": ["apple", "parakeet"],
                    "description": "Transcription engine (default \"apple\"). \
                        \"apple\" uses SFSpeechRecognizer (on-device, macOS 13+). \
                        \"parakeet\" uses NVIDIA Parakeet TDT 0.6B v3 via ONNX Runtime \
                        (25 European languages, auto language detection — model download \
                        required: huggingface-cli download istupakov/parakeet-tdt-0.6b-v3-onnx \
                        encoder-model.onnx encoder-model.onnx.data decoder_joint-model.onnx \
                        nemo128.onnx vocab.txt config.json \
                        --local-dir ~/.axterminator/models/parakeet-tdt-0.6b-v3).",
                    "default": "apple"
                },
                "language": {
                    "type": "string",
                    "description": "BCP-47 locale for speech recognition (default \"en-US\"). \
                        Applies to the Apple engine. The Parakeet engine performs automatic \
                        language detection and ignores this field. \
                        Examples: \"en-US\", \"fi-FI\", \"ja-JP\", \"de-DE\", \"fr-FR\", \
                        \"es-ES\", \"zh-Hans\"",
                    "default": "en-US"
                },
                "max_chunk_secs": {
                    "type": "number",
                    "description": "When set, split the captured audio into chunks of at most \
                        this many seconds. Returns a `chunks` array instead of a single \
                        `base64_wav`. Useful for keeping MCP payload size manageable on \
                        longer recordings.",
                    "minimum": 1.0,
                    "maximum": 30.0
                }
            },
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "captured":       { "type": "boolean" },
                "duration_ms":    { "type": "integer" },
                "sample_rate":    { "type": "integer" },
                "size_bytes":     { "type": "integer", "description": "WAV payload size before base64" },
                "base64_wav":     { "type": "string" },
                "transcript":     { "type": "string" },
                "engine_used":    { "type": "string", "description": "Transcription engine that produced the transcript (\"apple\" or \"parakeet\")" },
                "chunks": {
                    "type": "array",
                    "description": "Present when max_chunk_secs is set",
                    "items": {
                        "type": "object",
                        "properties": {
                            "index":       { "type": "integer" },
                            "duration_ms": { "type": "integer" },
                            "size_bytes":  { "type": "integer" },
                            "base64_wav":  { "type": "string" }
                        }
                    }
                }
            },
            "required": ["captured", "duration_ms", "sample_rate", "size_bytes"]
        }),
        annotations: annotations::READ_ONLY,
    }
}

#[cfg(feature = "audio")]
fn tool_ax_speak() -> Tool {
    Tool {
        name: TOOL_AX_SPEAK,
        title: "Synthesize and play text as speech",
        description: "Speak `text` through the default system audio output using \
            NSSpeechSynthesizer (on-device, no network). Blocks until synthesis \
            completes and returns the elapsed duration.\n\
            \n\
            Useful for: testing VoiceOver integrations, verifying audio feedback, \
            injecting voice prompts into the agent workflow.\n\
            \n\
            Example: `{\"text\": \"Test complete\"}`",
        input_schema: json!({
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "Text to synthesize and speak"
                }
            },
            "required": ["text"],
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "spoken":      { "type": "boolean" },
                "duration_ms": { "type": "integer" }
            },
            "required": ["spoken", "duration_ms"]
        }),
        annotations: annotations::ACTION,
    }
}

#[cfg(feature = "audio")]
fn tool_ax_audio_devices() -> Tool {
    Tool {
        name: TOOL_AX_AUDIO_DEVICES,
        title: "List available audio input/output devices",
        description: "Enumerate all CoreAudio devices on the system with their name, ID, \
            input/output capability, sample rate, and default-device status.\n\
            \n\
            Use this before `ax_listen` to confirm that a microphone or virtual audio \
            device is available.\n\
            \n\
            Example: `{}`",
        input_schema: json!({ "type": "object", "additionalProperties": false }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "device_count": { "type": "integer" },
                "devices": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name":              { "type": "string" },
                            "id":                { "type": "string" },
                            "is_input":          { "type": "boolean" },
                            "is_output":         { "type": "boolean" },
                            "sample_rate":       { "type": "number" },
                            "is_default_input":  { "type": "boolean" },
                            "is_default_output": { "type": "boolean" }
                        },
                        "required": ["name", "id", "is_input", "is_output",
                                     "sample_rate", "is_default_input", "is_default_output"]
                    }
                }
            },
            "required": ["device_count", "devices"]
        }),
        annotations: annotations::READ_ONLY,
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// Handle `ax_listen` — capture audio and optionally transcribe.
#[cfg(feature = "audio")]
pub(crate) fn handle_ax_listen(args: &Value) -> ToolCallResult {
    let duration = extract_f64_field_or(args, "duration", 5.0) as f32;
    let source = extract_string_field_or(args, "source", "microphone");
    let do_transcribe = extract_bool_field_or(args, "transcribe", false);
    let language = args["language"].as_str();
    let max_chunk_secs = args["max_chunk_secs"].as_f64().map(|v| v as f32);
    let engine_str = extract_string_field_or(args, "engine", "apple");

    // Validate engine name before touching any hardware.
    let engine = match crate::audio::AudioEngine::parse_str(engine_str) {
        Some(e) => e,
        None => {
            return ToolCallResult::error(
                json!({
                    "error": format!("Unknown engine \"{engine_str}\". Valid values: \"apple\", \"parakeet\"."),
                    "error_code": "invalid_engine"
                })
                .to_string(),
            );
        }
    };

    let source = match source {
        "microphone" => "microphone",
        "system" => "system",
        other => {
            return ToolCallResult::error(
                json!({
                    "error": format!("Unknown source \"{other}\". Valid values: \"microphone\", \"system\"."),
                    "error_code": "invalid_source"
                })
                .to_string(),
            );
        }
    };

    // AC5: validate duration cap before touching any hardware.
    if let Err(e) = crate::audio::validate_duration(duration) {
        return ToolCallResult::error(
            json!({ "error": e.to_string(), "error_code": e.code() }).to_string(),
        );
    }

    let capture_result = match source {
        "system" => crate::audio::capture_system_audio(duration),
        _ => crate::audio::capture_microphone(duration),
    };

    let audio_data = match capture_result {
        Ok(d) => d,
        Err(e) => {
            return ToolCallResult::error(
                json!({ "error": e.to_string(), "error_code": e.code() }).to_string(),
            );
        }
    };

    let duration_ms = audio_data.duration_ms();
    let sample_rate = audio_data.sample_rate;
    let size_bytes = audio_data.wav_size_bytes();

    let transcript = if do_transcribe {
        match crate::audio::transcribe_with_engine(&audio_data, language, engine) {
            Ok(t) => Some(t),
            Err(e) => {
                tracing::warn!(error = %e, "transcription failed — returning audio without transcript");
                None
            }
        }
    } else {
        None
    };

    // Chunking: split audio into smaller segments to keep MCP payload manageable.
    let mut payload = if let Some(chunk_secs) = max_chunk_secs {
        let chunks = audio_data.into_chunks(chunk_secs);
        let chunks_json: Vec<Value> = chunks
            .iter()
            .enumerate()
            .map(|(i, chunk)| {
                json!({
                    "index":       i,
                    "duration_ms": chunk.duration_ms(),
                    "size_bytes":  chunk.wav_size_bytes(),
                    "base64_wav":  chunk.to_wav_base64(),
                })
            })
            .collect();
        json!({
            "captured":    true,
            "duration_ms": duration_ms,
            "sample_rate": sample_rate,
            "size_bytes":  size_bytes,
            "chunks":      chunks_json,
        })
    } else {
        json!({
            "captured":    true,
            "duration_ms": duration_ms,
            "sample_rate": sample_rate,
            "size_bytes":  size_bytes,
            "base64_wav":  audio_data.to_wav_base64(),
        })
    };

    if let Some(t) = transcript {
        payload["transcript"] = serde_json::Value::String(t);
        payload["engine_used"] = serde_json::Value::String(engine.as_str().to_string());
    }

    ToolCallResult::ok(payload.to_string())
}

/// Handle `ax_speak` — text-to-speech via NSSpeechSynthesizer.
#[cfg(feature = "audio")]
pub(crate) fn handle_ax_speak(args: &Value) -> ToolCallResult {
    let text = extract_or_return!(extract_required_string_field(args, "text"));

    match crate::audio::speak(&text) {
        Ok(elapsed) => ToolCallResult::ok(
            json!({
                "spoken":      true,
                "duration_ms": elapsed.as_millis() as u64,
            })
            .to_string(),
        ),
        Err(e) => ToolCallResult::error(
            json!({ "error": e.to_string(), "error_code": e.code() }).to_string(),
        ),
    }
}

/// Handle `ax_audio_devices` — enumerate CoreAudio devices.
#[cfg(feature = "audio")]
pub(crate) fn handle_ax_audio_devices() -> ToolCallResult {
    let devices = crate::audio::list_audio_devices();
    let count = devices.len();
    match serde_json::to_value(&devices) {
        Ok(devices_val) => {
            ToolCallResult::ok(json!({ "device_count": count, "devices": devices_val }).to_string())
        }
        Err(e) => ToolCallResult::error(format!("Failed to serialize devices: {e}")),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(all(test, feature = "audio"))]
mod tests {
    use super::*;
    use crate::mcp::args::parse_json_string_array;

    #[test]
    fn audio_tools_returns_three_tools() {
        // GIVEN: audio feature is enabled
        // WHEN: audio_tools() is called
        // THEN: exactly three tools are returned
        let tools = audio_tools();
        assert_eq!(tools.len(), 3);
        let names: Vec<&str> = tools.iter().map(|t| t.name).collect();
        assert!(names.contains(&"ax_listen"));
        assert!(names.contains(&"ax_speak"));
        assert!(names.contains(&"ax_audio_devices"));
    }

    #[test]
    fn extended_tools_includes_audio_tools_when_feature_enabled() {
        // GIVEN: audio feature is active
        let tools = crate::mcp::tools_extended::extended_tools();
        let names: Vec<&str> = tools.iter().map(|t| t.name).collect();
        // THEN: all three audio tools are registered
        assert!(names.contains(&"ax_listen"), "ax_listen missing");
        assert!(names.contains(&"ax_speak"), "ax_speak missing");
        assert!(
            names.contains(&"ax_audio_devices"),
            "ax_audio_devices missing"
        );
    }

    #[test]
    fn ax_listen_tool_has_duration_parameter() {
        let tool = tool_ax_listen();
        let props = &tool.input_schema["properties"];
        assert!(
            props["duration"].is_object(),
            "duration property missing from schema"
        );
        assert_eq!(props["duration"]["maximum"], 30.0);
    }

    #[test]
    fn ax_speak_tool_requires_text_field() {
        let tool = tool_ax_speak();
        let req_names = parse_json_string_array(&tool.input_schema["required"]);
        assert!(req_names.contains(&"text"), "text must be required");
    }

    #[test]
    fn ax_audio_devices_tool_has_empty_input_schema() {
        let tool = tool_ax_audio_devices();
        // input_schema is an empty object with only additionalProperties: false
        assert!(
            tool.input_schema["properties"].is_null()
                || tool.input_schema.get("properties").is_none()
        );
    }

    // -----------------------------------------------------------------------
    // Audio handlers (feature = "audio") — unit tests (no hardware required)
    // -----------------------------------------------------------------------

    #[test]
    fn handle_ax_listen_duration_exceeded_returns_error() {
        // GIVEN: duration > 30s
        let args = json!({ "duration": 31.0 });
        // WHEN: dispatched
        let result = handle_ax_listen(&args);
        // THEN: is_error flag is set and error_code is duration_exceeded
        assert!(result.is_error);
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["error_code"], "duration_exceeded");
    }

    #[test]
    fn handle_ax_listen_zero_duration_returns_error() {
        let result = handle_ax_listen(&json!({ "duration": 0.0 }));
        assert!(result.is_error);
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["error_code"], "invalid_duration");
    }

    #[test]
    fn handle_ax_listen_invalid_source_returns_error() {
        let result = handle_ax_listen(&json!({ "duration": 1.0, "source": "loopback" }));
        assert!(result.is_error);
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["error_code"], "invalid_source");
    }

    #[test]
    fn handle_ax_speak_missing_text_returns_error() {
        // GIVEN: no text argument
        let args = json!({});
        let result = handle_ax_speak(&args);
        assert!(result.is_error);
        assert_eq!(result.content[0].text, "Missing required field: text");
    }

    #[test]
    fn handle_ax_audio_devices_returns_valid_json_with_required_keys() {
        // GIVEN: running macOS system
        // WHEN: ax_audio_devices is called
        let result = handle_ax_audio_devices();
        // THEN: parses as JSON with device_count and devices keys
        assert!(
            !result.is_error,
            "unexpected error: {}",
            result.content[0].text
        );
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert!(v["device_count"].is_number());
        assert!(v["devices"].is_array());
    }

    #[test]
    fn call_tool_extended_ax_audio_devices_dispatches() {
        use crate::mcp::tools::AppRegistry;
        use std::sync::Arc;

        // GIVEN: running system, no app registry needed
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        let result = crate::mcp::tools_extended::call_tool_extended(
            "ax_audio_devices",
            &json!({}),
            &registry,
            &mut out,
        );
        // THEN: the audio tool is dispatched (returns Some, not None)
        assert!(result.is_some(), "ax_audio_devices should dispatch");
        let r = result.unwrap();
        assert!(!r.is_error, "unexpected error: {}", r.content[0].text);
    }

    #[test]
    fn call_tool_extended_ax_listen_duration_exceeded_returns_error() {
        use crate::mcp::tools::AppRegistry;
        use std::sync::Arc;

        // GIVEN: duration well over the cap
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        let result = crate::mcp::tools_extended::call_tool_extended(
            "ax_listen",
            &json!({ "duration": 999.0 }),
            &registry,
            &mut out,
        );
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(r.is_error);
        let v: serde_json::Value = serde_json::from_str(&r.content[0].text).unwrap();
        assert_eq!(v["error_code"], "duration_exceeded");
    }

    #[test]
    fn call_tool_extended_ax_speak_missing_text_returns_error() {
        use crate::mcp::tools::AppRegistry;
        use std::sync::Arc;

        // GIVEN: no text field
        let registry = Arc::new(AppRegistry::default());
        let mut out = Vec::<u8>::new();
        let result = crate::mcp::tools_extended::call_tool_extended(
            "ax_speak",
            &json!({}),
            &registry,
            &mut out,
        );
        assert!(result.is_some());
        assert!(result.unwrap().is_error);
    }

    // -----------------------------------------------------------------------
    // engine parameter parsing
    // -----------------------------------------------------------------------

    #[test]
    fn ax_listen_schema_includes_engine_parameter() {
        // GIVEN: ax_listen tool definition
        let tool = tool_ax_listen();
        let props = &tool.input_schema["properties"];
        // THEN: engine property is present with correct enum values
        assert!(
            props["engine"].is_object(),
            "engine property missing from schema"
        );
        let names = parse_json_string_array(&props["engine"]["enum"]);
        assert!(names.contains(&"apple"), "apple missing from engine enum");
        assert!(
            names.contains(&"parakeet"),
            "parakeet missing from engine enum"
        );
    }

    #[test]
    fn ax_listen_output_schema_includes_engine_used_field() {
        let tool = tool_ax_listen();
        let props = &tool.output_schema["properties"];
        assert!(
            props["engine_used"].is_object(),
            "engine_used missing from output schema"
        );
    }

    #[test]
    fn handle_ax_listen_unknown_engine_returns_error() {
        // GIVEN: an unknown engine name
        let args = json!({ "duration": 1.0, "engine": "whisper" });
        // WHEN: dispatched
        let result = handle_ax_listen(&args);
        // THEN: error with "invalid_engine" code (no hardware touched)
        assert!(result.is_error);
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["error_code"], "invalid_engine");
    }

    #[test]
    fn handle_ax_listen_explicit_apple_engine_duration_exceeded_returns_error() {
        // GIVEN: valid engine but duration too long
        let args = json!({ "duration": 99.0, "engine": "apple" });
        let result = handle_ax_listen(&args);
        assert!(result.is_error);
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["error_code"], "duration_exceeded");
    }

    #[test]
    fn handle_ax_listen_parakeet_engine_duration_exceeded_returns_error() {
        // GIVEN: parakeet engine + overlong duration (engine validation happens
        //        before capture — so duration check fires independently of model presence)
        let args = json!({ "duration": 99.0, "engine": "parakeet" });
        let result = handle_ax_listen(&args);
        assert!(result.is_error);
        let v: serde_json::Value = serde_json::from_str(&result.content[0].text).unwrap();
        assert_eq!(v["error_code"], "duration_exceeded");
    }
}
