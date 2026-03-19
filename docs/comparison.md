# Tool Comparison

How AXTerminator compares to other macOS GUI automation tools.

## Performance Comparison

| Tool | Element Access | Click | Focus Stealing | Language |
|------|---------------|-------|----------------|----------|
| **AXTerminator** | **379µs** | ~1ms | **No** | Rust/Python |
| XCUITest | ~200ms | ~50ms | Yes | Swift |
| Appium Mac2 | ~500ms | ~100ms | Yes | Any |
| PyAutoGUI | ~100ms | ~50ms | Yes | Python |
| Maestro | ~300ms | ~100ms | Yes | YAML |
| SikuliX | ~1000ms | ~500ms | Yes | Java/Python |
| Atomac | ~500ms | ~200ms | Yes | Python |

## Feature Comparison

| Feature | AXTerminator | XCUITest | Appium | PyAutoGUI | Maestro |
|---------|:------------:|:--------:|:------:|:---------:|:-------:|
| Background testing | ✅ | ❌ | ❌ | ❌ | ❌ |
| Self-healing locators | ✅ (7) | ❌ | Basic | ❌ | ✅ |
| AI vision fallback | ✅ | ❌ | ❌ | ❌ | ❌ |
| Cross-app testing | ✅ | ❌ | Limited | ✅ | Limited |
| Python API | ✅ | ❌ | ✅ | ✅ | ❌ |
| No Xcode required | ✅ | ❌ | ❌ | ✅ | ✅ |
| Electron support | ✅ | ❌ | ✅ | ✅ | ✅ |
| WebView support | ✅ | Limited | ✅ | ❌ | ✅ |

## Speedup vs Competitors

AXTerminator element access measured with Criterion benchmarks on M1 MacBook Pro, macOS 14.2 (Finder.app target). Competitor numbers are estimates based on architecture analysis and community-reported values, not controlled head-to-head benchmarks.

| Competitor | Their Typical Speed | AXTerminator | Approximate Speedup |
|------------|---------------------|--------------|---------------------|
| XCUITest | ~200 ms (reported) | 379 us (measured) | ~528x |
| Appium | ~500 ms (reported) | 379 us (measured) | ~1,321x |
| PyAutoGUI | ~100 ms (reported) | 379 us (measured) | ~264x |
| SikuliX | ~1000 ms (reported) | 379 us (measured) | ~2,639x |

The speedup is primarily from eliminating HTTP/WebDriver/JSON protocol overhead. AXTerminator calls `AXUIElement` APIs directly via Rust FFI.

## Why AXTerminator is Faster

### Direct API Access

```
AXTerminator: Python → Rust FFI → AXUIElement → Element
                       ↓
                    ~379µs

Appium:       Python → HTTP → Node.js → XCTest → AXUIElement → Element
                       ↓         ↓         ↓
                    ~50ms    ~100ms    ~350ms = ~500ms total
```

### No HTTP Overhead

| Layer | Appium | AXTerminator |
|-------|--------|--------------|
| HTTP request | ~50ms | 0 |
| JSON serialization | ~10ms | 0 |
| WebDriver protocol | ~100ms | 0 |
| XCTest bridge | ~300ms | 0 |
| **Total overhead** | **~460ms** | **0** |

## Maintenance Status

| Tool | Status | Last Update | Notes |
|------|--------|-------------|-------|
| **AXTerminator** | ✅ Active | 2026 | New, actively developed |
| XCUITest | ✅ Active | Ongoing | Apple-maintained |
| Appium | ✅ Active | Ongoing | Large community |
| PyAutoGUI | ✅ Active | 2024 | Slower development |
| Maestro | ✅ Active | 2025 | Mobile-focused |
| SikuliX | 🟡 Slow | 2023 | Infrequent updates |
| **Atomac** | ❌ Dead | 2017 | **Do not use** |

## When to Use What

### Use AXTerminator when:
- You need **background testing** (unique feature)
- Speed matters (1000+ element operations)
- Testing native macOS apps
- You want Python + pytest integration
- You need self-healing locators

### Use XCUITest when:
- You're already in the Apple ecosystem
- Testing iOS apps primarily
- You have Xcode set up

### Use Appium when:
- Cross-platform tests (iOS + Android + macOS)
- Large existing Appium test suite
- Team knows WebDriver protocol

### Use PyAutoGUI when:
- Simple automation scripts
- Cross-platform (Windows + macOS + Linux)
- Image-based automation is acceptable

### Use Maestro when:
- Mobile-first testing
- YAML preference over code
- Anti-flakiness is priority

## Migration from Other Tools

### From PyAutoGUI

```python
# Before (PyAutoGUI)
import pyautogui
pyautogui.click(100, 200)
pyautogui.write('Hello')

# After (AXTerminator)
import axterminator as ax
app = ax.app(name="TextEdit")
app.find("role:AXTextArea").click()
app.find("role:AXTextArea").type_text("Hello")
```

### From Appium

```python
# Before (Appium)
from appium import webdriver
driver = webdriver.Remote(command_executor='http://127.0.0.1:4723')
element = driver.find_element(by='name', value='Save')
element.click()

# After (AXTerminator)
import axterminator as ax
app = ax.app(name="MyApp")
app.find("Save").click()  # 1,321× faster
```
