# MCP Tools Reference

AXTerminator v0.6.1 exposes up to 30 MCP tools (19 core + optional audio, camera, spaces), 5 resources, 4 guided prompts, and 9 elicitation scenarios.

## Core Tools (19)

### GUI Interaction

| Tool | Description | Annotations |
|------|-------------|-------------|
| `ax_connect` | Connect to a running application by name, bundle ID, or PID | readOnly |
| `ax_find` | Find a UI element by query string | readOnly |
| `ax_click` | Click an element (background by default) | destructive |
| `ax_click_at` | Click at screen coordinates | destructive |
| `ax_type` | Type text into an element (requires focus) | destructive |
| `ax_set_value` | Set element value directly via AXValue attribute | destructive |
| `ax_get_value` | Read element value | readOnly |
| `ax_scroll` | Scroll within an element | destructive |
| `ax_drag` | Drag from one element/position to another | destructive |
| `ax_key_press` | Press a keyboard key or combination | destructive |

### Observation

| Tool | Description | Annotations |
|------|-------------|-------------|
| `ax_is_accessible` | Check if accessibility permissions are granted | readOnly |
| `ax_screenshot` | Capture a PNG screenshot of an app or element | readOnly |
| `ax_get_tree` | Get the accessibility element hierarchy | readOnly |
| `ax_get_attributes` | Read all attributes of an element | readOnly |
| `ax_list_windows` | List all windows of an application | readOnly |
| `ax_list_apps` | List all running accessible applications | readOnly |
| `ax_wait_idle` | Wait for application to become idle | readOnly, idempotent |

### Verification

| Tool | Description | Annotations |
|------|-------------|-------------|
| `ax_assert` | Assert element state (exists, enabled, value, etc.) | readOnly |
| `ax_find_visual` | Find element using AX-first source priority, then VLM vision as fallback | readOnly |

`ax_find_visual` accepts optional `caller` (`agent` or `human`) and `user_prompt`
fields. For agent-mediated calls, `user_prompt` is treated as higher priority
than the agent-generated `description`. The handler also checks the AX tree
before returning a screenshot sampling request, so AX API facts win over screen
vision for the same target.

## Audio Tools (4) -- `audio` feature

| Tool | Description | Annotations |
|------|-------------|-------------|
| `ax_listen` | Capture audio and transcribe via SFSpeechRecognizer (48kHz native) | readOnly |
| `ax_speak` | Text-to-speech via system, Kokoro, or Piper engines | openWorld |
| `ax_audio_voices` | List installed macOS speech voices | readOnly |
| `ax_audio_devices` | List available audio input/output devices | readOnly |

## Camera Tools (3) -- `camera` feature

| Tool | Description | Annotations |
|------|-------------|-------------|
| `ax_camera_capture` | Capture a single frame from AVFoundation | readOnly |
| `ax_gesture_detect` | Detect hand gestures in a camera frame | readOnly |
| `ax_gesture_listen` | Continuous gesture detection (requires `watch` feature) | readOnly |

## Spaces Tools (5) -- `spaces` feature

| Tool | Description | Annotations |
|------|-------------|-------------|
| `ax_list_spaces` | List all virtual desktops | readOnly |
| `ax_create_space` | Create a new virtual desktop | destructive |
| `ax_move_to_space` | Move a window to a specific space | destructive |
| `ax_switch_space` | Switch active space | destructive |
| `ax_destroy_space` | Destroy a virtual desktop | destructive |

## Resources (5)

| URI | Description |
|-----|-------------|
| `axterminator://apps` | Running applications |
| `axterminator://app/{name}/tree` | Live element hierarchy |
| `axterminator://app/{name}/screenshot` | Current screenshot (base64 PNG) |
| `axterminator://app/{name}/state` | Focused element, window title |
| `axterminator://system/displays` | Monitor layout and Retina scaling |

## Guided Prompts (4)

| Prompt | Description |
|--------|-------------|
| `test-app` | Generate a test plan for an application |
| `navigate-to` | Navigate to a specific screen or element |
| `extract-data` | Extract structured data from a UI |
| `accessibility-audit` | Audit an app's accessibility compliance |

## Elicitation

Destructive and ambiguous operations trigger elicitation dialogs for user confirmation. There are 9 elicitation scenarios covering:

- Destructive clicks (e.g., "Delete", "Remove")
- Text input into sensitive fields
- Drag operations
- Space management (create/destroy)
- Application termination

## Tool Annotations

All tools carry MCP tool annotations:

| Annotation | Meaning |
|------------|---------|
| `readOnlyHint` | Tool only reads state, no side effects |
| `destructiveHint` | Tool modifies state (clicks, types, deletes) |
| `idempotentHint` | Calling multiple times has same effect |
| `openWorldHint` | Tool interacts with external systems |

## Security

- **Bearer token auth** required for HTTP transport (`--token` flag)
- **Localhost-only** binding for HTTP transport by default
- Structured MCP logging with progress notifications
