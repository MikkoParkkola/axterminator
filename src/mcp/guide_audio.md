# Audio Capture Guide

axterminator provides two audio workflows: **one-shot transcription** via
`ax_listen` and a **continuous capture session** that buffers audio and screen
frames for as long as needed.

---

## Device Selection

List available CoreAudio devices before starting a session:

```
ax_audio_devices
```

Returns an array of device objects:

```json
{
  "devices": [
    {
      "id": 42,
      "name": "MacBook Pro Microphone",
      "is_input": true,
      "is_output": false,
      "sample_rate": 48000.0,
      "is_default_input": true,
      "is_default_output": false
    }
  ]
}
```

Pass the `id` field to `ax_start_capture` as `device_id` to select a specific
device. Omitting `device_id` uses the system default input.

---

## Continuous Capture Workflow

### 1. Start a session

```
ax_start_capture  device_id=42  language="en-US"
```

Optional parameters:
- `device_id` — CoreAudio device ID from `ax_audio_devices` (default: system input)
- `language` — BCP-47 language tag for the speech recogniser (default: `"en"`)
- `screen_capture` — `true` to also capture screen frames (default: `false`)
- `screen_diff_threshold` — perceptual diff threshold 0–1 for frame storage
  (default: `0.05`; lower = store more frames)

### 2. Check session health

```
ax_capture_status
```

Returns a snapshot of the active session:

```json
{
  "running": true,
  "session_id": "cap_20240315_143022",
  "duration_ms": 12450,
  "audio_buffer_seconds": 3.2,
  "transcript_segment_count": 7
}
```

Poll this resource or subscribe to `axterminator://capture/status` for
change notifications.

### 3. Read transcription

```
ax_get_transcription
```

Returns all segments accumulated since session start, plus a joined `text` field:

```json
{
  "text": "Hello world. This is a test.",
  "segments": [
    {
      "index": 0,
      "text": "Hello world.",
      "start_ms": 0,
      "end_ms": 1200,
      "confidence": 0.97
    },
    {
      "index": 1,
      "text": "This is a test.",
      "start_ms": 1800,
      "end_ms": 3100,
      "confidence": 0.94
    }
  ]
}
```

Subscribe to `axterminator://capture/transcription` to receive a notification
whenever a new segment is recognised.

### 4. Stop the session

```
ax_stop_capture
```

Stops recording, flushes the ring buffer, and returns a final transcription
summary. After this call the session resources are released.

---

## Ring Buffer Behaviour

The audio capture engine maintains a fixed-size ring buffer.
- Default capacity: **60 seconds** of audio.
- When the buffer is full, the oldest audio is silently discarded.
- `audio_buffer_seconds` in `ax_capture_status` shows how much audio is
  currently buffered.
- Transcription runs in real time; segments are appended as speech is
  recognised, not when the buffer fills.

If you need longer recordings, call `ax_get_transcription` periodically to
drain completed segments before the buffer wraps.

---

## One-Shot Transcription

For short clips (up to ~30 seconds) without a persistent session:

```
ax_listen  duration_ms=5000
```

Blocks for the requested duration, then returns a single transcription result.
Does not use a ring buffer; audio is held in memory for the duration of the call.

---

## Text-to-Speech

Synthesise and play speech without any capture session:

```
ax_speak  text="The file has been saved."  voice="com.apple.speech.synthesis.voice.Alex"
```

- `voice` is optional; omit to use the system default voice.
- Available voices depend on the macOS system configuration.
- `ax_speak` blocks until playback completes.
