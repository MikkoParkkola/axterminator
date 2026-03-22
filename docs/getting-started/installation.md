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

The prebuilt binary is fully self-contained -- no runtime dependencies required.

### Build from Source

```bash
git clone https://github.com/MikkoParkkola/axterminator
cd axterminator
cargo build --release --features cli

# With all optional features
cargo build --release --features "cli,audio,camera,spaces"
```

## Verify Installation

### Rust Binary

```bash
axterminator apps
```

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| `command not found: axterminator` | Binary not in PATH | Use full path or `cargo install axterminator --features cli` |
| `error: no matching package` on crates.io | Rust toolchain too old | Update: `rustup update stable` |
