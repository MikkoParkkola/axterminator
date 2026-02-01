#!/usr/bin/env python3
"""Visual element detection using VLM (Vision Language Model).

Demonstrates:
- Configuring VLM backends (MLX local, Anthropic, OpenAI, Gemini)
- Visual element detection as fallback strategy
- Self-healing locator system

Prerequisites:
- For MLX: pip install axterminator[vlm]
- For Anthropic: pip install axterminator[vlm-anthropic] + ANTHROPIC_API_KEY
- For OpenAI: pip install axterminator[vlm-openai] + OPENAI_API_KEY
- For Gemini: pip install axterminator[vlm-gemini] + GOOGLE_API_KEY
- For all: pip install axterminator[vlm-all]
"""

import os
from axterminator.vlm import configure_vlm, detect_element_visual


def main():
    # Option 1: Use local MLX (default, fast, private, no API costs)
    print("=== VLM Backend Configuration ===\n")

    try:
        configure_vlm(backend="mlx")
        print("MLX backend configured (local, ~50ms inference)")
    except ImportError:
        print("MLX not installed. Trying cloud backends...")

        # Option 2: Use Anthropic Claude Vision
        if os.environ.get("ANTHROPIC_API_KEY"):
            configure_vlm(backend="anthropic")
            print("Anthropic backend configured")

        # Option 3: Use OpenAI GPT-4V
        elif os.environ.get("OPENAI_API_KEY"):
            configure_vlm(backend="openai")
            print("OpenAI backend configured")

        # Option 4: Use Google Gemini
        elif os.environ.get("GOOGLE_API_KEY"):
            configure_vlm(backend="gemini")
            print("Gemini backend configured")

        else:
            print("No VLM backend available.")
            print("Install mlx-vlm or set ANTHROPIC_API_KEY/OPENAI_API_KEY/GOOGLE_API_KEY")
            return

    # Example: Detect element in screenshot
    print("\n=== Visual Element Detection ===\n")

    # In real usage, this would be a screenshot from the application
    # For demo, we show how the API works
    fake_image = b"PNG image data would go here"

    result = detect_element_visual(
        image_data=fake_image,
        description="Save button in the toolbar",
        image_width=1920,
        image_height=1080,
    )

    if result:
        x, y = result
        print(f"Element found at coordinates: ({x}, {y})")
    else:
        print("Element not found in image")

    print("\n=== How Visual Healing Works ===\n")
    print("""
When a traditional locator fails, axterminator's self-healing system
tries these strategies in order:

1. data_testid  - Most reliable, dev-specified
2. aria_label   - Accessibility label
3. identifier   - macOS accessibility identifier
4. title        - Element title text
5. xpath        - Structural path in accessibility tree
6. position     - Relative position in parent
7. visual_vlm   - Visual detection using VLM (this module!)

The visual_vlm strategy:
- Takes a screenshot of the application
- Sends it to the configured VLM with the element description
- VLM returns bounding box coordinates
- axterminator clicks at the center of the detected element

This enables testing apps with dynamic UIs where traditional
locators break frequently.
""")


if __name__ == "__main__":
    main()
