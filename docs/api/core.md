# Core API

## Module Functions

### `app()`

Connect to a running application. Exactly one of `name`, `bundle_id`, or `pid` must be provided.

```python
import axterminator as ax

# By name
app = ax.app(name="Calculator")

# By bundle ID (recommended -- locale-independent)
app = ax.app(bundle_id="com.apple.Safari")

# By PID
app = ax.app(pid=12345)
```

**Parameters:**

| Name | Type | Description |
|------|------|-------------|
| `name` | `str` | Application name (e.g., `"Safari"`) |
| `bundle_id` | `str` | Bundle identifier (e.g., `"com.apple.Safari"`) |
| `pid` | `int` | Process ID |

**Returns:** `AXApp` instance

**Raises:** `ValueError` if no selector provided; `RuntimeError` if app not found or not accessible

---

### `is_accessibility_enabled()`

Check if accessibility permissions are granted.

```python
if ax.is_accessibility_enabled():
    print("Ready!")
```

**Returns:** `bool`

---

### `configure_healing()`

Install a `HealingConfig` as the global healing configuration. Call once at startup.

```python
config = ax.HealingConfig(
    strategies=["data_testid", "title", "visual_vlm"],
    max_heal_time_ms=200,
)
ax.configure_healing(config)
```

---

### `configure_vlm()`

Configure the VLM (Vision Language Model) backend for visual element detection.

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
| `backend` | `str` | Backend name (`"mlx"`, `"anthropic"`, `"openai"`, `"gemini"`, `"ollama"`) |
| `api_key` | `str` | API key (cloud backends only) |
| `model` | `str` | Model name override |
| `verbose` | `bool` | Enable verbose logging |

---

## Classes

### `HealingConfig`

Configuration for self-healing locators.

```python
config = ax.HealingConfig(
    strategies=["data_testid", "aria_label", "title"],
    max_heal_time_ms=200,
    cache_healed=True,
)
```

**Parameters:**

| Name | Type | Default | Description |
|------|------|---------|-------------|
| `strategies` | `list[str] \| None` | All 7 strategies | Ordered list of strategy names |
| `max_heal_time_ms` | `int` | `100` | Maximum time budget for healing (ms) |
| `cache_healed` | `bool` | `True` | Cache successful heals |

Valid strategy names: `"data_testid"`, `"aria_label"`, `"identifier"`, `"title"`, `"xpath"`, `"position"`, `"visual_vlm"`.

---

### `ActionMode`

Controls whether an action steals focus.

```python
ax.ActionMode.Background  # Default - no focus stealing
ax.ActionMode.Focus       # Brings app to foreground

# Convenience constants
ax.BACKGROUND
ax.FOCUS
```
