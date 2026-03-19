# r/rust Post

**Title:** `AXTerminator: Rust-powered macOS GUI testing with PyO3 bindings (379 us element access)`

**Body:**

I built a macOS GUI automation library in Rust with Python bindings via PyO3. The key feature: **background testing without focus stealing**.

## Architecture

```
Python API -> PyO3 FFI -> Rust Core -> AXUIElement API
                |
            ~379 us per element (Criterion benchmark, M1 MacBook Pro)
```

Direct Accessibility API access eliminates the HTTP/WebDriver overhead that makes Appium ~500 ms per operation.

## Why Rust?

- **Performance**: 379 us element access via direct FFI to Core Foundation
- **Safety**: macOS Accessibility API requires careful CFTypeRef memory management (get-rule vs create-rule)
- **PyO3**: Seamless Python bindings with `abi3-py39` for a single wheel across Python 3.9-3.14+

## Self-Healing Locators

The core implements 7 fallback strategies in Rust:
1. data_testid (AXIdentifier exact match)
2. aria_label (AXLabel/AXDescription)
3. identifier (AXIdentifier direct)
4. title (fuzzy matching via Levenshtein distance, 80% threshold)
5. xpath (structural path parser with predicate support)
6. position (spatial heuristic, 50px threshold)
7. visual_vlm (delegates to Python VLM backends)

All pure Rust except strategy 7, which calls back into Python.

## Background Testing

macOS `AXUIElementPerformAction` works on unfocused windows. This is undocumented by Apple but verified on macOS 12-15. We use `kAXPressAction` and friends to click elements without bringing the target window to the foreground.

```rust
// Simplified
unsafe {
    AXUIElementPerformAction(element, kAXPressAction as CFStringRef);
}
```

## Crate Dependencies

- `core-foundation` / `core-foundation-sys` / `core-graphics` for macOS APIs
- `pyo3` for Python bindings
- `ahash` for fast tree hashing (sync engine)
- `lru` for element caching
- `criterion` for benchmarks

GitHub: https://github.com/MikkoParkkola/axterminator
Docs: https://mikkoparkkola.github.io/axterminator/

Feedback welcome, especially on the PyO3 patterns and Core Foundation memory management!
