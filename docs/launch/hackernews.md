# Hacker News Post

**Title:** `Show HN: AXTerminator – First macOS GUI testing framework with true background testing`

**URL:** https://github.com/MikkoParkkola/axterminator

**Text (for Show HN comment):**

I built AXTerminator because every GUI testing tool steals focus. You start a test suite and can't use your computer for 10 minutes.

The solution: use macOS Accessibility APIs (AXUIElement) to interact with apps without bringing them to front. Tests run silently while you work.

Performance comparison (M1 MacBook Pro):
- AXTerminator: 379µs element access
- PyAutoGUI: ~100ms (264× slower)
- Appium: ~500ms (1,321× slower)

The speedup comes from eliminating HTTP/WebDriver overhead. Direct API calls:

  Python → Rust FFI → AXUIElement → Element

vs Appium:

  Python → HTTP → Node.js → XCTest → AXUIElement → Element

Other features:
- 7 self-healing locator strategies
- AI vision fallback (local MLX, Ollama, or cloud VLMs)
- pytest integration

pip install axterminator

Docs: https://mikkoparkkola.github.io/axterminator/
