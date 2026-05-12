# AXTerminator

<div align="center">

[![CI](https://img.shields.io/github/actions/workflow/status/MikkoParkkola/axterminator/ci.yml?style=for-the-badge&label=tests)](https://github.com/MikkoParkkola/axterminator/actions)
[![Crates.io](https://img.shields.io/crates/v/axterminator?style=for-the-badge)](https://crates.io/crates/axterminator)
[![Downloads](https://img.shields.io/crates/d/axterminator?style=for-the-badge)](https://crates.io/crates/axterminator)
[![macOS](https://img.shields.io/badge/macOS-12%2B-black?style=for-the-badge&logo=apple)](https://github.com/MikkoParkkola/axterminator)
[![Rust](https://img.shields.io/badge/rust-stable-orange?style=for-the-badge&logo=rust)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-community%20%2B%20commercial-blue?style=for-the-badge)](LICENSE.md)
[![Discussions](https://img.shields.io/github/discussions/MikkoParkkola/axterminator?style=for-the-badge&color=blue)](https://github.com/MikkoParkkola/axterminator/discussions)
[![Install in VS Code](https://img.shields.io/badge/VS_Code-Install_MCP-0078d4?style=for-the-badge&logo=visualstudiocode)](https://insiders.vscode.dev/redirect/mcp/install?name=axterminator&config=%7B%22command%22%3A%22axterminator%22%2C%22args%22%3A%5B%22mcp%22%2C%22serve%22%5D%7D)
[![Install in Cursor](https://img.shields.io/badge/Cursor-Install_MCP-black?style=for-the-badge&logo=cursor)](cursor://anysphere.cursor-deeplink/mcp/install?name=axterminator&config=%7B%22command%22%3A%22axterminator%22%2C%22args%22%3A%5B%22mcp%22%2C%22serve%22%5D%7D)

**MCP server that gives AI agents the ability to see and control macOS applications.**

[Deploy](#deploy) · [MCP Tools](#mcp-tools) · [CLI](#cli) · [Query Syntax](#query-syntax) · [AX-first vs Vision-first](#ax-first-vs-vision-first) · [FAQ](#faq) · [Troubleshooting](#troubleshooting) · [Wiki](https://github.com/MikkoParkkola/axterminator/wiki) · [Known Limitations](#known-limitations)

</div>

---

Up to 34+ MCP tools (27 core + optional audio, camera, spaces). Background interaction via the macOS Accessibility API. 379us per element access. Audio capture, camera input, virtual desktop isolation. Your AI agent connects and your Mac becomes an extension of it.

**Platform scope.** AX-first-with-vision-fallback is **macOS-only**. iOS/iPadOS support is tracked in [#43](https://github.com/MikkoParkkola/axterminator/issues/43) as a future capability and, when shipped, will be **screenshot + vision only** — the `idevice`/RPPairing surface does not expose AX trees for third-party apps, and UIAutomation would require an on-device test runner (breaks the "mac-only agent, no on-device prereqs" contract). Reported screenshot latency via CoreDeviceProxy: **~350ms USB, ~700ms WiFi** (credit [@m13v](https://github.com/MikkoParkkola/axterminator/issues/43#issuecomment-4274488358)). tvOS is **unverified** — the RSD surface is narrower than `idevice`'s tvOS target suggests; please open an issue with device evidence if you've tested. See [#42](https://github.com/MikkoParkkola/axterminator/issues/42) for the AX-first-vs-vision-first positioning rationale.

## Deploy

**Tell your AI assistant** (recommended):

> Read https://github.com/MikkoParkkola/axterminator and install axterminator as my macOS GUI automation MCP server

Your agent will install the binary, wire itself up, and request accessibility permissions. Works in Claude Code, Cursor, Windsurf, and any AI with terminal access.

**Or install manually:**

```bash
brew install MikkoParkkola/tap/axterminator
```

Grant accessibility permissions: **System Settings > Privacy & Security > Accessibility** (add your terminal app).

`axterminator mcp install` auto-detects your AI client. Specify one with `--client`:

```bash
axterminator mcp install                       # Claude Desktop (default)
axterminator mcp install --client claude-code  # Claude Code
axterminator mcp install --client cursor       # Cursor
axterminator mcp install --client windsurf     # Windsurf
axterminator mcp install --client codex        # OpenAI Codex CLI
axterminator mcp install --client vscode       # VS Code Copilot
axterminator mcp install --client zed          # Zed
axterminator mcp install --dry-run             # Preview without writing
```

Also supported: `gemini`, `amazon-q`, `lm-studio`.

<details>
<summary>More install options</summary>

```bash
# From crates.io
cargo install axterminator --features cli

# Build from source
git clone https://github.com/MikkoParkkola/axterminator
cd axterminator
cargo build --release --features cli

# Manual JSON (Claude Desktop, Cursor, Windsurf, etc.)
# { "mcpServers": { "axterminator": { "command": "axterminator", "args": ["mcp", "serve"] } } }
```

</details>

<details>
<summary>Codex config</summary>

For Codex (`~/.codex/config.toml`):

```toml
[mcp_servers.axterminator]
command = "axterminator"
args = ["mcp", "serve"]
```

</details>

Done. Your agent has 27 core tools (up to 34+ with all feature flags) to control any macOS app.

## MCP Tools

| Category | Tools | What the agent can do |
|----------|-------|----------------------|
| **GUI** | `ax_connect`, `ax_find`, `ax_click`, `ax_click_at`, `ax_type`, `ax_set_value`, `ax_get_value`, `ax_scroll`, `ax_drag`, `ax_key_press` | Connect to apps, find elements, interact |
| **Observe** | `ax_is_accessible`, `ax_screenshot`, `ax_get_tree`, `ax_get_attributes`, `ax_list_windows`, `ax_list_apps`, `ax_wait_idle` | Check permissions, see UI state, screenshots |
| **Verify** | `ax_assert`, `ax_find_visual`, `ax_visual_diff`, `ax_a11y_audit` | Assert element state, AI vision fallback, visual regression, WCAG audit |
| **System** | `ax_clipboard`, `ax_run_script`, `ax_undo`, `ax_session_info`, `ax_analyze` | Clipboard, AppleScript/JXA, undo actions, session state, UI analysis |
| **Audio** | `ax_listen`, `ax_speak`, `ax_audio_voices`, `ax_audio_devices` | Capture mic/system audio, text-to-speech, inspect installed macOS voices; optional Kokoro/Piper TTS via `enhanced-tts` |
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

## AX-first vs Vision-first

OpenAI Codex computer use, Anthropic Claude Computer Use, Google Gemini Operator, and Perplexity Personal Computer all use the same paradigm: **screenshot → vision LLM → pixel coordinates → cursor click**. The vendor changes; the paradigm does not.

axterminator uses the opposite default: **AX semantic tree → element reference → action**. Vision is a fallback for the rare surfaces the AX tree cannot reach (canvas apps, games, OpenGL/Metal renderers).

| Dimension | Vision-first (Codex CU / Claude CU / Gemini / …) | AX-first (axterminator) |
|-----------|---------------------------------------------------|-------------------------|
| **Speed** | seconds per action (vision round-trip + LLM) | ~379 µs per action (measured, Criterion) |
| **Cost** | vision tokens on every action | ~free (AX API call) |
| **Reliability** | pixel-brittle — breaks on theme, font, layout changes | element-stable — semantic addressing survives UI changes |
| **Background** | visible cursor movement | truly background — no visible cursor |
| **Dense / labeled UIs** | struggles with small targets | reads labels directly from the AX tree |
| **Canvas / games / OpenGL** | works (universal fallback) | needs vision fallback (`ax_find_visual`) |
| **Cross-platform** | anywhere a screenshot works | macOS only (see [#43](https://github.com/MikkoParkkola/axterminator/issues/43) for iOS roadmap) |

**The key insight:** axterminator is not a competitor to those agents — it is a layer *underneath* them. Any agent that can call an MCP tool (Claude Code, Codex CLI, Cursor, Windsurf, VS Code Copilot, Gemini CLI) can use axterminator as its hands. They provide reasoning; axterminator provides reliable, cheap, background-safe action.

> **Coverage gate.** Before investing heavily in vision-fallback features, run the AX coverage audit below for one week on your actual app surface. If >95% of your actions resolve via AX, vision fallback is a nice-to-have. If <80%, expand the vision fallback path first. See [benches/probes/README.md](benches/probes/README.md) for the audit harness.

## FAQ

**Why use this instead of Codex computer use / Claude Computer Use / Gemini Operator?**

You probably use *both*. Those agents call axterminator as an MCP tool. axterminator gives them AX-semantic actions that are ~1,000× faster and cost nothing per call. The vision model stays for tasks that genuinely need it (canvas apps, games), not for clicking a Save button in TextEdit.

**Isn't vision-first simpler — no setup, works everywhere?**

It works everywhere vision works: foreground, focused, slow, expensive. axterminator works in the background, costs nothing, and is sub-millisecond. For automation of native macOS apps — the majority of business software — AX is strictly better. Vision is the escape hatch, not the default.

The binding architecture decision is [ADR-0001: AX-first With Vision Fallback](docs/architecture/decisions/ADR-0001-ax-first-with-vision-fallback.md).

**What app surfaces does axterminator cover?**

Any macOS app that exposes the Accessibility API: all native AppKit/SwiftUI apps, Electron apps, web apps in Chrome/Safari/Firefox, terminal apps. Canvas-only surfaces (Figma canvas, game renderers, video players) use `ax_find_visual` — the built-in vision fallback that tries AX first and falls back to VLM automatically.

**Does it work with agents that are already doing computer use?**

Yes. Add `axterminator mcp install --client codex` (or `--client claude-code`, etc.) and the agent gains 34 semantic tools it can call instead of pixel-clicking. It doesn't replace the agent's vision; it gives the agent a faster, cheaper path for the 90%+ of actions that don't need vision.

**Who is it for beyond developers?**

- **Executive assistants:** file triage, calendar entry, copy-paste between apps — all in the background while you work
- **Sales ops:** CRM entry, proposal generation across native apps, screen scraping structured data from dense UIs
- **Event coordinators:** spreadsheet-to-calendar sync, venue research across multiple apps, confirmation email drafts — reliable because it reads labels semantically, not by pixel position

## Known Limitations

| Operation | Background? | Notes |
|-----------|:-----------:|-------|
| Click, press, read values, screenshots | Yes | Core operations work without focus |
| Text input | Partial | Some apps need focused text field |
| Drag, system dialogs | No | Require cursor control / always grab focus |
| Gesture recognition | Yes | Verified: thumbs_up at 88.8% confidence |
| Speech transcription | Yes | Verified: on-device, requires Dictation enabled |

**Platform coverage** (see [#43](https://github.com/MikkoParkkola/axterminator/issues/43)):

| Platform | AX tree | Screenshot | App launch | Status |
|----------|:-------:|:----------:|:----------:|--------|
| macOS 12+ | ✅ | ✅ | ✅ | Shipped; sub-ms AX path |
| iOS / iPadOS | ❌ (third-party apps) | ✅ (~350ms USB / ~700ms WiFi) | ✅ | **Planned, screenshot + vision only** — no AX semantics |
| tvOS | ❔ | ❔ | ❔ | **Unverified** — RSD surface narrower than `idevice` suggests |
| visionOS / watchOS | ❌ | ❌ | ❌ | Out of scope |

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| `Accessibility: DISABLED` | No permission granted | System Settings > Privacy & Security > Accessibility |
| `Element not found` for short labels | App uses `AXDescription` not `AXTitle` | Try `description:label` or inspect with `axterminator tree` |
| `Application not found` | Wrong name or app not running | Use bundle ID: `--bundle-id com.apple.calculator` |

**AI agents**: Fetch [llms.txt](https://raw.githubusercontent.com/MikkoParkkola/axterminator/main/llms.txt) for machine-readable installation instructions.

## Feature Flags

The Homebrew formula includes all features. When building from source, select capabilities with feature flags:

```bash
cargo build --release --features "cli,audio,enhanced-tts,camera,spaces"
```

| Flag | What |
|------|------|
| `cli` | CLI + MCP server (default) |
| `audio` | Microphone/system audio, speech |
| `enhanced-tts` | Optional Kokoro/Piper TTS engine routing and `axterminator models tts` downloads |
| `camera` | Camera capture, gesture detection |
| `spaces` | Virtual desktop management |
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

## Ecosystem

axterminator is part of a suite of MCP tools:

| Tool | Description |
|------|-------------|
| [mcp-gateway](https://github.com/MikkoParkkola/mcp-gateway) | Universal MCP gateway — compact 12-15 tool surface replaces 100+ registrations |
| [trvl](https://github.com/MikkoParkkola/trvl) | AI travel agent — 36 MCP tools for flights, hotels, ground transport |
| [nab](https://github.com/MikkoParkkola/nab) | Web content extraction — fetch any URL with cookies + anti-bot bypass |
| **[axterminator](https://github.com/MikkoParkkola/axterminator)** | **macOS GUI automation — 34 MCP tools via Accessibility API** |

## License

AXTerminator is free for personal, research, educational, noncommercial
open-source, and free public-good projects with attribution.

Business use requires a written commercial license. See [LICENSE.md](LICENSE.md)
and [COMMERCIAL.md](COMMERCIAL.md).

Earlier releases published under `MIT OR Apache-2.0` remain under those earlier
license grants for those earlier versions only.
