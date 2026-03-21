# Camera & Gestures

AXTerminator includes camera capture and gesture detection via the `camera` feature flag.

## Enable Camera

Build with the `camera` feature:

```bash
cargo build --release --features "cli,camera"
```

## MCP Tools

| Tool | Description |
|------|-------------|
| `ax_camera_capture` | Capture a single frame from AVFoundation |
| `ax_gesture_detect` | Detect hand gestures in a camera frame |
| `ax_gesture_listen` | Continuously detect gestures (with `watch` feature) |

## Gesture Detection

Uses Apple's Vision framework for on-device hand pose detection. Supported gestures:

| Gesture | Detection | Notes |
|---------|-----------|-------|
| thumbs_up | Verified 88.8% confidence | TIP vs MCP knuckle comparison |
| thumbs_down | Supported | Inverse of thumbs_up |
| open_hand | Supported | All fingers extended |
| fist | Supported | All fingers closed |
| peace | Supported | Index + middle extended |
| pointing | Supported | Index finger extended |
| ok_sign | Supported | Thumb + index circle |

### How It Works

1. AVFoundation captures a single frame from the default camera
2. Vision's VNDetectHumanHandPoseRequest analyzes hand landmarks
3. TIP (fingertip) vs MCP (knuckle) joint positions determine gesture
4. Confidence filtering removes low-quality detections

!!! note "Gesture Accuracy"
    v0.6.0 fixed the TIP vs MCP knuckle comparison logic and added confidence filtering, significantly improving gesture recognition accuracy.

## Camera Capture

Returns a single JPEG frame from the default camera device.

### Requirements

- **Camera permission** granted to your terminal app
- If camera permission status is undetermined, AXTerminator will request it automatically

## Continuous Watch Mode

The `watch` feature enables continuous background monitoring that combines audio VAD (voice activity detection) and camera gesture detection:

```bash
cargo build --release --features "cli,watch"
```

Events are pushed to Claude Code sessions via MCP notifications. The `watch` feature implies both `audio` and `camera`.
