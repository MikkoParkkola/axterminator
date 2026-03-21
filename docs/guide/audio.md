# Audio & Speech

AXTerminator includes on-device audio capture and speech recognition via the `audio` feature flag.

## Enable Audio

Build with the `audio` feature:

```bash
cargo build --release --features "cli,audio"
```

## MCP Tools

| Tool | Description |
|------|-------------|
| `ax_listen` | Capture microphone or system audio, transcribe via SFSpeechRecognizer |
| `ax_speak` | Text-to-speech via NSSpeechSynthesizer |
| `ax_audio_devices` | List available audio input/output devices |

## Speech Recognition

AXTerminator captures audio at **native 48kHz sample rate** and transcribes on-device using Apple's SFSpeechRecognizer. No cloud API required.

### Requirements

- **Dictation** must be enabled: System Settings > Keyboard > Dictation
- **Microphone permission** granted to your terminal app

### How It Works

1. CoreAudio captures from the default input device at 48kHz
2. Audio is buffered and written as a WAV file
3. SFSpeechRecognizer transcribes on-device
4. CFRunLoop is pumped for callback delivery

!!! note "48kHz Native Capture"
    Earlier versions captured at 48kHz but wrote 16kHz WAV headers, causing garbled audio. v0.6.0 fixed this to write correct 48kHz headers, resolving speech recognition accuracy.

## Text-to-Speech

```bash
# Via CLI
axterminator speak "Hello from AXTerminator"
```

Uses NSSpeechSynthesizer with the system default voice.

## Audio Devices

```bash
# List input/output devices
axterminator audio-devices
```

Lists all CoreAudio devices with their sample rates and channel counts.
