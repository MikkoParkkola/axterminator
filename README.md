# AXTerminator

<div align="center">

[![CI](https://img.shields.io/github/actions/workflow/status/MikkoParkkola/axterminator/ci.yml?style=for-the-badge&label=tests)](https://github.com/MikkoParkkola/axterminator/actions)
[![Crates.io](https://img.shields.io/crates/v/axterminator?style=for-the-badge)](https://crates.io/crates/axterminator)
[![Downloads](https://img.shields.io/crates/d/axterminator?style=for-the-badge)](https://crates.io/crates/axterminator)
[![macOS](https://img.shields.io/badge/macOS-12%2B-black?style=for-the-badge&logo=apple)](https://github.com/MikkoParkkola/axterminator)
[![Rust](https://img.shields.io/badge/rust-stable-orange?style=for-the-badge&logo=rust)](https://www.rust-lang.org/)
[![License](https://img.shields.io/crates/l/axterminator?style=for-the-badge)](LICENSE-MIT)
[![Discussions](https://img.shields.io/github/discussions/MikkoParkkola/axterminator?style=for-the-badge&color=blue)](https://github.com/MikkoParkkola/axterminator/discussions)

**MCP server that gives AI agents the ability to see and control macOS applications.**

[Deploy](#deploy) · [MCP Tools](#mcp-tools) · [CLI](#cli) · [Query Syntax](#query-syntax) · [Troubleshooting](#troubleshooting) · [Wiki](https://github.com/MikkoParkkola/axterminator/wiki) · [Known Limitations](#known-limitations)

</div>

---

Feature-gated MCP tool surface for macOS accessibility automation, with optional audio, camera, watch, spaces, docker browser, and context capabilities. Background interaction via the macOS Accessibility API. 379us per element access. Your AI agent connects and your Mac becomes an extension of it.

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

For Codex (`~/.codex/config.toml`):

```toml
[mcp_servers.axterminator]
command = "/path/to/axterminator"
args = ["mcp", "serve"]
```

Replace `/path/to/axterminator` with the actual binary path (run `which axterminator` or use the full build output path).

Done. Your agent can call `tools/list` to discover the exact runtime tool surface for this build.

## MCP Tools

| Category | Tools | What the agent can do |
|----------|-------|----------------------|
| **Discover** | `ax_is_accessible`, `ax_connect`, `ax_find`, `ax_list_windows`, `ax_list_apps`, `ax_get_tree`, `ax_find_visual` | Check permissions, connect to apps, inspect UI state |
| **Interact** | `ax_click`, `ax_click_at`, `ax_type`, `ax_set_value`, `ax_get_value`, `ax_get_attributes`, `ax_scroll`, `ax_drag`, `ax_key_press`, `ax_screenshot`, `ax_wait_idle` | Act on UI elements and capture app state |
| **Verify & analyze** | `ax_assert`, `ax_analyze`, `ax_visual_diff`, `ax_a11y_audit`, `ax_test_run` | Assertions, semantic analysis, visual regression, accessibility audits |
| **Workflow & system** | `ax_query`, `ax_workflow_create`, `ax_workflow_step`, `ax_workflow_status`, `ax_track_workflow`, `ax_record`, `ax_undo`, `ax_run_script`, `ax_clipboard`, `ax_session_info`, `ax_app_profile`, `ax_system_context` | Higher-level automation, scripting, workflow state, and system context |
| **Audio** | `ax_audio_devices`, `ax_listen`, `ax_speak`, `ax_start_capture`, `ax_stop_capture`, `ax_get_transcription`, `ax_capture_status` | Audio capture, transcription, and text-to-speech |
| **Camera** | `ax_camera_capture`, `ax_gesture_detect`, `ax_gesture_listen` | Camera frames and gesture recognition |
| **Watch** | `ax_watch_start`, `ax_watch_stop`, `ax_watch_status` | Continuous background audio/camera monitoring |
| **Browser** | `ax_browser_launch`, `ax_browser_stop` | Disposable browser containers via Docker |
| **Context** | `ax_location` | Location lookup when the optional context feature is enabled |
| **Spaces** | `ax_list_spaces`, `ax_create_space`, `ax_move_to_space`, `ax_switch_space`, `ax_destroy_space` | Virtual desktop isolation |

The exact set depends on enabled feature flags; always call `tools/list` for the live schema.

### Resources

Representative resources (call `resources/list` for the exact build surface):

| Resource | What |
|----------|------|
| `axterminator://apps` | Running applications |
| `axterminator://app/{name}/tree` | Live element hierarchy |
| `axterminator://app/{name}/screenshot` | Current screenshot |
| `axterminator://app/{name}/state` | Focused element, window title |
| `axterminator://system/displays` | Monitor layout |
| `axterminator://clipboard` | Clipboard text |
| `axterminator://workflows` | Detected cross-app workflow patterns |
| `axterminator://profiles` | Built-in Electron app profiles |

### Security

Destructive actions require confirmation via elicitation. HTTP transport requires bearer token auth. The AI has hands, not root.

Continuous integration now also checks RustSec advisories and dependency-source policy on every PR/push, with weekly Dependabot updates for both Cargo crates and GitHub Actions.

Privileged accessibility flows are validated separately from hosted CI: GitHub-hosted runners cannot grant macOS Accessibility/TCC permissions, so CI covers non-privileged logic and dependency security, while permissioned background-control paths still require manual validation on a real macOS host.

## CLI

```bash
axterminator apps                        # List accessible apps
axterminator find "Save" --app Safari    # Find element
axterminator click "Save" --app Safari   # Click it
axterminator screenshot --app Safari     # Capture screenshot
axterminator tree --app Finder           # Element hierarchy
axterminator mcp serve --http 8080 --token secret  # HTTP transport
```

## Query Syntax

```bash
# Simple text — matches ANY of: title, description, value, label, identifier
axterminator find "Save" --app Safari

# By role
axterminator find "role:AXButton" --app Safari

# Combined role + attribute (AND)
axterminator find "role:AXButton title:Save" --app Safari

# By description (useful for apps like Calculator)
axterminator find "description:equals" --app Calculator

# By value
axterminator find "value:42" --app Calculator

# XPath-like
axterminator find "//AXButton[@AXTitle='Save']" --app Safari
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

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| `Accessibility: DISABLED` | No permission granted | System Settings > Privacy & Security > Accessibility |
| `Element not found` for short labels | App uses `AXDescription` not `AXTitle` | Try `description:label` or inspect with `axterminator tree` |
| `Application not found` | Wrong name or app not running | Use bundle ID: `--bundle-id com.apple.calculator` |

**AI agents**: Fetch [llms.txt](https://raw.githubusercontent.com/MikkoParkkola/axterminator/main/llms.txt) for machine-readable installation instructions.

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
| `docker` | Disposable browser containers for isolated testing |
| `context` | Location lookup via CoreLocation |
| `http-transport` | HTTP MCP transport with auth |

## Community

- [Wiki](https://github.com/MikkoParkkola/axterminator/wiki) -- Full documentation
- [Discussions](https://github.com/MikkoParkkola/axterminator/discussions) -- Questions, ideas, show-and-tell
- [Issues](https://github.com/MikkoParkkola/axterminator/issues) -- Bugs

## Acknowledgements

Inspired by [Terminator](https://github.com/mediar-ai/terminator) by [mediar-ai](https://github.com/mediar-ai), which pioneered accessible desktop GUI automation on Windows.

## For AI Agents

Machine-readable installation guide: [`llms.txt`](https://raw.githubusercontent.com/MikkoParkkola/axterminator/main/llms.txt)

Your agent can fetch this URL to get step-by-step installation, MCP config for every host, and troubleshooting.

## License

[MIT](LICENSE-MIT) OR [Apache-2.0](LICENSE-APACHE)
