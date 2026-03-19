# AXTerminator MCP Server Design Document

**Status**: Draft | **Version**: 2.0 | **Date**: 2026-03-19
**Protocol**: MCP 2025-11-25 | **SDK**: mcp v1.26.0 (Python)
**Author**: Mikko Parkkola

---

## 1. Vision

### The Hands and Eyes of AI on macOS

AXTerminator's MCP server transforms any MCP-compatible AI agent into a full operator
of macOS applications. The agent can see (screenshots, element trees), understand
(accessibility attributes, UI state), and control (click, type, drag, scroll) any
macOS application -- all without stealing window focus.

**Positioning**: The definitive MCP server for macOS GUI interaction. Where
mediar-ai/terminator owns Windows and browser-use owns Chrome, axterminator owns
macOS.

### Design Principles

1. **Background-first**: Every operation defaults to background mode. The user's
   active window is never disturbed unless explicitly requested.
2. **Accessibility + Vision**: Accessibility API is the primary channel. VLM visual
   detection is the automatic fallback when accessibility fails (canvas, WebGL,
   shadow DOM).
3. **Sub-millisecond core**: The Rust FFI core operates at ~379us per element access.
   The MCP layer adds protocol overhead but never blocks the core path.
4. **Progressive disclosure**: Simple tools for common tasks, resources for rich
   context, prompts for guided workflows, sampling for autonomous reasoning.

### What This Enables

- Claude, GPT, Gemini, or any MCP client can operate any macOS app
- Test automation orchestrated by AI agents
- Agentic workflows that span multiple applications
- AI-assisted debugging of UI issues
- Data extraction from apps that have no API
- Accessibility auditing powered by AI

---

## 2. Protocol Features -- Complete MCP Capability Map

### 2.1 Protocol Version

Target: **2025-11-25** (latest). The SDK v1.26.0 supports `2024-11-05`, `2025-03-26`,
`2025-06-18`, and `2025-11-25`. We negotiate the latest version the client supports.

### 2.2 Server Capabilities Declaration

```python
ServerCapabilities(
    tools=ToolsCapability(listChanged=True),
    resources=ResourcesCapability(subscribe=True, listChanged=True),
    prompts=PromptsCapability(listChanged=True),
    logging=LoggingCapability(),
    completions=CompletionsCapability(),
)
```

### 2.3 Server Instructions

The `InitializeResult.instructions` field tells the agent how to use the server:

```
AXTerminator: macOS GUI control for AI agents.

Workflow:
1. Call ax_is_accessible to verify permissions
2. Call ax_connect with an app name, bundle ID, or PID
3. Use ax_find to locate elements, ax_click/ax_type/ax_set_value to interact
4. Use ax_screenshot for visual context
5. If ax_find fails, ax_find_visual uses AI vision as fallback

All actions run in background mode by default (no focus stealing).
Read the axterminator://apps resource for a list of running applications.
Read axterminator://app/{name}/tree for the element hierarchy of any connected app.
```

---

## 3. Tools -- Enhanced and New

### 3.1 Existing Tools (Enhanced with Annotations)

Every tool now carries `ToolAnnotations` with semantic hints that help the client
make better decisions about tool use, confirmation prompts, and retry behavior.

#### ax_is_accessible

| Property | Value | Rationale |
|----------|-------|-----------|
| `readOnlyHint` | `true` | Only checks system state |
| `destructiveHint` | `false` | No side effects |
| `idempotentHint` | `true` | Same result every time |
| `openWorldHint` | `false` | Closed system check |
| `title` | "Check accessibility permissions" | |

No changes to input/output schema.

#### ax_connect

| Property | Value | Rationale |
|----------|-------|-----------|
| `readOnlyHint` | `false` | Mutates server state (adds to connected apps) |
| `destructiveHint` | `false` | Additive -- connects, does not disconnect others |
| `idempotentHint` | `true` | Connecting twice to same app is safe |
| `openWorldHint` | `false` | Interacts with local system only |
| `title` | "Connect to a macOS application" | |

**Enhancement**: Add `outputSchema` returning structured JSON:
```json
{
  "type": "object",
  "properties": {
    "connected": {"type": "boolean"},
    "app_name": {"type": "string"},
    "pid": {"type": "integer"},
    "bundle_id": {"type": "string"},
    "windows": {"type": "integer"}
  },
  "required": ["connected", "app_name"]
}
```

#### ax_find

| Property | Value | Rationale |
|----------|-------|-----------|
| `readOnlyHint` | `true` | Only searches, no mutation |
| `destructiveHint` | `false` | -- |
| `idempotentHint` | `true` | Same query returns same element (if UI unchanged) |
| `openWorldHint` | `false` | Local accessibility tree |
| `title` | "Find UI element" | |

**Enhancement**: Return `ImageContent` when VLM fallback is used (annotated
screenshot with bounding box). Add `outputSchema` for structured element data.

#### ax_click

| Property | Value | Rationale |
|----------|-------|-----------|
| `readOnlyHint` | `false` | Triggers UI actions |
| `destructiveHint` | `true` | May cause irreversible state changes (delete button, etc.) |
| `idempotentHint` | `false` | Double-clicking has different effects |
| `openWorldHint` | `false` | Local app interaction |
| `title` | "Click UI element" | |

#### ax_type

| Property | Value | Rationale |
|----------|-------|-----------|
| `readOnlyHint` | `false` | Modifies text field content |
| `destructiveHint` | `true` | May overwrite existing text |
| `idempotentHint` | `false` | Typing same text twice appends |
| `openWorldHint` | `false` | Local app interaction |
| `title` | "Type text into element" | |

#### ax_set_value

| Property | Value | Rationale |
|----------|-------|-----------|
| `readOnlyHint` | `false` | Sets element value |
| `destructiveHint` | `true` | Replaces current value |
| `idempotentHint` | `true` | Setting same value twice is identical |
| `openWorldHint` | `false` | Local app interaction |
| `title` | "Set element value" | |

#### ax_get_value

