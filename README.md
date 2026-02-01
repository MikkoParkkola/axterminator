# AXTerminator

<div align="center">

[![PyPI version](https://img.shields.io/pypi/v/axterminator?color=00d4ff&style=for-the-badge)](https://pypi.org/project/axterminator/)
[![Python](https://img.shields.io/pypi/pyversions/axterminator?style=for-the-badge)](https://pypi.org/project/axterminator/)
[![macOS](https://img.shields.io/badge/macOS-12%2B-black?style=for-the-badge&logo=apple)](https://github.com/MikkoParkkola/axterminator)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue?style=for-the-badge)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-stable-orange?style=for-the-badge&logo=rust)](https://www.rust-lang.org/)

**World's Most Superior macOS GUI Testing Framework**

*Background testing • ~250µs element access • Self-healing locators • AI vision fallback*

[Quick Start](#quick-start) • [Features](#features) • [API](#api-reference) • [Examples](#examples) • [Docs](https://mikkoparkkola.github.io/axterminator/) • [Benchmarks](https://mikkoparkkola.github.io/axterminator/performance/)

</div>

---

## 🏆 World First: True Background Testing

**Test macOS apps without stealing focus.** Continue working while tests run in the background.

```python
import axterminator as ax

# Tests run IN THE BACKGROUND - your active window stays focused!
calculator = ax.app(name="Calculator")
calculator.find("5").click()    # No focus stealing
calculator.find("+").click()    # Continue your work
calculator.find("3").click()    # Tests just work
calculator.find("=").click()    # Magic ✨
```

## ⚡ Why AXTerminator?

| Capability | AXTerminator | XCUITest | Appium | Others |
|------------|:------------:|:--------:|:------:|:------:|
| **Background Testing** | ✅ **WORLD FIRST** | ❌ | ❌ | ❌ |
| **Element Access** | **~380µs** | ~200ms | ~500ms-2s | **1,321× faster** |
| **Cross-App Testing** | ✅ Native | ❌ | Limited | ❌ |
| **Self-Healing** | 7 strategies | ❌ | Basic | 1-2 |
| **AI Vision Fallback** | ✅ VLM | ❌ | ❌ | ❌ |

## Quick Start

### Installation

```bash
pip install axterminator
```

### Basic Usage

```python
import axterminator as ax

# 1. Check accessibility permissions
if not ax.is_accessibility_enabled():
    print("Enable in System Settings > Privacy > Accessibility")
    exit(1)

# 2. Connect to app
app = ax.app(name="Calculator")

# 3. Interact (background mode by default!)
app.find("7").click()
app.find("+").click()
app.find("3").click()
app.find("=").click()
# Result: 10
```

### CLI Tool

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

## Features

### 🎭 Background Testing
```python
# User continues working while tests run!
for i in range(100):
    app.find("Refresh").click()  # All in background
```

### 🔧 Self-Healing Locators (7 Strategies)
```python
# Element survives UI changes via fallback strategies:
# 1. data_testid  - Developer-set stable IDs
# 2. aria_label   - Accessibility labels
# 3. identifier   - AX identifier
# 4. title        - Element title (fuzzy matching)
# 5. xpath        - Structural path
# 6. position     - Relative position
# 7. visual_vlm   - AI vision fallback
```

### 🤖 AI Vision Detection (VLM)
```python
# When all else fails, use AI to find elements
ax.configure_vlm(backend="mlx")      # Local (fast, private)
ax.configure_vlm(backend="anthropic") # Claude Vision
ax.configure_vlm(backend="openai")    # GPT-4V
ax.configure_vlm(backend="gemini")    # Gemini Vision
ax.configure_vlm(backend="ollama")    # Local Ollama

# Natural language element description
app.find("the blue Save button in the toolbar")
```

### 🧪 pytest Integration
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

### 🎬 Recording Mode
```python
from axterminator import Recorder

recorder = Recorder(app)
recorder.start()
# ... perform actions ...
recorder.stop()

# Generate test code
print(recorder.generate_test())
```

## API Reference

### App Connection

```python
# By name
app = ax.app(name="Safari")

# By bundle ID (recommended)
app = ax.app(bundle_id="com.apple.Safari")

# By PID
app = ax.app(pid=12345)

# Launch if not running
app = ax.app(name="Notes", launch=True)
```

### Finding Elements

```python
# By text/title
button = app.find("Save")

# With timeout
button = app.find("Save", timeout_ms=5000)

# By role
text_field = app.find("", role="AXTextField")

# Find all matching
buttons = app.find_all("role:AXButton")
```

### Actions

```python
# Clicks (background by default)
element.click()
element.double_click()
element.right_click()

# Focused mode (for text input)
element.click(mode=ax.FOCUS)
element.type_text("Hello World!")

# Get properties
print(element.title)
print(element.value)
print(element.role)
```

### Synchronization

```python
from axterminator.sync import wait_for_idle, wait_for_element

# Wait for app to settle
wait_for_idle(app, timeout_ms=5000)

# Wait for element to appear
button = wait_for_element(app, "Done", timeout_ms=3000)
```

## Examples

See [`examples/`](examples/) for real-world automation:

| Script | Description |
|--------|-------------|
| `basic_usage.py` | Calculator automation |
| `system_preferences.py` | System Settings navigation |
| `finder_automation.py` | Finder file operations |
| `notes_app.py` | Notes app automation |
| `textedit_automation.py` | Document creation |
| `pytest_example.py` | pytest integration |
| `self_healing_locators.py` | Locator strategies |
| `vlm_visual_detection.py` | VLM fallback demo |

## Browser Extension

Record browser interactions → generate axterminator code:

1. Load `browser-extension/` in Chrome (Developer mode)
2. Click extension → Start Recording
3. Interact with web pages
4. Copy generated Python code

## Installation Options

```bash
# Basic
pip install axterminator

# With VLM backends
pip install axterminator[vlm]           # Local MLX
pip install axterminator[vlm-anthropic] # Claude Vision
pip install axterminator[vlm-openai]    # GPT-4V
pip install axterminator[vlm-gemini]    # Gemini Vision
pip install axterminator[vlm-ollama]    # Ollama
pip install axterminator[vlm-all]       # All backends
```

## Requirements

- **macOS 12+** (Monterey or later)
- **Python 3.9+**
- **Accessibility permissions** granted to terminal/IDE

## Building from Source

```bash
git clone https://github.com/MikkoParkkola/axterminator
cd axterminator

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build
pip install maturin
maturin develop

# Test
pytest python/tests/
```

## Performance

| Operation | Time | vs Appium |
|-----------|------|-----------|
| Single attribute | **54µs** | - |
| Element access | **379µs** | **1,321× faster** |
| Perform action | **20µs** | - |
| Find element | ~10-50ms | 100× faster |

*Benchmarked on M1 MacBook Pro, macOS 14.2. [Full benchmarks →](https://mikkoparkkola.github.io/axterminator/performance/)*

## License

[MIT](LICENSE-MIT) OR [Apache-2.0](LICENSE-APACHE)

## Contributing

Contributions welcome! See [docs/](docs/) for architecture details.

---

<div align="center">

**Built with 🦀 Rust + 🐍 Python**

[Report Bug](https://github.com/MikkoParkkola/axterminator/issues) • [Request Feature](https://github.com/MikkoParkkola/axterminator/issues)

</div>
