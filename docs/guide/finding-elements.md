# Finding Elements

AXTerminator provides multiple ways to locate UI elements, with automatic fallback through self-healing locators.

## Basic Finding

```python
import axterminator as ax

app = ax.app(name="Calculator")

# Find by title/label
button = app.find("5")

# Find with timeout (milliseconds)
button = app.find("Save", timeout_ms=5000)
```

## Query Syntax

Use prefixes to specify search type:

```python
# By identifier
app.find("identifier:_NS:9")

# By role
app.find("role:AXButton")

# By title (default)
app.find("title:Save")

# Combined
app.find("role:AXButton title:OK")
```

## Finding Multiple Elements

```python
# Find all buttons
buttons = app.find_all("role:AXButton")

# Iterate
for button in buttons:
    print(button.title)
```

## Element Properties

```python
element = app.find("Save")

# Read properties
print(element.title)       # "Save"
print(element.role)        # "AXButton"
print(element.value)       # Current value
print(element.identifier)  # Accessibility identifier
print(element.enabled)     # True/False
print(element.focused)     # True/False
```

## Hierarchical Navigation

```python
# Get main window
window = app.main_window()

# Find within element
toolbar = window.find("role:AXToolbar")
save_btn = toolbar.find("Save")

# Get children
children = element.children()
for child in children:
    print(child.role, child.title)
```

## Waiting for Elements

```python
from axterminator.sync import wait_for_element, wait_for_idle

# Wait for element to appear
button = wait_for_element(app, "Done", timeout_ms=5000)

# Wait for app to settle (no UI changes)
wait_for_idle(app, timeout_ms=3000)
```

## Error Handling

```python
try:
    element = app.find("NonExistent", timeout_ms=1000)
except RuntimeError as e:
    print(f"Element not found: {e}")
```