| Property | Value | Rationale |
|----------|-------|-----------|
| `readOnlyHint` | `true` | Only reads value |
| `destructiveHint` | `false` | -- |
| `idempotentHint` | `true` | Same element returns same value |
| `openWorldHint` | `false` | Local accessibility API |
| `title` | "Get element value" | |

#### ax_list_windows

| Property | Value | Rationale |
|----------|-------|-----------|
| `readOnlyHint` | `true` | Lists windows, no mutation |
| `destructiveHint` | `false` | -- |
| `idempotentHint` | `true` | Same windows at same point in time |
| `openWorldHint` | `false` | Local process inspection |
| `title` | "List application windows" | |

#### ax_screenshot

| Property | Value | Rationale |
|----------|-------|-----------|
| `readOnlyHint` | `true` | Captures display, no mutation |
| `destructiveHint` | `false` | -- |
| `idempotentHint` | `true` | Same visual state produces same screenshot |
| `openWorldHint` | `false` | Local screen capture |
| `title` | "Capture screenshot" | |

**Enhancement**: Return proper `ImageContent` (not base64 in text). The MCP protocol
has native `ImageContent` with `type: "image"`, `data` (base64), and `mimeType`.
This is a significant improvement -- agents can directly perceive the screenshot.

```python
ImageContent(type="image", data=b64_data, mimeType="image/png")
```

#### ax_click_at

| Property | Value | Rationale |
|----------|-------|-----------|
| `readOnlyHint` | `false` | Performs click action |
| `destructiveHint` | `true` | May trigger irreversible actions |
| `idempotentHint` | `false` | Repeated clicks have cumulative effects |
| `openWorldHint` | `false` | Local coordinate click |
| `title` | "Click at screen coordinates" | |

#### ax_find_visual

| Property | Value | Rationale |
|----------|-------|-----------|
| `readOnlyHint` | `true` | Only locates element, no mutation |
| `destructiveHint` | `false` | -- |
| `idempotentHint` | `true` | Same screenshot + description = same location |
| `openWorldHint` | `true` | Uses external VLM API (Anthropic/OpenAI/local) |
| `title` | "Find element using AI vision" | |

#### ax_wait_idle

| Property | Value | Rationale |
|----------|-------|-----------|
| `readOnlyHint` | `true` | Waits and observes, no mutation |
| `destructiveHint` | `false` | -- |
| `idempotentHint` | `true` | Waiting when already idle returns immediately |
| `openWorldHint` | `false` | Local process monitoring |
| `title` | "Wait for application idle" | |

### 3.2 New Tools

#### ax_scroll

Scroll within an element or at coordinates.

```python
Tool(
    name="ax_scroll",
    description="Scroll within an element or at screen coordinates.",
    inputSchema={
        "type": "object",
        "properties": {
            "app": {"type": "string"},
            "query": {"type": "string", "description": "Element to scroll within (optional)"},
            "direction": {"type": "string", "enum": ["up", "down", "left", "right"]},
            "amount": {"type": "integer", "description": "Scroll amount in pixels (default: 100)", "default": 100},
        },
        "required": ["app", "direction"],
    },
    annotations=ToolAnnotations(
        title="Scroll within element",
        readOnlyHint=False, destructiveHint=False,
        idempotentHint=False, openWorldHint=False,
    ),
)
```

#### ax_drag

Drag from one element/position to another.

```python
Tool(
    name="ax_drag",
    description="Drag from one element or position to another. Use for drag-and-drop, slider adjustment, and window resizing.",
    inputSchema={
        "type": "object",
        "properties": {
            "app": {"type": "string"},
            "from_query": {"type": "string", "description": "Source element query"},
            "to_query": {"type": "string", "description": "Target element query"},
            "from_x": {"type": "integer"}, "from_y": {"type": "integer"},
            "to_x": {"type": "integer"}, "to_y": {"type": "integer"},
            "duration_ms": {"type": "integer", "default": 500},
        },
        "required": ["app"],
    },
    annotations=ToolAnnotations(
        title="Drag element or between positions",
        readOnlyHint=False, destructiveHint=True,
        idempotentHint=False, openWorldHint=False,
    ),
)
```

#### ax_get_tree

Return the full or partial accessibility element tree for an app window. This is
critical for agents to understand what is on screen.

```python
Tool(
    name="ax_get_tree",
    description="Get the accessibility element tree for an app. Returns a hierarchical view of all UI elements with their roles, titles, values, and positions. Essential for understanding the current UI state.",
    inputSchema={
        "type": "object",
        "properties": {
            "app": {"type": "string"},
            "root_query": {"type": "string", "description": "Optional: start from a specific element"},
            "max_depth": {"type": "integer", "description": "Maximum tree depth (default: 5)", "default": 5},
            "include_invisible": {"type": "boolean", "default": False},
        },
        "required": ["app"],
    },
    annotations=ToolAnnotations(
        title="Get element tree hierarchy",
        readOnlyHint=True, destructiveHint=False,
        idempotentHint=True, openWorldHint=False,
    ),
)
```

#### ax_run_script

Execute AppleScript or JXA for operations not covered by the accessibility API.

```python
Tool(
    name="ax_run_script",
    description="Execute AppleScript or JXA (JavaScript for Automation) on macOS. Use for operations that the accessibility API cannot perform, such as menu bar access, system dialogs, or app-specific scripting dictionaries.",
    inputSchema={
        "type": "object",
        "properties": {
            "script": {"type": "string", "description": "AppleScript or JXA code"},
            "language": {"type": "string", "enum": ["applescript", "jxa"], "default": "applescript"},
            "timeout_ms": {"type": "integer", "default": 10000},
        },
        "required": ["script"],
    },
    annotations=ToolAnnotations(
        title="Run AppleScript/JXA",
        readOnlyHint=False, destructiveHint=True,
        idempotentHint=False, openWorldHint=True,
    ),
)
```

#### ax_list_apps

List all running applications and their accessibility status.

