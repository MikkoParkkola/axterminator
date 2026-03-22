# Quick Start

## MCP Server (AI Agents)

The primary use case is connecting an AI agent to your Mac via MCP.

### 1. Build or Install

```bash
cargo install axterminator --features cli
```

### 2. Configure Your Agent

**Claude Code** (`~/.claude/settings.json` or project `.mcp.json`):

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

**Codex** (`~/.codex/config.toml`):

```toml
[mcp_servers.axterminator]
command = "axterminator"
args = ["mcp", "serve"]
```

**Cursor / Windsurf** (`.cursor/mcp.json`):

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

> **Tip:** If `axterminator` is not in your PATH, use the full path to the binary (e.g., `~/.cargo/bin/axterminator` or `/usr/local/bin/axterminator`).

### 3. Grant Permissions

Open **System Settings > Privacy & Security > Accessibility** and add your terminal app.

Your agent now has 19 core tools to control any macOS app.

## CLI

```bash
# List apps
axterminator apps

# Find elements
axterminator find "Save" --app Safari

# Click elements
axterminator click "Save" --app Safari

# Element hierarchy
axterminator tree --app Finder

# Screenshots
axterminator screenshot --app Safari

# HTTP MCP transport
axterminator mcp serve --http 8080 --token secret
```

## Python API

```python
import axterminator as ax

# 1. Check permissions
if not ax.is_accessibility_enabled():
    print("Enable in System Settings > Privacy > Accessibility")
    exit(1)

# 2. Connect to an app
app = ax.app(name="Calculator")

# 3. Find and click elements (in BACKGROUND!)
app.find("7").click()
app.find("+").click()
app.find("3").click()
app.find("=").click()

# Result: 10
```

!!! tip "Background Testing"
    AXTerminator clicks in the background by default. Your active window stays focused while tests run!

## Connection Methods

```python
# By name
app = ax.app(name="Safari")

# By bundle ID (recommended for reliability)
app = ax.app(bundle_id="com.apple.Safari")

# By PID
app = ax.app(pid=12345)
```

## Finding Elements

```python
# Simple text — matches ANY of: title, description, value, label, identifier
button = app.find("Save")

# With timeout
button = app.find("Save", timeout_ms=5000)

# By role
text_field = app.find("role:AXTextField")

# Combined (AND semantics)
save_btn = app.find("role:AXButton title:Save")

# By description (useful for Calculator-style apps)
equals = app.find("description:equals")

# By value
display = app.find("value:42")
```

## Actions

```python
# Background clicks (default)
element.click()
element.double_click()
element.right_click()

# Focus mode for text input
element.click(mode=ax.FOCUS)
element.type_text("Hello World!")
```

## pytest Integration

```python
import pytest

@pytest.mark.ax_requires_app("Calculator")
def test_addition(ax_app, ax_wait):
    app = ax_app("Calculator")
    app.find("7").click()
    app.find("+").click()
    app.find("3").click()
    app.find("=").click()
    ax_wait(0.1)
```

Available fixtures: `ax_app`, `ax_wait`, `ax_calculator`, `ax_finder`
