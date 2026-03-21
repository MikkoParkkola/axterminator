# AXTerminator Design Document

**Status**: Implemented | **Version**: 0.6.1 | **Last Updated**: 2026-03-21

## Overview

AXTerminator is a macOS GUI testing framework that provides background testing (interacting with applications without stealing window focus), sub-millisecond element access via direct Accessibility API calls, and self-healing locators with 7 fallback strategies.

### Key Differentiators

| Capability | AXTerminator | XCUITest | Appium | PyAutoGUI |
|------------|:------------:|:--------:|:------:|:---------:|
| **Background testing** | Yes | No | No | No |
| **Element access** | ~379 us | ~200 ms | ~500 ms | ~100 ms |
| **Cross-app testing** | Yes | No | Limited | Yes |
| **Self-healing** | 7 strategies | No | Basic | No |
| **Electron support** | CDP | No | Via driver | No |
| **WebView support** | Auto-detect | Manual | Via context | No |

### Measured Performance (M1 MacBook Pro, macOS 14.2)

```
Single attribute read: 54 us    (Criterion, get_ax_role)
Element access:        379 us   (Criterion, search_first_button)
Action overhead:       20 us    (Criterion, perform_action_overhead)
```

---

## Architecture

```
+-----------------------------------------------------------------+
|                     AXTerminator Framework                      |
+-----------------------------------------------------------------+
|                                                                 |
|  +-----------------------------------------------------------+ |
|  |                    Python API (PyO3)                       | |
|  |  import axterminator as ax                                 | |
|  |  app = ax.app("Safari")                                    | |
|  |  app.find("Save").click()  # Background by default         | |
|  +-----------------------------------------------------------+ |
|                              |                                  |
|                              v                                  |
|  +-----------------------------------------------------------+ |
|  |              Rust Core Engine (~379 us/element)            | |
|  |  +-----------+  +-----------+  +---------------------+    | |
|  |  | AXBridge  |  | CGEvent   |  | Element Cache (LRU) |    | |
|  |  | Background|  | Focus-req |  |                     |    | |
|  |  +-----------+  +-----------+  +---------------------+    | |
|  +-----------------------------------------------------------+ |
|                              |                                  |
|                              v                                  |
|  +------------------+---------------+----------------------+    |
|  |  EspressoMac     | UnifiedTestOS |  Self-Healing        |    |
|  |  Sync Engine     | Cross-App     |  7-Strategy           |    |
|  |                  | Router        |  Fallback             |    |
|  |  +------------+  | +----------+  | +----------------+   |    |
|  |  | XPC Sync   |  | | Native   |  | | 1. data-testid |   |    |
|  |  | (SDK apps) |  | | AX       |  | | 2. aria-label  |   |    |
|  |  +------------+  | +----------+  | | 3. identifier  |   |    |
|  |  | Heuristic  |  | | Electron |  | | 4. title       |   |    |
|  |  | (non-SDK)  |  | | CDP      |  | | 5. xpath       |   |    |
|  |  +------------+  | +----------+  | | 6. position    |   |    |
|  |                  | | WebView  |  | | 7. visual(VLM) |   |    |
|  |                  | | Hybrid   |  | +----------------+   |    |
|  +------------------+---------------+----------------------+    |
|                                                                 |
+-----------------------------------------------------------------+
                              |
          +-------------------+-------------------+
          v                   v                   v
    +----------+       +----------+       +----------+
    | Native   |       | Electron |       | WebView  |
    | macOS    |       | Apps     |       | Content  |
    | Apps     |       | (CDP)    |       | (Hybrid) |
    +----------+       +----------+       +----------+
```

---

## Core Components

### 1. Background Action Engine

The ability to test apps without stealing focus. macOS `AXUIElementPerformAction` works on unfocused windows -- this is undocumented in Apple's developer documentation but verified working on macOS 12-15.

