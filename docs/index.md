# AXTerminator

<div align="center">

**World's Most Superior macOS GUI Testing Framework**

*Background testing • ~250µs element access • Self-healing locators • AI vision fallback*

[![PyPI](https://img.shields.io/pypi/v/axterminator?color=00d4ff)](https://pypi.org/project/axterminator/)
[![Python](https://img.shields.io/pypi/pyversions/axterminator)](https://pypi.org/project/axterminator/)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue)](https://github.com/MikkoParkkola/axterminator)

</div>

**World's Most Superior macOS GUI Testing Framework**

AXTerminator enables background GUI testing on macOS without stealing focus, with ~250µs element access times and self-healing locators.

## Features

- **🎯 Background Testing** - Run tests without stealing focus or interrupting your work
- **⚡ Ultra-Fast** - ~250µs element access (4000× faster than typical UI frameworks)
- **🔧 Self-Healing Locators** - 7-strategy healing system survives UI changes
- **🧠 Visual VLM Detection** - AI-powered element detection as ultimate fallback
- **🦀 Rust Core** - Native performance with Python bindings

## Quick Start

```bash
pip install axterminator
```

```python
import axterminator

# Check accessibility permissions
if not axterminator.is_accessibility_enabled():
    print("Enable accessibility in System Preferences")
    exit(1)

# Connect to app and interact
app = axterminator.app(name="Calculator")
app.find("5").click()
app.find("+").click()
app.find("3").click()
app.find("=").click()
```

## Installation

### Basic

```bash
pip install axterminator
```

### With VLM Support

```bash
# Local MLX (recommended - fast, private)
pip install axterminator[vlm]

# Cloud APIs
pip install axterminator[vlm-anthropic]  # Claude Vision
pip install axterminator[vlm-openai]     # GPT-4o
pip install axterminator[vlm-gemini]     # Gemini Vision

# All backends
pip install axterminator[vlm-all]
```

## Requirements

- macOS 12+ (Monterey or later)
- Python 3.9+
- Accessibility permissions granted to your terminal/IDE

## API Reference

### Core Functions

#### `axterminator.app(name=None, bundle_id=None, pid=None)`

Connect to a running application.

```python
# By name
app = axterminator.app(name="Safari")

# By bundle ID
app = axterminator.app(bundle_id="com.apple.Safari")

# By PID
app = axterminator.app(pid=12345)
```

#### `axterminator.is_accessibility_enabled()`

Check if accessibility permissions are granted.

```python
if not axterminator.is_accessibility_enabled():
    print("Grant accessibility permissions")
```

### AXApp Class

#### `app.find(query, timeout_ms=5000)`

Find an element by query.

```python
# By title/label
button = app.find("Save")

# By role
text_field = app.find("role:AXTextField")

# Combined query
save_btn = app.find("role:AXButton title:Save")
```

#### `app.find_all(query)`

Find all matching elements.

```python
buttons = app.find_all("role:AXButton")
```

#### `app.pid`

Get the process ID.

### AXElement Class

#### `element.click(mode=BACKGROUND)`

Click the element.

```python
# Background click (default) - won't steal focus
element.click()

# Foreground click - brings app to front
element.click(mode=axterminator.FOCUS)
```

#### `element.type_text(text)`

Type text into the element.

```python
text_field.type_text("Hello, World!")
```

#### `element.value`

Get the element's value attribute.

#### `element.title`

Get the element's title.

#### `element.role`

Get the element's accessibility role.

### VLM Configuration

#### `axterminator.configure_vlm(backend, model=None, api_key=None)`

Configure visual element detection.

```python
# Local MLX (default)
axterminator.configure_vlm(backend="mlx")

# Cloud APIs
axterminator.configure_vlm(backend="anthropic", api_key="sk-...")
axterminator.configure_vlm(backend="openai", api_key="sk-...")
axterminator.configure_vlm(backend="gemini", api_key="...")
```

## Self-Healing Locators

When an element is found, AXTerminator stores multiple locator strategies:

1. **data_testid** - Custom test identifiers (most stable)
2. **aria_label** - ARIA accessibility labels
3. **identifier** - macOS accessibility identifiers
4. **title** - Element title/text
5. **xpath** - Structural path in accessibility tree
6. **position** - Relative position within parent
7. **visual_vlm** - AI-powered visual detection

If the primary locator fails, the system automatically tries alternatives.

## Synchronization

```python
from axterminator.sync import wait_for_idle, wait_for_element

# Wait for app to settle
wait_for_idle(app, timeout_ms=5000)

# Wait for element to appear
button = wait_for_element(app, "Done", timeout_ms=3000)
```

## Performance

| Operation | Time |
|-----------|------|
| Element access | ~250µs |
| Click | ~1ms |
| Type text | ~5ms |
| Find element | ~10-50ms |

## CLI Tool

```bash
# Check accessibility permissions
axterminator check

# List running apps
axterminator list-apps

# Find an element
axterminator find Calculator "5"

# Click an element
axterminator click Calculator "+"

# Type text
axterminator type Calculator "textfield" "123"

# Record interactions
axterminator record Calculator
```

## pytest Integration

```python
import pytest

@pytest.mark.ax_requires_app("Calculator")
def test_addition(ax_app, ax_wait):
    app = ax_app("Calculator")
    app.find("7").click()
    app.find("+").click()
    app.find("3").click()
    app.find("=").click()
    ax_wait(0.1)
```

Available fixtures: `ax_app`, `ax_wait`, `ax_calculator`, `ax_finder`

Markers: `@pytest.mark.ax_background`, `@pytest.mark.ax_requires_app(name)`, `@pytest.mark.ax_slow`

## Browser Extension

Record browser interactions and generate axterminator code:

1. Install from `browser-extension/` folder
2. Click extension icon → Start Recording
3. Interact with web pages
4. Stop and copy generated Python code

## Examples

See the `examples/` directory:

- `basic_usage.py` - Calculator automation
- `system_preferences.py` - System Settings navigation
- `finder_automation.py` - Finder file operations
- `notes_app.py` - Notes app automation
- `textedit_automation.py` - Document creation
- `pytest_example.py` - pytest integration
- `self_healing_locators.py` - Locator strategies
- `vlm_visual_detection.py` - VLM fallback

## License

MIT OR Apache-2.0
