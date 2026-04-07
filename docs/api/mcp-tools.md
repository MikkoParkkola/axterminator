# MCP Tools Reference

AXTerminator's default CLI build exposes 35 MCP tools. Enabling all optional feature families (`audio`, `camera`, `watch`, `spaces`, `docker`, `context`) raises the source-tree surface to 56 tools. The same surface includes 9 static resources by default (15 with all optional resource families), 4 dynamic resource templates, 10 guided prompts, and 4 elicitation scenarios.

For the exact machine-readable surface of a running server, call `tools/list`, `resources/list`, `resources/templates/list`, and `prompts/list`. For a build-specific human-readable catalogue, read `axterminator://guide/quickstart`.

## Default CLI build tool surface (35)

| Group | Tools |
|-------|-------|
| Connection (2) | `ax_is_accessible`, `ax_connect` |
| Finding Elements (5) | `ax_find`, `ax_find_visual`, `ax_list_windows`, `ax_list_apps`, `ax_get_tree` |
| Interaction (11) | `ax_click`, `ax_click_at`, `ax_type`, `ax_set_value`, `ax_get_value`, `ax_get_attributes`, `ax_key_press`, `ax_scroll`, `ax_drag`, `ax_screenshot`, `ax_wait_idle` |
| Verification & Analysis (5) | `ax_assert`, `ax_test_run`, `ax_analyze`, `ax_visual_diff`, `ax_a11y_audit` |
| Workflow & System (12) | `ax_query`, `ax_workflow_create`, `ax_workflow_step`, `ax_workflow_status`, `ax_track_workflow`, `ax_record`, `ax_undo`, `ax_run_script`, `ax_clipboard`, `ax_session_info`, `ax_app_profile`, `ax_system_context` |

## Optional feature-gated additions (21 more tools)

| Feature | Tools added |
|---------|-------------|
| `audio` | `ax_audio_devices`, `ax_listen`, `ax_speak`, `ax_start_capture`, `ax_stop_capture`, `ax_get_transcription`, `ax_capture_status` |
| `camera` | `ax_camera_capture`, `ax_gesture_detect`, `ax_gesture_listen` |
| `watch` | `ax_watch_start`, `ax_watch_stop`, `ax_watch_status` (requires `watch`, which implies `audio` + `camera`) |
| `spaces` | `ax_list_spaces`, `ax_create_space`, `ax_move_to_space`, `ax_switch_space`, `ax_destroy_space` |
| `docker` | `ax_browser_launch`, `ax_browser_stop` |
| `context` | `ax_location` |

## Resources

### Static resources available in the default build (9)

- `axterminator://system/status`
- `axterminator://system/displays`
- `axterminator://apps`
- `axterminator://clipboard`
- `axterminator://workflows`
- `axterminator://guide/quickstart`
- `axterminator://guide/patterns`
- `axterminator://guide/audio`
- `axterminator://profiles`

### Optional static resource additions

- `spaces`: `axterminator://spaces`
- `audio`: `axterminator://audio/devices`, `axterminator://capture/transcription`, `axterminator://capture/screen`, `axterminator://capture/status`
- `camera`: `axterminator://camera/devices`

### Dynamic resource templates (4)

- `axterminator://app/{name}/tree`
- `axterminator://app/{name}/screenshot`
- `axterminator://app/{name}/state`
- `axterminator://app/{name}/query/{question}`

## Guided prompts (10)

| Prompt | Purpose |
|--------|---------|
| `test-app` | Guided testing workflow for a macOS application |
| `navigate-to` | Navigate to a specific screen, dialog, or state |
| `extract-data` | Extract structured data from a running application |
| `accessibility-audit` | Audit an application for accessibility issues |
| `troubleshooting` | Diagnose tool failures and missing elements |
| `app-guide` | Load app-specific playbook guidance and known quirks |
| `automate-workflow` | Create a session-scoped multi-step workflow |
| `debug-ui` | Debug why a UI query cannot find an element |
| `cross-app-copy` | Copy data between two macOS applications |
| `analyze-app` | Run a broad UI analysis of a connected application |

## Elicitation scenarios (4)

| Scenario | Trigger |
|----------|---------|
| Ambiguous app name | Multiple running apps match the name passed to `ax_connect` |
| Element not found | A query fails and the server offers close matches or a better path |
| Destructive action | A click or equivalent action targets destructive UI text |
| Permissions missing | `ax_is_accessible` reports that required permissions are not enabled |

## Tool annotations

All tools carry MCP tool annotations:

| Annotation | Meaning |
|------------|---------|
| `readOnlyHint` | Tool only reads state and has no side effects |
| `destructiveHint` | Tool modifies state or triggers an external action |
| `idempotentHint` | Calling the tool multiple times has the same effect |
| `openWorldHint` | Tool interacts with systems beyond the current app/session |

## Safety and transport notes

- Destructive actions require confirmation or elicitation before execution.
- HTTP transport (`http-transport`) requires bearer-token authentication unless the server is restricted to localhost-only mode.
- `ax_run_script` is intentionally blocked in safe and sandboxed security modes.
