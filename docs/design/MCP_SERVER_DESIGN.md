# AXTerminator MCP Server Design Document

**Status**: Draft | **Version**: 3.0 | **Date**: 2026-03-19
**Protocol**: MCP 2025-11-25 | **SDK**: `rmcp` 1.2.0 (official Rust MCP SDK)
**Runtime**: Pure Rust (unified `axterminator` binary) | **Deprecates**: `server.py` (Python)
**Author**: Mikko Parkkola

---

## 1. Vision

### The Hands and Eyes of AI on macOS

AXTerminator's MCP server transforms any MCP-compatible AI agent into a full operator
of macOS applications. The agent can see (screenshots, element trees), understand
(accessibility attributes, semantic UI patterns), and control (click, type, drag, scroll)
any macOS application -- all without stealing window focus.

**Positioning**: The definitive MCP server for macOS GUI interaction. Where
mediar-ai/terminator owns Windows and browser-use owns Chrome, axterminator owns
macOS.

### Design Principles

1. **Background-first**: Every operation defaults to background mode. The user's
   active window is never disturbed unless explicitly requested.
2. **Accessibility + Vision**: Accessibility API is the primary channel. VLM visual
   detection is the automatic fallback when accessibility fails (canvas, WebGL,
   shadow DOM).
3. **Sub-millisecond core**: The Rust core operates at ~379us per element access.
   The MCP protocol layer adds transport overhead but never blocks the core path.
4. **Progressive disclosure**: Simple tools for common tasks, resources for rich
   context, prompts for guided workflows, sampling for autonomous reasoning.
5. **Reactive, not polling**: Resource subscriptions and AX observer notifications
   push UI state changes to agents. Agents react to events rather than polling.
6. **Composable operations**: Complex multi-step workflows compose from atomic tools
   with transaction semantics, rollback on failure, and macro recording/replay.
7. **Pure Rust, three interfaces**: One Rust core exposes CLI, MCP server, and Python
   API. No Python in the hot path.

### What This Enables

- Claude, GPT, Gemini, or any MCP client can operate any macOS app
- Test automation orchestrated by AI agents with assertions and visual regression
- Agentic workflows that span multiple applications with cross-app coordination
- AI-assisted debugging of UI issues with semantic pattern recognition
- Data extraction from apps that have no API
- Accessibility auditing powered by AI with WCAG compliance checking
- Macro recording of human actions replayed as MCP tool sequences

---

## 2. Architecture -- Unified Rust Binary

### 2.1 Three Interfaces, One Core

```
                        +-----------------------+
                        |   axterminator core   |
                        |   (Rust, ~379us/op)   |
                        +-----------+-----------+
                                    |
              +---------------------+---------------------+
              |                     |                     |
    +---------+---------+ +--------+--------+ +----------+---------+
    | CLI (clap)        | | MCP Server      | | Python API (pyo3)  |
    | axterminator find | | axterminator    | | import axterminator|
    | axterminator click| |   mcp serve     | | ax.app("Safari")   |
    +---------+---------+ +--------+--------+ +----------+---------+
              |                     |                     |
        Human / shell          AI agents            Python scripts
```

The `axterminator` binary is the single entry point:

```
axterminator mcp serve              # Start MCP server (stdio, default)
axterminator mcp serve --http       # Start MCP server (Streamable HTTP)
axterminator mcp serve --http --port 9000
axterminator find Safari "URL bar"  # CLI: find element
axterminator click Safari "Submit"  # CLI: click element
axterminator screenshot Safari      # CLI: capture screenshot
axterminator tree Safari            # CLI: dump element tree
axterminator record                 # CLI: record macro
axterminator replay macro.json      # CLI: replay macro
```

### 2.2 Crate Architecture

```
axterminator/
  src/
    lib.rs                 # Core library: AXApp, AXElement, healing, cache
    main.rs                # CLI entry point (clap)
    cli/
      mod.rs               # CLI command routing
      find.rs              # find subcommand
      actions.rs           # click, type, scroll, drag subcommands
      observe.rs           # screenshot, tree, list subcommands
      record.rs            # macro recording
      replay.rs            # macro replay
    mcp/
      mod.rs               # MCP server setup, capability negotiation
      transport.rs         # stdio + Streamable HTTP + SSE transports
      session.rs           # Session state, AppConnectionManager
      tools/
        mod.rs             # Tool registration and dispatch
        connect.rs         # ax_connect, ax_list_apps, ax_is_accessible
        find.rs            # ax_find, ax_find_visual, ax_get_tree, ax_get_attributes
        actions.rs         # ax_click, ax_type, ax_set_value, ax_scroll, ax_drag, ax_key_press
        observe.rs         # ax_screenshot, ax_get_value, ax_list_windows, ax_wait_idle
        scripting.rs       # ax_run_script
        compose.rs         # ax_workflow (composed operations)
        assert.rs          # ax_assert, ax_visual_diff, ax_a11y_audit
        context.rs         # ax_undo, ax_clipboard, ax_session_info
      resources/
        mod.rs             # Resource registration, subscription engine
        apps.rs            # axterminator://apps, axterminator://system/status
        app_state.rs       # axterminator://app/{name}/tree, /state, /screenshot
        events.rs          # axterminator://events (notification stream)
        clipboard.rs       # axterminator://clipboard
      prompts/
        mod.rs             # Prompt registration
        workflows.rs       # test-app, navigate-to, automate-workflow
        debugging.rs       # debug-ui, accessibility-audit
        extraction.rs      # extract-data
      elicitation.rs       # Form + URL mode elicitation helpers
      sampling.rs          # Screenshot interpretation, action planning
      tasks.rs             # Task-augmented execution for long-running ops
      security.rs          # Allowlist, audit log, rate limiting, sandboxed mode
      auth.rs              # OAuth 2.1 for HTTP transport (RFC9728)
    intelligence/
      mod.rs               # Semantic UI understanding
      patterns.rs          # Login form, settings page, dialog detection
      state_detection.rs   # Loading, idle, error, auth-required detection
      suggestions.rs       # Next-action suggestions based on UI context
    compose/
      mod.rs               # Workflow composition engine
      workflow.rs          # Multi-step workflow definition and execution
      transaction.rs       # Transaction semantics with rollback
      recording.rs         # Macro recording from AX observer events
    python/
      mod.rs               # pyo3 module (existing)
```

### 2.3 Server Lifecycle

```rust
/// Server lifecycle managed via async context.
/// Initializes connection manager, VLM backend, subscription engine,
/// event bus, audit log, and undo stack.
struct ServerState {
    app_manager: AppConnectionManager,
    vlm_backend: Option<VlmBackend>,
    subscription_engine: SubscriptionEngine,
    event_bus: EventBus,
    audit_log: AuditLog,
    undo_stack: UndoStack,
    clipboard_monitor: ClipboardMonitor,
    security: SecurityPolicy,
    intelligence: UiIntelligence,
}
```

### 2.4 State Management

```rust
/// Manages connected application state across the server lifecycle.
/// Thread-safe via tokio::sync::RwLock for concurrent tool calls.
struct AppConnectionManager {
    apps: RwLock<HashMap<String, AppConnection>>,
}

struct AppConnection {
    app: AXApp,
    alias: String,
    connected_at: Instant,
    pid: u32,
    bundle_id: Option<String>,
    last_action: RwLock<Option<ActionRecord>>,
}

impl AppConnectionManager {
    async fn connect(&self, identifier: &str, alias: Option<&str>) -> Result<AppConnection>;
    async fn get(&self, name: &str) -> Result<&AppConnection>;
    async fn disconnect(&self, name: &str) -> Result<()>;
    async fn disconnect_all(&self) -> Result<()>;
    fn connected_names(&self) -> Vec<String>;
}
```

---

## 2A. Rust MCP SDK -- `rmcp`

### 2A.1 SDK Selection

After evaluating all available Rust MCP crates (as of 2026-03-19), `rmcp` is the
clear choice.

| Crate | Version | Downloads | Maintainer | Verdict |
|-------|---------|-----------|------------|---------|
| **`rmcp`** | **1.2.0** | **5.7M** | **Official (modelcontextprotocol org)** | **USE THIS** |
| `rust-mcp-sdk` | 0.9.0 | 92K | Community (rust-mcp-stack) | Third-party, lower adoption |
| `tower-mcp` | 0.9.1 | 5.8K | Community (joshrotenberg) | Tower-native, interesting but young |
| `mcp-attr` | 0.0.7 | 6.3K | Community (frozenlib) | Declarative macros, too early |
| `clap-mcp` | 0.0.3-rc.1 | 187 | Community (canardleteer) | CLI+MCP bridge, alpha quality |
| `mcp-kit` | 0.4.0 | 118 | Community (KSD-CO) | Plugin system, too early |

**`rmcp` advantages**:
- Official Rust SDK from the MCP protocol team (`github.com/modelcontextprotocol/rust-sdk`)
- 5.7M downloads, actively maintained (v1.2.0 released 2026-03-11)
- Full protocol support: tools, resources, prompts, sampling, completions, logging, tasks
- Procedural macros via `rmcp-macros` for ergonomic tool definitions (`#[tool]`, `#[prompt_router]`)
- Tokio async runtime (matches our existing async architecture)
- Built-in stdio and HTTP/SSE transports
- OAuth support for HTTP transport
- Used by production projects: Goose (Block), Apollo GraphQL, Terminator (mediar-ai)

### 2A.2 `rmcp` Feature Flags

```toml
[dependencies]
rmcp = { version = "1.2", features = [
    "server",                # Server-side handler and transport
    "transport-io",          # stdio transport (stdin/stdout)
    "transport-sse-server",  # SSE + Streamable HTTP transport
    "macros",                # #[tool], #[prompt_router], derive macros
] }
```

### 2A.3 Server Implementation Pattern

```rust
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler, ServiceExt,
    handler::server::router::tool::ToolRouter,
    model::*,
    service::RequestContext,
    tool, tool_router,
};

#[derive(Clone)]
pub struct AXTerminatorServer {
    tool_router: ToolRouter<Self>,
    state: Arc<ServerState>,
}

#[tool_router]
impl AXTerminatorServer {
    fn new(state: Arc<ServerState>) -> Self {
        Self {
            tool_router: Self::tool_router(),
            state,
        }
    }

    #[tool(
        name = "ax_find",
        description = "Find UI elements by title, role, description, or combined query.",
        annotations(
            title = "Find UI element",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false,
        )
    )]
    async fn find(
        &self,
        #[tool(param, description = "Application name, bundle ID, or PID")]
        app: String,
        #[tool(param, description = "Element query (title, role, or description)")]
        query: String,
    ) -> Result<CallToolResult, McpError> {
        let conn = self.state.app_manager.get(&app)?;
        let element = crate::core::find_element(&conn.app, &query)?;
        Ok(CallToolResult::text(format!("{:?}", element)))
    }

    // ... remaining tools follow the same pattern
}

impl ServerHandler for AXTerminatorServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            name: "axterminator".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            instructions: Some(SERVER_INSTRUCTIONS.into()),
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_resources_subscribe()
                .enable_prompts()
                .enable_logging()
                .enable_completions()
                .build(),
            ..Default::default()
        }
    }
    // ... resource, prompt, completion handlers
}
```

### 2A.4 Transport Startup

```rust
pub async fn serve_stdio(server: AXTerminatorServer) -> anyhow::Result<()> {
    let transport = (tokio::io::stdin(), tokio::io::stdout());
    let service = server.serve(transport).await?;
    service.waiting().await?;
    Ok(())
}

pub async fn serve_http(server: AXTerminatorServer, port: u16) -> anyhow::Result<()> {
    // rmcp's transport-sse-server provides Streamable HTTP
    // Bind 127.0.0.1 by default, require auth for non-localhost
    let addr = format!("127.0.0.1:{}", port);
    // ... HTTP transport setup
    Ok(())
}
```

### 2A.5 Why Not the Alternatives

- **`rust-mcp-sdk`** (92K downloads): Good quality but third-party. Choosing the
  official SDK ensures protocol compatibility as MCP evolves.
- **`tower-mcp`** (5.8K): Interesting tower middleware approach but too young for
  production use. Only 26 releases, no major adopters.
- **`clap-mcp`** (187 downloads): Attempts to bridge clap and MCP automatically.
  Too early, alpha quality. We build our own bridge trivially since CLI and MCP
  both call the same core functions.
- **Raw JSON-RPC**: Implementing the protocol from scratch with `serde` + `tokio`
  would work but is unnecessary complexity. `rmcp` handles lifecycle, capability
  negotiation, progress, cancellation, and all edge cases.

---

## 2B. CLI Design -- `clap` v4

### 2B.1 Subcommand Structure

```
axterminator mcp serve [--stdio|--http <port>]        # MCP server
axterminator find <query> [--app <name>]               # Find element
axterminator click <query> [--app <name>]              # Click element
axterminator type <text> [--app <name>] [--element <query>]  # Type text
axterminator screenshot [--app <name>] [--output <path>]     # Screenshot
axterminator tree [--app <name>] [--depth <n>]         # Element hierarchy
axterminator apps                                       # List accessible apps
axterminator check                                      # Verify accessibility permissions
axterminator record [--app <name>] [--output <path>]   # Record interactions
axterminator completions <shell>                        # Shell completions
```

### 2B.2 CLI Derive Structs

