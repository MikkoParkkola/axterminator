# Configuration

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `AXTERMINATOR_VLM_BACKEND` | VLM backend | None |
| `AXTERMINATOR_VLM_API_KEY` | VLM API key | None |
| `AXTERMINATOR_TIMEOUT_MS` | Default timeout | 5000 |
| `AXTERMINATOR_LOG_LEVEL` | Log level | INFO |

## HealingConfig

```python
import axterminator as ax

config = ax.HealingConfig(
    strategies=[
        "data_testid",  # Developer-set IDs (most stable)
        "aria_label",   # Accessibility labels
        "identifier",   # AX identifier
        "title",        # Element title
        "xpath",        # Structural path
        "position",     # Screen position
        "visual_vlm",   # AI vision (slowest)
    ],
    max_heal_time_ms=100,  # Total time budget for healing (ms), default: 100
)
```

### Strategy Details

| Strategy | Stability | Speed | Notes |
|----------|-----------|-------|-------|
| `data_testid` | Highest | Fast | Set by developers |
| `aria_label` | High | Fast | Accessibility-focused |
| `identifier` | High | Fast | System-assigned |
| `title` | Medium | Fast | May change with i18n |
| `xpath` | Low | Medium | Breaks on structure changes |
| `position` | Lowest | Fast | Breaks on layout changes |
| `visual_vlm` | High | Slow | Requires VLM backend |

## VLM Configuration

### MLX (Local, Recommended)

```python
ax.configure_vlm(backend="mlx")
```

No API key needed. Uses Apple Silicon GPU.

### Ollama (Local)

```python
ax.configure_vlm(
    backend="ollama",
    model="llava"  # or "bakllava", "llava:13b"
)
```

Requires Ollama running locally.

### Anthropic (Cloud)

```python
ax.configure_vlm(
    backend="anthropic",
    api_key="sk-ant-..."
)
```

### OpenAI (Cloud)

```python
ax.configure_vlm(
    backend="openai",
    api_key="sk-..."
)
```

### Gemini (Cloud)

```python
ax.configure_vlm(
    backend="gemini",
    api_key="..."
)
```

## Logging

```python
import logging

# Enable debug logging
logging.basicConfig(level=logging.DEBUG)

# Or via environment
# AXTERMINATOR_LOG_LEVEL=DEBUG
```

## pytest Integration

```python
# conftest.py
import pytest

@pytest.fixture
def ax_app():
    """Fixture to get app connections."""
    import axterminator as ax

    def _get_app(name):
        return ax.app(name=name)

    return _get_app

@pytest.fixture
def ax_wait():
    """Fixture for waiting."""
    import time
    return time.sleep
```

### Custom Markers

```python
# pytest.ini
[pytest]
markers =
    integration: Integration tests (require --run-integration)
    slow: Slow tests
    requires_app: Requires specific app running
```
