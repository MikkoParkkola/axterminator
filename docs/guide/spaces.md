# Virtual Desktops (Spaces)

AXTerminator can manage macOS virtual desktops via the `spaces` feature flag.

## Enable Spaces

Build with the `spaces` feature:

```bash
cargo build --release --features "cli,spaces"
```

!!! warning "Private API"
    The spaces feature uses Apple's `CGSSpace` private SPI. It is tested on macOS 14+ but is incompatible with App Store distribution.

## MCP Tools

| Tool | Description |
|------|-------------|
| `ax_list_spaces` | List all virtual desktops |
| `ax_create_space` | Create a new virtual desktop |
| `ax_move_to_space` | Move a window to a specific space |
| `ax_switch_space` | Switch to a specific space |
| `ax_destroy_space` | Destroy a virtual desktop |

## Use Cases

### Test Isolation

Run tests on a dedicated virtual desktop so they do not interfere with your work:

```bash
# Create isolated test space
axterminator create-space "Test Environment"

# Move test target to that space
axterminator move-to-space "Calculator" --space "Test Environment"

# Run tests
axterminator switch-space "Test Environment"
# ... run tests ...

# Clean up
axterminator destroy-space "Test Environment"
```

### Multi-Environment Testing

Test the same app in different configurations by isolating each instance on its own space.

## Limitations

- Requires macOS 14+ (tested)
- Uses private API -- not available for App Store apps
- Space creation/destruction may trigger Mission Control animations