```rust
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "axterminator", version, about = "macOS GUI automation for humans and AI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Output format: text, json, or quiet
    #[arg(long, global = true, default_value = "text")]
    format: OutputFormat,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the MCP server
    Mcp {
        #[command(subcommand)]
        action: McpAction,
    },
    /// Find a UI element
    Find {
        query: String,
        #[arg(long)]
        app: Option<String>,
    },
    /// Click a UI element
    Click {
        query: String,
        #[arg(long)]
        app: Option<String>,
    },
    /// Type text into a UI element
    Type {
        text: String,
        #[arg(long)]
        app: Option<String>,
        #[arg(long)]
        element: Option<String>,
    },
    /// Capture a screenshot
    Screenshot {
        #[arg(long)]
        app: Option<String>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Show the accessibility element tree
    Tree {
        #[arg(long)]
        app: Option<String>,
        #[arg(long, default_value = "5")]
        depth: u32,
    },
    /// List all accessible applications
    Apps,
    /// Verify accessibility permissions are granted
    Check,
    /// Record UI interactions for replay
    Record {
        #[arg(long)]
        app: Option<String>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Generate shell completions
    Completions {
        shell: clap_complete::Shell,
    },
}

#[derive(Subcommand)]
enum McpAction {
    /// Start the MCP server
    Serve {
        #[arg(long, group = "transport")]
        stdio: bool,
        #[arg(long, group = "transport")]
        http: Option<u16>,
    },
}
```

### 2B.3 Output Formats

Every subcommand supports `--format`:
- `text` (default): Human-readable for terminal
- `json`: Machine-readable for scripting and piping
- `quiet`: Exit code only (for CI/CD assertions)

### 2B.4 CLI Examples

```bash
# Check permissions
axterminator check

# List running apps
axterminator apps
axterminator apps --format json | jq '.[] | select(.accessible)'

# Find an element
axterminator find "Submit" --app Safari
axterminator find "URL bar" --app Safari --format json

# Click a button
axterminator click "Submit" --app Safari

# Type into a field
axterminator type "hello world" --app TextEdit
axterminator type "search query" --app Safari --element "URL bar"

# Screenshot
axterminator screenshot --app Safari --output safari.png

# Element tree
axterminator tree --app Finder --depth 3
axterminator tree --app Safari --format json | jq '.children[0]'

# Start MCP server (for AI agents)
axterminator mcp serve --stdio
axterminator mcp serve --http 8741

# Shell completions
axterminator completions zsh > ~/.zfunc/_axterminator
```

---

## 2C. Distribution

### 2C.1 Single Binary

The `axterminator` binary includes both CLI and MCP server. No separate install,
no Python runtime required for CLI or MCP use.

### 2C.2 Distribution Channels

| Channel | Command | What You Get |
|---------|---------|-------------|
| **Homebrew** | `brew install axterminator` | Single binary (CLI + MCP server) |
| **Cargo** | `cargo install axterminator` | Single binary (CLI + MCP server) |
| **PyPI** | `pip install axterminator` | Python bindings + bundled binary |

### 2C.3 MCP Client Configuration

After installation, configure your MCP client to use the binary directly:

```json
{
  "mcpServers": {
    "axterminator": {
      "command": "axterminator",
      "args": ["mcp", "serve", "--stdio"],
      "env": {
        "AXTERMINATOR_LOG_LEVEL": "info",
        "ANTHROPIC_API_KEY": "sk-ant-..."
      }
    }
  }
}
```

No `uv run`, no `python`, no virtual environments. Just the binary.

### 2C.4 Homebrew Tap

```ruby
class Axterminator < Formula
  desc "macOS GUI automation for humans and AI"
  homepage "https://github.com/MikkoParkkola/axterminator"
  url "https://github.com/MikkoParkkola/axterminator/releases/download/v0.5.0/axterminator-0.5.0-aarch64-apple-darwin.tar.gz"
  sha256 "..."
  license "MIT OR Apache-2.0"

  def install
    bin.install "axterminator"
  end

  test do
    system "#{bin}/axterminator", "check"
  end
end
```

---

## 2D. `server.py` Deprecation Plan

The existing `server.py` (772 lines, Python, using `mcp` SDK v1.26.0) will be
deprecated and removed once the Rust MCP server reaches feature parity.

### 2D.1 Migration Phases

| Phase | Action | server.py Status |
|-------|--------|-----------------|
| Phase 0 | Build CLI with clap (foundation) | Active (still primary) |
| Phase 1 | Port MCP tools to Rust via `rmcp` | Deprecated (prints warning) |
| Phase 2 | Add resources, prompts, completions | Deprecated |
| Phase 3 | Add elicitation, HTTP transport | Deprecated |
| Phase 4 | Remove server.py | Removed |

### 2D.2 Phase 0: CLI Foundation

Build the unified binary with all CLI subcommands calling the existing Rust core.
Add `axterminator mcp serve` as a stub.

### 2D.3 Phase 1: Rust MCP Server

1. Add `rmcp` dependency with `server`, `transport-io`, `macros` features
2. Implement `AXTerminatorServer` with `#[tool_router]`
3. Port all tools from `server.py` to Rust `#[tool]` macros
4. Wire `axterminator mcp serve --stdio` to the Rust MCP server
5. Add deprecation warning to `server.py` (prints to stderr on startup)

### 2D.4 Phase 4: Removal

1. Remove `server.py` from the repository
2. Remove Python MCP SDK dependency from `pyproject.toml`
3. Update `run-mcp.sh` to use the binary
4. Update all documentation and MCP client configs (no more `uv run`)
5. Publish to crates.io and Homebrew tap

### 2D.5 Why Deprecate (Not Maintain Both)

Maintaining two MCP servers (Python and Rust) doubles the testing surface, creates
version skew risk, and confuses users about which to use. The Python API via PyO3
remains the correct path for Python users; the MCP server should be a single Rust
binary.

---

## 3. Protocol Features -- Complete MCP Capability Map

### 3.1 Protocol Version

Target: **2025-11-25** (latest). We negotiate using the version the client supports,
with fallback to `2025-06-18` and `2025-03-26`. The server responds with the same
version if supported, otherwise the latest version we support.

### 3.2 Server Capabilities Declaration

```json
{
  "capabilities": {
    "tools": { "listChanged": true },
    "resources": { "subscribe": true, "listChanged": true },
    "prompts": { "listChanged": true },
    "logging": {},
    "completions": {},
    "tasks": {
      "list": {},
      "cancel": {},
      "requests": {
        "tools": { "call": {} }
      }
    }
  },
  "serverInfo": {
    "name": "axterminator",
    "title": "AXTerminator - macOS GUI Automation",
    "version": "0.5.0",
    "description": "Background-first macOS GUI automation with accessibility API, VLM fallback, and self-healing locators.",
    "websiteUrl": "https://github.com/MikkoParkkola/axterminator",
    "icons": [
      { "src": "https://axterminator.dev/icon.png", "mimeType": "image/png", "sizes": ["48x48"] }
    ]
  },
  "instructions": "AXTerminator: macOS GUI control for AI agents.\n\nWorkflow:\n1. Call ax_is_accessible to verify permissions\n2. Call ax_connect with an app name, bundle ID, or PID\n3. Use ax_find to locate elements, ax_click/ax_type/ax_set_value to interact\n4. Use ax_screenshot for visual context\n5. If ax_find fails, ax_find_visual uses AI vision as fallback\n6. Use ax_workflow for multi-step composed operations\n7. Use ax_assert for verification\n\nAll actions run in background mode by default (no focus stealing).\nSubscribe to axterminator://app/{name}/events for reactive UI state updates.\nRead axterminator://apps for running applications.\nRead axterminator://app/{name}/tree for the element hierarchy."
}
```

### 3.3 Key Protocol Decisions

| MCP Feature | Decision | Rationale |
|-------------|----------|-----------|
| Tasks | YES | VLM search, AppleScript, workflows are long-running |
| Resource subscriptions | YES | AX observer pushes UI changes reactively |
| Elicitation (form) | YES | Ambiguity, confirmation, clarification, permissions |
| Elicitation (URL) | YES | VLM OAuth, AppleScript security, HTTP auth |
| Sampling | YES | Screenshot interpretation, action planning |
| Sampling with tools | YES | Multi-turn autonomous workflows |
| Completions | YES | App names, element queries, prompt arguments |
| Structured output | YES | outputSchema on all tools with structured returns |
| Authorization (HTTP) | YES | OAuth 2.1 for remote HTTP transport |

---

## 4. Tools -- Complete Tool Set

### 4.1 Tool Design Philosophy

Every tool carries `annotations` with semantic hints that help the client make better
decisions about confirmation prompts, retry behavior, and caching. Every tool with
structured output declares `outputSchema`. Tools that may take >2 seconds support
task-augmented execution via `execution.taskSupport: "optional"`.

Tool names follow the `ax_` namespace convention for discoverability. Tools are grouped
by function: connect, find, action, observe, compose, assert, context, and scripting.

### 4.2 Connection Tools

#### ax_is_accessible

Check if macOS accessibility permissions are enabled.

| Annotation | Value |
|-----------|-------|
| `readOnlyHint` | `true` |
| `destructiveHint` | `false` |
| `idempotentHint` | `true` |
| `openWorldHint` | `false` |
| `title` | "Check accessibility permissions" |

```json
{
  "name": "ax_is_accessible",
  "title": "Check accessibility permissions",
  "description": "Check if macOS accessibility permissions are enabled for this process. Must return true before any other tool will work. If false, guide the user to System Settings > Privacy & Security > Accessibility.",
  "inputSchema": { "type": "object", "additionalProperties": false },
  "outputSchema": {
    "type": "object",
    "properties": {
      "enabled": { "type": "boolean" },
      "process_name": { "type": "string" },
      "suggestion": { "type": "string" }
    },
    "required": ["enabled"]
  },
  "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
}
```

#### ax_connect

Connect to a macOS application by name, bundle ID, or PID.

| Annotation | Value |
|-----------|-------|
| `readOnlyHint` | `false` |
| `destructiveHint` | `false` |
| `idempotentHint` | `true` |
| `openWorldHint` | `false` |
| `title` | "Connect to a macOS application" |

```json
{
  "name": "ax_connect",
  "inputSchema": {
    "type": "object",
    "properties": {
      "app": { "type": "string", "description": "App name, bundle ID (e.g., com.apple.Safari), or PID" },
      "alias": { "type": "string", "description": "Optional alias for referencing this app in subsequent calls" }
    },
    "required": ["app"]
  },
  "outputSchema": {
    "type": "object",
    "properties": {
      "connected": { "type": "boolean" },
      "app_name": { "type": "string" },
      "pid": { "type": "integer" },
      "bundle_id": { "type": "string" },
      "windows": { "type": "integer" },
      "alias": { "type": "string" }
    },
    "required": ["connected", "app_name"]
  }
}
```

**Elicitation**: If multiple apps match (e.g., "Chrome" matches Google Chrome and Chrome
Canary), uses form elicitation to ask the user to choose. Falls back to error with
instructions if client does not support elicitation.

#### ax_list_apps

List all running applications with accessibility info.

```json
{
  "name": "ax_list_apps",
  "title": "List running applications",
  "inputSchema": { "type": "object", "additionalProperties": false },
  "outputSchema": {
    "type": "object",
    "properties": {
      "apps": {
        "type": "array",
        "items": {
          "type": "object",
          "properties": {
            "name": { "type": "string" },
            "pid": { "type": "integer" },
            "bundle_id": { "type": "string" },
            "connected": { "type": "boolean" }
          }
        }
      }
    },
    "required": ["apps"]
  },
  "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
}
```

### 4.3 Find Tools

#### ax_find

Find UI elements by accessibility query. The query language supports role, title, value,
identifier, and compound expressions. Returns the first matching element.

```json
{
  "name": "ax_find",
  "title": "Find UI element",
  "inputSchema": {
    "type": "object",
    "properties": {
      "app": { "type": "string", "description": "Connected app name or alias" },
      "query": { "type": "string", "description": "Element query: title text, role:title, or compound expression" },
      "strategy": { "type": "string", "enum": ["auto", "exact", "contains", "regex", "role", "identifier", "xpath"], "description": "Search strategy (default: auto -- tries all 7 strategies)" }
    },
    "required": ["app", "query"]
  },
  "outputSchema": {
    "type": "object",
    "properties": {
      "found": { "type": "boolean" },
      "method": { "type": "string", "description": "Strategy that matched: accessibility or visual" },
      "strategy": { "type": "string", "description": "Specific strategy: title_exact, role_title, contains, etc." },
      "role": { "type": "string" },
      "title": { "type": "string" },
      "value": { "type": "string" },
      "position": { "type": "array", "items": { "type": "integer" }, "description": "[x, y]" },
      "size": { "type": "array", "items": { "type": "integer" }, "description": "[width, height]" },
      "enabled": { "type": "boolean" },
      "focused": { "type": "boolean" },
      "element_ref": { "type": "string", "description": "Persistent element reference for subsequent calls" }
    },
    "required": ["found"]
  },
  "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
}
```

**Enhancement**: Returns `ImageContent` alongside structured content when VLM fallback
is used (annotated screenshot with bounding box). The `element_ref` field enables
persistent references across calls without re-searching.

#### ax_find_visual

Find element using AI vision when accessibility fails.

```json
{
  "name": "ax_find_visual",
  "title": "Find element using AI vision",
  "inputSchema": {
    "type": "object",
    "properties": {
      "app": { "type": "string" },
      "description": { "type": "string", "description": "Natural language description of the element" },
      "screenshot": { "type": "string", "description": "Optional: base64 PNG to analyze instead of capturing" }
    },
    "required": ["app", "description"]
  },
  "outputSchema": {
    "type": "object",
    "properties": {
      "found": { "type": "boolean" },
      "position": { "type": "array", "items": { "type": "integer" } },
      "size": { "type": "array", "items": { "type": "integer" } },
      "confidence": { "type": "number" },
      "vlm_backend": { "type": "string" }
    },
    "required": ["found"]
  },
  "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true },
  "execution": { "taskSupport": "optional" }
}
```

Returns both `ImageContent` (annotated screenshot) and `structuredContent`. Supports
task-augmented execution because VLM inference can take 2-10 seconds.

#### ax_get_tree

Return the full or partial accessibility element tree for an app window.