```python
Tool(
    name="ax_list_apps",
    description="List all running macOS applications with their process IDs, bundle IDs, and whether they are accessible. Use this to discover what apps are available before connecting.",
    inputSchema={"type": "object", "properties": {}},
    annotations=ToolAnnotations(
        title="List running applications",
        readOnlyHint=True, destructiveHint=False,
        idempotentHint=True, openWorldHint=False,
    ),
)
```

#### ax_key_press

Send keyboard shortcuts and special keys.

```python
Tool(
    name="ax_key_press",
    description="Send keyboard shortcuts or special key presses. Supports modifier keys (cmd, ctrl, opt, shift) and special keys (return, escape, tab, arrow keys, F1-F12).",
    inputSchema={
        "type": "object",
        "properties": {
            "app": {"type": "string"},
            "key": {"type": "string", "description": "Key to press (e.g., 'return', 'escape', 'a', 'F5')"},
            "modifiers": {
                "type": "array",
                "items": {"type": "string", "enum": ["cmd", "ctrl", "opt", "shift"]},
                "description": "Modifier keys to hold (e.g., ['cmd', 'shift'] for Cmd+Shift)",
            },
        },
        "required": ["key"],
    },
    annotations=ToolAnnotations(
        title="Send keyboard shortcut",
        readOnlyHint=False, destructiveHint=True,
        idempotentHint=False, openWorldHint=False,
    ),
)
```

#### ax_get_attributes

Get all accessibility attributes for an element. Useful for debugging and deep
inspection.

```python
Tool(
    name="ax_get_attributes",
    description="Get all accessibility attributes for a UI element. Returns every AX attribute including role, title, value, position, size, enabled, focused, description, help text, and custom attributes.",
    inputSchema={
        "type": "object",
        "properties": {
            "app": {"type": "string"},
            "query": {"type": "string"},
        },
        "required": ["app", "query"],
    },
    annotations=ToolAnnotations(
        title="Get all element attributes",
        readOnlyHint=True, destructiveHint=False,
        idempotentHint=True, openWorldHint=False,
    ),
)
```

### 3.3 Tool Summary Table

| Tool | Read | Destruct | Idempotent | OpenWorld | New? |
|------|:----:|:--------:|:----------:|:---------:|:----:|
| ax_is_accessible | yes | no | yes | no | |
| ax_connect | no | no | yes | no | |
| ax_find | yes | no | yes | no | |
| ax_click | no | yes | no | no | |
| ax_type | no | yes | no | no | |
| ax_set_value | no | yes | yes | no | |
| ax_get_value | yes | no | yes | no | |
| ax_list_windows | yes | no | yes | no | |
| ax_screenshot | yes | no | yes | no | |
| ax_click_at | no | yes | no | no | |
| ax_find_visual | yes | no | yes | yes | |
| ax_wait_idle | yes | no | yes | no | |
| ax_scroll | no | no | no | no | NEW |
| ax_drag | no | yes | no | no | NEW |
| ax_get_tree | yes | no | yes | no | NEW |
| ax_run_script | no | yes | no | yes | NEW |
| ax_list_apps | yes | no | yes | no | NEW |
| ax_key_press | no | yes | no | no | NEW |
| ax_get_attributes | yes | no | yes | no | NEW |

Total: **19 tools** (12 enhanced + 7 new).

---

## 4. Resources -- Live Application State

Resources provide read-only, URI-addressable views of application state. Unlike
tools, resources can be subscribed to for change notifications and are ideal for
providing context that the agent reads repeatedly.

### 4.1 Static Resources

#### `axterminator://system/status`

System-level accessibility status and server information.

```python
Resource(
    uri="axterminator://system/status",
    name="system-status",
    title="System Accessibility Status",
    description="Accessibility permissions, connected apps, VLM backend status, and server version.",
    mimeType="application/json",
)
```

Returns:
```json
{
  "accessibility_enabled": true,
  "server_version": "0.4.0",
  "protocol_version": "2025-11-25",
  "vlm_backend": "anthropic",
  "vlm_available": true,
  "connected_apps": ["Safari", "Finder"],
  "platform": "macOS 15.2, Apple M1"
}
```

#### `axterminator://apps`

List of running applications with accessibility info.

```python
Resource(
    uri="axterminator://apps",
    name="running-apps",
    title="Running Applications",
    description="All running macOS applications with their PIDs, bundle IDs, and accessibility status.",
    mimeType="application/json",
)
```

### 4.2 Resource Templates (Dynamic)

Resource templates use RFC 6570 URI templates to provide dynamic, parameterized
access to application state.

#### `axterminator://app/{name}/tree`

Live element tree for a connected application.

```python
ResourceTemplate(
    uriTemplate="axterminator://app/{name}/tree",
    name="app-element-tree",
    title="Application Element Tree",
    description="The full accessibility element hierarchy for a connected app. Returns roles, titles, values, and positions for all visible elements.",
    mimeType="application/json",
)
```

#### `axterminator://app/{name}/screenshot`

Current screenshot of an application.

```python
ResourceTemplate(
    uriTemplate="axterminator://app/{name}/screenshot",
    name="app-screenshot",
    title="Application Screenshot",
    description="Current screenshot of a connected app as a PNG image.",
    mimeType="image/png",
)
```

Returns `BlobResourceContents` with base64-encoded PNG.

#### `axterminator://app/{name}/state`

Structured UI state summary (window titles, focused element, menu bar state).

```python
ResourceTemplate(
    uriTemplate="axterminator://app/{name}/state",
    name="app-ui-state",
    title="Application UI State",
    description="Current UI state summary: window titles, focused element, menu bar items, toolbar state.",
    mimeType="application/json",
)
```

#### `axterminator://app/{name}/window/{index}/tree`

Element tree for a specific window (when apps have multiple windows).

```python
ResourceTemplate(
    uriTemplate="axterminator://app/{name}/window/{index}/tree",
    name="window-element-tree",
    title="Window Element Tree",
    description="Element hierarchy for a specific window by index.",
    mimeType="application/json",
)
```

### 4.3 Resource Subscriptions

