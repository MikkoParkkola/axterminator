#!/usr/bin/env python3
"""Finder automation example.

Demonstrates:
- File browser navigation
- Creating new folders
- Column/List view switching
- Keyboard shortcuts
"""

import axterminator
import time


def main():
    if not axterminator.is_accessibility_enabled():
        print("ERROR: Accessibility permissions not granted.")
        return

    print("Connecting to Finder...")
    app = axterminator.app(name="Finder")
    print(f"Connected! PID: {app.pid}")

    # Open a new Finder window using menu
    print("\nOpening new window via File menu...")

    try:
        # Access menu bar
        file_menu = app.find("File", role="AXMenuBarItem", timeout_ms=2000)
        file_menu.click()
        time.sleep(0.3)

        new_window = app.find("New Finder Window", timeout_ms=2000)
        new_window.click()
        time.sleep(0.5)
        print("New window opened!")
    except Exception as e:
        print(f"Could not open new window: {e}")

    # Navigate to Documents
    print("\nNavigating to Documents...")
    try:
        # Use Go menu
        go_menu = app.find("Go", role="AXMenuBarItem", timeout_ms=2000)
        go_menu.click()
        time.sleep(0.3)

        documents = app.find("Documents", timeout_ms=2000)
        documents.click()
        time.sleep(0.5)
        print("Navigated to Documents!")
    except Exception as e:
        print(f"Could not navigate: {e}")

    # Switch view modes
    print("\nSwitching to Column View...")
    try:
        view_menu = app.find("View", role="AXMenuBarItem", timeout_ms=2000)
        view_menu.click()
        time.sleep(0.3)

        column_view = app.find("as Columns", timeout_ms=2000)
        column_view.click()
        time.sleep(0.3)
        print("Switched to Column View!")
    except Exception as e:
        print(f"Could not switch view: {e}")

    print("\nFinder automation complete!")


if __name__ == "__main__":
    main()