```json
{
  "name": "ax_get_tree",
  "title": "Get element tree hierarchy",
  "inputSchema": {
    "type": "object",
    "properties": {
      "app": { "type": "string" },
      "root_query": { "type": "string", "description": "Optional: start from a specific element" },
      "max_depth": { "type": "integer", "description": "Maximum tree depth (default: 5)", "default": 5 },
      "include_invisible": { "type": "boolean", "default": false },
      "compact": { "type": "boolean", "description": "Return compact format (role:title only, no attributes)", "default": false }
    },
    "required": ["app"]
  },
  "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false },
  "execution": { "taskSupport": "optional" }
}
```

Task-augmented because large element trees (hundreds of elements) can take >1 second.

#### ax_get_attributes

Get all accessibility attributes for a specific element.

```json
{
  "name": "ax_get_attributes",
  "title": "Get all element attributes",
  "inputSchema": {
    "type": "object",
    "properties": {
      "app": { "type": "string" },
      "query": { "type": "string" }
    },
    "required": ["app", "query"]
  },
  "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
}
```

### 4.4 Action Tools

#### ax_click

Click a UI element by query. Supports single, double, and right-click. All clicks
default to background mode (no focus stealing).

```json
{
  "name": "ax_click",
  "title": "Click UI element",
  "inputSchema": {
    "type": "object",
    "properties": {
      "app": { "type": "string" },
      "query": { "type": "string" },
      "click_type": { "type": "string", "enum": ["single", "double", "right"], "default": "single" },
      "mode": { "type": "string", "enum": ["background", "focus"], "default": "background" }
    },
    "required": ["app", "query"]
  },
  "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": false, "openWorldHint": false }
}
```

**Elicitation**: When clicking an element whose title contains destructive keywords
("delete", "remove", "erase", "quit", "close", "format", "reset"), the server uses form
elicitation to confirm. Falls back to proceeding with a warning log if the client does
not support elicitation. Configurable via `AXTERMINATOR_CONFIRM_DESTRUCTIVE`.

#### ax_click_at

Click at absolute screen coordinates.

```json
{
  "name": "ax_click_at",
  "title": "Click at screen coordinates",
  "inputSchema": {
    "type": "object",
    "properties": {
      "x": { "type": "integer" },
      "y": { "type": "integer" },
      "click_type": { "type": "string", "enum": ["single", "double", "right"], "default": "single" }
    },
    "required": ["x", "y"]
  },
  "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": false, "openWorldHint": false }
}
```

#### ax_type

Type text into a focused element or specified element.

```json
{
  "name": "ax_type",
  "title": "Type text into element",
  "inputSchema": {
    "type": "object",
    "properties": {
      "app": { "type": "string" },
      "query": { "type": "string", "description": "Element to type into (optional if element is focused)" },
      "text": { "type": "string" },
      "clear_first": { "type": "boolean", "description": "Clear existing text before typing", "default": false },
      "mode": { "type": "string", "enum": ["background", "focus"], "default": "background" }
    },
    "required": ["app", "text"]
  },
  "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": false, "openWorldHint": false }
}
```

#### ax_set_value

Set the accessibility value of an element directly (for sliders, text fields, checkboxes).

```json
{
  "name": "ax_set_value",
  "title": "Set element value",
  "inputSchema": {
    "type": "object",
    "properties": {
      "app": { "type": "string" },
      "query": { "type": "string" },
      "value": { "type": "string" }
    },
    "required": ["app", "query", "value"]
  },
  "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": true, "openWorldHint": false }
}
```

#### ax_scroll

Scroll within an element or at coordinates.

```json
{
  "name": "ax_scroll",
  "title": "Scroll within element",
  "inputSchema": {
    "type": "object",
    "properties": {
      "app": { "type": "string" },
      "query": { "type": "string", "description": "Element to scroll within (optional)" },
      "direction": { "type": "string", "enum": ["up", "down", "left", "right"] },
      "amount": { "type": "integer", "description": "Scroll amount in pixels (default: 100)", "default": 100 }
    },
    "required": ["app", "direction"]
  },
  "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false }
}
```

#### ax_drag

Drag from one element/position to another.

```json
{
  "name": "ax_drag",
  "title": "Drag element or between positions",
  "inputSchema": {
    "type": "object",
    "properties": {
      "app": { "type": "string" },
      "from_query": { "type": "string", "description": "Source element query" },
      "to_query": { "type": "string", "description": "Target element query" },
      "from_x": { "type": "integer" }, "from_y": { "type": "integer" },
      "to_x": { "type": "integer" }, "to_y": { "type": "integer" },
      "duration_ms": { "type": "integer", "default": 500 }
    },
    "required": ["app"]
  },
  "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": false, "openWorldHint": false }
}
```

#### ax_key_press

Send keyboard shortcuts and special keys.

```json
{
  "name": "ax_key_press",
  "title": "Send keyboard shortcut",
  "inputSchema": {
    "type": "object",
    "properties": {
      "app": { "type": "string", "description": "Target app (optional for global shortcuts)" },
      "key": { "type": "string", "description": "Key to press (e.g., 'return', 'escape', 'a', 'F5')" },
      "modifiers": {
        "type": "array",
        "items": { "type": "string", "enum": ["cmd", "ctrl", "opt", "shift"] }
      }
    },
    "required": ["key"]
  },
  "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": false, "openWorldHint": false }
}
```

### 4.5 Observe Tools

#### ax_screenshot

Capture a screenshot of an application or the full screen. Returns native MCP
`ImageContent` so the agent can directly perceive the screenshot.

```json
{
  "name": "ax_screenshot",
  "title": "Capture screenshot",
  "inputSchema": {
    "type": "object",
    "properties": {
      "app": { "type": "string", "description": "App name (omit for full screen)" },
      "window_index": { "type": "integer", "description": "Specific window (default: frontmost)", "default": 0 },
      "region": {
        "type": "object",
        "properties": { "x": {"type":"integer"}, "y": {"type":"integer"}, "w": {"type":"integer"}, "h": {"type":"integer"} },
        "description": "Optional sub-region to capture"
      }
    }
  },
  "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
}
```

Returns `ImageContent(type="image", data=<base64 PNG>, mimeType="image/png")`.
The MCP protocol has native image support -- we never return base64 inside a text block.

#### ax_get_value

Read the current value of a UI element.

```json
{
  "name": "ax_get_value",
  "title": "Get element value",
  "inputSchema": {
    "type": "object",
    "properties": {
      "app": { "type": "string" },
      "query": { "type": "string" }
    },
    "required": ["app", "query"]
  },
  "outputSchema": {
    "type": "object",
    "properties": {
      "found": { "type": "boolean" },
      "value": { "type": "string" },
      "role": { "type": "string" },
      "title": { "type": "string" }
    },
    "required": ["found"]
  },
  "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
}
```

#### ax_list_windows

List windows for a connected application.

```json
{
  "name": "ax_list_windows",
  "title": "List application windows",
  "inputSchema": {
    "type": "object",
    "properties": {
      "app": { "type": "string" }
    },
    "required": ["app"]
  },
  "outputSchema": {
    "type": "object",
    "properties": {
      "windows": {
        "type": "array",
        "items": {
          "type": "object",
          "properties": {
            "index": { "type": "integer" },
            "title": { "type": "string" },
            "position": { "type": "array", "items": { "type": "integer" } },
            "size": { "type": "array", "items": { "type": "integer" } },
            "focused": { "type": "boolean" },
            "minimized": { "type": "boolean" }
          }
        }
      }
    },
    "required": ["windows"]
  },
  "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
}
```

#### ax_wait_idle

Wait for an application to become idle (no pending UI updates).

```json
{
  "name": "ax_wait_idle",
  "title": "Wait for application idle",
  "inputSchema": {
    "type": "object",
    "properties": {
      "app": { "type": "string" },
      "timeout_ms": { "type": "integer", "default": 5000 },
      "poll_ms": { "type": "integer", "default": 100 }
    },
    "required": ["app"]
  },
  "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false },
  "execution": { "taskSupport": "optional" }
}
```

### 4.6 Composition Tools (NEW)

#### ax_workflow

Execute a multi-step composed workflow with transaction semantics. Each step is an
atomic tool call. If any step fails, all preceding steps are rolled back (where
reversible). This is the key tool for complex multi-step operations.

```json
{
  "name": "ax_workflow",
  "title": "Execute composed workflow",
  "description": "Execute a multi-step workflow as a single atomic operation. Steps execute sequentially. If any step fails, preceding reversible steps are rolled back. Use for 'click Save, wait for dialog, click OK' patterns.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "steps": {
        "type": "array",
        "items": {
          "type": "object",
          "properties": {
            "tool": { "type": "string", "description": "Tool name (e.g., ax_click, ax_type, ax_wait_idle)" },
            "args": { "type": "object", "description": "Tool arguments" },
            "on_fail": { "type": "string", "enum": ["rollback", "skip", "abort"], "default": "abort" },
            "wait_after_ms": { "type": "integer", "description": "Wait after this step (ms)", "default": 0 },
            "retry": { "type": "integer", "description": "Retry count on failure", "default": 0 }
          },
          "required": ["tool", "args"]
        }
      },
      "name": { "type": "string", "description": "Optional workflow name for logging" }
    },
    "required": ["steps"]
  },
  "outputSchema": {
    "type": "object",
    "properties": {
      "success": { "type": "boolean" },
      "steps_completed": { "type": "integer" },
      "steps_total": { "type": "integer" },
      "results": { "type": "array", "items": { "type": "object" } },
      "rolled_back": { "type": "boolean" },
      "error": { "type": "string" }
    },
    "required": ["success", "steps_completed", "steps_total"]
  },
  "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": false, "openWorldHint": false },
  "execution": { "taskSupport": "optional" }
}
```

**Example: Save and confirm dialog**
```json
{
  "name": "ax_workflow",
  "arguments": {
    "name": "save-and-confirm",
    "steps": [
      { "tool": "ax_key_press", "args": { "app": "TextEdit", "key": "s", "modifiers": ["cmd"] } },
      { "tool": "ax_wait_idle", "args": { "app": "TextEdit", "timeout_ms": 2000 } },
      { "tool": "ax_find", "args": { "app": "TextEdit", "query": "Save" }, "retry": 3 },
      { "tool": "ax_click", "args": { "app": "TextEdit", "query": "Save" } }
    ]
  }
}
```

**Rollback semantics**: Actions are classified as reversible or irreversible:
- `ax_type` with `clear_first=false`: reverse by selecting and deleting typed text
- `ax_set_value`: reverse by restoring previous value (captured before action)
- `ax_click`: irreversible (side effects unknown)
- `ax_key_press`: irreversible (depends on context)
- `ax_scroll`: reverse by scrolling opposite direction

When `on_fail: "rollback"`, the engine walks the completed steps in reverse and applies
inverses where available. Steps without a known inverse are logged as "not rollbackable".

#### ax_record (resource-based)

Macro recording is exposed as a toggleable state, not a tool. Start recording via the
`axterminator://recording/start` resource, which activates the AX observer to capture
user actions as a sequence of tool calls. Stop via `axterminator://recording/stop`,
which returns the recorded sequence as an `ax_workflow` compatible step list.

See Section 6 (Resources) for details.

### 4.7 Assertion Tools (NEW)

These tools enable verification -- not just automation but proof that the UI is correct.

#### ax_assert

Assert that a UI element matches expected state. Returns pass/fail with details.

```json
{
  "name": "ax_assert",
  "title": "Assert element state",
  "description": "Verify that a UI element matches expected state. Use for test automation to confirm actions had the expected effect.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "app": { "type": "string" },
      "query": { "type": "string" },
      "assertions": {
        "type": "object",
        "description": "Expected state. Keys: exists, enabled, focused, value, title, role, visible, contains_text",
        "properties": {
          "exists": { "type": "boolean" },
          "enabled": { "type": "boolean" },
          "focused": { "type": "boolean" },
          "value": { "type": "string" },
          "title": { "type": "string" },
          "role": { "type": "string" },
          "visible": { "type": "boolean" },
          "contains_text": { "type": "string" }
        }
      }
    },
    "required": ["app", "query", "assertions"]
  },
  "outputSchema": {
    "type": "object",
    "properties": {
      "passed": { "type": "boolean" },
      "failures": {
        "type": "array",
        "items": {
          "type": "object",
          "properties": {
            "property": { "type": "string" },
            "expected": {},
            "actual": {}
          }
        }
      }
    },
    "required": ["passed"]
  },
  "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
}
```

#### ax_visual_diff

Compare a screenshot against a baseline for visual regression testing.

```json
{
  "name": "ax_visual_diff",
  "title": "Visual regression diff",
  "description": "Compare current app screenshot against a baseline image. Returns pixel diff percentage and highlighted diff image. Use for visual regression testing.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "app": { "type": "string" },
      "baseline": { "type": "string", "description": "Base64 PNG baseline image, or URI to a stored baseline" },
      "threshold": { "type": "number", "description": "Acceptable diff percentage (default: 1.0)", "default": 1.0 },
      "ignore_regions": {
        "type": "array",
        "items": {
          "type": "object",
          "properties": { "x": {"type":"integer"}, "y": {"type":"integer"}, "w": {"type":"integer"}, "h": {"type":"integer"} }
        },
        "description": "Regions to ignore in comparison (e.g., timestamps, ads)"
      }
    },
    "required": ["app", "baseline"]
  },
  "outputSchema": {
    "type": "object",
    "properties": {
      "passed": { "type": "boolean" },
      "diff_percentage": { "type": "number" },
      "diff_pixels": { "type": "integer" },
      "total_pixels": { "type": "integer" }
    },
    "required": ["passed", "diff_percentage"]
  },
  "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false },
  "execution": { "taskSupport": "optional" }
}
```

Returns both the structured result and an `ImageContent` showing the highlighted diff.

#### ax_a11y_audit

Audit an application for accessibility compliance issues.