The server supports `resources/subscribe` for live change notifications. When the
agent subscribes to a resource, the server monitors the underlying application state
and emits `notifications/resources/updated` when changes are detected.

**Subscribable resources**:
- `axterminator://apps` -- when applications launch or quit
- `axterminator://app/{name}/state` -- when focused element or window changes
- `axterminator://app/{name}/tree` -- when element tree structure changes

**Implementation**: A background polling loop (configurable interval, default 1s)
compares hashed state snapshots and emits notifications on change.

### 4.4 Completions

The server supports `completion/complete` for resource URI templates and prompt
arguments. When the agent types `axterminator://app/` the server suggests connected
app names.

---

## 5. Prompts -- Guided Workflows

Prompts provide pre-built conversation starters for common axterminator workflows.
Each prompt returns `PromptMessage` objects that guide the agent through a multi-step
process.

### 5.1 test-app

**Purpose**: Guide an agent through testing a macOS application.

```python
Prompt(
    name="test-app",
    title="Test a macOS Application",
    description="Step-by-step guide to test a macOS application. Checks accessibility, connects, explores the UI, and runs basic interaction tests.",
    arguments=[
        PromptArgument(name="app_name", description="Name of the app to test", required=True),
        PromptArgument(name="focus_area", description="Specific area to test (e.g., 'toolbar', 'sidebar', 'main content')", required=False),
    ],
)
```

Returns messages that instruct the agent to:
1. Verify accessibility with `ax_is_accessible`
2. Connect with `ax_connect`
3. Read the element tree with `ax_get_tree`
4. Take a screenshot with `ax_screenshot`
5. Identify interactive elements and test them
6. Report findings

### 5.2 navigate-to

**Purpose**: Guide navigation to a specific screen or state.

```python
Prompt(
    name="navigate-to",
    title="Navigate to a Screen",
    description="Navigate to a specific screen, dialog, or state within a macOS application.",
    arguments=[
        PromptArgument(name="app_name", description="Name of the app", required=True),
        PromptArgument(name="destination", description="Where to navigate (e.g., 'Settings > General', 'File > New')", required=True),
    ],
)
```

### 5.3 extract-data

**Purpose**: Extract structured data from an application's UI.

```python
Prompt(
    name="extract-data",
    title="Extract Data from Application",
    description="Extract structured data from a running macOS application. Reads element values, table contents, or form fields and returns them as structured data.",
    arguments=[
        PromptArgument(name="app_name", description="Name of the app", required=True),
        PromptArgument(name="data_description", description="What data to extract (e.g., 'all contacts', 'table in main window')", required=True),
        PromptArgument(name="output_format", description="Desired format: json, csv, or markdown", required=False),
    ],
)
```

### 5.4 automate-workflow

**Purpose**: Automate a multi-step workflow across one or more apps.

```python
Prompt(
    name="automate-workflow",
    title="Automate a Workflow",
    description="Automate a multi-step workflow in one or more macOS applications. Describe the workflow in natural language and the agent will plan and execute it.",
    arguments=[
        PromptArgument(name="workflow", description="Natural language description of the workflow", required=True),
        PromptArgument(name="apps", description="Comma-separated app names involved", required=False),
    ],
)
```

### 5.5 debug-ui

**Purpose**: Debug why a UI element cannot be found.

```python
Prompt(
    name="debug-ui",
    title="Debug UI Element",
    description="Debug why a UI element cannot be found. Explores the element tree, takes screenshots, and suggests alternative locators.",
    arguments=[
        PromptArgument(name="app_name", description="Name of the app", required=True),
        PromptArgument(name="element_description", description="Description of the element you're looking for", required=True),
        PromptArgument(name="query_tried", description="Query that failed", required=False),
    ],
)
```

### 5.6 accessibility-audit

**Purpose**: Audit an application's accessibility compliance.

```python
Prompt(
    name="accessibility-audit",
    title="Accessibility Audit",
    description="Audit a macOS application for accessibility issues. Checks for missing labels, roles, keyboard navigation, and WCAG compliance.",
    arguments=[
        PromptArgument(name="app_name", description="Name of the app to audit", required=True),
    ],
)
```

---

## 6. Logging

### 6.1 Structured MCP Logging

Every tool call emits MCP log notifications via `session.send_log_message()`. This
makes server operations visible to the agent and to the client's log viewer.

**Log levels used**:
- `debug`: Element cache hits, accessibility attribute reads
- `info`: Tool calls, connection events, search results
- `notice`: VLM fallback triggered, element found via non-primary strategy
- `warning`: Slow operations (>100ms), deprecated API usage
- `error`: Element not found after all strategies, accessibility denied
- `critical`: Server crash, axterminator library unavailable

**Log format**:
```python
await session.send_log_message(
    level="info",
    data={
        "tool": "ax_find",
        "app": "Safari",
        "query": "URL bar",
        "strategy": "title_match",
        "duration_ms": 0.4,
        "found": True,
    },
    logger="axterminator.tools",
)
```

### 6.2 Performance Metrics

Every tool invocation is timed. The log data includes:
- `duration_ms`: Total time from call to response
- `ax_duration_ms`: Time spent in the Rust accessibility layer
- `vlm_duration_ms`: Time spent in VLM inference (when applicable)
- `strategy`: Which self-healing strategy succeeded

---

## 7. Progress Notifications

Long-running operations report progress via `send_progress_notification()`. The
client can display a progress bar or status message.

### 7.1 When Progress is Reported

| Operation | Progress Steps |
|-----------|---------------|
| VLM visual search | "Capturing screenshot" -> "Running VLM inference" -> "Parsing result" |
| Element tree scan | "Scanning window 1/3" -> "Processing 247 elements" -> "Building tree" |
| Wait idle | "Waiting... (0.5s / 5s)" -> "Waiting... (2.1s / 5s)" -> "App idle" |
| AppleScript execution | "Compiling script" -> "Executing" -> "Complete" |

### 7.2 Implementation

Progress is sent only when the request includes a `progressToken` in `_meta`:

