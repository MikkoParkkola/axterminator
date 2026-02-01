# Quick Start

## Your First Test

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

# Launch if not running
app = ax.app(name="Notes", launch=True)
```

## Finding Elements

```python
# By text/title
button = app.find("Save")

# With timeout
button = app.find("Save", timeout_ms=5000)

# Find all matching
buttons = app.find_all("role:AXButton")
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

## CLI Tool

```bash
# Check permissions
axterminator check

# Find elements
axterminator find Calculator "5"

# Click elements
axterminator click Calculator "+"

# Record interactions
axterminator record Calculator
```
