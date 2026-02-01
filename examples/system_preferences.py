#!/usr/bin/env python3
"""System Preferences automation example.

Demonstrates:
- Navigating complex UI hierarchies
- Working with sidebars and panes
- Background operation
"""

import axterminator
import time


def main():
    if not axterminator.is_accessibility_enabled():
        print("ERROR: Accessibility permissions not granted.")
        return

    # Open System Settings (macOS Ventura+) or System Preferences
    print("Connecting to System Settings...")

    try:
        app = axterminator.app(name="System Settings", launch=True)
    except Exception:
        # Fallback for older macOS
        app = axterminator.app(name="System Preferences", launch=True)

    print(f"Connected! PID: {app.pid}")
    time.sleep(1)  # Wait for UI to load

    # Navigate to Accessibility settings
    print("\nNavigating to Accessibility...")

    # Find and click Accessibility in sidebar
    accessibility = app.find("Accessibility", timeout_ms=5000)
    accessibility.click()
    time.sleep(0.5)

    # Explore the accessibility tree
    print("\nAccessibility panel loaded.")
    print("Available options in this pane:")

    # Find all clickable items in the main content area
    try:
        # Look for common accessibility options
        options = ["VoiceOver", "Zoom", "Display", "Spoken Content"]
        for opt in options:
            try:
                element = app.find(opt, timeout_ms=1000)
                print(f"  - {opt}: Found")
            except Exception:
                print(f"  - {opt}: Not visible")
    except Exception as e:
        print(f"Could not enumerate options: {e}")

    print("\nSystem Settings automation complete!")
    print("Note: All operations ran in the background.")


if __name__ == "__main__":
    main()