```python
if progress_token:
    await ctx.session.send_progress_notification(
        progress_token=progress_token,
        progress=1,
        total=3,
        message="Capturing screenshot for VLM analysis",
    )
```

---

## 8. Transport

### 8.1 stdio (Existing -- Enhanced)

The primary transport for local use. Claude Code, Claude Desktop, and most MCP
clients use stdio.

**Current**: `stdio_server()` from `mcp.server.stdio`.
**Enhancement**: None needed. This works and is the standard.

### 8.2 Streamable HTTP (New -- High Priority)

Enables remote control of macOS applications. A headless Mac in a rack room or CI
farm can serve its GUI state over HTTP.

```python
from mcp.server.streamable_http_manager import StreamableHTTPSessionManager

manager = StreamableHTTPSessionManager(
    server=server,
    event_store=InMemoryEventStore(),
    stateless=False,  # Stateful sessions for connected apps
    json_response=False,  # SSE streaming for progress/notifications
)
```

**Use cases**:
- CI/CD: Automated UI tests triggered by GitHub Actions, results streamed back
- Remote pair: Agent on one machine controls an app on another
- Fleet management: One agent orchestrates multiple Macs

**Security**: Bind to `127.0.0.1` by default. Require authentication token for
non-localhost access. Use `TransportSecuritySettings` for DNS rebinding protection.

### 8.3 SSE (Deprecated but Supported)

The SDK includes `SseServerTransport` for backward compatibility. Some older clients
may only support SSE. We will offer it but recommend Streamable HTTP.

### 8.4 Transport Selection

```
axterminator-mcp              # stdio (default)
axterminator-mcp --http       # Streamable HTTP on :8741
axterminator-mcp --http --port 9000  # Custom port
axterminator-mcp --sse        # SSE (legacy)
```

---

## 9. Elicitation

Elicitation lets the server ask the user for input when the tool cannot proceed
autonomously.

### 9.1 Form Mode Elicitation

#### Ambiguous App Name

When `ax_connect` matches multiple running apps (e.g., "Chrome" matches both Google
Chrome and Chrome Canary):

```python
result = await ctx.session.elicit_form(
    message="Multiple apps match 'Chrome'. Which one?",
    requestedSchema={
        "type": "object",
        "properties": {
            "app": {
                "type": "string",
                "description": "Select application",
                "enum": ["Google Chrome (PID 1234)", "Chrome Canary (PID 5678)"],
            }
        },
        "required": ["app"],
    },
)
```

#### Destructive Action Confirmation

When a click targets an element whose title suggests destruction (contains "delete",
"remove", "erase", "quit", "close"):

```python
result = await ctx.session.elicit_form(
    message="This will click 'Delete All Data'. Confirm?",
    requestedSchema={
        "type": "object",
        "properties": {
            "confirm": {"type": "boolean", "description": "Confirm destructive action"}
        },
        "required": ["confirm"],
    },
)
```

#### Element Not Found -- Clarification

When accessibility and VLM both fail to find an element:

```python
result = await ctx.session.elicit_form(
    message="Could not find element matching 'Submit'. Can you describe it differently?",
    requestedSchema={
        "type": "object",
        "properties": {
            "description": {"type": "string", "description": "Alternative description of the element"},
            "use_screenshot": {"type": "boolean", "description": "Should I take a screenshot and use vision?"},
        },
        "required": ["description"],
    },
)
```

### 9.2 URL Mode Elicitation

For VLM backends that require OAuth or API key entry via a web UI:

```python
result = await ctx.session.elicit_url(
    message="Anthropic API key required for VLM visual search. Please log in to add your key.",
    url="https://console.anthropic.com/settings/keys",
    elicitation_id=f"vlm-auth-{uuid4()}",
)
```

### 9.3 Capability Check

Elicitation requires the client to advertise `ElicitationCapability`. Always check
before calling:

```python
if ctx.session.check_client_capability(
    ClientCapabilities(elicitation=ElicitationCapability(form=FormElicitationCapability()))
):
    result = await ctx.session.elicit_form(...)
else:
    # Fall back: return error with instructions
    return [TextContent(type="text", text="Error: Multiple apps match. Specify bundle ID.")]
```

---

## 10. Sampling

Sampling allows the server to request LLM inference from the client. This is the
most powerful MCP capability for axterminator -- it enables the server to reason about
visual UI state.

### 10.1 Screenshot Interpretation

When the server needs to understand what is visible on screen (beyond what
accessibility attributes provide):

```python
result = await ctx.session.create_message(
    messages=[
        SamplingMessage(
            role="user",
            content=[
                ImageContent(type="image", data=screenshot_b64, mimeType="image/png"),
                TextContent(type="text", text="Describe what you see in this macOS application window. List all visible UI elements, buttons, text fields, and their approximate positions."),
            ],
        ),
    ],
    max_tokens=1000,
    system_prompt="You are analyzing a macOS application screenshot for UI automation. Be precise about element positions and types.",
    model_preferences=ModelPreferences(
        intelligencePriority=0.8,
        costPriority=0.3,
        speedPriority=0.5,
    ),
)
```

### 10.2 Next Action Suggestion

After performing an action, the server can ask the LLM what to do next:

```python
result = await ctx.session.create_message(
    messages=[
        SamplingMessage(
            role="user",
            content=[
                TextContent(type="text", text=f"Current UI state of {app_name}:\n{element_tree_json}\n\nGoal: {user_goal}\nActions taken so far: {action_history}\n\nWhat should be the next action?"),
            ],
        ),
    ],
    max_tokens=500,
    system_prompt="You are a macOS UI automation planner. Suggest the single next action to take.",
)
```

### 10.3 Capability Check

```python
if ctx.session.check_client_capability(
    ClientCapabilities(sampling=SamplingCapability())
):
    # Full sampling available
    result = await ctx.session.create_message(...)
else:
    # No sampling -- fall back to returning data for the agent to reason about
    return [ImageContent(...), TextContent(type="text", text="Here is the screenshot. Please analyze it.")]
```

