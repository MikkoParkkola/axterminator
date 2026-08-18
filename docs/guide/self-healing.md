# Self-Healing Locators

The `axterminator` Rust library ships 7 fallback strategies for finding an
element after the UI has changed, in `axterminator::find_with_healing`.

The MCP `ax_find` tool and the `axterminator find` CLI command do not use this
chain. They parse the query into search criteria, run a breadth-first search of
the accessibility tree with a result cache, and fall back to fuzzy matching over
the scene graph when that finds nothing.

## Strategy Order

`find_with_healing` tries these strategies in order:

| Priority | Strategy | Description |
|----------|----------|-------------|
| 1 | `data_testid` | Developer-set stable test IDs |
| 2 | `aria_label` | Accessibility labels |
| 3 | `identifier` | AX identifier |
| 4 | `title` | Element title (fuzzy matching) |
| 5 | `xpath` | Structural path in tree |
| 6 | `position` | Relative screen position |
| 7 | `visual_vlm` | Placeholder: always returns no match |

## Why This Order?

- **data_testid** is most stable - set by developers specifically for testing
- **aria_label** is stable and accessibility-focused
- **identifier** is system-assigned but reliable
- **title** may change with localization
- **xpath** breaks if element moves in tree
- **position** breaks if layout changes
- **visual_vlm** is a stub in this chain; visual lookup lives in the `ax_find_visual` MCP tool, described in [VLM Vision](vlm.md)

## Configuration

`HealingConfig` carries the strategy list plus a total time budget in
milliseconds. Defaults: all 7 strategies, `max_heal_time_ms` 100,
`cache_healed` true.

```rust
use axterminator::{HealingConfig, set_global_config};

set_global_config(HealingConfig::new(
    Some(vec!["data_testid".into(), "aria_label".into(), "title".into()]),
    200,
    true,
))?;
```

## Best Practices

### For Developers

Add accessibility identifiers to your UI elements. The `data_testid` strategy
matches the macOS `AXIdentifier` attribute exactly:

```swift
// SwiftUI
Button("Save") { ... }
    .accessibilityIdentifier("save-button")

// AppKit
button.setAccessibilityIdentifier("save-button")
```

### For Testers

1. Prefer stable identifiers over titles, which change with localization
2. Keep the identifier in the query so `data_testid` can match it
3. Do not rely on `visual_vlm` here: it never matches. Use the `ax_find_visual`
   MCP tool for visual lookup
