# Contributing to AXTerminator

Thank you for your interest in contributing to AXTerminator!

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/YOUR_USERNAME/axterminator`
3. Create a feature branch: `git checkout -b feature/your-feature`

## Development Setup

**Prerequisites:**
- macOS 11.0 or later
- Rust 1.70+
- Python 3.9+ (for bindings)
- Xcode Command Line Tools

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install Python dependencies
pip install maturin pytest

# Build the Rust library with Python bindings
maturin develop

# Run tests
cargo test
pytest tests/
```

## Testing Notes

AXTerminator requires Accessibility permissions to function. When running tests:

1. Grant Terminal/IDE accessibility access in System Preferences
2. Some tests may require launching test applications

## Code Standards

- Run `cargo fmt` before committing
- Run `cargo clippy` and fix all warnings
- Add tests for new functionality
- Document public APIs

## Architecture Overview

```
src/
├── lib.rs           # Main library entry + PyO3 bindings
├── element.rs       # AXUIElement wrapper
├── app.rs           # Application connection
├── actions.rs       # Click, type, scroll operations
├── healing.rs       # Self-healing locator strategies
└── tree.rs          # Accessibility tree traversal
```

## Pull Request Process

1. Ensure CI passes (fmt, clippy, test)
2. Update README.md if adding features
3. Add examples for new APIs
4. Request review

## License

By contributing, you agree that your contributions will be dual-licensed under MIT OR Apache-2.0.