### 10.4 Sampling with Tools (New in 2025-11-25)

The server can provide tools to the LLM during sampling, enabling multi-turn tool
use within a single sampling call. This is powerful for autonomous workflows:

```python
result = await ctx.session.create_message(
    messages=[...],
    max_tokens=2000,
    tools=[
        Tool(name="describe_element", inputSchema={...}),
        Tool(name="suggest_locator", inputSchema={...}),
    ],
    tool_choice=ToolChoice(mode="auto"),
)
```

---

## 11. Roots

The server can request `roots/list` from the client to discover relevant directories.
For axterminator, this is used to:

- Find test scripts the user might want to run
- Locate app bundles for inspection
- Access configuration files

```python
if ctx.session.check_client_capability(
    ClientCapabilities(roots=RootsCapability())
):
    roots = await ctx.session.list_roots()
    for root in roots.roots:
        # Check for .axterminator config files, test scripts, etc.
        pass
```

---

## 12. Architecture

### 12.1 Module Structure

Migrate from single 772-line `server.py` to a clean module layout:

```
axterminator/
  mcp/
    __init__.py          # Package init, exports create_server()
    server.py            # FastMCP server setup, lifespan, transport selection
    tools/
      __init__.py        # Tool registration
      connect.py         # ax_connect, ax_list_apps, ax_is_accessible
      find.py            # ax_find, ax_find_visual, ax_get_tree, ax_get_attributes
      actions.py         # ax_click, ax_type, ax_set_value, ax_scroll, ax_drag, ax_key_press
      observe.py         # ax_screenshot, ax_get_value, ax_list_windows, ax_wait_idle
      scripting.py       # ax_run_script
    resources/
      __init__.py        # Resource registration
      apps.py            # axterminator://apps, axterminator://system/status
      app_state.py       # axterminator://app/{name}/tree, /state, /screenshot
    prompts/
      __init__.py        # Prompt registration
      workflows.py       # test-app, navigate-to, automate-workflow
      debugging.py       # debug-ui, accessibility-audit
      extraction.py      # extract-data
    middleware/
      __init__.py
      logging.py         # Structured MCP logging for every tool call
      progress.py        # Progress notification helpers
      elicitation.py     # Elicitation helpers (ambiguity, confirmation)
      sampling.py        # Sampling helpers (screenshot interpretation)
    state.py             # AppConnectionManager (replaces global dict)
    config.py            # Server configuration (env vars, VLM backend)
```

### 12.2 State Management

Replace the global `_connected_apps: dict` with a proper state manager:

```python
class AppConnectionManager:
    """Manages connected application state across the server lifecycle."""

    def __init__(self):
        self._apps: dict[str, AppConnection] = {}
        self._lock = asyncio.Lock()

    async def connect(self, identifier: str, alias: str | None = None) -> AppConnection:
        async with self._lock:
            app_obj = self._resolve_app(identifier)
            conn = AppConnection(
                app=app_obj,
                alias=alias or identifier,
                connected_at=datetime.now(),
                pid=app_obj.pid(),
            )
            self._apps[conn.alias] = conn
            return conn

    def get(self, name: str) -> AppConnection:
        if name not in self._apps:
            raise AppNotConnectedError(name, list(self._apps.keys()))
        return self._apps[name]

    @property
    def connected_names(self) -> list[str]:
        return list(self._apps.keys())
```

### 12.3 Server Lifecycle

Use the FastMCP lifespan pattern for initialization and cleanup:

```python
@asynccontextmanager
async def lifespan(server: FastMCP):
    """Server lifecycle: initialize on start, cleanup on stop."""
    manager = AppConnectionManager()
    vlm_backend = configure_vlm_from_env()
    yield {"app_manager": manager, "vlm_backend": vlm_backend}
    # Cleanup: disconnect all apps
    await manager.disconnect_all()
```

### 12.4 Error Handling

Structured error responses with actionable guidance:

```python
class AXTerminatorError(Exception):
    """Base error with MCP-friendly formatting."""
    def to_content(self) -> list[ContentBlock]:
        return [TextContent(type="text", text=f"Error: {self.message}\n\nSuggestion: {self.suggestion}")]

class AppNotConnectedError(AXTerminatorError):
    def __init__(self, name: str, connected: list[str]):
        self.message = f"App '{name}' not connected"
        self.suggestion = f"Use ax_connect first. Connected apps: {connected}"

class ElementNotFoundError(AXTerminatorError):
    def __init__(self, query: str, strategies_tried: list[str]):
        self.message = f"Element not found: '{query}'"
        self.suggestion = (
            f"Strategies tried: {strategies_tried}. "
            "Try ax_get_tree to see available elements, or ax_find_visual for vision-based search."
        )

class AccessibilityDeniedError(AXTerminatorError):
    def __init__(self):
        self.message = "Accessibility not enabled"
        self.suggestion = "Open System Settings > Privacy & Security > Accessibility and add your terminal app."
```

### 12.5 Performance Considerations

1. **Element tree caching**: Cache `ax_get_tree` results for 500ms (configurable).
   Invalidate on any action tool call. The Rust LRU cache handles element-level
   caching; the Python layer caches the serialized tree.

2. **Screenshot debouncing**: If `ax_screenshot` is called multiple times within
   100ms for the same app, return the cached result.

3. **Lazy VLM loading**: Do not import/configure VLM at startup. Load on first
   `ax_find_visual` call.

4. **Async tool execution**: All tool handlers are async. Long operations (VLM,
   AppleScript) run in thread pool executors to avoid blocking the event loop.

5. **Resource template resolution**: Template matching uses a pre-compiled regex
   table, not dynamic regex per request.

---

## 13. Security

### 13.1 Threat Model

The MCP server runs on the user's machine with the user's accessibility permissions.
It can see and interact with ANY application the user has open. This is powerful and
dangerous.

### 13.2 Security Controls

#### Permission Scope

- The server inherits the terminal's accessibility permissions
- It can only interact with apps visible to the Accessibility API
- It cannot grant itself additional permissions

