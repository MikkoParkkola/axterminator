# AXTerminator: Background GUI Testing for macOS

**TL;DR** -- AXTerminator lets you run GUI tests on macOS applications without stealing window focus. Tests execute in the background while you keep working. Element access takes ~379 us (measured on M1 MacBook Pro), and self-healing locators use 7 strategies to survive UI changes.

---

## The Problem

Every macOS GUI testing tool steals focus. XCUITest, Appium, PyAutoGUI -- they all bring the application under test to the foreground, lock your screen, and prevent you from doing anything else until the test suite finishes. For a 20-minute test suite, that means 20 minutes of watching your machine do things without you.

CI runners partially solve this, but local development iteration still requires surrendering your desktop. And CI Macs are expensive.

## The Discovery

macOS has an Accessibility API (`AXUIElement`) that every testing framework uses. But every framework also calls `AXUIElementPerformAction` only after activating the target application and raising its window. It turns out this activation step is unnecessary -- the API works on unfocused windows.

This is undocumented in Apple's developer documentation, but verified working on macOS 12 through 15. AXTerminator uses this capability to perform button clicks, menu selections, and value reads while the application sits in the background.

We have not found another macOS GUI testing framework that does this. XCUITest, Appium Mac2, PyAutoGUI, Maestro, and SikuliX all require focus. If you know of one that does, please let us know.

## What You Get

### Background Testing

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

### Sub-Millisecond Element Access

AXTerminator talks directly to the Accessibility API through Rust FFI. No HTTP server. No JSON serialization. No WebDriver protocol. The entire path from Python call to AX API response is a single function call through PyO3.

Benchmarked on Apple M1 MacBook Pro, macOS 14.2, using Criterion:

| Framework | Element Access | How Measured |
|-----------|---------------|--------------|
| **AXTerminator** | **379 us** | Criterion `search_first_button` against Finder.app |
| PyAutoGUI | ~100 ms | Estimated (coordinate-based, screen capture) |
| XCUITest | ~200 ms | Apple documentation, typical reported values |
| Appium | ~500 ms | Estimated (HTTP + WebDriver + XCTest bridge overhead) |

The competitor numbers are estimates based on architecture analysis and community-reported values, not controlled benchmarks. The AXTerminator number is directly measured. The speedup is a function of eliminating HTTP/WebDriver/JSON protocol overhead.

### 7-Strategy Self-Healing Locators

When a UI element moves, changes label, or gets restructured, AXTerminator tries seven strategies in order to relocate it:

1. **data-testid** -- stable test identifiers set by developers
2. **aria-label** -- accessibility labels
3. **identifier** -- AXIdentifier attribute
4. **title** -- window and button titles (fuzzy matching)
5. **xpath** -- structural tree path
6. **position** -- spatial location heuristic
7. **visual-vlm** -- AI vision model fallback (local MLX, Ollama, or cloud VLMs)

The healing system runs within a configurable time budget (default: 100 ms). Successful heals are cached so subsequent lookups skip failed strategies.

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
app.find("filename").set_value("doc.txt")  # Background mode for value

# Wait for UI state
app.wait_for_element("Success", timeout_ms=3000)
app.wait_for_idle(timeout_ms=5000)
```

Full type stubs (`.pyi`) ship with the package -- autocomplete and type checking work out of the box in VS Code, PyCharm, and mypy.

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

## Who Should Use This

- **macOS developers** who want to run GUI tests without losing their desktop
- **QA engineers** building automated test suites for native macOS applications
- **CI/CD teams** who need fast, reliable GUI tests on macOS runners
- **AI/LLM tool builders** who want to give models real GUI interaction capabilities

## Getting Started

```bash
pip install axterminator
```

- Documentation: [mikkoparkkola.github.io/axterminator](https://mikkoparkkola.github.io/axterminator/)
- Source: [github.com/MikkoParkkola/axterminator](https://github.com/MikkoParkkola/axterminator)
- PyPI: [pypi.org/project/axterminator](https://pypi.org/project/axterminator/)
- Discussions: [github.com/MikkoParkkola/axterminator/discussions](https://github.com/MikkoParkkola/axterminator/discussions)

---

*AXTerminator is dual-licensed under MIT and Apache 2.0.*

## Acknowledgements

AXTerminator was inspired by [Terminator](https://github.com/mediar-ai/terminator) from mediar-ai, which brought accessible desktop GUI automation to Windows. We wanted the same power on macOS, plus the ability to test apps without stealing focus from your work.