```json
{
  "name": "ax_a11y_audit",
  "title": "Accessibility audit",
  "description": "Audit a connected application for accessibility issues. Checks for missing labels, roles, keyboard navigation support, contrast (via VLM), and WCAG 2.1 AA compliance.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "app": { "type": "string" },
      "scope": { "type": "string", "enum": ["full", "focused_window", "element"], "default": "focused_window" },
      "query": { "type": "string", "description": "Element to audit (when scope is 'element')" },
      "standards": {
        "type": "array",
        "items": { "type": "string", "enum": ["wcag_a", "wcag_aa", "wcag_aaa", "section508"] },
        "default": ["wcag_aa"]
      }
    },
    "required": ["app"]
  },
  "outputSchema": {
    "type": "object",
    "properties": {
      "passed": { "type": "boolean" },
      "score": { "type": "number", "description": "0-100 accessibility score" },
      "issues": {
        "type": "array",
        "items": {
          "type": "object",
          "properties": {
            "severity": { "type": "string", "enum": ["critical", "serious", "moderate", "minor"] },
            "rule": { "type": "string" },
            "element": { "type": "string" },
            "description": { "type": "string" },
            "suggestion": { "type": "string" }
          }
        }
      },
      "summary": { "type": "string" }
    },
    "required": ["passed", "issues"]
  },
  "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false },
  "execution": { "taskSupport": "optional" }
}
```

### 4.8 Context Tools (NEW)

#### ax_undo

Undo the last N actions performed by the server. Uses the internal undo stack that
records every mutating tool call with its pre-action state.

```json
{
  "name": "ax_undo",
  "title": "Undo last actions",
  "description": "Undo the last N actions performed by the server. Only works for reversible actions (set_value, type with clear). Irreversible actions (click, key_press) are skipped with a warning.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "count": { "type": "integer", "description": "Number of actions to undo (default: 1)", "default": 1 }
    }
  },
  "outputSchema": {
    "type": "object",
    "properties": {
      "undone": { "type": "integer" },
      "skipped": { "type": "integer" },
      "actions": {
        "type": "array",
        "items": {
          "type": "object",
          "properties": {
            "tool": { "type": "string" },
            "status": { "type": "string", "enum": ["undone", "skipped_irreversible"] }
          }
        }
      }
    },
    "required": ["undone", "skipped"]
  },
  "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false }
}
```

#### ax_clipboard

Read or write the system clipboard.

```json
{
  "name": "ax_clipboard",
  "title": "System clipboard",
  "description": "Read from or write to the macOS system clipboard. Useful for copy-paste workflows across applications.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "action": { "type": "string", "enum": ["read", "write"] },
      "text": { "type": "string", "description": "Text to write (when action is 'write')" }
    },
    "required": ["action"]
  },
  "outputSchema": {
    "type": "object",
    "properties": {
      "text": { "type": "string", "description": "Clipboard content (when reading)" },
      "success": { "type": "boolean" }
    },
    "required": ["success"]
  },
  "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false }
}
```

#### ax_session_info

Return server session state: connected apps, action history, undo stack depth,
subscription count.

```json
{
  "name": "ax_session_info",
  "title": "Session information",
  "inputSchema": { "type": "object", "additionalProperties": false },
  "outputSchema": {
    "type": "object",
    "properties": {
      "connected_apps": { "type": "array", "items": { "type": "string" } },
      "action_count": { "type": "integer" },
      "undo_stack_depth": { "type": "integer" },
      "subscriptions": { "type": "integer" },
      "uptime_seconds": { "type": "number" },
      "security_mode": { "type": "string", "enum": ["normal", "safe", "sandboxed"] }
    },
    "required": ["connected_apps", "action_count"]
  },
  "annotations": { "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
}
```

### 4.9 Scripting Tools

#### ax_run_script

Execute AppleScript or JXA. This is the highest-risk tool and requires explicit
security controls.

```json
{
  "name": "ax_run_script",
  "title": "Run AppleScript/JXA",
  "description": "Execute AppleScript or JXA (JavaScript for Automation). Use for operations the accessibility API cannot perform: menu bar access, system dialogs, app-specific scripting dictionaries. REQUIRES safe_mode=false.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "script": { "type": "string", "description": "AppleScript or JXA source code" },
      "language": { "type": "string", "enum": ["applescript", "jxa"], "default": "applescript" },
      "timeout_ms": { "type": "integer", "default": 10000 }
    },
    "required": ["script"]
  },
  "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": false, "openWorldHint": true },
  "execution": { "taskSupport": "optional" }
}
```

**Elicitation**: Before executing any script, the server uses form elicitation to show
the script and ask for confirmation. If the client does not support elicitation, the
server checks the security policy. In safe mode, this tool returns an error. In sandboxed
mode, this tool is not listed.

### 4.10 Tool Summary Table

| Tool | Read | Destruct | Idempotent | OpenWorld | Task | Category |
|------|:----:|:--------:|:----------:|:---------:|:----:|----------|
| ax_is_accessible | yes | no | yes | no | | connect |
| ax_connect | no | no | yes | no | | connect |
| ax_list_apps | yes | no | yes | no | | connect |
| ax_find | yes | no | yes | no | | find |
| ax_find_visual | yes | no | yes | yes | opt | find |
| ax_get_tree | yes | no | yes | no | opt | find |
| ax_get_attributes | yes | no | yes | no | | find |
| ax_click | no | yes | no | no | | action |
| ax_click_at | no | yes | no | no | | action |
| ax_type | no | yes | no | no | | action |
| ax_set_value | no | yes | yes | no | | action |
| ax_scroll | no | no | no | no | | action |
| ax_drag | no | yes | no | no | | action |
| ax_key_press | no | yes | no | no | | action |
| ax_screenshot | yes | no | yes | no | | observe |
| ax_get_value | yes | no | yes | no | | observe |
| ax_list_windows | yes | no | yes | no | | observe |
| ax_wait_idle | yes | no | yes | no | opt | observe |
| ax_workflow | no | yes | no | no | opt | compose |
| ax_assert | yes | no | yes | no | | assert |
| ax_visual_diff | yes | no | yes | no | opt | assert |
| ax_a11y_audit | yes | no | yes | no | opt | assert |
| ax_undo | no | no | no | no | | context |
| ax_clipboard | no | no | no | no | | context |
| ax_session_info | yes | no | yes | no | | context |
| ax_run_script | no | yes | no | yes | opt | script |

Total: **26 tools** across 7 categories.

---

## 5. Tasks -- Long-Running Operations

### 5.1 Why Tasks

Several axterminator operations can take seconds to minutes:
- VLM visual search (2-10s depending on backend)
- Large element tree scans (>1s for complex apps)
- AppleScript execution (up to 10s timeout)
- Multi-step workflows (variable)
- Accessibility audits (5-30s for full app)

Without tasks, the client blocks on these calls. With task-augmented execution, the
client gets an immediate `CreateTaskResult` and can poll or continue other work.

### 5.2 Task Capability Declaration

```json
{
  "capabilities": {
    "tasks": {
      "list": {},
      "cancel": {},
      "requests": {
        "tools": { "call": {} }
      }
    }
  }
}
```

### 5.3 Task-Augmented Tool Execution

Tools that declare `execution.taskSupport: "optional"` can be called with or without
task augmentation. The client decides based on whether it wants to block or poll.

**Example: VLM visual search as task**

```
--> {"jsonrpc":"2.0","id":1,"method":"tools/call",
     "params":{
       "name":"ax_find_visual",
       "arguments":{"app":"Safari","description":"the login button"},
       "task":{"ttl":30000}
     }}

<-- {"jsonrpc":"2.0","id":1,"result":{
      "task":{
        "taskId":"a1b2c3d4",
        "status":"working",
        "statusMessage":"Capturing screenshot for VLM analysis",
        "createdAt":"2026-03-19T10:30:00Z",
        "lastUpdatedAt":"2026-03-19T10:30:00Z",
        "ttl":30000,
        "pollInterval":1000
      }
    }}

<-- {"jsonrpc":"2.0","method":"notifications/tasks/status",
     "params":{
       "taskId":"a1b2c3d4",
       "status":"working",
       "statusMessage":"Running VLM inference (Anthropic Claude)"
     }}

--> {"jsonrpc":"2.0","id":2,"method":"tasks/result",
     "params":{"taskId":"a1b2c3d4"}}

<-- {"jsonrpc":"2.0","id":2,"result":{
      "content":[
        {"type":"image","data":"<base64>","mimeType":"image/png"},
        {"type":"text","text":"Found login button at (450, 320), confidence 0.94"}
      ],
      "structuredContent":{
        "found":true,"position":[450,320],"confidence":0.94,"vlm_backend":"anthropic"
      }
    }}
```

### 5.4 Model Immediate Response

For task-augmented calls, the server provides
`_meta["io.modelcontextprotocol/model-immediate-response"]` so the host can return
control to the model while the task executes:

```json
{
  "task": { "taskId": "a1b2c3d4", "status": "working" },
  "_meta": {
    "io.modelcontextprotocol/model-immediate-response": "VLM visual search started for 'login button' in Safari. The task is running in the background. You can continue with other operations or poll for results."
  }
}
```

### 5.5 Cancellation

All task-augmented operations support cancellation via `tasks/cancel`. The server uses
`tokio::select!` to race the operation against a cancellation channel. VLM HTTP requests
use the `reqwest` client's cancel-on-drop behavior.

---

## 6. Resources -- Live Application State

Resources provide read-only, URI-addressable views of application state. Unlike tools,
resources can be subscribed to for change notifications and are ideal for context that
the agent reads repeatedly.

### 6.1 Static Resources

#### `axterminator://system/status`

System-level accessibility status and server information.

```json
{
  "uri": "axterminator://system/status",
  "name": "system-status",
  "title": "System Accessibility Status",
  "description": "Accessibility permissions, connected apps, VLM backend status, server version, and security mode.",
  "mimeType": "application/json",
  "annotations": { "audience": ["user", "assistant"], "priority": 0.9 }
}
```

Returns:
```json
{
  "accessibility_enabled": true,
  "server_version": "0.5.0",
  "protocol_version": "2025-11-25",
  "vlm_backend": "anthropic",
  "vlm_available": true,
  "connected_apps": ["Safari", "Finder"],
  "platform": "macOS 15.2, Apple M4",
  "security_mode": "normal",
  "uptime_seconds": 142.5
}
```

#### `axterminator://apps`

List of all running applications with accessibility info. **Subscribable**: the server
emits `notifications/resources/updated` when applications launch or quit.

```json
{
  "uri": "axterminator://apps",
  "name": "running-apps",
  "title": "Running Applications",
  "description": "All running macOS applications with PIDs, bundle IDs, and accessibility status. Subscribe for launch/quit notifications.",
  "mimeType": "application/json",
  "annotations": { "audience": ["assistant"], "priority": 0.7 }
}
```

#### `axterminator://clipboard`

Current clipboard content. **Subscribable**: emits updates when clipboard changes.

```json
{
  "uri": "axterminator://clipboard",
  "name": "clipboard",
  "title": "System Clipboard",
  "description": "Current macOS clipboard content. Subscribe for change notifications.",
  "mimeType": "application/json",
  "annotations": { "audience": ["assistant"], "priority": 0.3 }
}
```

### 6.2 Resource Templates (Dynamic)

Resource templates use RFC 6570 URI templates to provide dynamic, parameterized
access to application state.

#### `axterminator://app/{name}/tree`

Live element tree for a connected application. **Subscribable**: emits updates when the
UI structure changes (new window, dialog appears, tab switches).

```json
{
  "uriTemplate": "axterminator://app/{name}/tree",
  "name": "app-element-tree",
  "title": "Application Element Tree",
  "description": "The accessibility element hierarchy for a connected app. Returns roles, titles, values, and positions for all visible elements.",
  "mimeType": "application/json"
}
```

#### `axterminator://app/{name}/screenshot`

Current screenshot of an application (binary resource, PNG).

```json
{
  "uriTemplate": "axterminator://app/{name}/screenshot",
  "name": "app-screenshot",
  "title": "Application Screenshot",
  "description": "Current screenshot of a connected app as a PNG image.",
  "mimeType": "image/png"
}
```

Returns `BlobResourceContents` with base64-encoded PNG.

#### `axterminator://app/{name}/state`

Structured UI state summary. **Subscribable**: emits updates when focus, window title,
or dialog state changes.

```json
{
  "uriTemplate": "axterminator://app/{name}/state",
  "name": "app-ui-state",
  "title": "Application UI State",
  "description": "Current UI state: window titles, focused element, menu bar items, toolbar state, detected UI pattern (login form, settings page, etc.).",
  "mimeType": "application/json"
}
```

Returns:
```json
{
  "app": "Safari",
  "pid": 1234,
  "windows": [
    { "title": "Apple", "focused": true, "position": [0, 25], "size": [1440, 875] }
  ],
  "focused_element": { "role": "AXTextField", "title": "Address and Search" },
  "detected_state": "idle",
  "detected_pattern": "browser_main",
  "menu_bar": ["Safari", "File", "Edit", "View", "History", "Bookmarks", "Window", "Help"]
}
```

The `detected_state` and `detected_pattern` fields come from the intelligence engine
(Section 10).

#### `axterminator://app/{name}/window/{index}/tree`

Element tree for a specific window (for multi-window apps).

```json
{
  "uriTemplate": "axterminator://app/{name}/window/{index}/tree",
  "name": "window-element-tree",
  "title": "Window Element Tree",
  "description": "Element hierarchy for a specific window by index.",
  "mimeType": "application/json"
}
```

#### `axterminator://app/{name}/events`

Event stream for an application. **Subscribable**: this is the primary reactive channel.
The server emits `notifications/resources/updated` whenever significant events occur.

```json
{
  "uriTemplate": "axterminator://app/{name}/events",
  "name": "app-events",
  "title": "Application Event Stream",
  "description": "UI events for a connected app. Subscribe to receive notifications when windows open/close, dialogs appear, focus changes, or notifications arrive.",
  "mimeType": "application/json"
}
```