#### App Allowlist (Optional)

```python
# .axterminator/config.toml
[security]
allowed_apps = ["Calculator", "com.apple.Safari", "com.microsoft.VSCode"]
blocked_apps = ["com.apple.Keychain-Access", "System Settings"]
```

If configured, `ax_connect` refuses to connect to apps not in the allowlist.

#### Destructive Action Safeguards

- Tools with `destructiveHint=True` can optionally require elicitation confirmation
- A configurable "safe mode" that blocks `ax_run_script` entirely
- All destructive actions are logged at `notice` level

#### Credential Handling

- VLM API keys read from environment variables only (never stored)
- No secrets in MCP tool responses
- Streamable HTTP transport requires auth token for non-localhost
- `TransportSecuritySettings` enabled for DNS rebinding protection

#### AppleScript Sandboxing

`ax_run_script` is the highest-risk tool. Safeguards:
- Default timeout of 10 seconds
- Logged at `warning` level
- Can be disabled via config
- Agent must provide the script as a string (no file execution)

### 13.3 What We Do Not Do

- We do not proxy credentials between apps
- We do not capture passwords from password fields (AX returns `****`)
- We do not modify system settings
- We do not install anything
- We do not send data to external servers (VLM calls are optional and explicit)

---

## 14. Comparison with Competitors

### 14.1 vs mediar-ai/terminator (Windows)

| Aspect | axterminator (macOS) | terminator (Windows) |
|--------|---------------------|---------------------|
| Platform | macOS only | Windows only |
| Core | Rust FFI to AXUIElement | Rust + UI Automation |
| Element access | ~379us | ~10-50ms (estimated) |
| Background mode | Yes (unique) | No |
| MCP server | Full protocol (resources, prompts, sampling) | Tools only |
| VLM fallback | Anthropic/OpenAI/MLX/Ollama | OpenAI |
| Self-healing | 7 strategies | Basic retry |
| Transport | stdio + Streamable HTTP | stdio |
| Cross-app | Yes | Limited |
| Electron CDP | Yes | No |

**Our advantages**: Background testing, 100x faster element access, richer MCP
integration, multiple VLM backends, 7-strategy self-healing.

### 14.2 vs Appium Mac2 Driver

| Aspect | axterminator | Appium Mac2 |
|--------|-------------|-------------|
| Architecture | Direct FFI | HTTP + XCTest bridge |
| Element access | ~379us | ~500ms |
| Setup | `pip install axterminator` | Xcode + Appium server + driver |
| MCP | Native | None |
| Background | Yes | No |
| Electron | CDP bridge | Via WebDriver |
| Language | Python (+ Rust core) | Any WebDriver client |

**Our advantages**: 1300x faster, zero Xcode dependency, background mode, native MCP,
simpler setup.

### 14.3 vs NVIDIA OpenShell

| Aspect | axterminator | OpenShell |
|--------|-------------|-----------|
| Focus | GUI automation | System shell + GUI |
| Platform | macOS | Windows/Linux |
| MCP | Full (tools + resources + prompts + sampling) | Tools only |
| GUI approach | Accessibility API | Mixed (a11y + vision) |

**Our advantage**: Deeper MCP integration, macOS specialization, background mode.

### 14.4 Competitive Moat

1. **Only** background GUI testing tool for macOS
2. **Only** MCP server with resources, prompts, sampling, and elicitation for GUI
3. **Fastest** element access (379us vs 10-500ms for competitors)
4. **Most strategies** for element location (7 + VLM)
5. **Broadest** VLM support (5 backends: Anthropic, OpenAI, MLX, Gemini, Ollama)

---

## 15. Implementation Plan

### Phase 1: Tool Annotations + Logging + New Tools (Week 1)

**Goal**: Every tool has annotations, structured logging, proper ImageContent.

1. Add `ToolAnnotations` to all 12 existing tools
2. Add `outputSchema` to tools with structured returns
3. Fix `ax_screenshot` to return `ImageContent` instead of base64 in text
4. Implement structured MCP logging (wrap every tool handler)
5. Add `ax_scroll`, `ax_key_press`, `ax_list_apps` tools
6. Add `ax_get_tree` tool (critical for agent understanding)
7. Add `ax_get_attributes` tool
8. Migrate from global dict to `AppConnectionManager`
9. Declare `ServerCapabilities` with logging and tools

**Deliverables**: Enhanced `server.py` with 19 annotated tools, structured logging,
`ImageContent` screenshots.

### Phase 2: Resources + Prompts (Week 2)

**Goal**: Resources and prompts make the server a rich context provider.

1. Implement `axterminator://system/status` resource
2. Implement `axterminator://apps` resource
3. Implement `axterminator://app/{name}/tree` resource template
4. Implement `axterminator://app/{name}/screenshot` resource template
5. Implement `axterminator://app/{name}/state` resource template
6. Add resource subscription support (background change detection)
7. Implement all 6 prompt templates
8. Add `completion/complete` support for resource URIs and prompt arguments
9. Declare resources, prompts, completions in `ServerCapabilities`

**Deliverables**: 5 resources/templates, 6 prompts, subscription support, completions.

### Phase 3: HTTP Transport + Progress + Elicitation (Week 3)

**Goal**: Remote access, progress feedback, interactive clarification.

1. Add Streamable HTTP transport with session management
2. Add `--http` and `--sse` CLI flags
3. Implement progress notifications for VLM, tree scan, wait idle
4. Implement form elicitation for ambiguous app names
5. Implement form elicitation for destructive action confirmation
6. Implement URL elicitation for VLM auth
7. Add capability checking before elicitation/sampling calls
8. Add authentication for HTTP transport (bearer token)
9. Begin module split (`server.py` -> `mcp/` package)

**Deliverables**: HTTP transport, progress on 4 operation types, 3 elicitation flows.

### Phase 4: Sampling + Advanced + Module Split (Week 4)

**Goal**: Full autonomous agent capabilities, clean architecture.

