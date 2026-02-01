# Performance Guide

AXTerminator is designed for speed. This document covers performance characteristics, benchmarks, and optimization tips.

## Benchmarks

### Element Access

| Operation | Time | Notes |
|-----------|------|-------|
| Get element attribute | ~250µs | Title, value, role, etc. |
| Find element by title | ~10-50ms | Depends on tree depth |
| Find with cached path | ~1-5ms | After first find |
| Click (background) | ~1ms | No focus change |
| Click (foreground) | ~5-10ms | Includes focus switch |
| Type text | ~5ms + 1ms/char | Character-by-character |

### Comparison with Other Frameworks

| Framework | Element Access | Find Element |
|-----------|---------------|--------------|
| **AXTerminator** | **~250µs** | **~10-50ms** |
| Appium | ~50-100ms | ~500ms-2s |
| XCUITest | ~10-50ms | ~100-500ms |
| Playwright (Web) | ~1-5ms | ~50-200ms |

AXTerminator is **4000× faster** for element access than typical mobile testing frameworks.

## Why So Fast?

1. **Direct Accessibility API Access**
   - Uses native macOS AX APIs via Rust FFI
   - No intermediate process or network layers

2. **Zero-Copy Where Possible**
   - Rust core minimizes memory allocations
   - Python bindings use efficient PyO3 patterns

3. **Smart Caching**
   - Element paths are cached after first find
   - Accessibility tree structure is partially cached

4. **Background Operation**
   - No focus switching overhead
   - No window activation delays

## Profiling Your Tests

### Built-in Timing

```python
import time
import axterminator

app = axterminator.app(name="Calculator")

# Time element finding
start = time.perf_counter()
element = app.find("5")
find_time = (time.perf_counter() - start) * 1000
print(f"Find took {find_time:.2f}ms")

# Time clicking
start = time.perf_counter()
element.click()
click_time = (time.perf_counter() - start) * 1000
print(f"Click took {click_time:.2f}ms")
```

### Using pytest-benchmark

```python
def test_element_find_performance(benchmark, calculator_app):
    app = axterminator.app(name="Calculator")
    result = benchmark(lambda: app.find("5"))
    assert result is not None
```

## Optimization Tips

### 1. Reuse Element References

```python
# ❌ Slow: Re-finds element each time
for i in range(10):
    app.find("button").click()

# ✅ Fast: Find once, click multiple times
button = app.find("button")
for i in range(10):
    button.click()
```

### 2. Use Specific Queries

```python
# ❌ Slow: Searches entire tree
app.find("Save")

# ✅ Fast: More specific query
app.find("role:AXButton title:Save")

# ✅ Fastest: By identifier
app.find("identifier:save_button")
```

### 3. Set Appropriate Timeouts

```python
# ❌ Wasteful: 5 second timeout for fast apps
element = app.find("button", timeout_ms=5000)

# ✅ Better: Adjust based on expected response time
element = app.find("button", timeout_ms=500)
```

### 4. Batch Operations

```python
# ❌ Slow: Individual finds
num1 = app.find("1")
num2 = app.find("2")
plus = app.find("+")

# ✅ Faster: Find parent, then children
keypad = app.find("role:AXGroup identifier:keypad")
num1 = keypad.find("1")  # Searches smaller tree
```

### 5. Use Synchronization Wisely

```python
from axterminator.sync import wait_for_idle

# ❌ Wasteful: Always wait
wait_for_idle(app, timeout_ms=5000)
element.click()
wait_for_idle(app, timeout_ms=5000)

# ✅ Better: Wait only when needed
element.click()
if expecting_animation:
    wait_for_idle(app, timeout_ms=1000)
```

## Memory Usage

AXTerminator has low memory overhead:

| Component | Memory |
|-----------|--------|
| Rust core | ~2-5 MB |
| Python bindings | ~1 MB |
| Per-element cache | ~1 KB |
| VLM model (MLX) | ~1-2 GB |

The VLM model is only loaded when visual detection is actually used.

## CI/CD Considerations

### Headless Environments

AXTerminator requires a GUI session (not headless). In CI:

```yaml
# GitHub Actions
jobs:
  test:
    runs-on: macos-latest  # Has GUI
    steps:
      - run: pytest tests/ --ignore=tests/test_integration.py
```

### Parallel Testing

Tests can run in parallel safely:

```python
# Different apps = safe
app1 = axterminator.app(name="Calculator")
app2 = axterminator.app(name="Notes")  # OK: Different process

# Same app = be careful
# Use locking or sequential execution for same-app tests
```

## Troubleshooting Performance

### Slow Element Finding

1. Check query specificity
2. Verify element exists (typos in query)
3. Consider app loading time
4. Use shorter timeouts with retries

### High Memory Usage

1. Don't store many element references
2. Clear caches if running long sessions
3. Avoid loading VLM for simple tests

### Flaky Tests

1. Add synchronization after actions
2. Use `wait_for_element()` instead of `find()` with long timeout
3. Check for race conditions in app state
