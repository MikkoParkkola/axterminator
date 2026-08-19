# MCP Tools Reference

AXTerminator v0.10.2 exposes 62 MCP tools with default features and 84 with
`audio,camera,spaces,watch,context,docker,http-transport` enabled, plus 6
resources, 4 resource templates and 10 guided prompts. Counts measured with
`tools/list`, `resources/list` and `prompts/list` against the built binary.

Many of these tools reach outside the app you connected to: a shell, the
filesystem, interactive terminal sessions and outbound HTTP are all on by
default. See [Beyond the UI](#beyond-the-ui-shell-filesystem-terminal) before
you point an agent at this server.

## UI Tools (19)

### GUI Interaction

| Tool | Description | Annotations |
|------|-------------|-------------|
| `ax_connect` | Connect to a running application by name, bundle ID, or PID | idempotent |
| `ax_find` | Find a UI element by query string | readOnly |
| `ax_click` | Click an element (background by default); refuses destructive-looking labels without `confirm=true` | action |
| `ax_click_at` | Click at screen coordinates | action |
| `ax_type` | Type text into an element (requires focus) | destructive |
| `ax_set_value` | Set element value directly via AXValue attribute | destructive |
| `ax_get_value` | Read element value | readOnly |
| `ax_scroll` | Scroll within an element | action |
| `ax_drag` | Drag from one element/position to another | action |
| `ax_key_press` | Press a keyboard key or combination | action |

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
| `ax_find_visual` | Find element visually, with optional AX-first source priority before VLM fallback | readOnly |

`ax_find_visual` accepts optional `caller` (`agent` or `human`) and `user_prompt`
fields. When `AXTERMINATOR_PRIORITY_MODE=explicit`, agent-mediated calls treat
`user_prompt` as higher priority than the agent-generated `description`, and the
handler checks the AX tree before returning a screenshot sampling request. AX API
facts then win over screen vision for the same target. Unset or `legacy` mode
preserves historical visual lookup behavior.

## Session, Analysis and Workflow (16)

| Tool | Description | Annotations |
|------|-------------|-------------|
| `ax_analyze` | Detect UI patterns, infer app state, suggest next actions | readOnly |
| `ax_query` | Natural-language question about the current UI | readOnly |
| `ax_app_profile` | Electron/web app metadata for a connected app | readOnly |
| `ax_a11y_audit` | Accessibility compliance audit | readOnly |
| `ax_visual_diff` | Compare a screenshot against a stored baseline | readOnly |
| `ax_record` | Record a UI interaction for test generation | action |
| `ax_test_run` | Black-box test execution | action |
| `ax_track_workflow` | Cross-app workflow tracking | action |
| `ax_workflow_create` / `ax_workflow_step` / `ax_workflow_status` | Multi-step workflow planning and progress | action / action / readOnly |
| `ax_session_info` | Server session state | readOnly |
| `ax_undo` | Undo the last actions in an app (Cmd+Z) | destructive |
| `ax_clipboard` | Read or write the system clipboard | destructive |
| `ax_run_script` | Execute AppleScript or JXA | destructive |
| `ax_system_context` | System context snapshot (battery, volume, dark mode, and similar) | readOnly |

## Beyond the UI: shell, filesystem, terminal

These tools are part of the default build. They are not scoped to the app you
connected to, and they are not scoped to a project directory: paths are used as
given (with `~/` expanded), and commands run as the user who started the server.
Run the server in `safe` or `sandboxed` mode, or with an app policy file, if
that is more reach than you want. See
[Configuration](config.md#security-modes).

### System and Shell (7)

| Tool | Description | Annotations |
|------|-------------|-------------|
| `ax_exec` | Run a shell command via `/bin/sh -c`; returns stdout, stderr, exit code | destructive |
| `ax_process_list` | List running processes | readOnly |
| `ax_system_memory` | Memory statistics | readOnly |
| `ax_system_disk` | Disk usage | readOnly |
| `ax_system_network` | Network interfaces | readOnly |
| `ax_system_power` | Power and thermal status | readOnly |
| `ax_system_launchd` | List launchd agents | readOnly |

### Filesystem (6)

| Tool | Description | Annotations |
|------|-------------|-------------|
| `ax_fs_read` | Read file contents | readOnly |
| `ax_fs_write` | Write content to a file | destructive |
| `ax_fs_list` | List directory contents | readOnly |
| `ax_fs_edit` | Find and replace in a file (backup on by default) | destructive |
| `ax_fs_search` | Search file contents | readOnly |
| `ax_fs_delete` | Delete a file or directory | destructive |

### Terminal Sessions (5)

PTY-backed sessions that persist across tool calls, so an agent can drive an
interactive program (a REPL, `ssh`, an installer) over several turns.

| Tool | Description | Annotations |
|------|-------------|-------------|
| `ax_term_start` | Start a long-lived interactive command, returns a session ID | destructive |
| `ax_term_send` | Send input to a session | destructive |
| `ax_term_read` | Read output from a session | readOnly |
| `ax_term_close` | Close a session | destructive |
| `ax_term_list` | List active sessions | readOnly |

### Network, Apps and Notifications (3)

| Tool | Description | Annotations |
|------|-------------|-------------|
| `ax_http_get` | HTTP GET to any URL; returns status, headers, body | readOnly |
| `ax_app_launch` | Launch or quit an application | destructive |
| `ax_notify` | Post a macOS notification via `osascript` | destructive |

## Window Management (6)

| Tool | Description | Annotations |
|------|-------------|-------------|
| `ax_window_list` | List windows with positions | readOnly |
| `ax_window_focus` | Focus a window by title or app | destructive |
| `ax_window_move` | Move a window | destructive |
| `ax_window_resize` | Resize a window | destructive |
| `ax_window_minimize` | Minimize a window | destructive |
| `ax_window_tile` | Tile a window to a screen region | destructive |

## Audio Tools (8) -- `audio` feature

| Tool | Description | Annotations |
|------|-------------|-------------|
| `ax_listen` | Capture audio and transcribe via SFSpeechRecognizer (48kHz native) | readOnly |
| `ax_speak` | Text-to-speech via system, Kokoro, or Piper engines | action |
| `ax_audio_voices` | List installed macOS speech voices | readOnly |
| `ax_audio_devices` | List available audio input/output devices | readOnly |
| `ax_start_capture` | Start background screen and audio capture | action |
| `ax_stop_capture` | Stop a running capture session | action |
| `ax_get_transcription` | Read recent transcription from the capture buffer | readOnly |
| `ax_capture_status` | Query capture session status | readOnly |

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
| `ax_create_space` | Create a new virtual desktop | action |
| `ax_move_to_space` | Move a window to a specific space | action |
| `ax_switch_space` | Switch active space | action |
| `ax_destroy_space` | Destroy a virtual desktop | action |

## Watch Tools (3) -- `watch` feature

| Tool | Description | Annotations |
|------|-------------|-------------|
| `ax_watch_start` | Start continuous audio/camera monitoring | action |
| `ax_watch_stop` | Stop all active watchers | action |
| `ax_watch_status` | Check watch monitoring status | readOnly |

## Browser Container Tools (2) -- `docker` feature

| Tool | Description | Annotations |
|------|-------------|-------------|
| `ax_browser_launch` | Launch an isolated browser container | action |
| `ax_browser_stop` | Stop and remove a browser container | destructive |

## Location (1) -- `context` feature

| Tool | Description | Annotations |
|------|-------------|-------------|
| `ax_location` | Current GPS location via CoreLocation | readOnly |

## Resources (6)

| URI | Description |
|-----|-------------|
| `axterminator://apps` | Running applications with PIDs, bundle IDs, accessibility info |
| `axterminator://system/status` | Accessibility permissions, connected app count, server version |
| `axterminator://system/displays` | Monitor layout and Retina scaling |
| `axterminator://clipboard` | Current clipboard text; subscribable for change notifications |
| `axterminator://workflows` | Cross-app patterns observed via `ax_track_workflow` |
| `axterminator://profiles` | Built-in Electron app profiles (VS Code, Slack, Chrome, Terminal, Finder) |

### Resource Templates (4)

| URI template | Description |
|--------------|-------------|
| `axterminator://app/{name}/tree` | Live element hierarchy (depth 3 by default) |
| `axterminator://app/{name}/screenshot` | Current screenshot (base64 PNG) |
| `axterminator://app/{name}/state` | Focused element, window title, visible text |
| `axterminator://app/{name}/query/{question}` | Natural-language scene query |

## Guided Prompts (10)

| Prompt | Description |
|--------|-------------|
| `test-app` | Generate a test plan for an application |
| `navigate-to` | Navigate to a specific screen or element |
| `extract-data` | Extract structured data from a UI |
| `accessibility-audit` | Audit an app's accessibility compliance |
| `troubleshooting` | Guidance when something fails: element not found, click not working, screenshot failing |
| `app-guide` | Per-app query syntax, interaction methods and known quirks (Calculator, TextEdit, Safari, Chrome, Finder, Notes) |
| `automate-workflow` | Plan and track a multi-step workflow with `ax_workflow_create` / `step` / `status` |
| `debug-ui` | Debug why an element cannot be found: walk the tree, check attributes, suggest queries |
| `cross-app-copy` | Copy data between two applications |
| `analyze-app` | Detect patterns, infer state, suggest actions, audit accessibility |

## Confirmation of Destructive Actions

One tool asks for confirmation: `ax_click` refuses to click an element whose
title or description contains a destructive word (delete, remove, erase, quit,
close, format, reset, clear, wipe, destroy, terminate, uninstall) and returns an
error telling the caller to re-send with `confirm=true`. The confirmation is a
tool argument, so the calling agent can satisfy it on its own; it is not a
prompt to the human.

No other tool has a confirmation gate. `ax_exec`, `ax_fs_write`,
`ax_fs_delete`, `ax_term_send` and the rest run on the first call. The server
declares the MCP `elicitation` capability and the request builders exist in
`src/mcp/elicitation.rs`, but nothing in the server calls them, so no
`elicitation/create` request is ever sent.

## Tool Annotations

All tools carry MCP tool annotations:

| Annotation | Meaning |
|------------|---------|
| `readOnlyHint` | Tool only reads state, no side effects |
| `destructiveHint` | Tool modifies state (clicks, types, deletes) |
| `idempotentHint` | Calling multiple times has same effect |
| `openWorldHint` | Tool interacts with external systems |

"action" in the tables above means the tool sets none of these hints: it changes
state and is not idempotent, but is not marked destructive.

## Security

- Stdio transport has no authentication: whatever launches the server can call every tool.
- HTTP transport (`http-transport` feature) binds `127.0.0.1` by default and uses a bearer token. Without `--token` a random one is generated and printed to stderr; `--localhost-only` skips authentication and is refused on a non-loopback bind.
- `AXTERMINATOR_SECURITY_MODE=safe` blocks the scripting tools, `sandboxed` allows read-only tools only. Both are described in [Configuration](config.md#security-modes).
- Every mutating tool call is appended to `~/.local/share/axterminator/audit.jsonl`.
- Structured MCP logging with progress notifications.
