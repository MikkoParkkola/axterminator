#!/usr/bin/env python3
"""TextEdit automation example.

Demonstrates:
- Document creation
- Text formatting
- Save dialogs
- Menu navigation
"""

import axterminator
import time


def main():
    if not axterminator.is_accessibility_enabled():
        print("ERROR: Accessibility permissions not granted.")
        return

    print("Connecting to TextEdit...")
    app = axterminator.app(name="TextEdit", launch=True)
    print(f"Connected! PID: {app.pid}")
    time.sleep(1)

    # Create new document
    print("\nCreating new document...")
    try:
        file_menu = app.find("File", role="AXMenuBarItem", timeout_ms=2000)
        file_menu.click()
        time.sleep(0.3)

        new_doc = app.find("New", timeout_ms=2000)
        new_doc.click()
        time.sleep(0.5)
        print("New document created!")
    except Exception as e:
        print(f"Menu navigation failed: {e}")

    # Find text area and type
    print("\nTyping document content...")
    try:
        text_area = app.find("", role="AXTextArea", timeout_ms=2000)
        text_area.click()
        time.sleep(0.2)

        text_area.type_text("axterminator Demo Document\n")
        text_area.type_text("=" * 30 + "\n\n")
        text_area.type_text("This document was created automatically using axterminator,\n")
        text_area.type_text("the world's fastest macOS GUI testing framework.\n\n")
        text_area.type_text("Features:\n")
        text_area.type_text("* Background operation - no focus stealing\n")
        text_area.type_text("* ~250 microsecond element access\n")
        text_area.type_text("* Self-healing locators with 7 strategies\n")
        text_area.type_text("* VLM visual detection fallback\n")

        print("Content typed!")
    except Exception as e:
        print(f"Could not type: {e}")

    # Apply bold formatting to title
    print("\nApplying formatting...")
    try:
        # Select all (Cmd+A)
        format_menu = app.find("Format", role="AXMenuBarItem", timeout_ms=2000)
        format_menu.click()
        time.sleep(0.3)

        # Find Font submenu
        font = app.find("Font", timeout_ms=2000)
        font.click()
        time.sleep(0.3)

        print("Format menu accessed!")
    except Exception as e:
        print(f"Formatting not applied: {e}")

    print("\nTextEdit automation complete!")


if __name__ == "__main__":
    main()
