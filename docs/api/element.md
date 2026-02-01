# Element API

## AXApp

Represents a connected application.

### Properties

| Property | Type | Description |
|----------|------|-------------|
| `pid` | `int` | Process ID |
| `name` | `str` | Application name |
| `bundle_id` | `str` | Bundle identifier |

### Methods

#### `find()`

Find a single element.

```python
element = app.find("Save", timeout_ms=5000)
```

**Parameters:**

| Name | Type | Description |
|------|------|-------------|
| `query` | `str` | Search query (title, identifier:X, role:X) |
| `timeout_ms` | `int` | Timeout in milliseconds (default: 5000) |

**Returns:** `AXElement`

**Raises:** `RuntimeError` if not found

---

#### `find_all()`

Find all matching elements.

```python
buttons = app.find_all("role:AXButton")
```

**Returns:** `list[AXElement]`

---

#### `main_window()`

Get the main window.

```python
window = app.main_window()
```

**Returns:** `AXElement`

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

```python
if app.is_running():
    print("Still alive")
```

**Returns:** `bool`

---

## AXElement

Represents a UI element.

### Properties

| Property | Type | Description |
|----------|------|-------------|
| `title` | `str` | Element title/label |
| `role` | `str` | Accessibility role (AXButton, etc.) |
| `value` | `str` | Current value |
| `identifier` | `str` | Accessibility identifier |
| `description` | `str` | Accessibility description |
| `enabled` | `bool` | Is element enabled |
| `focused` | `bool` | Is element focused |
| `bounds` | `tuple` | (x, y, width, height) |

### Actions

#### `click()`

Click the element.

```python
element.click()  # Background (default)
element.click(mode=ax.FOCUS)  # Focus mode
```

---

#### `double_click()`

Double-click the element.

```python
element.double_click()
```

---

#### `right_click()`

Right-click (context menu).

```python
element.right_click()
```

---

#### `type_text()`

Type text into element.

```python
element.type_text("Hello World!", mode=ax.FOCUS)
```

!!! warning
    Requires `mode=ax.FOCUS` for text input.

---

#### `set_value()`

Set element value directly.

```python
element.set_value("New value")
```

---

#### `press_key()`

Press a keyboard key.

```python
element.press_key("Return")
element.press_key("Command+S")
```

---

### Navigation

#### `children()`

Get child elements.

```python
for child in element.children():
    print(child.role, child.title)
```

**Returns:** `list[AXElement]`

---

#### `parent()`

Get parent element.

```python
parent = element.parent()
```

**Returns:** `AXElement`

---

#### `find()`

Find within this element.

```python
button = toolbar.find("Save")
```

---

#### `screenshot()`

Capture element screenshot.

```python
png_bytes = element.screenshot()
```

**Returns:** `bytes` (PNG data)
