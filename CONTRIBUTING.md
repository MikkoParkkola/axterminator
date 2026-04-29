# Contributing to AXTerminator

Thank you for your interest in contributing to AXTerminator!

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/YOUR_USERNAME/axterminator`
3. Create a feature branch: `git checkout -b feature/your-feature`

## Development Setup

**Prerequisites:**
- macOS 12+ (Monterey or later)
- Rust 1.70+
- Python 3.9+
- Xcode Command Line Tools

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install Python dependencies
pip install maturin pytest

# Build the Rust library with Python bindings
maturin develop

# Run tests
cargo test
pytest python/tests/
```

## Testing Notes

AXTerminator requires Accessibility permissions to function. When running tests:

1. Grant Terminal/IDE accessibility access in System Settings > Privacy & Security > Accessibility
2. Some tests require a running application (e.g., Finder, Calculator)
3. Tests marked `#[ignore]` or `@pytest.mark.requires_app` need real apps

## Code Standards

- Run `cargo fmt` before committing
- Run `cargo clippy` and fix all warnings
- Run `cargo doc --no-deps` and fix any doc warnings
- Add tests for new functionality
- Document public APIs with doc comments
- Python code: run `ruff check` and `mypy`

## Architecture Overview

```
src/
  lib.rs              Main library entry + PyO3 module definition
  accessibility.rs    AXUIElement FFI bindings (Core Foundation)
  app.rs              Application connection + element search
  element.rs          AXElement wrapper (PyO3 class)
  actions.rs          Click, type, scroll operations
  healing.rs          Self-healing locator strategies
  healing_match.rs    Fuzzy matching + XPath parsing
  cache.rs            LRU element cache
  sync.rs             EspressoMac sync engine (XPC + heuristic)
  router.rs           App type detection (native/Electron/WebView)
  error.rs            Error types
  ...                 Additional modules (copilot, recording, etc.)

python/
  axterminator/
    __init__.py       Re-exports from Rust extension
    __init__.pyi      Type stubs for IDE support
    cli.py            CLI tool
    sync.py           Python-side sync utilities
    vlm.py            VLM backend integration
    recorder.py       Action recorder
    pytest_plugin.py  pytest fixtures and markers
```

## Pull Request Process

1. Ensure CI passes (`cargo fmt`, `cargo clippy`, `cargo test`)
2. Update CHANGELOG.md if adding features or fixing bugs
3. Add examples for new APIs
4. Request review

## Discussions

Have questions? Use [GitHub Discussions](https://github.com/MikkoParkkola/axterminator/discussions) for Q&A and feature ideas.

## License

By contributing, you agree that your contributions may be distributed by the
project owner under the AXTerminator Community License and under separate
commercial licenses. Do not submit code you cannot license on those terms.
