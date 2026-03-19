# r/Python Post

**Title:** `AXTerminator: macOS GUI testing that runs in the background (Rust core, Python API)`

**Body:**

I built a GUI testing framework for macOS that solves the biggest pain point: **tests that steal your focus**.

## The Problem
Every existing tool (PyAutoGUI, Appium, XCUITest) takes over your screen. You can't work while tests run.

## The Solution
AXTerminator uses macOS `AXUIElementPerformAction` on unfocused windows -- an undocumented capability verified on macOS 12-15. Your tests run silently in the background.

```python
import axterminator as ax

# Tests run in the background -- keep working!
calculator = ax.app(name="Calculator")
calculator.find("5").click()    # No focus stealing
calculator.find("+").click()    # Continue your work
calculator.find("3").click()
```

## Benchmarks (M1 MacBook Pro, macOS 14.2)

| Operation | Time | Method |
|-----------|------|--------|
| Element access | **379 us** | Criterion benchmark |
| Single attribute | **54 us** | Criterion benchmark |
| Action overhead | **20 us** | Criterion benchmark |

For comparison, Appium element access is typically ~500 ms due to HTTP/WebDriver overhead.

## Features
- Background testing (no focus stealing)
- 379 us element access (Rust FFI to AXUIElement)
- 7 self-healing locator strategies
- AI vision fallback (local MLX, Ollama, or cloud VLMs)
- pytest integration
- Full `.pyi` type stubs for IDE autocomplete

```bash
pip install axterminator
```

GitHub: https://github.com/MikkoParkkola/axterminator
PyPI: https://pypi.org/project/axterminator/
Docs: https://mikkoparkkola.github.io/axterminator/

Built with Rust + PyO3. Inspired by mediar-ai/terminator (Windows). Happy to answer questions!
