# VLM Vision Detection

When traditional locators fail, AXTerminator can use AI vision models to find elements by natural language description.

## Supported Backends

| Backend | Speed | Privacy | Cost |
|---------|-------|---------|------|
| **MLX** | Fast (~50ms) | Local | Free |
| **Ollama** | Medium (~200ms) | Local | Free |
| **Anthropic** | Slow (~1s) | Cloud | $$$ |
| **OpenAI** | Slow (~1s) | Cloud | $$$ |
| **Gemini** | Slow (~1s) | Cloud | $$ |

## Configuration

```python
import axterminator as ax

# Local MLX (recommended)
ax.configure_vlm(backend="mlx")

# Local Ollama
ax.configure_vlm(backend="ollama", model="llava")

# Claude Vision
ax.configure_vlm(
    backend="anthropic",
    api_key="sk-ant-..."
)

# GPT-4o
ax.configure_vlm(
    backend="openai",
    api_key="sk-..."
)

# Gemini Vision
ax.configure_vlm(
    backend="gemini",
    api_key="..."
)
```

## Usage

```python
# Natural language element description
button = app.find("the blue Save button in the toolbar")

# Works with complex descriptions
menu = app.find("the dropdown menu showing 'File' options")
icon = app.find("the red notification badge on the bell icon")
```

## How It Works

1. AXTerminator takes a screenshot of the app
2. Sends it to the VLM with your description
3. VLM returns bounding box coordinates
4. AXTerminator maps coordinates to accessibility element

```
[Screenshot] + "Find the blue Save button"
        ↓
    [VLM Model]
        ↓
{x: 450, y: 120, width: 80, height: 30}
        ↓
    [Element Mapping]
        ↓
    AXButton("Save")
```

## Performance Tips

1. **Use MLX locally** - 50ms vs 1s for cloud
2. **Be specific** - "blue Save button in toolbar" > "save button"
3. **Use as fallback** - Configure after other strategies

```python
config = ax.HealingConfig(
    strategies=["data_testid", "title", "visual_vlm"]
)
```

## Debugging

```python
# Enable verbose VLM logging
ax.configure_vlm(backend="mlx", verbose=True)

# Manual detection
result = ax.detect_element_visual(
    app.screenshot(),
    "the search icon"
)
print(result)  # {"x": 100, "y": 50, "confidence": 0.95}
```
