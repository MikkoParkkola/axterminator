# Hacker News Post

**Title:** `Show HN: AXTerminator -- macOS GUI testing that runs in the background (Rust + Python)`

**URL:** https://github.com/MikkoParkkola/axterminator

**Text (for Show HN comment):**

I built AXTerminator because every macOS GUI testing tool steals focus. You start a test suite and can't use your computer until it finishes.

The key insight: macOS `AXUIElementPerformAction` works on unfocused windows. This is undocumented by Apple, but verified on macOS 12-15. AXTerminator uses this to run tests in the background while you keep working.

Performance (M1 MacBook Pro, macOS 14.2, Criterion benchmarks):
- Element access: 379 us (direct AXUIElement via Rust FFI)
- Single attribute read: 54 us
- Action overhead: 20 us

The speed comes from eliminating HTTP/WebDriver overhead:

  AXTerminator: Python -> PyO3 FFI -> Rust -> AXUIElement
  Appium:       Python -> HTTP -> Node.js -> XCTest -> AXUIElement

Other features:
- 7 self-healing locator strategies (from data-testid to AI vision)
- pytest plugin
- MCP server for Claude Code integration
- Espresso-style synchronization (XPC for SDK apps, heuristic fallback for all apps)

Inspired by mediar-ai/terminator (Windows GUI automation). This brings similar capabilities to macOS.

pip install axterminator

Source: https://github.com/MikkoParkkola/axterminator
Docs: https://mikkoparkkola.github.io/axterminator/