Reading this resource returns the most recent events:
```json
{
  "events": [
    { "type": "window_opened", "title": "Save As", "timestamp": "2026-03-19T10:30:00Z" },
    { "type": "focus_changed", "element": "AXTextField 'File name'", "timestamp": "2026-03-19T10:30:01Z" },
    { "type": "dialog_appeared", "title": "Save As", "pattern": "file_save_dialog", "timestamp": "2026-03-19T10:30:00Z" }
  ]
}
```

### 6.3 Resource Subscriptions -- Reactive UI State

The server declares `resources.subscribe: true` in capabilities. The subscription
engine uses macOS accessibility observer (`AXObserver`) notifications for real-time
events, not polling.

#### Subscription Architecture

```
  macOS AX Observer                    MCP Server                    MCP Client
  ================                    ==========                    ==========
  kAXFocusedUIElementChangedNotification -->+
  kAXWindowCreatedNotification ----------->|
  kAXUIElementDestroyedNotification ------>|  SubscriptionEngine     notifications/
  kAXTitleChangedNotification ------------>|    |                    resources/updated
  kAXValueChangedNotification ------------>|    +-- match URI --------> Client
  kAXSelectedTextChangedNotification ----->|    +-- debounce 100ms      |
  kAXMenuOpenedNotification -------------->|    +-- hash compare        |
  kAXSheetCreatedNotification ------------>|                            |
  kAXDrawerCreatedNotification ----------->+                            |
                                                                       |
  NSWorkspace notifications:                                          |
  didLaunchApplicationNotification ------->+-- axterminator://apps ---> Client
  didTerminateApplicationNotification ---->+                           |
                                                                       |
  NSPasteboard change count polling (1s) ->+-- axterminator://clipboard -> Client
```

**Subscribable resources and their event sources**:

| Resource | Event Source | Debounce |
|----------|-------------|----------|
| `axterminator://apps` | NSWorkspace launch/terminate | 0ms |
| `axterminator://app/{name}/state` | AXObserver: focus, title, value changed | 100ms |
| `axterminator://app/{name}/tree` | AXObserver: created, destroyed, moved | 500ms |
| `axterminator://app/{name}/events` | All AXObserver events for this app | 0ms |
| `axterminator://clipboard` | NSPasteboard changeCount polling (1s) | 0ms |

**Implementation**: When a client sends `resources/subscribe`, the server registers an
AXObserver for the relevant app (if not already registered). The observer callback hashes
the new state and compares with the last emitted state. If different (and debounce window
has passed), it emits `notifications/resources/updated`. The client then calls
`resources/read` to get the new state.

**Unsubscribe**: When the client sends `resources/unsubscribe` (or the session ends),
the observer is removed if no other subscriptions reference the same app.

### 6.4 Completions

The server supports `completion/complete` for resource URI templates and prompt
arguments.

**Resource completion**: When the client types `axterminator://app/`, the server
suggests connected app names. When typing `axterminator://app/Safari/window/`, it
suggests window indices.

**Prompt completion**: For the `app_name` argument in any prompt, the server suggests
running app names. For `focus_area`, it suggests detected UI regions.

**Context-aware completion**: The server uses `context.arguments` to refine suggestions.
For example, if `app_name` is already "Safari", the `focus_area` completions include
Safari-specific areas like "toolbar", "sidebar", "tab bar".

---

## 7. Prompts -- Guided Workflows

Prompts provide pre-built conversation starters for common axterminator workflows.
Each prompt returns `PromptMessage` objects that guide the agent through a multi-step
process.

### 7.1 test-app

Guide an agent through testing a macOS application.

```json
{
  "name": "test-app",
  "title": "Test a macOS Application",
  "description": "Step-by-step guide to test a macOS application. Checks accessibility, connects, explores the UI, runs interactions, and reports findings with assertions.",
  "arguments": [
    { "name": "app_name", "description": "Name of the app to test", "required": true },
    { "name": "focus_area", "description": "Specific area to test (e.g., 'toolbar', 'sidebar')", "required": false },
    { "name": "include_a11y_audit", "description": "Include accessibility audit?", "required": false }
  ]
}
```

Returns messages instructing the agent to:
1. Verify accessibility with `ax_is_accessible`
2. Connect with `ax_connect`
3. Subscribe to `axterminator://app/{name}/events` for reactive monitoring
4. Read the element tree with `ax_get_tree`
5. Take a screenshot with `ax_screenshot`
6. Identify interactive elements and test them
7. Run `ax_assert` to verify expected states
8. Optionally run `ax_a11y_audit`
9. Report findings

### 7.2 navigate-to

Guide navigation to a specific screen or state.

```json
{
  "name": "navigate-to",
  "title": "Navigate to a Screen",
  "description": "Navigate to a specific screen, dialog, or state within a macOS application.",
  "arguments": [
    { "name": "app_name", "required": true },
    { "name": "destination", "description": "Where to navigate (e.g., 'Settings > General', 'File > New')", "required": true }
  ]
}
```

### 7.3 extract-data

Extract structured data from an application's UI.

```json
{
  "name": "extract-data",
  "title": "Extract Data from Application",
  "description": "Extract structured data from a running macOS application. Reads element values, table contents, or form fields.",
  "arguments": [
    { "name": "app_name", "required": true },
    { "name": "data_description", "description": "What data to extract", "required": true },
    { "name": "output_format", "description": "json, csv, or markdown", "required": false }
  ]
}
```

### 7.4 automate-workflow

Automate a multi-step workflow across one or more apps.

```json
{
  "name": "automate-workflow",
  "title": "Automate a Workflow",
  "description": "Automate a multi-step workflow in one or more macOS applications. Uses ax_workflow for composed operations with rollback support.",
  "arguments": [
    { "name": "workflow", "description": "Natural language description of the workflow", "required": true },
    { "name": "apps", "description": "Comma-separated app names involved", "required": false },
    { "name": "dry_run", "description": "Plan only, do not execute", "required": false }
  ]
}
```

### 7.5 debug-ui

Debug why a UI element cannot be found.

```json
{
  "name": "debug-ui",
  "title": "Debug UI Element",
  "description": "Debug why a UI element cannot be found. Explores the tree, takes screenshots, tries alternative locators, and suggests solutions.",
  "arguments": [
    { "name": "app_name", "required": true },
    { "name": "element_description", "required": true },
    { "name": "query_tried", "description": "Query that failed", "required": false }
  ]
}
```

### 7.6 accessibility-audit

Comprehensive accessibility compliance audit.

```json
{
  "name": "accessibility-audit",
  "title": "Accessibility Audit",
  "description": "Audit a macOS application for accessibility issues: missing labels, roles, keyboard navigation, contrast, and WCAG compliance. Uses ax_a11y_audit tool.",
  "arguments": [
    { "name": "app_name", "required": true },
    { "name": "standard", "description": "WCAG level: A, AA, or AAA", "required": false }
  ]
}
```

### 7.7 cross-app-copy (NEW)

Guide a cross-application data transfer.

```json
{
  "name": "cross-app-copy",
  "title": "Copy Data Between Apps",
  "description": "Copy data from one application to another. Handles clipboard coordination, app switching, and verification.",
  "arguments": [
    { "name": "source_app", "required": true },
    { "name": "source_element", "description": "What to copy", "required": true },
    { "name": "target_app", "required": true },
    { "name": "target_element", "description": "Where to paste", "required": true }
  ]
}
```

---

## 8. Logging

### 8.1 Structured MCP Logging

Every tool call emits MCP log notifications via `notifications/message`. This makes
server operations visible to the agent and to the client's log viewer.

**Log levels used** (RFC 5424 syslog severity):
- `debug`: Element cache hits, accessibility attribute reads, subscription checks
- `info`: Tool calls, connection events, search results, workflow steps
- `notice`: VLM fallback triggered, element found via non-primary strategy
- `warning`: Slow operations (>100ms), deprecated API usage, irreversible undo skip
- `error`: Element not found after all strategies, accessibility denied, task failed
- `critical`: Server crash, axterminator library unavailable
- `alert`: Security policy violation (blocked app, rate limit exceeded)

**Log format**:
```json
{
  "jsonrpc": "2.0",
  "method": "notifications/message",
  "params": {
    "level": "info",
    "logger": "axterminator.tools.find",
    "data": {
      "tool": "ax_find",
      "app": "Safari",
      "query": "URL bar",
      "strategy": "title_match",
      "duration_ms": 0.38,
      "found": true,
      "cache_hit": false
    }
  }
}
```

### 8.2 Performance Metrics in Logs

Every tool invocation is timed. The log data includes:
- `duration_ms`: Total time from call to response
- `ax_duration_ms`: Time spent in the Rust accessibility layer
- `vlm_duration_ms`: Time spent in VLM inference (when applicable)
- `strategy`: Which self-healing strategy succeeded
- `cache_hit`: Whether the element cache was used

### 8.3 Audit Log

Every mutating action is recorded in a persistent audit log (Section 13). The audit log
uses the same `notifications/message` channel at `notice` level with logger
`axterminator.audit`, ensuring the client can see all actions taken.

---

## 9. Progress Notifications

Long-running operations report progress via `notifications/progress`. The client can
display a progress bar or status message.

### 9.1 When Progress is Reported

| Operation | Progress Steps |
|-----------|---------------|
| VLM visual search | "Capturing screenshot" -> "Running VLM inference" -> "Parsing result" |
| Element tree scan | "Scanning window 1/3" -> "Processing 247 elements" -> "Building tree" |
| Wait idle | "Waiting... (0.5s / 5s)" -> "Waiting... (2.1s / 5s)" -> "App idle" |
| AppleScript | "Compiling script" -> "Executing" -> "Complete" |
| Workflow | "Step 1/4: ax_key_press" -> "Step 2/4: ax_wait_idle" -> ... |
| A11y audit | "Scanning elements (47/312)" -> "Checking labels" -> "Checking contrast" |
| Visual diff | "Capturing current" -> "Computing diff" -> "Generating overlay" |

### 9.2 Implementation

Progress is sent only when the request includes a `progressToken` in `_meta`. For
task-augmented requests, the same `progressToken` continues to be used throughout the
task's lifetime.

```rust
if let Some(token) = request.meta.progress_token {
    session.send_progress(token, 1, 3, "Capturing screenshot for VLM analysis").await;
}
```

---

## 10. Accessibility Intelligence (NEW)

Beyond raw UI access, the server provides semantic understanding of UI patterns. This
intelligence layer helps agents make better decisions without needing to interpret raw
accessibility trees.

### 10.1 UI Pattern Detection

The intelligence engine recognizes common UI patterns from the accessibility tree
structure:

| Pattern | Detection Heuristic |
|---------|-------------------|
| `login_form` | Contains password field + text field + button with "sign in"/"log in" |
| `file_save_dialog` | Sheet/dialog with "Save"/"Cancel" buttons + file browser |
| `file_open_dialog` | Sheet/dialog with "Open"/"Cancel" buttons + file browser |
| `settings_page` | Window/tab with groups of labeled controls (toggles, dropdowns) |
| `confirmation_dialog` | Alert with 2-3 buttons including "OK"/"Cancel" or "Yes"/"No" |
| `error_alert` | Alert with critical/warning icon + single dismiss button |
| `search_interface` | Text field with search role + results list/table |
| `table_view` | AXTable or AXOutline with columns and rows |
| `form` | Group of labeled text fields, dropdowns, checkboxes |
| `browser_main` | Address bar + tab bar + web content area |
| `text_editor` | Large text area with toolbar (font, formatting) |
| `progress_indicator` | Progress bar or spinner present and animating |

Patterns are detected on every `ax_get_tree` call and included in the
`axterminator://app/{name}/state` resource under `detected_pattern`.

### 10.2 Application State Detection

The server infers high-level application state:

| State | Detection |
|-------|-----------|
| `idle` | No progress indicators, no spinners, responsive to AX queries |
| `loading` | Progress bar visible, spinner animating, or "Loading" text present |
| `error` | Error alert visible, or status bar contains "Error"/"Failed" |
| `auth_required` | Login form pattern detected, or "Sign In" dialog present |
| `busy` | App not responding to AX queries within 500ms |
| `modal_dialog` | Sheet or modal window is frontmost |

State is included in `axterminator://app/{name}/state` under `detected_state`.

### 10.3 Next-Action Suggestions

When the agent reads an app's state resource, the intelligence engine includes
suggested next actions based on the detected pattern:

```json
{
  "detected_pattern": "file_save_dialog",
  "suggestions": [
    { "action": "Type a filename in the text field", "tool": "ax_type", "query": "Save As:" },
    { "action": "Click Save to confirm", "tool": "ax_click", "query": "Save" },
    { "action": "Click Cancel to dismiss", "tool": "ax_click", "query": "Cancel" }
  ]
}
```

This is purely informational -- the agent decides whether to follow suggestions. The
intelligence engine never performs actions autonomously.

---

## 11. Multi-App Orchestration (NEW)

### 11.1 Cross-App Operations

The server supports orchestrating actions across multiple connected applications.
This is built on top of the existing `ax_connect` (connect to multiple apps) and
`ax_workflow` (composed steps) tools.

**Copy from App A, paste into App B**:
```json
{
  "name": "ax_workflow",
  "arguments": {
    "name": "cross-app-copy",
    "steps": [
      { "tool": "ax_find", "args": { "app": "Safari", "query": "article text" } },
      { "tool": "ax_get_value", "args": { "app": "Safari", "query": "article text" } },
      { "tool": "ax_clipboard", "args": { "action": "write", "text": "{{step_1.value}}" } },
      { "tool": "ax_click", "args": { "app": "Notes", "query": "note body" } },
      { "tool": "ax_key_press", "args": { "app": "Notes", "key": "v", "modifiers": ["cmd"] } }
    ]
  }
}
```

Note: Step result interpolation (`{{step_N.field}}`) is supported in workflow arguments.
This enables data flow between steps without the agent needing to extract and re-inject
values.

