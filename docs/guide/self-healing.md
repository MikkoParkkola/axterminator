# Self-Healing Locators

AXTerminator uses 7 fallback strategies to find elements even when the UI changes. This makes tests more resilient to updates.

## Strategy Order

When you call `app.find("element")`, AXTerminator tries these strategies in order:

| Priority | Strategy | Description |
|----------|----------|-------------|
| 1 | `data_testid` | Developer-set stable test IDs |
| 2 | `aria_label` | Accessibility labels |
| 3 | `identifier` | AX identifier |
| 4 | `title` | Element title (fuzzy matching) |
| 5 | `xpath` | Structural path in tree |
| 6 | `position` | Relative screen position |
| 7 | `visual_vlm` | AI vision detection |

## Why This Order?

- **data_testid** is most stable - set by developers specifically for testing
- **aria_label** is stable and accessibility-focused
- **identifier** is system-assigned but reliable
- **title** may change with localization
- **xpath** breaks if element moves in tree
- **position** breaks if layout changes
- **visual_vlm** is slowest but most flexible

## Configuration

```python
import axterminator as ax

# Customize strategy order
config = ax.HealingConfig(
    strategies=["data_testid", "aria_label", "title"],
    max_heal_time_ms=200,
)
ax.configure_healing(config)
```

## Best Practices

### For Developers

Add `data-testid` attributes to your UI elements:

```swift
// SwiftUI
Button("Save") { ... }
    .accessibilityIdentifier("save-button")

// AppKit
button.setAccessibilityIdentifier("save-button")
```

### For Testers

1. Prefer `data_testid` when available
2. Use descriptive locators: `app.find("data_testid:submit-order-btn")`
3. Enable VLM for maximum resilience

## Healing in Action

```python
# First run - finds by title
button = app.find("Submit Order")  # Uses title strategy

# After UI update - title changed but testid same
# Still finds it via data_testid fallback
button = app.find("Submit Order")  # Heals to data_testid

# Logs show healing
# [HEAL] title failed, trying data_testid
# [HEAL] Found via data_testid: submit-order-btn
```

## Timeout Budget

The timeout is split across strategies:

```python
config = ax.HealingConfig(
    strategies=["data_testid", "title", "visual_vlm"],
    max_heal_time_ms=300,  # Total budget split across strategies
)
ax.configure_healing(config)
```
