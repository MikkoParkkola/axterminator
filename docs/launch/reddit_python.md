# r/Python Post

**Title:** `AXTerminator: macOS GUI testing that runs in the background (264× faster than PyAutoGUI)`

**Body:**

I built a GUI testing framework for macOS that solves the biggest pain point: **tests that steal your focus**.

## The Problem
Every existing tool (PyAutoGUI, Appium, XCUITest) takes over your screen. You can't work while tests run.

## The Solution
AXTerminator uses macOS Accessibility APIs to interact with apps **without stealing focus**. Your tests run silently in the background.

```python
import axterminator as ax

# Tests run IN THE BACKGROUND - keep working!
calculator = ax.app(name="Calculator")
calculator.find("5").click()    # No focus stealing
calculator.find("+").click()    # Continue your work
calculator.find("3").click()    # Tests just work
```

## Benchmarks (M1 MacBook Pro)

| Tool | Element Access | Speedup |
|------|---------------|---------|
| **AXTerminator** | **379µs** | baseline |
| PyAutoGUI | ~100ms | 264× slower |
| Appium | ~500ms | 1,321× slower |

## Features
- 🎭 **Background testing** (world first)
- ⚡ 379µs element access
- 🔧 7 self-healing locator strategies
- 🤖 AI vision fallback (local MLX, Ollama, or cloud VLMs)
- 🧪 pytest integration

```bash
pip install axterminator
```

GitHub: https://github.com/MikkoParkkola/axterminator
PyPI: https://pypi.org/project/axterminator/
Docs: https://mikkoparkkola.github.io/axterminator/

Built with Rust + PyO3. Happy to answer questions!