```rust
pub fn perform_background_action(
    element: AXUIElementRef,
    action: &str,
) -> Result<(), AXError> {
    // AXUIElementPerformAction works on unfocused windows
    unsafe {
        let action_str = CFString::new(action);
        AXUIElementPerformAction(element, action_str.as_concrete_TypeRef())
    }
}
```

**Supported in background** (verified):
- `kAXPressAction` -- Button clicks, menu items
- `kAXPickAction` -- Selection in pickers/lists
- `kAXIncrementAction` / `kAXDecrementAction` -- Steppers, sliders
- `kAXShowMenuAction` -- Context menus
- `kAXConfirmAction` -- Dialog confirmation

**Requires focus** (falls back automatically):
- Text input (requires `AXValue` setting with focus)
- Drag operations (requires CGEvent)
- Multi-touch gestures

### 2. EspressoMac Sync Engine

Espresso-style synchronization for macOS, with two strategies:

- **XPC Client**: Direct communication with EspressoMac SDK-enabled apps (~1 ms latency)
- **Heuristic Sync**: Accessibility tree hashing for any app (~50 ms polling, ~95% accuracy)

The heuristic fallback watches for structural stability: 3 consecutive identical tree hashes indicates the UI has settled.

### 3. UnifiedTestOS Router

Automatic detection and routing for different app architectures:

- **Native**: Pure AXUIElement API
- **Electron**: Chrome DevTools Protocol (auto-detected by checking for Chromium helper processes)
- **WebView Hybrid**: Switches protocol at WebView boundaries
- **Catalyst**: iPad apps on Mac

### 4. Self-Healing System (7-Strategy)

Deterministic fallback chain with configurable time budget (default: 100 ms):

1. `data_testid` -- Developer-set stable IDs (most reliable)
2. `aria_label` -- Accessibility labels
3. `identifier` -- AX identifier
4. `title` -- Fuzzy title matching (Levenshtein, 80% threshold)
5. `xpath` -- Structural path in accessibility tree
6. `position` -- Relative spatial position (50px threshold)
7. `visual_vlm` -- AI vision fallback (local MLX, Ollama, or cloud VLMs)

Successful heals are cached so subsequent lookups skip failed strategies.

---

## Python API

### Basic Usage

```python
import axterminator as ax

# Connect by bundle ID (locale-independent, recommended)
app = ax.app(bundle_id="com.apple.Safari")

# Find elements
button = app.find("Save")
button = app.find(role="AXButton", title="Save")

# Background click (default)
button.click()

# Focus mode when needed
text_field = app.find(role="AXTextField")
text_field.type_text("Hello", mode=ax.FOCUS)

# Synchronization
app.wait_for_idle()
app.wait_for_element("Loading Complete", timeout=5.0)
```

### Healing Configuration

```python
config = ax.HealingConfig(
    strategies=["data_testid", "aria_label", "title"],
    max_heal_time_ms=200,
    cache_healed=True,
)
ax.configure_healing(config)
```

---

## Swift SDK (EspressoMac)

Optional SDK for deterministic synchronization in your own apps:

```swift
import EspressoMacSDK

@main
struct MyApp: App {
    init() {
        EspressoMac.install()  // 1-line integration
    }
    var body: some Scene {
        WindowGroup { ContentView() }
    }
}
```

---

## Performance Budget

Measured on Apple M1 MacBook Pro, macOS 14.2 using Criterion benchmarks:

| Operation | Measured | Method |
|-----------|----------|--------|
| Single attribute read | 54 us | Criterion `get_ax_role` |
| Element access | 379 us | Criterion `search_first_button` |
| Background click | ~1 ms | Manual timing |
| Focus click | ~5 ms | Manual timing (includes app activation) |
| Healing (all 7 strategies) | <100 ms | Budget-enforced |
| VLM fallback | ~400 ms | Depends on backend |

---

*Last updated: 2026-03-21*