### 11.2 App-to-App Triggers

The subscription engine enables reactive cross-app workflows. An agent can subscribe
to events on one app and take action in another:

1. Subscribe to `axterminator://app/Mail/events` for new mail notifications
2. When a `notification_appeared` event fires, read the notification content
3. Based on content, switch to the relevant app and take action

This is orchestrated by the agent, not the server. The server provides the reactive
primitives (subscriptions, events); the agent provides the logic.

### 11.3 Multi-App State View

The `axterminator://apps` resource includes connected apps' summary state, so the agent
can get a quick overview of all active applications without querying each one:

```json
{
  "apps": [
    { "name": "Safari", "pid": 1234, "connected": true, "state": "idle", "focused_window": "Apple" },
    { "name": "Notes", "pid": 5678, "connected": true, "state": "idle", "focused_window": "My Note" },
    { "name": "Mail", "pid": 9012, "connected": true, "state": "loading", "focused_window": "Inbox" }
  ]
}
```

---

## 12. Elicitation -- All 9 Scenarios

Elicitation lets the server ask the user for input when the tool cannot proceed
autonomously. The server supports both form mode and URL mode, with graceful fallback
when the client does not support elicitation.

### 12.1 Capability Check Pattern

```rust
// Always check before eliciting
if session.client_supports_elicitation_form() {
    let result = session.elicit_form(message, schema).await?;
    match result.action {
        "accept" => { /* use result.content */ },
        "decline" => { /* user said no, return error or default */ },
        "cancel" => { /* user cancelled, abort operation */ },
    }
} else {
    // Fallback: return error with instructions
    return Err(ToolError::new("Multiple apps match. Specify bundle ID to disambiguate."));
}
```

### 12.2 Form Mode Scenarios

#### Scenario 1: Ambiguous App Name

When `ax_connect("Chrome")` matches both Google Chrome and Chrome Canary:

```json
{
  "mode": "form",
  "message": "Multiple apps match 'Chrome'. Which one?",
  "requestedSchema": {
    "type": "object",
    "properties": {
      "app": {
        "type": "string",
        "title": "Select application",
        "oneOf": [
          { "const": "com.google.Chrome", "title": "Google Chrome (PID 1234)" },
          { "const": "com.google.Chrome.canary", "title": "Chrome Canary (PID 5678)" }
        ]
      }
    },
    "required": ["app"]
  }
}
```

#### Scenario 2: Element Not Found -- Clarification

When both accessibility and VLM fail to find an element:

```json
{
  "mode": "form",
  "message": "Could not find 'Submit' in Safari. Can you describe it differently?",
  "requestedSchema": {
    "type": "object",
    "properties": {
      "description": { "type": "string", "title": "Alternative description" },
      "use_visual": { "type": "boolean", "title": "Try AI vision search?", "default": true }
    },
    "required": ["description"]
  }
}
```

#### Scenario 3: Destructive Action Confirmation

When clicking an element with destructive keywords:

```json
{
  "mode": "form",
  "message": "This will click 'Delete All Data' in Settings. This action may be irreversible.",
  "requestedSchema": {
    "type": "object",
    "properties": {
      "confirm": { "type": "boolean", "title": "Confirm destructive action", "default": false }
    },
    "required": ["confirm"]
  }
}
```

#### Scenario 4: Permission Request

When `ax_is_accessible` returns false:

```json
{
  "mode": "form",
  "message": "Accessibility permissions are not enabled. Would you like instructions?",
  "requestedSchema": {
    "type": "object",
    "properties": {
      "action": {
        "type": "string",
        "title": "Action",
        "oneOf": [
          { "const": "open_settings", "title": "Open System Settings for me (requires focus)" },
          { "const": "show_instructions", "title": "Show manual instructions" },
          { "const": "cancel", "title": "Cancel" }
        ]
      }
    },
    "required": ["action"]
  }
}
```

#### Scenario 5: AppleScript Review

Before executing any AppleScript (highest-risk tool):

```json
{
  "mode": "form",
  "message": "Review this AppleScript before execution:\n\ntell application \"Finder\"\n  delete (every item of trash)\nend tell",
  "requestedSchema": {
    "type": "object",
    "properties": {
      "approve": { "type": "boolean", "title": "Execute this script?", "default": false },
      "timeout_seconds": { "type": "integer", "title": "Timeout (seconds)", "default": 10, "minimum": 1, "maximum": 60 }
    },
    "required": ["approve"]
  }
}
```

#### Scenario 6: Admin/Sudo Escalation

When an action requires elevated privileges (e.g., modifying system settings):

```json
{
  "mode": "form",
  "message": "This operation requires administrator privileges to modify system settings.",
  "requestedSchema": {
    "type": "object",
    "properties": {
      "proceed": { "type": "boolean", "title": "Proceed with admin prompt?", "default": false }
    },
    "required": ["proceed"]
  }
}
```

### 12.3 URL Mode Scenarios

#### Scenario 7: VLM API Key Setup

When the VLM backend requires an API key that is not set:

```json
{
  "mode": "url",
  "message": "Anthropic API key required for VLM visual search. Please add your key.",
  "url": "https://console.anthropic.com/settings/keys",
  "elicitationId": "vlm-auth-550e8400-e29b-41d4-a716-446655440000"
}
```

#### Scenario 8: OAuth Authorization

When the HTTP transport requires OAuth authorization and the user needs to consent:

```json
{
  "mode": "url",
  "message": "Please authorize this MCP client to access the axterminator server.",
  "url": "https://axterminator.example.com/oauth/authorize?client_id=...",
  "elicitationId": "oauth-auth-660f9500-f3ac-52e5-b827-557766551111"
}
```

#### Scenario 9: HTTP Basic Auth

When an application being automated requires HTTP authentication (e.g., a local
development server):

```json
{
  "mode": "url",
  "message": "The app requires authentication. Please log in via the browser.",
  "url": "http://localhost:3000/login",
  "elicitationId": "http-auth-770a0600-g4bd-63f6-c938-668877662222"
}
```

---

## 13. Security Model (NEW -- Comprehensive)

### 13.1 Threat Model

The MCP server runs on the user's machine with the user's accessibility permissions.
It can see and interact with ANY application the user has open. This is both the core
value proposition and the primary security concern.

**Threat categories**:
1. **Runaway agent**: Agent performs unintended destructive actions (delete files, send emails)
2. **Data exfiltration**: Agent reads sensitive data (passwords, financial info) and sends to external services via VLM or sampling
3. **Privilege escalation**: Agent uses AppleScript to gain elevated access
4. **Denial of service**: Agent hammers the server with requests, degrading system performance
5. **Prompt injection**: Malicious UI content tricks the agent into harmful actions
6. **Session hijacking**: Unauthorized access to HTTP transport sessions

### 13.2 Security Modes

The server operates in one of three security modes, configured via `AXTERMINATOR_SECURITY_MODE`:

| Mode | Description |
|------|-------------|
| `normal` | All tools available, destructive actions logged, elicitation for high-risk |
| `safe` | `ax_run_script` blocked, destructive actions require elicitation confirmation, VLM calls logged |
| `sandboxed` | Read-only: only observe/find/screenshot/tree tools available. No actions, no scripts, no clipboard write. Agent can see but not touch. |

### 13.3 App Allowlist / Denylist

```toml
# ~/.config/axterminator/security.toml
[apps]
allowed = ["Calculator", "com.apple.Safari", "com.microsoft.VSCode"]
denied = ["com.apple.Keychain-Access", "System Settings", "1Password"]

# If allowed is non-empty, ONLY these apps can be connected.
# If denied is non-empty, these apps are always blocked.
# denied takes precedence over allowed.
```

When `ax_connect` targets a blocked app:
- Log at `alert` level
- Return error: "App 'Keychain Access' is blocked by security policy"
- Increment rate-limit counter for the session

### 13.4 Action Audit Log

Every mutating action is recorded with:
- Timestamp
- Tool name and arguments
- Target app and element
- Result (success/failure)
- Session ID (for HTTP transport)

```json
{
  "timestamp": "2026-03-19T10:30:00.000Z",
  "session_id": "abc123",
  "tool": "ax_click",
  "args": { "app": "Safari", "query": "Delete Browsing Data" },
  "target": { "role": "AXButton", "title": "Delete Browsing Data", "position": [400, 300] },
  "result": "elicitation_declined",
  "security_flags": ["destructive_keyword"]
}
```

The audit log is written to `~/.local/share/axterminator/audit.jsonl` and emitted as
MCP log messages at `notice` level on logger `axterminator.audit`.

### 13.5 Rate Limiting

To prevent runaway agents:

| Limit | Default | Configurable |
|-------|---------|-------------|
| Tool calls per second | 50 | `AXTERMINATOR_RATE_LIMIT_RPS` |
| Tool calls per minute | 1000 | `AXTERMINATOR_RATE_LIMIT_RPM` |
| Concurrent tool calls | 10 | `AXTERMINATOR_CONCURRENCY` |
| VLM calls per minute | 10 | `AXTERMINATOR_VLM_RATE_LIMIT` |
| AppleScript calls per minute | 5 | `AXTERMINATOR_SCRIPT_RATE_LIMIT` |

When a limit is exceeded:
- Return JSON-RPC error `-32000` with message "Rate limit exceeded"
- Log at `alert` level
- Include `Retry-After` hint in error data

### 13.6 Sandboxed Mode (Read-Only)

In sandboxed mode, the server only lists these tools:
- `ax_is_accessible`, `ax_connect`, `ax_list_apps` (connection)
- `ax_find`, `ax_find_visual`, `ax_get_tree`, `ax_get_attributes` (find)
- `ax_screenshot`, `ax_get_value`, `ax_list_windows` (observe)
- `ax_assert`, `ax_a11y_audit` (assert -- read-only)
- `ax_session_info` (context)

All action tools, compose tools, clipboard write, and scripting tools are excluded from
the `tools/list` response. The `tools.listChanged` notification fires when security
mode changes.

### 13.7 Credential Protection

- VLM API keys are read from environment variables only (never stored by the server)
- The server never reads password field values (AX returns `****` for `AXSecureTextField`)
- Screenshot content is not logged (only metadata)
- The audit log does not record text content typed via `ax_type` (only the target element)
- Streamable HTTP transport requires bearer token or OAuth for non-localhost

### 13.8 Authorization for HTTP Transport

When running with `--http` on a non-localhost interface, the server implements the MCP
authorization spec:

1. **Protected Resource Metadata** at `/.well-known/oauth-protected-resource`
2. **Bearer token validation** for all requests
3. **Session management** with cryptographically secure session IDs
4. **Origin header validation** to prevent DNS rebinding
5. **Bind to 127.0.0.1 by default**

For simple deployments, bearer token auth (`AXTERMINATOR_HTTP_TOKEN`) is sufficient.
For production/fleet deployments, full OAuth 2.1 flow is supported.

### 13.9 What We Do Not Do

- We do not proxy credentials between apps
- We do not capture passwords from password fields (AX returns `****`)
- We do not modify system settings (unless via AppleScript with explicit confirmation)
- We do not install anything
- We do not send data to external servers (VLM calls are optional and explicit)
- We do not execute arbitrary code (AppleScript is the closest, and it requires confirmation)

---

## 14. Sampling

Sampling allows the server to request LLM inference from the client. This enables the
server to reason about visual UI state beyond what accessibility attributes provide.

### 14.1 Screenshot Interpretation

When the server needs to understand what is visible on screen:

```rust
let result = session.create_message(
    messages: vec![SamplingMessage {
        role: "user",
        content: vec![
            ImageContent { data: screenshot_b64, mime_type: "image/png" },
            TextContent { text: "Describe what you see in this macOS application window. List all visible UI elements." },
        ],
    }],
    max_tokens: 1000,
    system_prompt: "You are analyzing a macOS application screenshot for UI automation.",
    model_preferences: ModelPreferences {
        intelligence_priority: 0.8,
        cost_priority: 0.3,
        speed_priority: 0.5,
    },
).await?;
```

### 14.2 Next Action Planning

After performing an action, the server can ask the LLM what to do next:

```rust
let result = session.create_message(
    messages: vec![SamplingMessage {
        role: "user",
        content: vec![TextContent {
            text: format!(
                "Current UI state of {app}:\n{tree}\n\nGoal: {goal}\nActions so far: {history}\n\nWhat should be the next action?"
            ),
        }],
    }],
    max_tokens: 500,
    system_prompt: "You are a macOS UI automation planner. Suggest the single next action.",
).await?;
```

### 14.3 Sampling with Tools

The server can provide tools to the LLM during sampling, enabling multi-turn autonomous
workflows. This is powerful for complex tasks where the server orchestrates the LLM:

```rust
let result = session.create_message(
    messages: vec![...],
    max_tokens: 2000,
    tools: vec![
        Tool { name: "describe_element", input_schema: ... },
        Tool { name: "suggest_locator", input_schema: ... },
    ],
    tool_choice: ToolChoice { mode: "auto" },
).await?;
```

### 14.4 Capability Check

```rust
if session.client_supports_sampling() {
    let result = session.create_message(...).await?;
} else {
    // No sampling: return screenshot + text for the agent to reason about
    return Ok(vec![ImageContent { ... }, TextContent { text: "Analyze this screenshot." }]);
}
```

---

## 15. Transport

### 15.1 stdio (Primary)

The primary transport for local use. Claude Code, Claude Desktop, and most MCP
clients use stdio. The `axterminator mcp serve` command defaults to stdio.

**Implementation**: JSON-RPC messages on stdin/stdout, newline-delimited. Logging on
stderr. Standard MCP stdio transport.

### 15.2 Streamable HTTP (New)

Enables remote control of macOS applications. A headless Mac in a rack room or CI
farm can serve its GUI state over HTTP.

```
axterminator mcp serve --http                    # localhost:8741
axterminator mcp serve --http --port 9000        # custom port
axterminator mcp serve --http --bind 0.0.0.0     # all interfaces (requires auth)
```

