# AXTerminator -- Complete Setup for AI Assistants

> **Human**: Give this URL to your AI assistant and say "set up axterminator":
> `https://raw.githubusercontent.com/MikkoParkkola/axterminator/main/AGENTS.md`

## What To Do

You are setting up `axterminator` -- an MCP server that gives you the ability to see and control macOS applications via the Accessibility API. 27 core tools, up to 34+ with optional features. Run these steps in order:

### Step 1: Install

```bash
brew install MikkoParkkola/tap/axterminator
```

Pre-built binary, no toolchain needed. If Homebrew is not available:

```bash
cargo install axterminator --features cli
```

Verify:
```bash
axterminator check
# Expected: version string + "Accessibility: OK" (or DISABLED -- see Step 2)
```

### Step 2: Grant Accessibility Permission

The host application (Terminal, iTerm2, Ghostty, Claude Desktop, etc.) must have Accessibility permission:

1. Open: **System Settings > Privacy & Security > Accessibility**
2. Click the **+** button
3. Add the terminal application or AI host app
4. Ensure the toggle is **ON**
5. **Restart** the terminal / host app

Verify:
```bash
axterminator check
# Must print: "Accessibility: OK"
```

### Step 3: Register as MCP Server

```json
{
  "mcpServers": {
    "axterminator": {
      "command": "axterminator",
      "args": ["mcp", "serve"]
    }
  }
}
```

Add the above to your MCP client config:
- **Claude Code / Claude Desktop**: `~/.claude/settings.json` or project `.mcp.json`
- **Cursor / Windsurf / Continue.dev**: `.cursor/mcp.json` (or equivalent)
- **Codex**: `~/.codex/config.toml` (use TOML format: `[mcp_servers.axterminator]`)

For Claude Code, you can also run:
```bash
claude mcp add axterminator --transport stdio -- axterminator mcp serve
```

Then restart your MCP client.

### Step 4: Verify

```bash
axterminator apps
# Expected: list of running applications

axterminator find "File" --app Finder
# Expected: UI element match
```

Tell the user: "AXTerminator is installed with up to 34 MCP tools. I can see your screen, click buttons, type text, take screenshots, read UI trees, run AppleScript, audit accessibility, and control apps in the background while you work. Just tell me what to do."

---

## How To Use (after setup)

You now have MCP tools to control any macOS application. Tools are grouped by category:

### GUI Interaction

| Tool | What it does |
|------|-------------|
| `ax_connect` | Connect to an app by name, bundle ID, or PID |
| `ax_find` | Find UI element by text, role, or XPath query |
| `ax_click` | Click an element (background or focus mode) |
| `ax_click_at` | Click at screen coordinates |
| `ax_type` | Type text into an element |
| `ax_set_value` | Set an element's value |
| `ax_get_value` | Get an element's value |
| `ax_scroll` | Scroll an element |
| `ax_drag` | Drag between coordinates |
| `ax_key_press` | Send keyboard shortcuts |

### Observation

| Tool | What it does |
|------|-------------|
| `ax_is_accessible` | Check if Accessibility permission is granted |
| `ax_screenshot` | Capture app or element screenshot (base64 PNG) |
| `ax_get_tree` | Dump the accessibility tree |
| `ax_get_attributes` | Get all attributes of an element |
| `ax_list_windows` | List app windows with bounds |
| `ax_list_apps` | List running applications |
| `ax_wait_idle` | Wait for app to become idle |

### Verification

| Tool | What it does |
|------|-------------|
| `ax_assert` | Assert element state (exists, enabled, value) |
| `ax_find_visual` | Visual element detection (requires VLM) |
| `ax_visual_diff` | Visual regression testing (compare screenshots) |
| `ax_a11y_audit` | WCAG accessibility compliance audit |

### System

| Tool | What it does |
|------|-------------|
| `ax_clipboard` | Read/write system clipboard |
| `ax_run_script` | Execute AppleScript/JXA scripts |
| `ax_undo` | Undo last N actions (Cmd+Z) |
| `ax_session_info` | Server session state |
| `ax_analyze` | Detect UI patterns, infer app state, suggest actions |
| `ax_query` | Natural language UI questions |

### Workflows

| Tool | What it does |
|------|-------------|
| `ax_record` | Interaction recording for test generation |
| `ax_workflow_create` | Create a durable multi-step workflow |
| `ax_workflow_step` | Execute next workflow step |
| `ax_workflow_status` | Check workflow progress |
| `ax_track_workflow` | Cross-app pattern detection |
| `ax_test_run` | Black-box test execution |
| `ax_app_profile` | Electron app metadata |

### Optional Features (build-time flags)

| Tool | Feature | What it does |
|------|---------|-------------|
| `ax_listen` | audio | Capture mic/system audio |
| `ax_speak` | audio | Text-to-speech |
| `ax_audio_devices` | audio | List audio devices |
| `ax_camera_capture` | camera | Camera frames |
| `ax_gesture_detect` | camera | Gesture recognition |
| `ax_list_spaces` | spaces | List virtual desktops |
| `ax_create_space` | spaces | Create virtual desktop |

The Homebrew formula includes all features.

### Query Syntax

```
# Simple text -- matches title, description, value, label, identifier
ax_find query="Save"

# By role
ax_find query="role:AXButton"

# Combined (AND)
ax_find query="role:AXButton title:Save"

# By description (useful for Calculator-style apps)
ax_find query="description:equals"

# XPath-like
ax_find query="//AXButton[@AXTitle='Save']"
```

### Tips

- Background interaction works for click, read, screenshots -- the user's focus is not interrupted.
- Text input may need a focused text field in some apps.
- Use `ax_get_tree` first to understand an app's element hierarchy before interacting.
- Use `ax_screenshot` to see what the user sees when the tree is not enough.
- Elements found by `ax_find` persist in session -- use `ax_click` with the same query.

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| `Accessibility: DISABLED` | No permission | See Step 2 |
| `Element not found` for short labels | App uses `AXDescription` not `AXTitle` | Try `description:label` or inspect with `ax_get_tree` |
| `Application not found` | Wrong name or app not running | Use bundle ID: `ax_connect app="com.apple.calculator"` |
| Old version (missing tools) | Outdated binary | `brew upgrade axterminator` |

## Source

- GitHub: https://github.com/MikkoParkkola/axterminator
- License: AXTerminator Community License + commercial license. Free for
  personal, research, educational, noncommercial open-source, and free
  public-good projects with attribution. Business use requires a written
  commercial license. See `LICENSE.md` and `COMMERCIAL.md`.
