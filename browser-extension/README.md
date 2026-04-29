# axterminator Browser Recorder

Chrome extension to record browser interactions and generate axterminator test code.

## Installation

1. Open Chrome and go to `chrome://extensions/`
2. Enable "Developer mode" (toggle in top right)
3. Click "Load unpacked"
4. Select the `browser-extension` folder

## Usage

1. Click the axterminator icon in Chrome toolbar
2. Click "Start Recording"
3. Interact with the webpage (click buttons, fill forms)
4. Click "Stop Recording"
5. Copy the generated Python code

## Generated Code

The extension generates axterminator-compatible Python code:

```python
#!/usr/bin/env python3
import axterminator

def main():
    app = axterminator.app(name="Safari")

    # Click: Submit
    app.find("submit-btn", strategy="data_testid").click()

    # Type into: email
    element = app.find("Email", strategy="aria_label")
    element.type_text("user@example.com")

if __name__ == "__main__":
    main()
```

## Locator Strategies

The recorder prioritizes locators in this order:

1. `data-testid` / `data-test-id` / `data-cy`
2. `aria-label`
3. `id`
4. Visible text
5. `placeholder`
6. Tag name with role

## Notes

- The generated code is for macOS Safari/Chrome automation
- You may need to adjust the app name for your browser
- Some complex interactions may need manual refinement

## Development

To modify the extension:

1. Edit the source files
2. Go to `chrome://extensions/`
3. Click the refresh icon on the axterminator extension

## License

Part of the AXTerminator project. Free personal, research, educational,
noncommercial open-source, and free public-good use requires attribution.
Business use requires a written commercial license. See
[`../LICENSE.md`](../LICENSE.md) and [`../COMMERCIAL.md`](../COMMERCIAL.md).