**Features**:
- Stateful sessions (connected apps persist across requests)
- SSE streaming for progress notifications and resource subscription updates
- Session management with `MCP-Session-Id` header
- Resumability with SSE event IDs and `Last-Event-ID`
- `MCP-Protocol-Version` header on all requests
- Origin header validation for DNS rebinding protection

**Use cases**:
- CI/CD: Automated UI tests triggered by GitHub Actions, results streamed back
- Remote pair: Agent on one machine controls an app on another
- Fleet management: One agent orchestrates multiple Macs

**Security**: Bind to `127.0.0.1` by default. For non-localhost, requires bearer token
(`AXTERMINATOR_HTTP_TOKEN`) or full OAuth 2.1 authorization.

### 15.3 SSE (Legacy)

For backward compatibility with older clients that only support the deprecated
HTTP+SSE transport from MCP 2024-11-05. The server detects which transport the client
expects and adapts.

### 15.4 Transport Selection

| Command | Transport |
|---------|-----------|
| `axterminator mcp serve` | stdio (default) |
| `axterminator mcp serve --http` | Streamable HTTP on :8741 |
| `axterminator mcp serve --http --port 9000` | Streamable HTTP on :9000 |
| `axterminator mcp serve --sse` | SSE (legacy) |

---

## 16. Roots

The server can request `roots/list` from the client to discover relevant directories.
For axterminator, this is used to:

- Find test scripts the user might want to run
- Locate `.axterminator/` config directories
- Access macro recordings
- Find baseline images for visual regression

```rust
if session.client_supports_roots() {
    let roots = session.list_roots().await?;
    for root in roots {
        // Check for .axterminator/ config, test scripts, macros, baselines
    }
}
```

---

## 17. Error Handling

### 17.1 Error Categories

Errors follow the MCP distinction between protocol errors and tool execution errors.

**Protocol errors** (standard JSON-RPC):
- `-32600`: Invalid request
- `-32601`: Method not found
- `-32602`: Invalid params
- `-32603`: Internal error
- `-32002`: Resource not found

**Tool execution errors** (returned in tool result with `isError: true`):

```rust
enum ToolError {
    AppNotConnected { name: String, connected: Vec<String> },
    ElementNotFound { query: String, strategies_tried: Vec<String> },
    AccessibilityDenied,
    SecurityBlocked { app: String, reason: String },
    RateLimitExceeded { retry_after_ms: u64 },
    VlmUnavailable { backend: String },
    ScriptTimeout { timeout_ms: u64 },
    WorkflowFailed { step: usize, error: String, rolled_back: bool },
}
```

### 17.2 Actionable Error Messages

Every error includes a suggestion for the agent:

```json
{
  "isError": true,
  "content": [{
    "type": "text",
    "text": "Error: Element not found: 'Submit'\n\nStrategies tried: title_exact, title_contains, role_title, identifier, xpath, semantic, regex\n\nSuggestion: Try ax_get_tree to see available elements, or ax_find_visual for vision-based search."
  }]
}
```

---

## 18. Performance Considerations

1. **Element tree caching**: Cache `ax_get_tree` results for 500ms (configurable via
   `AXTERMINATOR_TREE_CACHE_MS`). Invalidate on any action tool call. The Rust LRU
   cache handles element-level caching; the MCP layer caches the serialized tree.

2. **Screenshot debouncing**: If `ax_screenshot` is called multiple times within 100ms
   for the same app, return the cached result.

3. **Lazy VLM loading**: VLM backend is not initialized at startup. Loaded on first
   `ax_find_visual` call to avoid startup latency.

4. **Async tool execution**: All tool handlers are async. Long operations (VLM,
   AppleScript) run on dedicated tokio tasks to avoid blocking other tool calls.

5. **Resource template resolution**: Template matching uses a pre-compiled regex table,
   not dynamic regex per request.

6. **Subscription debouncing**: AX observer events fire at high frequency. The
   subscription engine debounces per-resource with configurable intervals (100ms-500ms).

7. **Tree pagination**: Element trees exceeding 100KB are paginated. The first response
   includes the top N levels with a `nextCursor` for deeper traversal.

8. **Connection pooling**: For Streamable HTTP transport, the server uses hyper's
   connection keep-alive to avoid TCP/TLS overhead on repeated requests.

---

## 19. Comparison with Competitors

### 19.1 vs mediar-ai/terminator (Windows)

| Aspect | axterminator (macOS) | terminator (Windows) |
|--------|---------------------|---------------------|
| Platform | macOS only | Windows only |
| Core | Rust AXUIElement FFI | Rust + UI Automation |
| Element access | ~379us | ~10-50ms (estimated) |
| Background mode | Yes (unique) | No |
| MCP server | Full (tools, resources, prompts, sampling, tasks, elicitation) | Tools only |
| VLM fallback | Anthropic/OpenAI/MLX/Gemini/Ollama | OpenAI |
| Self-healing | 7 strategies | Basic retry |
| Transport | stdio + Streamable HTTP | stdio |
| Cross-app | Yes (orchestrated workflows) | Limited |
| Electron CDP | Yes | No |
| Composition | ax_workflow with rollback | None |
| Assertions | ax_assert, ax_visual_diff, ax_a11y_audit | None |
| Security | 3 modes + audit + rate limiting | Basic |
| Resource subscriptions | AX observer (reactive) | None |
| Tasks | Long-running VLM/script/audit | None |

### 19.2 vs Appium Mac2 Driver

| Aspect | axterminator | Appium Mac2 |
|--------|-------------|-------------|
| Architecture | Direct FFI, pure Rust | HTTP + XCTest bridge |
| Element access | ~379us | ~500ms |
| Setup | `cargo install axterminator` | Xcode + Appium server + driver |
| MCP | Native, full spec | None |
| Background | Yes | No |
| Electron | CDP bridge | Via WebDriver |
| Language | Rust core, Python/CLI/MCP | Any WebDriver client |
| A11y audit | Built-in | None |

### 19.3 vs NVIDIA OpenShell

| Aspect | axterminator | OpenShell |
|--------|-------------|-----------|
| Focus | GUI automation | System shell + GUI |
| Platform | macOS | Windows/Linux |
| MCP | Full (tools + resources + prompts + sampling + tasks) | Tools only |
| GUI approach | Accessibility API + VLM | Mixed (a11y + vision) |
| Composition | ax_workflow + rollback | None |
| Security | 3 modes + audit + rate limiting | Basic |

### 19.4 Competitive Moat

1. **Only** background GUI testing tool for macOS
2. **Only** MCP server with full spec coverage (resources, prompts, sampling, tasks, elicitation) for GUI
3. **Fastest** element access (379us vs 10-500ms for competitors)
4. **Most strategies** for element location (7 + VLM)
5. **Broadest** VLM support (5 backends)
6. **Only** MCP server with reactive UI subscriptions via AX observer
7. **Only** MCP server with transaction-based workflow composition and rollback
8. **Only** MCP server with built-in accessibility auditing
9. **Only** MCP server with 3-tier security model (normal/safe/sandboxed)

---

## 20. Implementation Plan

See also: **Section 2D** for the `server.py` deprecation timeline.

### Phase 0: CLI Foundation (Week 0)

**Goal**: Unified binary with clap subcommands, calling existing Rust core.

1. Add `clap` v4 + `clap_complete` dependencies
2. Implement `main.rs` with `Commands` enum and dispatch
3. Implement `axterminator check`, `axterminator apps` (simplest subcommands)
4. Implement `axterminator find`, `axterminator click`, `axterminator type`
5. Implement `axterminator screenshot`, `axterminator tree`
6. Implement `axterminator record` (interaction recorder)
7. Implement `axterminator completions` (shell completions)
8. Add output formatting layer (text, json, quiet)
9. Add `axterminator mcp serve` subcommand (stub)

**Deliverables**: Working CLI binary with all subcommands. MCP server is a stub.

### Phase 1: Core Tools + Logging + Rust MCP (Week 1-2)

**Goal**: Pure Rust MCP server via `rmcp` with stdio transport, all core tools, structured logging.

1. Add `rmcp = { version = "1.2", features = ["server", "transport-io", "macros"] }` dependency
2. Implement `AXTerminatorServer` with `#[tool_router]` (see Section 2A.3)
3. Implement `tools/list` and `tools/call` with all 19 core tools (connect, find, action, observe)
4. Add `ToolAnnotations` to all tools
5. Add `outputSchema` to tools with structured returns
6. Implement structured MCP logging (`notifications/message`)
7. Implement `ax_screenshot` returning native `ImageContent`
8. Implement `AppConnectionManager` with RwLock
9. Implement progress notifications for VLM and tree scan
10. Add `ServerCapabilities` with tools and logging
11. Wire up `axterminator mcp serve` CLI subcommand (clap)

12. Add deprecation warning to `server.py` (prints to stderr on startup)

**Deliverables**: Working `rmcp`-based MCP server on stdio with 19 tools, logging,
progress. `server.py` deprecated but still functional.

### Phase 2: Resources + Prompts + Subscriptions (Week 3)

**Goal**: Full context provider with reactive subscriptions.

1. Implement `resources/list`, `resources/read`, `resources/templates/list`
2. Implement all 7 resources/templates
3. Implement `resources/subscribe` with AX observer backend
4. Implement subscription debouncing and hash comparison
5. Implement all 7 prompt templates
6. Implement `completion/complete` for resource URIs and prompt arguments
7. Add clipboard monitoring resource
8. Declare full capabilities (resources, prompts, completions)

**Deliverables**: 7 resources, 7 prompts, reactive subscriptions, completions.

### Phase 3: Elicitation + Composition + Intelligence (Week 4)

**Goal**: Interactive clarification, composed workflows, semantic understanding.

1. Implement form elicitation for all 6 form scenarios
2. Implement URL elicitation for all 3 URL scenarios
3. Implement `ax_workflow` with step execution and rollback
4. Implement undo stack with action recording
5. Implement `ax_clipboard` tool
6. Implement UI pattern detection engine (12 patterns)
7. Implement app state detection (6 states)
8. Implement next-action suggestions
9. Add intelligence data to `app/{name}/state` resource

**Deliverables**: 9 elicitation scenarios, workflow composition, intelligence engine.

### Phase 4: Tasks + Sampling + Assertions + HTTP (Week 5-6)

**Goal**: Long-running tasks, autonomous reasoning, testing tools, remote access.

1. Implement task-augmented execution for 7 tools
2. Implement `tasks/list`, `tasks/get`, `tasks/result`, `tasks/cancel`
3. Implement task status notifications
4. Implement sampling for screenshot interpretation
5. Implement sampling for next-action planning
6. Implement `ax_assert` tool
7. Implement `ax_visual_diff` tool
8. Implement `ax_a11y_audit` tool
9. Implement Streamable HTTP transport with sessions
10. Implement bearer token auth for HTTP
11. Implement Origin header validation
12. Implement SSE legacy transport

**Deliverables**: Task system, sampling, 3 assertion tools, HTTP transport.

### Phase 5: Security + Polish + Python API (Week 7-8)

**Goal**: Production security, complete feature set, all three interfaces.

1. Implement 3 security modes (normal/safe/sandboxed)
2. Implement app allowlist/denylist
3. Implement audit log (jsonl + MCP notifications)
4. Implement rate limiting
5. Implement `ax_session_info` tool
6. Implement macro recording via AX observer
7. Implement OAuth 2.1 authorization for HTTP transport
8. Wire up Python API via pyo3 (existing, extend with MCP-aligned methods)
9. Integration tests for all MCP features
10. Performance benchmarks (MCP overhead, subscription latency)
11. Update documentation, examples, MCP client configs

12. Remove `server.py` from the repository
13. Remove Python MCP SDK dependency from `pyproject.toml`
14. Update `run-mcp.sh` to use the binary instead of Python
15. Set up Homebrew tap with GitHub Actions release automation
16. Publish to crates.io

**Deliverables**: Full security model, audit, rate limiting, OAuth, Python API, tests.
`server.py` removed. Homebrew tap and crates.io published.

---

## 21. Configuration

### 21.1 Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `AXTERMINATOR_LOG_LEVEL` | `info` | MCP log level |
| `AXTERMINATOR_VLM_BACKEND` | auto-detect | VLM backend: anthropic, openai, mlx, gemini, ollama |
| `AXTERMINATOR_SECURITY_MODE` | `normal` | Security mode: normal, safe, sandboxed |
| `AXTERMINATOR_CONFIRM_DESTRUCTIVE` | `true` | Elicit confirmation for destructive actions |
| `AXTERMINATOR_ALLOWED_APPS` | (none) | Comma-separated allowlist |
| `AXTERMINATOR_DENIED_APPS` | (none) | Comma-separated denylist |
| `AXTERMINATOR_HTTP_PORT` | `8741` | Streamable HTTP port |
| `AXTERMINATOR_HTTP_BIND` | `127.0.0.1` | HTTP bind address |
| `AXTERMINATOR_HTTP_TOKEN` | (none) | Bearer token for HTTP auth |
| `AXTERMINATOR_TREE_CACHE_MS` | `500` | Element tree cache TTL |
| `AXTERMINATOR_RATE_LIMIT_RPS` | `50` | Max tool calls per second |
| `AXTERMINATOR_RATE_LIMIT_RPM` | `1000` | Max tool calls per minute |
| `AXTERMINATOR_CONCURRENCY` | `10` | Max concurrent tool calls |
| `AXTERMINATOR_VLM_RATE_LIMIT` | `10` | Max VLM calls per minute |
| `AXTERMINATOR_SCRIPT_RATE_LIMIT` | `5` | Max AppleScript calls per minute |
| `AXTERMINATOR_AUDIT_LOG` | `~/.local/share/axterminator/audit.jsonl` | Audit log path |
| `ANTHROPIC_API_KEY` | (none) | Anthropic VLM API key |
| `OPENAI_API_KEY` | (none) | OpenAI VLM API key |

