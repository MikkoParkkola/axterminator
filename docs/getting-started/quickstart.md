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

Your agent can now call `tools/list` to inspect the exact runtime tool surface for this build.

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
