# AXTerminator

<div align="center">

[![Tests](https://img.shields.io/github/actions/workflow/status/MikkoParkkola/axterminator/ci.yml?style=for-the-badge&label=tests)](https://github.com/MikkoParkkola/axterminator/actions)
[![macOS](https://img.shields.io/badge/macOS-12%2B-black?style=for-the-badge&logo=apple)](https://github.com/MikkoParkkola/axterminator)
[![Rust](https://img.shields.io/badge/rust-stable-orange?style=for-the-badge&logo=rust)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue?style=for-the-badge)](LICENSE-MIT)
[![Discussions](https://img.shields.io/github/discussions/MikkoParkkola/axterminator?style=for-the-badge&color=blue)](https://github.com/MikkoParkkola/axterminator/discussions)

**MCP server that gives AI agents the ability to see and control macOS applications.**

[Deploy](#deploy) · [MCP Tools](#mcp-tools) · [CLI](#cli) · [Wiki](https://github.com/MikkoParkkola/axterminator/wiki) · [Known Limitations](#known-limitations)

</div>

---

Up to 30 MCP tools (19 core + optional audio, camera, spaces). Background interaction via the macOS Accessibility API. 379us per element access. Audio capture, camera input, virtual desktop isolation. Your AI agent connects and your Mac becomes an extension of it.

## Deploy

```bash
git clone https://github.com/MikkoParkkola/axterminator
cd axterminator
cargo build --release --features cli
```

Grant accessibility permissions: **System Settings > Privacy & Security > Accessibility** (add your terminal app).

### Connect your AI agent

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

Done. Your agent has 19 core tools (up to 30 with all feature flags) to control any macOS app.

## MCP Tools

| Category | Tools | What the agent can do |
|----------|-------|----------------------|
| **GUI** | `ax_connect`, `ax_find`, `ax_click`, `ax_click_at`, `ax_type`, `ax_set_value`, `ax_get_value`, `ax_scroll`, `ax_drag`, `ax_key_press` | Connect to apps, find elements, interact |
| **Observe** | `ax_is_accessible`, `ax_screenshot`, `ax_get_tree`, `ax_get_attributes`, `ax_list_windows`, `ax_list_apps`, `ax_wait_idle` | Check permissions, see UI state, screenshots |
| **Verify** | `ax_assert`, `ax_find_visual` | Assert element state, AI vision fallback |
| **Audio** | `ax_listen`, `ax_speak`, `ax_audio_devices` | Capture mic/system audio, text-to-speech |
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

## Known Limitations

| Operation | Background? | Notes |
|-----------|:-----------:|-------|
| Click, press, read values, screenshots | Yes | Core operations work without focus |
| Text input | Partial | Some apps need focused text field |
| Drag, system dialogs | No | Require cursor control / always grab focus |
| Gesture recognition | Yes | Verified: thumbs_up at 88.8% confidence |
| Speech transcription | Yes | Verified: on-device, requires Dictation enabled |

## Feature Flags

Build with optional capabilities:

```bash
cargo build --release --features "cli,audio,camera,spaces"
```

| Flag | What |
|------|------|
| `cli` | CLI + MCP server (default) |
| `audio` | Microphone/system audio, speech |
| `camera` | Camera capture, gesture detection |
| `spaces` | Virtual desktop management |
| `http-transport` | HTTP MCP transport with auth |

## Python API

Also available as a Python package for test scripts and pytest:

```bash
pip install axterminator
```

```python
import axterminator as ax
app = ax.app(name="Calculator")
app.find("7").click()
app.find("+").click()
app.find("3").click()
app.find("=").click()
```

See [API Reference](https://github.com/MikkoParkkola/axterminator/wiki/API-Reference) for full Python docs.

## Community

- [Wiki](https://github.com/MikkoParkkola/axterminator/wiki) -- Full documentation
- [Discussions](https://github.com/MikkoParkkola/axterminator/discussions) -- Questions, ideas, show-and-tell
- [Issues](https://github.com/MikkoParkkola/axterminator/issues) -- Bugs

## Acknowledgements

Inspired by [Terminator](https://github.com/mediar-ai/terminator) by [mediar-ai](https://github.com/mediar-ai), which pioneered accessible desktop GUI automation on Windows.

## License

[MIT](LICENSE-MIT) OR [Apache-2.0](LICENSE-APACHE)