### 21.2 Configuration File

```toml
# ~/.config/axterminator/config.toml

[server]
log_level = "info"
security_mode = "normal"
confirm_destructive = true

[vlm]
backend = "anthropic"
rate_limit = 10

[http]
port = 8741
bind = "127.0.0.1"

[cache]
tree_ttl_ms = 500
screenshot_debounce_ms = 100

[security]
allowed_apps = ["Calculator", "Safari", "VSCode"]
denied_apps = ["Keychain Access", "System Settings"]

[rate_limits]
rps = 50
rpm = 1000
concurrency = 10
script_rpm = 5
```

### 21.3 MCP Client Configuration

For Claude Desktop / Claude Code:

```json
{
  "mcpServers": {
    "axterminator": {
      "command": "axterminator",
      "args": ["mcp", "serve"],
      "env": {
        "AXTERMINATOR_LOG_LEVEL": "info",
        "ANTHROPIC_API_KEY": "sk-ant-..."
      }
    }
  }
}
```

For remote HTTP:

```json
{
  "mcpServers": {
    "axterminator-remote": {
      "url": "http://mac-ci-01.local:8741/mcp",
      "headers": {
        "Authorization": "Bearer <token>"
      }
    }
  }
}
```

---

## 22. Open Questions

1. **Workflow step result interpolation**: The `{{step_N.field}}` syntax for
   `ax_workflow` needs careful design to avoid injection. Should we use a restricted
   expression language, or require the agent to extract and pass values explicitly?
   Decision: implement restricted interpolation (field access only, no expressions) in
   Phase 3; evaluate need for full expressions after user feedback.

2. **Macro recording format**: Should recorded macros use our `ax_workflow` JSON format,
   or a more compact DSL? The JSON format is verbose but directly replayable. A DSL would
   be more readable but requires a parser. Decision: JSON first, DSL later if demand
   warrants.

3. **Visual diff implementation**: Should we use pixel-level comparison (fast, brittle)
   or perceptual hashing (slower, robust)? Decision: implement both. Pixel diff by
   default with configurable `--perceptual` mode.

4. **AX observer reliability**: macOS AX observers can miss events under heavy load or
   when apps are not well-behaved. Should we fall back to polling for apps that don't
   fire observer callbacks reliably? Decision: yes, implement hybrid mode. Start with
   observer; if no events received for 5 seconds on a subscribed resource, fall back to
   1-second polling. Log the fallback at `warning` level.

5. **Task TTL management**: How long should completed tasks be retained? Decision:
   retain for the TTL specified in the request (default 60 seconds), then garbage collect.
   The `tasks/list` endpoint only returns non-expired tasks.

---

## Appendix A: MCP Protocol Capability Matrix

Complete mapping of every MCP 2025-11-25 capability to our implementation decision.

| MCP Capability | Use? | Phase | Notes |
|----------------|:----:|:-----:|-------|
| **Tools** | YES | 1 | 26 tools with annotations |
| Tool annotations | YES | 1 | All 5 hint types on every tool |
| Tool outputSchema | YES | 1 | Structured returns on 15+ tools |
| Tool execution (tasks) | YES | 4 | 7 tools support task augmentation |
| **Resources** | YES | 2 | 3 static + 4 templates |
| Resource subscriptions | YES | 2 | AX observer backed, 5 subscribable |
| Resource list changed | YES | 2 | When apps connect/disconnect |
| Resource annotations | YES | 2 | audience + priority on all resources |
| **Resource Templates** | YES | 2 | RFC 6570 URI templates |
| **Prompts** | YES | 2 | 7 guided workflow prompts |
| Prompt list changed | YES | 2 | Dynamic prompts based on connected apps |
| **Completions** | YES | 2 | For resource URIs and prompt args |
| **Logging** | YES | 1 | RFC 5424 levels, structured JSON, audit logger |
| **Progress** | YES | 1 | VLM, tree scan, wait, script, workflow, audit |
| **Elicitation (form)** | YES | 3 | 6 form scenarios |
| **Elicitation (URL)** | YES | 3 | 3 URL scenarios |
| **Sampling** | YES | 4 | Screenshot interpretation, action planning |
| Sampling with tools | YES | 4 | Multi-turn tool use in sampling |
| **Roots** | YES | 5 | Find test scripts, config, macros, baselines |
| **Tasks** | YES | 4 | Full lifecycle: create, get, result, list, cancel |
| Task status notifications | YES | 4 | For long-running operations |
| Task cancellation | YES | 4 | With tokio cancellation support |
| **Transports** | | | |
| stdio | YES | 1 | Primary, default |
| Streamable HTTP | YES | 4 | Remote access, CI/CD |
| SSE | YES | 4 | Legacy compatibility |
| WebSocket | SKIP | -- | Streamable HTTP preferred |
| **Authorization** | YES | 5 | OAuth 2.1 for HTTP, bearer token simple mode |
| **Ping** | YES | 1 | Handled automatically |
| **Cancellation** | YES | 1 | Respected in long operations via tokio::select! |
| **Server info** | YES | 1 | name, title, version, description, websiteUrl, icons |
| **Session management** | YES | 4 | MCP-Session-Id for HTTP transport |
| **Resumability** | YES | 4 | SSE event IDs, Last-Event-ID reconnection |

## Appendix B: Rust MCP SDK Landscape (as of 2026-03-19)

Research summary of all Rust MCP crates evaluated. See Section 2A for the selection rationale.

| Crate | Version | Downloads | Repository | Notes |
|-------|---------|-----------|------------|-------|
| `rmcp` | 1.2.0 | 5.7M | github.com/modelcontextprotocol/rust-sdk | **Official SDK**. Full protocol, macros, tokio, stdio+HTTP. Used by Goose, Apollo, Terminator. |
| `rmcp-macros` | (bundled) | -- | (same repo) | Procedural macros for `#[tool]`, `#[prompt_router]` etc. |
| `rust-mcp-sdk` | 0.9.0 | 92K | github.com/rust-mcp-stack/rust-mcp-sdk | Third-party, async SDK. Good but not official, lower adoption. |
| `tower-mcp` | 0.9.1 | 5.8K | github.com/joshrotenberg/tower-mcp | Tower-native, interesting middleware approach. Young. |
| `mcp-attr` | 0.0.7 | 6.3K | github.com/frozenlib/mcp-attr | Declarative attribute macros. Too early. |
| `clap-mcp` | 0.0.3-rc.1 | 187 | github.com/canardleteer/clap-mcp | CLI+MCP bridge. Alpha, too few downloads. |
| `mcp-kit` | 0.4.0 | 118 | github.com/KSD-CO/mcp-kit | Plugin system. Too early. |
| `mcp-gateway` | 2.7.3 | 105 | github.com/MikkoParkkola/mcp-gateway | Meta-MCP multiplexer (our own). |

**Decision**: `rmcp` 1.2.0 is the only viable choice. Official, 5.7M downloads,
full protocol, used by our Windows counterpart (mediar-ai/terminator).

## Appendix C: Wire Protocol Examples

### C.1Tool Call with Annotations, Logging, and Progress

```
--> {"jsonrpc":"2.0","id":1,"method":"tools/list"}
<-- {"jsonrpc":"2.0","id":1,"result":{"tools":[
  {
    "name":"ax_find",
    "title":"Find UI element",
    "description":"Find UI elements by accessibility query...",
    "inputSchema":{...},
    "outputSchema":{...},
    "annotations":{
      "readOnlyHint":true,
      "destructiveHint":false,
      "idempotentHint":true,
      "openWorldHint":false
    }
  }, ...]}}

--> {"jsonrpc":"2.0","id":2,"method":"tools/call",
     "params":{"name":"ax_find","arguments":{"app":"Safari","query":"URL bar"},
               "_meta":{"progressToken":"p-42"}}}

<-- {"jsonrpc":"2.0","method":"notifications/progress",
     "params":{"progressToken":"p-42","progress":1,"total":2,
               "message":"Searching accessibility tree"}}

<-- {"jsonrpc":"2.0","method":"notifications/message",
     "params":{"level":"info","logger":"axterminator.tools.find",
               "data":{"tool":"ax_find","app":"Safari","query":"URL bar",
                        "strategy":"role_title","duration_ms":0.38,"found":true}}}

<-- {"jsonrpc":"2.0","method":"notifications/progress",
     "params":{"progressToken":"p-42","progress":2,"total":2,
               "message":"Element found"}}

<-- {"jsonrpc":"2.0","id":2,"result":{
     "content":[{"type":"text","text":"{\"found\":true,\"method\":\"accessibility\",\"role\":\"AXTextField\",\"title\":\"URL bar\",\"position\":[400,52],\"size\":[600,22],\"enabled\":true,\"value\":\"https://example.com\"}"}],
     "structuredContent":{"found":true,"method":"accessibility","role":"AXTextField",
                          "title":"URL bar","position":[400,52],"size":[600,22],
                          "enabled":true,"value":"https://example.com",
                          "element_ref":"el-a1b2c3"}}}
```

### C.2Resource Subscription with Reactive Update

```
--> {"jsonrpc":"2.0","id":3,"method":"resources/subscribe",
     "params":{"uri":"axterminator://app/Safari/events"}}
<-- {"jsonrpc":"2.0","id":3,"result":{}}

  ... user opens a dialog in Safari ...

<-- {"jsonrpc":"2.0","method":"notifications/resources/updated",
     "params":{"uri":"axterminator://app/Safari/events"}}

--> {"jsonrpc":"2.0","id":4,"method":"resources/read",
     "params":{"uri":"axterminator://app/Safari/events"}}
<-- {"jsonrpc":"2.0","id":4,"result":{"contents":[{
       "uri":"axterminator://app/Safari/events",
       "mimeType":"application/json",
       "text":"{\"events\":[{\"type\":\"dialog_appeared\",\"title\":\"Save As\",\"pattern\":\"file_save_dialog\",\"timestamp\":\"2026-03-19T10:30:00Z\"}]}"
     }]}}
```

### C.3Elicitation for Destructive Action

```
--> {"jsonrpc":"2.0","id":5,"method":"tools/call",
     "params":{"name":"ax_click","arguments":{"app":"Settings","query":"Delete All Data"}}}

<-- {"jsonrpc":"2.0","id":100,"method":"elicitation/create",
     "params":{
       "mode":"form",
       "message":"This will click 'Delete All Data' in Settings. This action may be irreversible.",
       "requestedSchema":{
         "type":"object",
         "properties":{
           "confirm":{"type":"boolean","title":"Confirm destructive action","default":false}
         },
         "required":["confirm"]
       }
     }}

--> {"jsonrpc":"2.0","id":100,"result":{"action":"accept","content":{"confirm":true}}}

<-- {"jsonrpc":"2.0","method":"notifications/message",
     "params":{"level":"notice","logger":"axterminator.audit",
               "data":{"tool":"ax_click","app":"Settings","query":"Delete All Data",
                        "confirmed":true,"security_flags":["destructive_keyword"]}}}

<-- {"jsonrpc":"2.0","id":5,"result":{
     "content":[{"type":"text","text":"Clicked 'Delete All Data' in Settings"}]}}
```

### C.4Task-Augmented Workflow Execution

```
--> {"jsonrpc":"2.0","id":6,"method":"tools/call",
     "params":{
       "name":"ax_workflow",
       "arguments":{
         "name":"save-document",
         "steps":[
           {"tool":"ax_key_press","args":{"app":"TextEdit","key":"s","modifiers":["cmd"]}},
           {"tool":"ax_wait_idle","args":{"app":"TextEdit","timeout_ms":3000}},
           {"tool":"ax_find","args":{"app":"TextEdit","query":"Save"},"retry":3},
           {"tool":"ax_click","args":{"app":"TextEdit","query":"Save"}}
         ]
       },
       "task":{"ttl":30000},
       "_meta":{"progressToken":"p-99"}
     }}

<-- {"jsonrpc":"2.0","id":6,"result":{
      "task":{
        "taskId":"wf-d4e5f6",
        "status":"working",
        "statusMessage":"Starting workflow 'save-document' (4 steps)",
        "createdAt":"2026-03-19T10:35:00Z",
        "pollInterval":500
      },
      "_meta":{
        "io.modelcontextprotocol/model-immediate-response":"Workflow 'save-document' started with 4 steps. Running in background."
      }
    }}

<-- {"jsonrpc":"2.0","method":"notifications/progress",
     "params":{"progressToken":"p-99","progress":1,"total":4,"message":"Step 1/4: ax_key_press (Cmd+S)"}}

<-- {"jsonrpc":"2.0","method":"notifications/progress",
     "params":{"progressToken":"p-99","progress":2,"total":4,"message":"Step 2/4: ax_wait_idle"}}

<-- {"jsonrpc":"2.0","method":"notifications/progress",
     "params":{"progressToken":"p-99","progress":3,"total":4,"message":"Step 3/4: ax_find 'Save' (attempt 1/3)"}}

<-- {"jsonrpc":"2.0","method":"notifications/progress",
     "params":{"progressToken":"p-99","progress":4,"total":4,"message":"Step 4/4: ax_click 'Save'"}}

<-- {"jsonrpc":"2.0","method":"notifications/tasks/status",
     "params":{"taskId":"wf-d4e5f6","status":"completed"}}

--> {"jsonrpc":"2.0","id":7,"method":"tasks/result",
     "params":{"taskId":"wf-d4e5f6"}}

<-- {"jsonrpc":"2.0","id":7,"result":{
      "content":[{"type":"text","text":"Workflow 'save-document' completed: 4/4 steps succeeded"}],
      "structuredContent":{
        "success":true,"steps_completed":4,"steps_total":4,
        "results":[{"tool":"ax_key_press","ok":true},{"tool":"ax_wait_idle","ok":true},
                   {"tool":"ax_find","ok":true},{"tool":"ax_click","ok":true}],
        "rolled_back":false
      }
    }}
```
