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

use serde_json::{json, Value};

use crate::mcp::annotations;
use crate::mcp::protocol::{Tool, ToolCallResult};

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
        name: "ax_listen",
        title: "Capture audio and optionally transcribe it",
        description: "Capture audio from the system (microphone or loopback output) for \
            `duration` seconds and return the raw WAV data as base64. When `transcribe` is \
            true the audio is also transcribed on-device via SFSpeechRecognizer (macOS 13+, \
            no cloud — privacy-preserving).\n\
            \n\
            Sources:\n\
            - `\"microphone\"` — default input device (requires TCC microphone permission)\n\
            - `\"system\"` — system audio output loopback\n\
            \n\
            Duration is capped at 30 seconds. The call returns within `duration + 1s`.\n\
            \n\
            Example: verify an error sound played\n\
            `{\"duration\": 3, \"source\": \"system\", \"transcribe\": false}`",
        input_schema: json!({
            "type": "object",
            "properties": {
                "duration": {
                    "type": "number",
                    "description": "Capture length in seconds (default 5, max 30)",
                    "default": 5.0,
                    "minimum": 0.1,
                    "maximum": 30.0
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
                }
            },
            "additionalProperties": false
        }),
        output_schema: json!({
            "type": "object",
            "properties": {
                "captured":     { "type": "boolean" },
                "duration_ms":  { "type": "integer" },
                "sample_rate":  { "type": "integer" },
                "base64_wav":   { "type": "string" },
                "transcript":   { "type": "string" }
            },
            "required": ["captured", "duration_ms", "sample_rate", "base64_wav"]
        }),
        annotations: annotations::READ_ONLY,
    }
}

#[cfg(feature = "audio")]
fn tool_ax_speak() -> Tool {
    Tool {
        name: "ax_speak",
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
        name: "ax_audio_devices",
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
    let duration = args["duration"].as_f64().unwrap_or(5.0) as f32;
    let source = args["source"].as_str().unwrap_or("microphone");
    let do_transcribe = args["transcribe"].as_bool().unwrap_or(false);

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

    let base64_wav = audio_data.to_wav_base64();
    let duration_ms = audio_data.duration_ms();
    let sample_rate = audio_data.sample_rate;

    let transcript = if do_transcribe {
        match crate::audio::transcribe(&audio_data) {
            Ok(t) => Some(t),
            Err(e) => {
                tracing::warn!(error = %e, "transcription failed — returning audio without transcript");
                None
            }
        }
    } else {
        None
    };

    let mut payload = json!({
        "captured":    true,
        "duration_ms": duration_ms,
        "sample_rate": sample_rate,
        "base64_wav":  base64_wav,
    });

    if let Some(t) = transcript {
        payload["transcript"] = serde_json::Value::String(t);
    }

    ToolCallResult::ok(payload.to_string())
}

/// Handle `ax_speak` — text-to-speech via NSSpeechSynthesizer.
#[cfg(feature = "audio")]
pub(crate) fn handle_ax_speak(args: &Value) -> ToolCallResult {
    let Some(text) = args["text"].as_str().map(str::to_string) else {
        return ToolCallResult::error("Missing required field: text");
    };

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
        let required = tool.input_schema["required"].as_array().unwrap();
        let req_names: Vec<&str> = required.iter().map(|v| v.as_str().unwrap()).collect();
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
    fn handle_ax_speak_missing_text_returns_error() {
        // GIVEN: no text argument
        let args = json!({});
        let result = handle_ax_speak(&args);
        assert!(result.is_error);
        assert!(result.content[0]
            .text
            .contains("Missing required field: text"));
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
}
