#!/usr/bin/env python3
"""Basic usage example for axterminator.

Demonstrates:
- Connecting to an application
- Finding elements
- Clicking buttons
- Reading values
- Background operation (no focus steal)
"""

import axterminator


def main():
    # Check accessibility permissions first
    if not axterminator.is_accessibility_enabled():
        print("ERROR: Accessibility permissions not granted.")
        print("Go to System Preferences > Privacy & Security > Accessibility")
        print("and add your terminal/IDE to the allowed list.")
        return

    # Connect to Calculator (must be running)
    print("Connecting to Calculator...")
    app = axterminator.app(name="Calculator")
    print(f"Connected! PID: {app.pid}")

    # Find and click number buttons (background operation - won't steal focus)
    print("\nPerforming calculation: 5 + 3 = ")

    # These operations happen in the background
    button_5 = app.find("5", timeout_ms=2000)
    button_5.click()

    button_plus = app.find("+", timeout_ms=2000)
    button_plus.click()

    button_3 = app.find("3", timeout_ms=2000)
    button_3.click()

    button_equals = app.find("=", timeout_ms=2000)
    button_equals.click()

    # Read the result
    # Note: Calculator display accessibility varies by macOS version
    print("Calculation performed!")
    print("\nNote: This script ran in the background without stealing focus.")


if __name__ == "__main__":
    main()
