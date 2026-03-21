# Element API

## AXApp

Represents a connected application. Create via `ax.app()`.

### Properties

| Property | Type | Description |
|----------|------|-------------|
| `pid` | `int` | Process ID |
| `bundle_id` | `str \| None` | Bundle identifier (e.g., `"com.apple.Safari"`) |

### Methods

#### `find(query, timeout_ms=None)`

Find a single element.

```python
element = app.find("Save", timeout_ms=5000)
```

**Parameters:**

| Name | Type | Description |
|------|------|-------------|
| `query` | `str` | Search query (title, `role:X`, `identifier:X`, or `role:X title:Y`) |
| `timeout_ms` | `int \| None` | Timeout in milliseconds (retry until found or timeout) |

**Returns:** `AXElement`

**Raises:** `RuntimeError` if not found

---

#### `find_by_role(role, title=None, identifier=None, label=None)`

Find an element by accessibility role with optional attribute filters.

```python
button = app.find_by_role("AXButton", title="Save")
```

**Returns:** `AXElement`

---

#### `wait_for_element(query, timeout_ms=5000)`

Poll until a matching element appears or timeout expires.

```python
button = app.wait_for_element("Done", timeout_ms=5000)
```

**Returns:** `AXElement`

**Raises:** `RuntimeError` if element did not appear within timeout

---

#### `wait_for_idle(timeout_ms=5000)`

Wait for the application to become idle using EspressoMac SDK or heuristic fallback.

```python
if app.wait_for_idle(timeout_ms=3000):
    print("App is idle")
```

**Returns:** `bool`

---

#### `is_idle()`

Non-blocking check if the application is currently idle.

**Returns:** `bool`

---

#### `main_window()`

Get the main window.

```python
window = app.main_window()
```

**Returns:** `AXElement`

---

#### `windows()`

Get all windows of the application.

```python
windows = app.windows()
```

**Returns:** `list[AXElement]`

---

#### `screenshot()`

Capture application screenshot.

```python
png_bytes = app.screenshot()
```

**Returns:** `bytes` (PNG data)

---

#### `is_running()`

Check if application is still running.

**Returns:** `bool`

---

#### `terminate()`

Send SIGTERM to the application process.

---

## AXElement

Represents a UI element (`AXUIElementRef` wrapper).

### Properties

| Property | Type | Description |
|----------|------|-------------|
| `role()` | `str \| None` | Accessibility role (e.g., `"AXButton"`) |
| `title()` | `str \| None` | Element title/label |
| `value()` | `str \| None` | Current value |
| `description()` | `str \| None` | Accessibility description |
| `label()` | `str \| None` | Accessibility label |
| `identifier()` | `str \| None` | Accessibility identifier |
| `enabled()` | `bool` | Is element enabled |
| `focused()` | `bool` | Is element focused |
| `exists()` | `bool` | Is element still present in UI |
| `bounds()` | `tuple \| None` | `(x, y, width, height)` in screen points |

### Actions

#### `click(mode=None)`

Click the element. Default mode is `BACKGROUND`.

```python
element.click()  # Background (default)
element.click(mode=ax.FOCUS)  # Focus mode
```

---

#### `double_click(mode=None)`

Double-click the element (two presses with 50ms gap).

```python
element.double_click()
```

---

#### `right_click(mode=None)`

Right-click (triggers `AXShowMenu` to open contextual menu).

```python
element.right_click()
```

---

#### `type_text(text, mode=None)`

Type text into element. Defaults to `FOCUS` mode (text input requires focus).

```python
element.type_text("Hello World!")
```

!!! warning
    Text input requires focus mode. Passing `BACKGROUND` explicitly will raise `RuntimeError`.

---

#### `set_value(value)`

Set element value directly via `AXValue` attribute. Works in background mode.

```python
element.set_value("New value")
```

---

### Navigation

#### `find(query, timeout_ms=None)`

Find a descendant element within this element.

```python
button = toolbar.find("Save")
```

---

#### `screenshot()`

Capture element screenshot (bounding rect only).

```python
png_bytes = element.screenshot()
```

**Returns:** `bytes` (PNG data)
