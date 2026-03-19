# AXTerminator

<div align="center">

[![PyPI version](https://img.shields.io/pypi/v/axterminator?color=00d4ff&style=for-the-badge)](https://pypi.org/project/axterminator/)
[![Downloads](https://img.shields.io/pypi/dm/axterminator?style=for-the-badge&color=00d4ff)](https://pypi.org/project/axterminator/)
[![Tests](https://img.shields.io/github/actions/workflow/status/MikkoParkkola/axterminator/ci.yml?style=for-the-badge&label=tests)](https://github.com/MikkoParkkola/axterminator/actions)
[![Python](https://img.shields.io/pypi/pyversions/axterminator?style=for-the-badge)](https://pypi.org/project/axterminator/)
[![macOS](https://img.shields.io/badge/macOS-12%2B-black?style=for-the-badge&logo=apple)](https://github.com/MikkoParkkola/axterminator)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue?style=for-the-badge)](LICENSE-MIT)
[![Rust](https://img.shields.io/badge/rust-stable-orange?style=for-the-badge&logo=rust)](https://www.rust-lang.org/)
[![Discussions](https://img.shields.io/github/discussions/MikkoParkkola/axterminator?style=for-the-badge&color=blue)](https://github.com/MikkoParkkola/axterminator/discussions)

**macOS GUI testing framework with background testing, sub-millisecond element access, and self-healing locators.**

[Quick Start](#quick-start) · [Features](#features) · [API](#api-reference) · [Examples](#examples) · [Docs](https://mikkoparkkola.github.io/axterminator/) · [Benchmarks](https://mikkoparkkola.github.io/axterminator/performance/)

</div>

---

## Background Testing

**Test macOS apps without stealing focus.** AXTerminator uses the macOS Accessibility API (`AXUIElement`) to interact with applications in the background. Your active window stays focused while tests run behind it.

No other macOS GUI testing framework offers this -- XCUITest, Appium, PyAutoGUI, and Maestro all require the application under test to be in the foreground.

```python
import axterminator as ax

# Tests run in the background -- your active window stays focused
calculator = ax.app(name="Calculator")
calculator.find("5").click()    # No focus stealing
calculator.find("+").click()    # Continue your work
calculator.find("3").click()    # Tests just work
calculator.find("=").click()
```

## Quick Start

### 1. Install

```bash
pip install axterminator
```

### 2. Grant accessibility permissions

Open **System Settings > Privacy & Security > Accessibility** and add your terminal app (Terminal, iTerm2, VS Code, etc.).

### 3. Run your first test

```python
import axterminator as ax

# Check permissions
if not ax.is_accessibility_enabled():
    print("Enable in System Settings > Privacy & Security > Accessibility")
    exit(1)

# Connect to any running app
app = ax.app(name="Calculator")

# Interact -- background mode by default
app.find("7").click()
app.find("+").click()
app.find("3").click()
app.find("=").click()
```

That's it. Three lines to connect and interact.

## Why AXTerminator?

| Capability | AXTerminator | XCUITest | Appium | PyAutoGUI | Maestro |
|------------|:------------:|:--------:|:------:|:---------:|:-------:|
| **Background testing** | Yes | No | No | No | No |
| **Element access** | **379 us** | ~200 ms | ~500 ms | ~100 ms | ~300 ms |
| **Cross-app testing** | Yes | No | Limited | Yes | Limited |
| **Self-healing** | 7 strategies | No | Basic | No | Yes |
| **AI vision fallback** | Yes | No | No | No | No |
| **Python API** | Yes | No | Yes | Yes | No |
| **No Xcode required** | Yes | No | No | Yes | Yes |

*Element access benchmarked on M1 MacBook Pro, macOS 14.2. See [full benchmarks](https://mikkoparkkola.github.io/axterminator/performance/).*

## Features

### Background Testing
```python
# User continues working while tests run
for i in range(100):
    app.find("Refresh").click()  # All in background
```

### Self-Healing Locators (7 Strategies)
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

### AI Vision Detection (VLM)
```python
# When all else fails, use AI to find elements visually
ax.configure_vlm(backend="mlx")      # Local (fast, private)
ax.configure_vlm(backend="anthropic") # Claude Vision
ax.configure_vlm(backend="openai")    # OpenAI Vision
ax.configure_vlm(backend="gemini")    # Gemini Vision
ax.configure_vlm(backend="ollama")    # Local Ollama

# Natural language element description
app.find("the blue Save button in the toolbar")
```

### pytest Integration
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

### Recording Mode
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

# By bundle ID (recommended -- locale-independent)
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

## Performance

Measured on Apple M1 MacBook Pro, macOS 14.2, using Criterion benchmarks against Finder.app:

| Operation | Time | Method |
|-----------|------|--------|
| Single attribute read | **54 us** | Criterion (`get_ax_role`) |
| Element access (window -> child) | **379 us** | Criterion (`search_first_button`) |
| Perform action | **20 us** | Criterion (`perform_action_overhead`) |
| Find element (Python, incl. overhead) | ~0.5-1 ms | `time.perf_counter()` loop |

The speedup over Appium (~500 ms per element access) comes from eliminating HTTP/WebDriver/JSON overhead. AXTerminator calls the Accessibility API directly: `Python -> PyO3 FFI -> Rust -> AXUIElement`.

*Reproduce: `cargo bench` or compile and run `benches/bench_quick.rs`. See [full benchmarks](https://mikkoparkkola.github.io/axterminator/performance/).*

## Known Limitations

Background testing works for most interactions via `AXUIElementPerformAction()`, but some operations still require or steal window focus:

| Operation | Background? | Why |
|-----------|:-----------:|-----|
| Click, press, pick | Yes | AX actions work on unfocused windows |
| Read attributes/values | Yes | AX queries don't need focus |
| Screenshots | Yes | `CGWindowListCreateImage` captures any window |
| **Text input** | **Partial** | Some apps accept AX value setting; others require focused text field + CGEvent keystrokes |
| **Drag operations** | **No** | Mouse events are global — requires cursor control |
| **System dialogs** | **No** | Authentication prompts and file pickers always grab focus |
| **Some Electron apps** | **Partial** | May not respond to background AX actions; use CDP fallback |

**Workarounds:**
- For text input: prefer `ax_set_value` (AX-based) over `ax_type` (CGEvent-based) when possible
- For Electron apps: enable CDP integration (`ax_connect` with bundle ID detection)
- For unavoidable focus stealing: use virtual desktop isolation (planned — [#23](https://github.com/MikkoParkkola/axterminator/issues/23))

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

Record browser interactions and generate axterminator code:

1. Load `browser-extension/` in Chrome (Developer mode)
2. Click extension icon, then Start Recording
3. Interact with web pages
4. Copy generated Python code

## Installation Options

```bash
# Basic
pip install axterminator

# With VLM backends
pip install axterminator[vlm]           # Local MLX
pip install axterminator[vlm-anthropic] # Claude Vision
pip install axterminator[vlm-openai]    # OpenAI Vision
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

# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build
pip install maturin
maturin develop

# Run tests
cargo test
pytest python/tests/
```

## Community

- [GitHub Discussions](https://github.com/MikkoParkkola/axterminator/discussions) -- Questions, feature requests, and show-and-tell
- [Issue Tracker](https://github.com/MikkoParkkola/axterminator/issues) -- Bug reports
- [Contributing Guide](CONTRIBUTING.md) -- How to contribute

## Acknowledgements

AXTerminator was inspired by [Terminator](https://github.com/mediar-ai/terminator) by [mediar-ai](https://github.com/mediar-ai), which pioneered accessible desktop GUI automation on Windows. AXTerminator brings similar capabilities to macOS, with the addition of background testing -- the ability to test applications without stealing window focus -- made possible by leveraging undocumented behavior of Apple's Accessibility API.

## License

[MIT](LICENSE-MIT) OR [Apache-2.0](LICENSE-APACHE)

---

<div align="center">

Built with Rust + Python | [Report Bug](https://github.com/MikkoParkkola/axterminator/issues) · [Request Feature](https://github.com/MikkoParkkola/axterminator/discussions/categories/ideas)

</div>
