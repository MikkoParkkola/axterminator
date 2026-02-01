# Core API

## Module Functions

### `app()`

Connect to a running application.

```python
import axterminator as ax

# By name
app = ax.app(name="Calculator")

# By bundle ID
app = ax.app(bundle_id="com.apple.calculator")

# By PID
app = ax.app(pid=12345)

# With options
app = ax.app(
    name="Notes",
    launch=True,           # Launch if not running
    healing_config=config  # Custom healing config
)
```

**Parameters:**

| Name | Type | Description |
|------|------|-------------|
| `name` | `str` | Application name |
| `bundle_id` | `str` | Bundle identifier |
| `pid` | `int` | Process ID |
| `launch` | `bool` | Launch if not running (default: False) |
| `healing_config` | `HealingConfig` | Custom healing configuration |

**Returns:** `AXApp` instance

**Raises:** `RuntimeError` if app not found or not accessible

---

### `is_accessibility_enabled()`

Check if accessibility permissions are granted.

```python
if ax.is_accessibility_enabled():
    print("Ready!")
```

**Returns:** `bool`

---

### `request_accessibility()`

Prompt user to grant accessibility permissions.

```python
ax.request_accessibility()
# Opens System Settings to Accessibility pane
```

---

### `configure_vlm()`

Configure the VLM (Vision Language Model) backend.

```python
ax.configure_vlm(
    backend="mlx",       # or "anthropic", "openai", "gemini", "ollama"
    api_key="...",       # Required for cloud backends
    model="...",         # Optional model override
    verbose=False        # Enable debug logging
)
```

**Parameters:**

| Name | Type | Description |
|------|------|-------------|
| `backend` | `str` | Backend name |
| `api_key` | `str` | API key (cloud backends) |
| `model` | `str` | Model name override |
| `verbose` | `bool` | Enable verbose logging |

---

## Classes

### `HealingConfig`

Configuration for self-healing locators.

```python
config = ax.HealingConfig(
    strategies=["data_testid", "title", "visual_vlm"],
    timeout_budget_ms=5000
)
```

**Attributes:**

| Name | Type | Default |
|------|------|---------|
| `strategies` | `list[str]` | All 7 strategies |
| `timeout_budget_ms` | `int` | 5000 |

---

### `ActionMode`

Enum for action modes.

```python
ax.ActionMode.Background  # Default - no focus stealing
ax.ActionMode.Focus       # Brings app to foreground

# Convenience constants
ax.BACKGROUND
ax.FOCUS
```
