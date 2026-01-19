# AXTerminator Element Interactions - Usage Examples

## Basic Element Operations

### Getting Element Attributes

```python
import axterminator as ax

# Connect to an application
app = ax.connect(name="TextEdit")

# Find a button
button = app.find("Save")

# Get element properties
print(f"Role: {button.role()}")           # "AXButton"
print(f"Title: {button.title()}")         # "Save"
print(f"Enabled: {button.enabled()}")     # True
print(f"Focused: {button.focused()}")     # False

# Get element bounds (x, y, width, height)
bounds = button.bounds()
print(f"Position: ({bounds[0]}, {bounds[1]})")
print(f"Size: {bounds[2]}x{bounds[3]}")
```

### Clicking Elements

```python
# Background click (default - no focus stealing!)
button.click()

# Click with focus
button.click(mode=ax.FOCUS)

# Double-click
button.double_click()

# Right-click
button.right_click()
```

### Text Input

```python
# Find a text field
text_field = app.find("AXRole:AXTextField")

# Type text (character by character, human-like)
text_field.type_text("Hello, World!")

# Set value directly (faster for large text)
text_field.set_value("Lorem ipsum dolor sit amet...")

# Get current value
current_text = text_field.value()
print(f"Current text: {current_text}")
```

### Screenshots

```python
# Screenshot of entire application
app_screenshot = app.screenshot()
with open("app.png", "wb") as f:
    f.write(app_screenshot)

# Screenshot of specific element
button_screenshot = button.screenshot()
with open("button.png", "wb") as f:
    f.write(button_screenshot)
```

### Element Search

```python
# Simple title search
save_btn = app.find("Save")

# Search by role and title
text_field = app.find("AXRole:AXTextField")

# Search with custom attribute
element = app.find("AXIdentifier:submit-button")

# Search with timeout
element = app.find("Loading...", timeout_ms=5000)

# Search in child elements
parent = app.find("Form")
submit = parent.find("Submit")
```

## Advanced Examples

### Form Filling

```python
app = ax.connect(name="MyApp")

# Fill out a form
username = app.find("AXIdentifier:username")
username.set_value("john.doe@example.com")

password = app.find("AXIdentifier:password")
password.type_text("SecurePassword123!")  # Type for password fields

# Check a checkbox
checkbox = app.find("AXRole:AXCheckBox")
if not checkbox.value():  # Not checked
    checkbox.click()

# Submit
submit = app.find("Submit")
submit.click()
```

### Element Verification

```python
# Wait for element to appear
try:
    success_msg = app.find("Success!", timeout_ms=10000)
    assert success_msg.exists()
    print("Form submitted successfully!")
except:
    print("Timeout waiting for success message")

# Verify element state
assert save_btn.enabled(), "Save button should be enabled"
assert not cancel_btn.focused(), "Cancel should not be focused"
```

### Multi-step Workflows

```python
# Complex workflow with error handling
def fill_registration_form(app, user_data):
    try:
        # Step 1: Fill personal info
        first_name = app.find("AXIdentifier:firstName")
        first_name.set_value(user_data["first_name"])

        last_name = app.find("AXIdentifier:lastName")
        last_name.set_value(user_data["last_name"])

        # Step 2: Fill contact info
        email = app.find("AXIdentifier:email")
        email.set_value(user_data["email"])

        # Step 3: Set preferences
        newsletter = app.find("Subscribe to newsletter")
        if user_data.get("subscribe", False):
            newsletter.click()

        # Step 4: Submit
        submit = app.find("Register")
        submit.click(mode=ax.FOCUS)

        # Step 5: Verify success
        app.find("Registration Complete", timeout_ms=5000)
        return True

    except Exception as e:
        print(f"Registration failed: {e}")
        return False

# Use it
user = {
    "first_name": "Jane",
    "last_name": "Doe",
    "email": "jane@example.com",
    "subscribe": True
}

app = ax.connect(name="RegistrationApp")
success = fill_registration_form(app, user)
```

### Testing Patterns

```python
import axterminator as ax
import pytest

@pytest.fixture
def calculator_app():
    app = ax.connect(name="Calculator")
    yield app
    app.terminate()

def test_basic_addition(calculator_app):
    app = calculator_app

    # Enter: 5 + 3 =
    app.find("5").click()
    app.find("+").click()
    app.find("3").click()
    app.find("=").click()

    # Verify result
    display = app.find("AXRole:AXStaticText")
    assert display.value() == "8"

def test_element_bounds(calculator_app):
    app = calculator_app

    button = app.find("1")
    x, y, w, h = button.bounds()

    assert w > 0, "Button width should be positive"
    assert h > 0, "Button height should be positive"
    assert x >= 0, "Button x should be non-negative"
    assert y >= 0, "Button y should be non-negative"

def test_screenshot_capture(calculator_app):
    app = calculator_app

    # Capture app screenshot
    screenshot = app.screenshot()
    assert len(screenshot) > 0, "Screenshot should have data"
    assert screenshot[:8] == b'\\x89PNG\\r\\n\\x1a\\n', "Should be PNG format"
```

## Character-by-Character Typing

The `type_text()` function simulates human-like typing with proper key events:

```python
text_field = app.find("AXRole:AXTextField")

# Types: Hello, World!
# - Shift down → H → Shift up
# - e → l → l → o
# - Shift down → , → Shift up
# - Space
# - Shift down → W → Shift up
# - o → r → l → d
# - Shift down → ! → Shift up

text_field.type_text("Hello, World!")
```

Supports:
- All letters (a-z, A-Z)
- Numbers (0-9)
- Symbols (!@#$%^&*())
- Punctuation (-=[]\\;',./`~)
- Special keys (Enter, Tab, Space)

## Performance Tips

1. **Use set_value() for large text**: Much faster than type_text()
   ```python
   # Slow (character-by-character)
   field.type_text(long_text)

   # Fast (direct attribute set)
   field.set_value(long_text)
   ```

2. **Background mode for automation**: No focus stealing
   ```python
   # Your work uninterrupted
   button.click(mode=ax.BACKGROUND)
   ```

3. **Efficient search**: Search from specific parent
   ```python
   # Slow (searches entire app tree)
   field = app.find("Username")

   # Fast (searches only form children)
   form = app.find("LoginForm")
   field = form.find("Username")
   ```

4. **Reuse elements**: Don't search repeatedly
   ```python
   # Bad
   for i in range(10):
       app.find("Button").click()

   # Good
   button = app.find("Button")
   for i in range(10):
       button.click()
   ```

## Error Handling

```python
from axterminator import AXError

try:
    app = ax.connect(name="MyApp")
    button = app.find("Save")
    button.click()

except AXError.AccessibilityNotEnabled:
    print("Please enable accessibility permissions")

except AXError.ElementNotFound as e:
    print(f"Element not found: {e}")

except AXError.ActionFailed as e:
    print(f"Action failed: {e}")

except AXError.Timeout as e:
    print(f"Timeout: {e}")
```

## World-First Features

### Background Testing (No Focus Stealing!)

```python
# Your workflow stays uninterrupted
# while AXTerminator tests in background

app = ax.connect(name="BackgroundApp")

# All these work WITHOUT stealing focus
button = app.find("Process")
button.click()  # Default is BACKGROUND mode

checkbox = app.find("Enable")
checkbox.click()

menu = app.find("File")
menu.right_click()

# Only text input requires focus
text = app.find("AXRole:AXTextField")
text.type_text("test", mode=ax.FOCUS)  # Briefly focuses
```

This is revolutionary for:
- CI/CD pipelines (run tests while working)
- Parallel test execution (no focus conflicts)
- Developer productivity (test in background)
