# Performance Benchmarks

AXTerminator achieves sub-millisecond element access by calling the macOS Accessibility API directly via Rust FFI, eliminating HTTP/WebDriver overhead.

## Measured Performance

*Benchmarked on Apple M1 MacBook Pro, macOS 14.2*

| Operation | Time |
|-----------|------|
| Single attribute access | **53.6 µs** |
| Element access (window→child) | **378.6 µs** |
| Perform action (AXRaise) | **20.4 µs** |

## Competitor Comparison

AXTerminator element access is directly measured. Competitor numbers are estimates based on architecture analysis and community-reported values, not controlled head-to-head benchmarks.

| Framework | Element Access | vs AXTerminator | Source |
|-----------|---------------|-----------------|--------|
| **AXTerminator** | **379 us** | 1x (baseline) | Criterion benchmark |
| XCUITest | ~200 ms | ~528x slower | Community-reported typical values |
| Appium (Mac2) | ~500 ms | ~1,321x slower | Architecture estimate (HTTP + WebDriver + XCTest) |

The difference is architectural: AXTerminator eliminates all network and serialization overhead.

## Why So Fast?

### 1. Direct API Access

AXTerminator uses the macOS Accessibility API directly via Rust bindings:

```
AXTerminator: Python → Rust FFI → AX API → Element
Appium:       Python → HTTP → Node.js → XCUITest → AX API → Element
```

### 2. Zero HTTP Overhead

Appium adds ~500ms of network latency per operation:

| Layer | Appium Latency | AXTerminator |
|-------|----------------|--------------|
| HTTP request | ~50ms | 0 |
| JSON parse | ~10ms | 0 |
| WebDriver protocol | ~100ms | 0 |
| XCUITest bridge | ~300ms | 0 |
| **Total overhead** | **~460ms** | **0** |

### 3. Rust Performance

Rust provides zero-cost abstractions with no garbage collection pauses:

```
Memory: No GC → No pauses
CPU: Native machine code
FFI: Zero-overhead C interop
```

## Background Mode Overhead

Background clicking adds minimal overhead:

| Mode | Click Time |
|------|------------|
| Background (default) | **~1ms** |
| Focus | ~5ms (includes app activation) |

## Scaling Performance

| Elements Accessed | AXTerminator | Appium |
|-------------------|--------------|--------|
| 10 | 3.8ms | 5s |
| 100 | 38ms | 50s |
| 1,000 | 380ms | **8+ minutes** |

!!! tip "1000 Elements in Under 400ms"
    Access 1,000 elements in the time it takes Appium to access 1.

## Reproducing Benchmarks

```bash
# Clone repository
git clone https://github.com/MikkoParkkola/axterminator
cd axterminator

# Compile benchmark
rustc -O benches/bench_quick.rs \
  -l framework=ApplicationServices \
  -l framework=CoreFoundation \
  -o bench_quick

# Run (requires Finder running)
./bench_quick
```

## Python Performance

The Python bindings add minimal overhead:

```python
import axterminator as ax
import time

app = ax.app(name="Finder")

# Benchmark 1000 element accesses
start = time.perf_counter()
for _ in range(1000):
    app.find("File", timeout_ms=100)
elapsed = time.perf_counter() - start

print(f"1000 finds: {elapsed*1000:.1f}ms")
print(f"Per find: {elapsed:.3f}ms")
```

Typical results: **0.5-1ms per find** including Python overhead.

## Memory Usage

| Framework | Memory (typical test) |
|-----------|----------------------|
| AXTerminator | ~15 MB |
| Appium + Node | ~150 MB |
| XCUITest process | ~100 MB |
