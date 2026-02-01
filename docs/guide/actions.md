# Actions

AXTerminator supports two action modes: **Background** (default) and **Focus**.

## Action Modes

### Background Mode (Default)

Clicks happen without stealing focus from your current window:

```python
import axterminator as ax

app = ax.app(name="Calculator")
element = app.find("5")

# Background click (default)
element.click()

# Explicit background mode
element.click(mode=ax.BACKGROUND)
```

!!! success "World First Feature"
    AXTerminator is the first framework to support true background testing on macOS. Your workflow stays uninterrupted while tests run.

### Focus Mode

Brings the application to the foreground:

```python
# Focus click - brings app forward
element.click(mode=ax.FOCUS)

# Required for text input
element.type_text("Hello", mode=ax.FOCUS)
```

## Click Actions

```python
# Single click
element.click()

# Double click
element.double_click()

# Right click (context menu)
element.right_click()
```

## Text Input

```python
# Type text (requires FOCUS mode)
text_field = app.find("role:AXTextField")
text_field.click(mode=ax.FOCUS)
text_field.type_text("Hello World!")

# Set value directly (where supported)
text_field.set_value("New value")
```

## Keyboard Actions

```python
# Press specific keys
element.press_key("Return")
element.press_key("Tab")
element.press_key("Escape")

# Key combinations
element.press_key("Command+S")
element.press_key("Command+Shift+N")
```

## Screenshots

```python
# App screenshot
png_data = app.screenshot()

# Element screenshot
element_png = element.screenshot()

# Save to file
with open("screenshot.png", "wb") as f:
    f.write(png_data)
```

## Action Timing

```python
import time

# Add delays between actions if needed
element1.click()
time.sleep(0.1)
element2.click()

# Or use wait_for_idle
from axterminator.sync import wait_for_idle
element1.click()
wait_for_idle(app, timeout_ms=1000)
element2.click()
```
