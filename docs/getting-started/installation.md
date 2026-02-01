# Installation

## Requirements

- **macOS 12+** (Monterey or later)
- **Python 3.9+**
- **Accessibility permissions** granted to your terminal/IDE

## Install from PyPI

```bash
pip install axterminator
```

## Install with VLM backends

```bash
# Local AI (MLX - fastest, private)
pip install axterminator[vlm]

# Cloud AI backends
pip install axterminator[vlm-anthropic]  # Claude Vision
pip install axterminator[vlm-openai]     # GPT-5
pip install axterminator[vlm-gemini]     # Gemini Vision
pip install axterminator[vlm-ollama]     # Local Ollama

# All backends
pip install axterminator[vlm-all]
```

## Build from Source

```bash
git clone https://github.com/MikkoParkkola/axterminator
cd axterminator

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build
pip install maturin
maturin develop

# Test
pytest python/tests/
```

## Verify Installation

```python
import axterminator as ax

# Check accessibility permissions
if ax.is_accessibility_enabled():
    print("Ready to go!")
else:
    print("Grant accessibility permissions in System Settings")
```
