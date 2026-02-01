# AXTerminator Demo

## Recording the Demo GIF

### Option 1: VHS (Recommended)

```bash
# Install VHS
brew install vhs

# Install ttyd (required by VHS)
brew install ttyd

# Record the demo
vhs demo/demo.tape
```

### Option 2: Manual Recording

1. Open Terminal
2. Run: `python demo/demo_script.py`
3. Use macOS screen recording or Kap to capture
4. Convert to GIF using `ffmpeg` or online converter

### Option 3: GitHub Actions

The demo workflow automatically generates a GIF when changes are pushed to `demo/`:

```bash
# Trigger manually
gh workflow run demo.yml
```

## Demo Script

`demo_script.py` showcases:
- Background testing (WORLD FIRST feature)
- 800-2000× speed advantage
- Self-healing locators (7 strategies)
- AI vision detection (VLM)
- Installation instructions

The script uses ANSI colors for terminal output:
- Cyan: Headers
- Yellow: Code
- Green: Results
- Dim: Descriptions