1. Implement sampling for screenshot interpretation
2. Implement sampling for next-action suggestion
3. Add `ax_drag` tool
4. Add `ax_run_script` tool (with security controls)
5. Complete module split to `mcp/` package
6. Add app allowlist/blocklist security
7. Write integration tests for all MCP features
8. Performance benchmarks for MCP overhead
9. Update `run-mcp.sh` and pyproject.toml entry points

**Deliverables**: Sampling integration, 2 remaining tools, final architecture, tests.

---

## 16. Configuration

### 16.1 Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `AXTERMINATOR_LOG_LEVEL` | `info` | MCP log level |
| `AXTERMINATOR_VLM_BACKEND` | auto-detect | VLM backend: anthropic, openai, mlx, gemini, ollama |
| `AXTERMINATOR_SAFE_MODE` | `false` | Block ax_run_script, require confirmation for destructive |
| `AXTERMINATOR_ALLOWED_APPS` | (none) | Comma-separated allowlist |
| `AXTERMINATOR_HTTP_PORT` | `8741` | Streamable HTTP port |
| `AXTERMINATOR_HTTP_TOKEN` | (none) | Bearer token for HTTP auth |
| `AXTERMINATOR_TREE_CACHE_MS` | `500` | Element tree cache TTL |
| `ANTHROPIC_API_KEY` | (none) | Anthropic VLM API key |
| `OPENAI_API_KEY` | (none) | OpenAI VLM API key |

### 16.2 MCP Client Configuration

```json
{
  "mcpServers": {
    "axterminator": {
      "command": "uv",
      "args": ["run", "--directory", "/path/to/axterminator", "python", "server.py"],
      "env": {
        "AXTERMINATOR_LOG_LEVEL": "info",
        "ANTHROPIC_API_KEY": "sk-ant-..."
      }
    }
  }
}
```

---

## 17. Open Questions

1. **Task-augmented execution**: MCP 2025-11-25 adds experimental "tasks" support
   for long-running operations. Should `ax_run_script` and VLM search use task
   augmentation? This would let clients poll for results instead of blocking.
   Decision: defer to Phase 4 -- evaluate SDK stability first.

2. **Audio content**: The MCP SDK now has `AudioContent`. Should we support
   text-to-speech for accessibility labels? Low priority, skip for now.

3. **WebSocket transport**: The SDK has `mcp.server.websocket`. Is there demand?
   Streamable HTTP covers the same use case with better compatibility. Skip unless
   requested.

4. **Resource template pagination**: Large element trees could exceed reasonable
   response sizes. Should we paginate `axterminator://app/{name}/tree`? Yes, but
   implement in Phase 2 only if trees exceed 100KB.

---

## Appendix A: MCP Protocol Capability Matrix

Complete mapping of every MCP 2025-11-25 capability to our implementation decision.

| MCP Capability | Use? | Phase | Notes |
|----------------|:----:|:-----:|-------|
| **Tools** | YES | 1 | 19 tools with annotations |
| Tool annotations | YES | 1 | All 5 hint types on every tool |
| Tool outputSchema | YES | 1 | Structured returns for find, connect, tree |
| Tool execution (tasks) | DEFER | 4+ | Experimental, evaluate stability |
| **Resources** | YES | 2 | 2 static + 3 templates |
| Resource subscriptions | YES | 2 | For apps, state, tree |
| Resource list changed | YES | 2 | When apps connect/disconnect |
| **Resource Templates** | YES | 2 | RFC 6570 URI templates |
| **Prompts** | YES | 2 | 6 guided workflow prompts |
| Prompt list changed | YES | 2 | Dynamic prompts based on connected apps |
| **Completions** | YES | 2 | For resource URIs and prompt args |
| **Logging** | YES | 1 | Structured JSON logs for every operation |
| **Progress** | YES | 3 | VLM, tree scan, wait, script execution |
| **Elicitation (form)** | YES | 3 | Ambiguity, confirmation, clarification |
| **Elicitation (URL)** | YES | 3 | VLM auth redirect |
| **Sampling** | YES | 4 | Screenshot interpretation, action planning |
| Sampling with tools | DEFER | 4+ | Multi-turn tool use in sampling |
| **Roots** | YES | 4 | Find test scripts, config files |
| **Transports** | | | |
| stdio | YES | 1 | Existing, primary |
| Streamable HTTP | YES | 3 | Remote access |
| SSE | YES | 3 | Legacy compatibility |
| WebSocket | SKIP | -- | Streamable HTTP preferred |
| **Tasks** | DEFER | 4+ | Experimental in SDK |
| **Ping** | YES | 1 | Handled by SDK automatically |
| **Cancellation** | YES | 1 | Handled by SDK, respect in long ops |
| **Server info** | YES | 1 | name, version, websiteUrl, icons |

## Appendix B: Wire Protocol Example

A complete example of a tool call with annotations, logging, and progress:

```
--> {"jsonrpc":"2.0","id":1,"method":"tools/list"}
<-- {"jsonrpc":"2.0","id":1,"result":{"tools":[
  {
    "name":"ax_find",
    "title":"Find UI element",
    "description":"Find UI elements...",
    "inputSchema":{...},
    "outputSchema":{...},
    "annotations":{
      "title":"Find UI element",
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
     "params":{"level":"info","logger":"axterminator.tools",
               "data":{"tool":"ax_find","app":"Safari","query":"URL bar",
                        "strategy":"role_title","duration_ms":0.38,"found":true}}}

<-- {"jsonrpc":"2.0","method":"notifications/progress",
     "params":{"progressToken":"p-42","progress":2,"total":2,
               "message":"Element found"}}

<-- {"jsonrpc":"2.0","id":2,"result":{
     "content":[{"type":"text","text":"Found element: AXTextField 'URL bar' at (400, 52)"}],
     "structuredContent":{"found":true,"method":"accessibility","role":"AXTextField",
                          "title":"URL bar","position":[400,52],"size":[600,22],
                          "enabled":true,"value":"https://example.com"}}}
```
