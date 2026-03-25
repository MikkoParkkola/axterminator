# axterminator Quickstart

axterminator is a macOS accessibility automation server that exposes the macOS
Accessibility API and related system services as MCP tools, resources, and prompts.

---

## Tool Catalogue

### Connection
| Tool | Purpose |
|------|---------|
| `ax_is_accessible` | Check whether macOS accessibility permissions are granted |
| `ax_connect` | Connect to a running application by name or PID |

### Finding Elements
| Tool | Purpose |
|------|---------|
| `ax_find` | Search the accessibility tree by role, label, or text |
| `ax_find_visual` | Locate an element using a screenshot + vision model |
| `ax_list_windows` | List all windows of a connected application |
| `ax_list_apps` | List all running applications |
| `ax_get_tree` | Dump the full accessibility element tree |

### Interaction
| Tool | Purpose |
|------|---------|
| `ax_click` | Click an element found via `ax_find` |
| `ax_click_at` | Click at absolute screen coordinates |
| `ax_type` | Type text into the focused or specified element |
| `ax_set_value` | Set the value of a form element programmatically |
| `ax_get_value` | Read the current value of an element |
| `ax_get_attributes` | Read all accessibility attributes of an element |
| `ax_key_press` | Send a keyboard shortcut or key combination |
| `ax_scroll` | Scroll a scrollable element or region |
| `ax_drag` | Drag from one point to another |
| `ax_screenshot` | Capture a screenshot of a connected application |
| `ax_wait_idle` | Wait until the application UI stops changing |

### Audio
| Tool | Purpose |
|------|---------|
| `ax_audio_devices` | List available CoreAudio input/output devices |
| `ax_listen` | Transcribe a short audio clip from the microphone |
| `ax_speak` | Synthesise and play text via text-to-speech |
| `ax_start_capture` | Start a continuous audio + screen capture session |
| `ax_stop_capture` | Stop the active capture session |
| `ax_get_transcription` | Read accumulated transcription segments |
| `ax_capture_status` | Check health and buffer levels of the capture session |

### Workflow & Scripting
| Tool | Purpose |
|------|---------|
| `ax_workflow_create` | Define a named multi-step automation workflow |
| `ax_workflow_step` | Execute the next step in a workflow |
| `ax_workflow_status` | Query the state of a running workflow |
| `ax_run_script` | Run an AppleScript or shell command |
| `ax_record` | Record user interactions for later replay |
| `ax_undo` | Undo the most recent recorded interaction |
| `ax_track_workflow` | Track a cross-app operation for pattern discovery |
| `ax_app_profile` | Retrieve capabilities and selectors for a known Electron app |
| `ax_session_info` | Describe the current MCP session and connected apps |
| `ax_query` | Natural-language query against the live accessibility scene |
| `ax_clipboard` | Read or write the system clipboard |
| `ax_system_context` | Retrieve ambient system context (locale, display, OS version) |
| `ax_location` | Read the current geographic location (if permitted) |

### Testing & Analysis
| Tool | Purpose |
|------|---------|
| `ax_test_run` | Execute a UI test defined with `ax_workflow_create` |
| `ax_assert` | Assert that an accessibility condition holds (fails the test if not) |
| `ax_analyze` | Analyse a UI element or scene for semantic information |
| `ax_visual_diff` | Compare two screenshots and return a diff description |
| `ax_a11y_audit` | Run an accessibility compliance audit on a connected app |

---

## Core Workflow Pattern

Every automation follows the same four-phase loop:

```
ax_connect  →  ax_find  →  ax_click / ax_type / ax_get_value  →  ax_assert / ax_get_value
 CONNECT         FIND              ACT                                  VERIFY
```

1. **Connect** — identify the target application with `ax_connect`.
2. **Find** — locate the element of interest with `ax_find` (or `ax_find_visual`
   when the accessibility tree is sparse).
3. **Act** — perform the interaction: click, type, set a value, press a key.
4. **Verify** — confirm the expected outcome with `ax_get_value`, `ax_assert`,
   or `ax_screenshot`.

---

## Key Concepts

### Accessibility Tree
macOS exposes every application's UI as a tree of **AXUIElement** nodes.
Each node has a *role*, optional *title*/*label*, and a set of typed *attributes*.
`ax_get_tree` dumps this structure; `ax_find` searches it.

### Roles
A role describes the semantic type of an element.
Common roles: `AXButton`, `AXTextField`, `AXStaticText`, `AXList`,
`AXTable`, `AXGroup`, `AXWindow`, `AXCheckBox`, `AXRadioButton`,
`AXComboBox`, `AXMenuItem`, `AXToolbar`.

### Attributes
Each element exposes named attributes, for example:
- `AXTitle` — visible label
- `AXValue` — current value (text, toggle state, slider position)
- `AXEnabled` — whether the element accepts interaction
- `AXFocused` — whether the element currently has keyboard focus
- `AXFrame` — bounding rectangle in screen coordinates

`ax_get_attributes` returns all attributes for a given element reference.

### AXUIElement
An `AXUIElement` is the opaque handle used by the macOS Accessibility API
to refer to a specific UI node. axterminator tools accept element handles
returned by `ax_find` and pass them to interaction tools
(`ax_click`, `ax_type`, `ax_get_value`, etc.).
