# AXTerminator: World's First Background GUI Testing for macOS

**TL;DR** -- AXTerminator lets you run GUI tests on macOS applications without stealing window focus. Tests execute in the background while you keep working. It is 1,321x faster than Appium, ships as a Python package, and self-heals broken locators with 7 strategies.

---

## The Problem

Every macOS GUI testing tool steals focus. XCUITest, Appium, PyAutoGUI -- they all bring the application under test to the foreground, lock your screen, and prevent you from doing anything else until the test suite finishes. For a 20-minute test suite, that means 20 minutes of watching your machine do things without you.

CI runners partially solve this, but local development iteration still requires surrendering your desktop. And CI Macs are expensive.

## The Discovery

macOS has an Accessibility API (`AXUIElement`) that every testing framework uses. But every framework also calls `AXUIElementPerformAction` only after activating the target application and raising its window. It turns out this activation step is unnecessary. The API works on unfocused windows.

AXTerminator exploits this undocumented capability. Button clicks, menu selections, value reads -- they all work while the application sits in the background.

## What You Get

### Background Testing (World First)

```python
import axterminator as ax

# Your IDE stays focused. Tests run behind it.
safari = ax.app(bundle_id="com.apple.Safari")
safari.find("URL").click()
safari.find("URL").set_value("https://example.com")
safari.find("Go").click()

# Check results -- still in the background
title = safari.main_window().title()
assert "Example" in title
```

No window flashing. No focus stealing. Your cursor stays where it is.

### 379 Microsecond Element Access

AXTerminator talks directly to the Accessibility API through Rust FFI. No HTTP server. No JSON serialization. No WebDriver protocol. The entire path from Python call to AX API response is a single function call through PyO3.

| Framework | Element Access | Relative |
|-----------|---------------|----------|
| **AXTerminator** | **379 us** | 1x |
| PyAutoGUI | ~100 ms | 264x slower |
| XCUITest | ~200 ms | 528x slower |
| Maestro | ~300 ms | 792x slower |
| Appium | ~500 ms | 1,321x slower |

Access 1,000 elements in 380 ms -- the time it takes Appium to access one.

### 7-Strategy Self-Healing Locators

When a UI element moves, changes label, or gets restructured, AXTerminator tries seven strategies in order to relocate it:

1. **data-testid** -- stable test identifiers set by developers
2. **aria-label** -- accessibility labels
3. **identifier** -- AXIdentifier attribute
4. **title** -- window and button titles
5. **xpath** -- structural tree path
6. **position** -- spatial location heuristic
7. **visual-vlm** -- AI vision model fallback (Claude, GPT-4o, Gemini, Ollama)

The healing system runs in under 100 ms by default. Successful heals are cached so subsequent lookups skip failed strategies.

```python
import axterminator as ax

config = ax.HealingConfig(
    strategies=["data_testid", "aria_label", "title"],
    max_heal_time_ms=200,
    cache_healed=True,
)
ax.configure_healing(config)
```

### Python-Native API

Install from PyPI, import, and go:

```bash
pip install axterminator
```

```python
import axterminator as ax

# Connect by bundle ID (locale-independent, recommended)
app = ax.app(bundle_id="com.apple.TextEdit")

# Find elements by text, role, or XPath
save_btn = app.find("Save")
save_btn = app.find("role:AXButton title:Save")
save_btn = app.find("//AXButton[@AXTitle='Save']")

# Background interactions
save_btn.click()                           # No focus steal
app.find("filename").type_text("doc.txt")  # Focus mode for typing
app.find("filename").set_value("doc.txt")  # Background mode for value

# Wait for UI state
app.wait_for_element("Success", timeout_ms=3000)
app.wait_for_idle(timeout_ms=5000)

# Screenshots (element or full window)
png_data = save_btn.screenshot()
window_png = app.screenshot()
```

Full type stubs (`.pyi`) ship with the package -- autocomplete and type checking work out of the box in VSCode, PyCharm, and mypy.

### pytest Plugin

```python
# conftest.py
import pytest
import axterminator as ax

@pytest.fixture
def calculator():
    return ax.app(name="Calculator")

# test_calc.py
def test_addition(calculator):
    calculator.find("5").click()
    calculator.find("+").click()
    calculator.find("3").click()
    calculator.find("=").click()
    # Assert result...
```

### MCP Server for Claude Code

AXTerminator includes an MCP server that gives Claude Code direct GUI interaction capabilities:

```json
{
  "mcpServers": {
    "axterminator": {
      "command": "axterminator",
      "args": ["mcp"]
    }
  }
}
```

This enables AI-driven GUI testing where Claude can see, click, and type in macOS applications.

## Architecture

AXTerminator is a Rust library with Python bindings via PyO3:

```
Python (your tests)
  |
  v
PyO3 FFI (zero-cost bindings)
  |
  v
Rust core (accessibility, healing, sync)
  |
  v
macOS Accessibility API (AXUIElement)
```

The `abi3-py39` build target produces a single `.so` that works across Python 3.9 through 3.14+ without recompilation.

## Patent Claims

AXTerminator's background testing capability is covered by pending patent claims (application date: 2026-01-10):

- **Patent 1**: Background GUI testing without focus acquisition via AXUIElementPerformAction on unfocused windows
- **Patent 2**: XPC-based test synchronization for deterministic idle detection
- **Patent 3**: Self-healing locator system with 7-strategy cascading fallback

## Who Should Use This

- **macOS developers** who want to run GUI tests without losing their desktop
- **QA engineers** building automated test suites for native macOS applications
- **CI/CD teams** who need fast, reliable GUI tests in headless or shared-runner environments
- **AI/LLM tool builders** who want to give models real GUI interaction capabilities

## Getting Started

```bash
pip install axterminator
```

Documentation: [mikkoparkkola.github.io/axterminator](https://mikkoparkkola.github.io/axterminator/)
Source: [github.com/MikkoParkkola/axterminator](https://github.com/MikkoParkkola/axterminator)
PyPI: [pypi.org/project/axterminator](https://pypi.org/project/axterminator/)

---

*AXTerminator is dual-licensed under MIT and Apache 2.0.*
