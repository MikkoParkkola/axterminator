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
