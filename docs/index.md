# AXTerminator

<div align="center">

**MCP server that gives AI agents the ability to see and control macOS applications.**

[![Crates.io](https://img.shields.io/crates/v/axterminator?style=for-the-badge)](https://crates.io/crates/axterminator)
[![Downloads](https://img.shields.io/crates/d/axterminator?style=for-the-badge)](https://crates.io/crates/axterminator)
[![macOS](https://img.shields.io/badge/macOS-12%2B-black?style=for-the-badge&logo=apple)](https://github.com/MikkoParkkola/axterminator)
[![License](https://img.shields.io/badge/license-community%20%2B%20commercial-blue?style=for-the-badge)](https://github.com/MikkoParkkola/axterminator/blob/main/LICENSE.md)

</div>

Up to 34+ MCP tools (27 core + optional audio, camera, spaces). Background interaction via the macOS Accessibility API. 379us per element access. Audio capture with native 48kHz speech recognition, camera input with gesture detection (88.8% thumbs_up verified), virtual desktop isolation. Your AI agent connects and your Mac becomes an extension of it.

**Current version: 0.8.0** --- Rust binary with MCP server, CLI, and optional audio/camera/spaces features.

## Deploy

### Rust Binary (Primary)

```bash
git clone https://github.com/MikkoParkkola/axterminator
cd axterminator
cargo build --release --features cli
```

Or install from crates.io:

```bash
cargo install axterminator --features cli
```

Or via Homebrew:

```bash
brew install MikkoParkkola/tap/axterminator
```

Grant accessibility permissions: **System Settings > Privacy & Security > Accessibility** (add your terminal app).

### Connect Your AI Agent

Add to MCP config (Claude Code, OpenCode, Cursor):

```json
{
  "mcpServers": {
    "axterminator": {
      "command": "/path/to/axterminator",
      "args": ["mcp", "serve"]
    }
  }
}
```

For Codex (`~/.codex/config.toml`):

```toml
[mcp_servers.axterminator]
command = "/path/to/axterminator"
args = ["mcp", "serve"]
```

Replace `/path/to/axterminator` with the actual binary path.

Done. Your agent has 27 core tools (up to 34+ with all feature flags) to control any macOS app.

## MCP Tools

| Category | Tools | What the agent can do |
|----------|-------|----------------------|
| **GUI** | `ax_connect`, `ax_find`, `ax_click`, `ax_click_at`, `ax_type`, `ax_set_value`, `ax_get_value`, `ax_scroll`, `ax_drag`, `ax_key_press` | Connect to apps, find elements, interact |
| **Observe** | `ax_is_accessible`, `ax_screenshot`, `ax_get_tree`, `ax_get_attributes`, `ax_list_windows`, `ax_list_apps`, `ax_wait_idle` | Check permissions, see UI state, screenshots |
| **Verify** | `ax_assert`, `ax_find_visual`, `ax_visual_diff`, `ax_a11y_audit` | Assert element state, AI vision fallback, visual regression, WCAG audit |
| **System** | `ax_clipboard`, `ax_run_script`, `ax_undo`, `ax_session_info`, `ax_analyze` | Clipboard, AppleScript/JXA, undo actions, session state, UI analysis |
| **Audio** | `ax_listen`, `ax_speak`, `ax_audio_devices` | Capture mic/system audio (48kHz native), text-to-speech |
| **Camera** | `ax_camera_capture`, `ax_gesture_detect`, `ax_gesture_listen` | Camera frames, gesture recognition |
| **Spaces** | `ax_list_spaces`, `ax_create_space`, `ax_move_to_space`, `ax_switch_space`, `ax_destroy_space` | Virtual desktop isolation |

### Resources

Agents can browse app state without tool calls:

| Resource | What |
|----------|------|
| `axterminator://apps` | Running applications |
| `axterminator://app/{name}/tree` | Live element hierarchy |
| `axterminator://app/{name}/screenshot` | Current screenshot |
| `axterminator://app/{name}/state` | Focused element, window title |
| `axterminator://system/displays` | Monitor layout |

### Security

Destructive actions require confirmation via elicitation. HTTP transport requires bearer token auth. The AI has hands, not root.

## Feature Flags

Build with optional capabilities:

```bash
cargo build --release --features "cli,audio,camera,spaces"
```

| Flag | What |
|------|------|
| `cli` | CLI + MCP server (default) |
| `audio` | Microphone/system audio, speech recognition (48kHz native capture) |
| `camera` | Camera capture, gesture detection (88.8% thumbs_up verified) |
| `watch` | Continuous background monitoring (implies audio + camera) |
| `spaces` | Virtual desktop management (CGSSpace private API) |
| `http-transport` | Streamable HTTP MCP transport with bearer token auth |

## CLI

```bash
axterminator apps                        # List accessible apps
axterminator find "Save" --app Safari    # Find element
axterminator click "Save" --app Safari   # Click it
axterminator screenshot --app Safari     # Capture screenshot
axterminator tree --app Finder           # Element hierarchy
axterminator mcp serve --http 8080 --token secret  # HTTP transport
```

## How It Works

AXTerminator uses an undocumented behavior of Apple's Accessibility API: `AXUIElementPerformAction()` works on unfocused windows. Your agent clicks buttons in one app while you work in another. Neither notices.

379us per element access (Criterion, M1 MacBook Pro). Appium needs 500ms for the same thing.

7-strategy self-healing locators survive UI changes: data_testid, aria_label, identifier, title, xpath, position, visual_vlm.

## Performance

Measured on Apple M1 MacBook Pro, macOS 14.2:

| Operation | Time |
|-----------|------|
| Single attribute read | ~54 us |
| Element access | ~379 us |
| Perform action | ~20 us |

## Known Limitations

| Operation | Background? | Notes |
|-----------|:-----------:|-------|
| Click, press, read values, screenshots | Yes | Core operations work without focus |
| Text input | Partial | Some apps need focused text field |
| Drag, system dialogs | No | Require cursor control / always grab focus |
| Gesture recognition | Yes | Verified: thumbs_up at 88.8% confidence |
| Speech transcription | Yes | On-device, 48kHz native capture, requires Dictation enabled |

## Release Channels

| Channel | Package |
|---------|---------|
| crates.io | [`axterminator`](https://crates.io/crates/axterminator) |
| Homebrew | `brew install MikkoParkkola/tap/axterminator` |
| GitHub Releases | [Binary assets](https://github.com/MikkoParkkola/axterminator/releases) |

## For AI Agents

Machine-readable installation guide: [`llms.txt`](https://raw.githubusercontent.com/MikkoParkkola/axterminator/main/llms.txt)

## License

AXTerminator is free for personal, research, educational, noncommercial
open-source, and free public-good use with attribution.

Business use requires a written commercial license. See
[`LICENSE.md`](../LICENSE.md) and [`COMMERCIAL.md`](../COMMERCIAL.md).

Earlier releases published under `MIT OR Apache-2.0` remain under those earlier
license grants for those earlier versions only.
