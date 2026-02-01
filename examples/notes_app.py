#!/usr/bin/env python3
"""Notes app automation example.

Demonstrates:
- Creating new notes
- Typing text content
- Working with rich text areas
"""

import axterminator
import time


def main():
    if not axterminator.is_accessibility_enabled():
        print("ERROR: Accessibility permissions not granted.")
        return

    print("Connecting to Notes...")
    app = axterminator.app(name="Notes", launch=True)
    print(f"Connected! PID: {app.pid}")
    time.sleep(1)

    # Create a new note
    print("\nCreating new note...")
    try:
        # Use File > New Note or keyboard shortcut
        file_menu = app.find("File", role="AXMenuBarItem", timeout_ms=2000)
        file_menu.click()
        time.sleep(0.3)

        new_note = app.find("New Note", timeout_ms=2000)
        new_note.click()
        time.sleep(0.5)
        print("New note created!")
    except Exception as e:
        print(f"Could not create note via menu: {e}")
        # Try toolbar button
        try:
            new_btn = app.find("New Note", role="AXButton", timeout_ms=2000)
            new_btn.click()
            time.sleep(0.5)
            print("New note created via toolbar!")
        except Exception:
            print("Could not create note")
            return

    # Type content into the note
    print("\nTyping note content...")
    try:
        # Find the text area
        text_area = app.find("", role="AXTextArea", timeout_ms=2000)
        text_area.click()
        time.sleep(0.2)

        # Type the title
        text_area.type_text("Automated Note\n\n")
        time.sleep(0.2)

        # Type body content
        text_area.type_text("This note was created by axterminator!\n")
        text_area.type_text("- Fast (~250 microsecond element access)\n")
        text_area.type_text("- Background operation (no focus stealing)\n")
        text_area.type_text("- Self-healing locators\n")

        print("Content typed!")
    except Exception as e:
        print(f"Could not type content: {e}")

    print("\nNotes automation complete!")
    print("Check the Notes app to see your new note.")


if __name__ == "__main__":
    main()
