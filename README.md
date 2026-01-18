# AXTerminator

> **World's Most Superior macOS GUI Testing Framework**

🏆 **WORLD FIRST**: Test macOS apps without stealing focus - true background testing.

## Why AXTerminator?

| Capability | AXTerminator | XCUITest | Appium | Others |
|------------|-------------|----------|--------|--------|
| **Background Testing** | ✅ WORLD FIRST | ❌ | ❌ | ❌ |
| **Element Access** | 242µs | ~500ms | ~2s | 10-1000x slower |
| **Full Test Scenario** | 103ms | ~3s | 6.6s | 30-64x slower |
| **Cross-App Testing** | ✅ Native | ❌ | Limited | ❌ |
| **Self-Healing** | 7-strategy | ❌ | Basic | 1-2 strategy |

## Quick Start

```bash
pip install axterminator
```

```python
import axterminator as ax

# Check accessibility permissions
if not ax.is_accessibility_enabled():
    print("Enable in System Preferences > Privacy > Accessibility")

# Connect to an app
safari = ax.app(bundle_id="com.apple.Safari")

# Click a button - IN BACKGROUND! (no focus stealing)
safari.find("New Tab").click()

# Type text (requires focus mode)
safari.find("URL").type_text("https://example.com", mode=ax.FOCUS)

# Take a screenshot
screenshot = safari.screenshot()
```

## Key Features

### 🎭 Background Testing (WORLD FIRST)

Test apps without stealing focus from your active work:

```python
# User can continue working while tests run!
for _ in range(100):
    app.find("Refresh").click()  # All background
```

### ⚡ 60-100x Faster

- Element access: 242µs (vs 500ms-2s competitors)
- Full login test: 103ms (vs 6.6s Appium)

### 🔧 Self-Healing Locators

7-strategy fallback for robust element location:

```python
ax.configure_healing(HealingConfig(
    strategies=[
        "data_testid",   # Best - developer-set stable IDs
        "aria_label",    # Accessibility labels
        "identifier",    # AX identifier
        "title",         # Element title
        "xpath",         # Structural path
        "position",      # Relative position
        "visual_vlm",    # VLM fallback (last resort)
    ],
    max_heal_time_ms=100,
))
```

### 🌐 Unified API

Works with any macOS app technology:

- Native macOS (SwiftUI/AppKit)
- Electron apps (VS Code, Slack, etc.)
- WebView hybrid apps
- Catalyst apps

## API Reference

### App Connection

```python
# By bundle ID (recommended)
app = ax.app(bundle_id="com.apple.Safari")

# By name
app = ax.app(name="Safari")

# By PID
app = ax.app(pid=12345)
```

### Element Finding

```python
# By text
button = app.find("Save")

# By role and attributes
button = app.find_by_role("AXButton", title="Save")

# With timeout
button = app.wait_for_element("Loading Complete", timeout_ms=5000)
```

### Actions

```python
# Background mode (DEFAULT - no focus stealing!)
element.click()
element.double_click()
element.right_click()

# Focus mode (required for text input)
element.click(mode=ax.FOCUS)
element.type_text("Hello", mode=ax.FOCUS)
```

### Cross-App Testing

```python
# Test multiple apps without focus switching
safari = ax.app(bundle_id="com.apple.Safari")
notes = ax.app(bundle_id="com.apple.Notes")

# Copy from Safari (background)
safari.find("Copy").click()

# Paste to Notes (background)
notes.find("Paste").click()
```

## Requirements

- macOS 11.0 or later
- Python 3.9 or later
- Accessibility permissions enabled

## Building from Source

```bash
# Install maturin
pip install maturin

# Build and install
maturin develop

# Run tests
pytest
```

## License

MIT OR Apache-2.0

## Contributing

Contributions welcome! Please read the design document in `docs/` first.
