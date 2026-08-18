# Actions

AXTerminator supports two action modes: **background** (default) and **focus**.
Both are reached through the MCP tools; the CLI exposes a subset.

## Action Modes

### Background Mode (Default)

Clicks happen without stealing focus from your current window:

```json
{
  "name": "ax_click",
  "arguments": { "app": "Calculator", "query": "5" }
}
```

!!! success "Background Testing"
    AXTerminator uses `AXUIElementPerformAction` on unfocused windows. Your workflow stays uninterrupted while tests run.

### Focus Mode

Brings the application to the foreground. Text input requires it: `ax_type`
with `"mode": "background"` returns an error rather than typing into a window
that cannot receive keystrokes.

```json
{
  "name": "ax_type",
  "arguments": { "app": "TextEdit", "query": "role:AXTextArea", "text": "Hello", "mode": "focus" }
}
```

## Click Actions

`ax_click` takes a `click_type` of `single` (default), `double`, or `right`.
A right click triggers `AXShowMenu` to open the contextual menu.

```json
{ "name": "ax_click", "arguments": { "app": "Finder", "query": "Documents", "click_type": "double" } }
```

When the target element's title or description contains a destructive keyword
(delete, remove, erase, quit, close, format, reset, clear, wipe, destroy,
terminate, uninstall, revoke), the tool returns an error instead of clicking.
Re-send the call with `"confirm": true` once you have verified the action is
intended.

## Text Input

```json
{ "name": "ax_type", "arguments": { "app": "TextEdit", "query": "role:AXTextArea", "text": "Hello World!", "mode": "focus" } }
```

To write a value without simulating keystrokes, use `ax_set_value`, which sets
the `AXValue` attribute directly and works in the background:

```json
{ "name": "ax_set_value", "arguments": { "app": "Safari", "query": "role:AXTextField", "value": "New value" } }
```

## Keyboard Actions

`ax_key_press` posts events to the application's PID with `CGEventPostToPid`,
so it does not steal focus. Modifiers are combined with `+`:

```json
{ "name": "ax_key_press", "arguments": { "app": "TextEdit", "keys": "cmd+s" } }
```

Accepted keys include `enter`, `return`, `tab`, `escape`, `space`, `delete`,
the arrow keys, `f1`-`f20`, `a`-`z`, `0`-`9`, and combinations such as
`ctrl+c`, `opt+tab`, `shift+cmd+p`.

## Screenshots

`ax_screenshot` returns base64-encoded PNG data. Omit `query` for the whole
app, or pass one to crop to a single element:

```json
{ "name": "ax_screenshot", "arguments": { "app": "Safari" } }
```

From the CLI, `--output` writes the PNG to a file instead of printing base64:

```bash
axterminator screenshot --app Safari --output shot.png
```

## Action Timing

Rather than sleeping between actions, wait for the app to settle:

```json
{ "name": "ax_wait_idle", "arguments": { "app": "Safari", "timeout_ms": 1000 } }
```
