# r/rust Post

**Title:** `AXTerminator: Rust-powered macOS GUI testing with PyO3 bindings (379µs element access)`

**Body:**

I built a macOS GUI automation library in Rust with Python bindings via PyO3. The key feature: **background testing without focus stealing**.

## Architecture

```
Python API → PyO3 FFI → Rust Core → AXUIElement API
                ↓
            ~379µs per element
```

Direct Accessibility API access eliminates the HTTP/WebDriver overhead that makes Appium ~500ms per operation.

## Why Rust?

- **Performance**: 379µs element access (1,321× faster than Appium)
- **Safety**: The macOS Accessibility API requires careful memory management
- **PyO3**: Seamless Python bindings for the testing community

## Self-Healing Locators

The core implements 7 fallback strategies in Rust:
1. data_testid
2. aria_label
3. identifier
4. title (fuzzy matching)
5. xpath
6. position
7. visual_vlm (AI vision via configurable backends)

## The "Background Testing" Trick

macOS Accessibility API supports `AXPerformAction` without bringing windows to front. We use `kAXPressAction` with coordinate translation to click elements in unfocused windows.

```rust
// Simplified - actual implementation handles edge cases
unsafe {
    AXUIElementPerformAction(element, kAXPressAction as CFStringRef);
}
```

GitHub: https://github.com/MikkoParkkola/axterminator
Docs: https://mikkoparkkola.github.io/axterminator/

Crate uses `accessibility` and `core-foundation` crates.

Feedback welcome, especially on the PyO3 patterns!
