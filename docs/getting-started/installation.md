# Installation

## Requirements

- **macOS 12+** (Monterey or later)
- **Accessibility permissions** granted to your terminal/IDE

## Rust Binary (Primary)

The recommended way to use AXTerminator is the Rust binary, which includes the MCP server and CLI.

### From crates.io

```bash
cargo install axterminator --features cli
```

### Via Homebrew

```bash
brew install MikkoParkkola/tap/axterminator
```

### From GitHub Releases

Download the latest binary from [GitHub Releases](https://github.com/MikkoParkkola/axterminator/releases).

### Prebuilt Binary (Direct Download)

```bash
# Detect architecture
ARCH=$(uname -m)
case "$ARCH" in
  arm64) TARGET="aarch64-apple-darwin" ;;
  x86_64) TARGET="x86_64-apple-darwin" ;;
esac

curl -L --fail -o /usr/local/bin/axterminator \
  "https://github.com/MikkoParkkola/axterminator/releases/latest/download/axterminator-${TARGET}"
chmod +x /usr/local/bin/axterminator
```

The prebuilt binary is fully self-contained — no Python or other runtime dependencies required.

### Build from Source

```bash
git clone https://github.com/MikkoParkkola/axterminator
cd axterminator
cargo build --release --features cli

# With all optional features
cargo build --release --features "cli,audio,camera,spaces"
```

## Python Package (Secondary)

For pytest integration and Python scripting, install via PyPI:

```bash
pip install axterminator
```

> **Important:** The PyPI package provides the Python library for scripting and pytest, but does NOT include the `mcp serve` CLI command. For MCP server usage, install the Rust binary via one of the methods above.

**Requirements:** Python 3.9+

### With VLM Backends

```bash
# Local AI (MLX - fastest, private)
pip install axterminator[vlm]

# Cloud AI backends
pip install axterminator[vlm-anthropic]  # Claude Vision
pip install axterminator[vlm-openai]     # OpenAI Vision
pip install axterminator[vlm-gemini]     # Gemini Vision
pip install axterminator[vlm-ollama]     # Local Ollama

# All backends
pip install axterminator[vlm-all]
```

### Build Python Package from Source

```bash
git clone https://github.com/MikkoParkkola/axterminator
cd axterminator

# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build Python extension
pip install maturin
maturin develop

# Test
pytest python/tests/
```

## Verify Installation

### Rust Binary

```bash
axterminator apps
```

### Python

```python
import axterminator as ax

if ax.is_accessibility_enabled():
    print("Ready to go!")
else:
    print("Grant accessibility permissions in System Settings")
```

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| `command not found: axterminator` | Binary not in PATH | Use full path or `cargo install axterminator --features cli` |
| `error: no matching package` on crates.io | Rust toolchain too old | Update: `rustup update stable` |
| Python `import axterminator` fails | Missing Rust extension | Rebuild: `pip install maturin && maturin develop` |
