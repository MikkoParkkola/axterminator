#!/usr/bin/env python3
"""Self-healing locator demonstration.

Demonstrates:
- How self-healing works when elements change
- Priority order of healing strategies
- Automatic locator recovery

The self-healing system makes tests resilient to UI changes.
"""

import axterminator


def main():
    if not axterminator.is_accessibility_enabled():
        print("ERROR: Accessibility permissions required.")
        return

    print("=== Self-Healing Locator System ===\n")

    print("""
axterminator uses a 7-strategy self-healing system for robust element location.

When you find an element, it stores multiple locator strategies.
If the primary locator fails (e.g., after UI update), it automatically
tries alternatives until the element is found.

Strategy Priority:
─────────────────────────────────────────────────────────────────────
1. data_testid  - Custom test identifiers (most stable)
2. aria_label   - ARIA accessibility labels
3. identifier   - macOS accessibility identifiers
4. title        - Element title/text
5. xpath        - Structural path in accessibility tree
6. position     - Relative position within parent container
7. visual_vlm   - Visual detection using AI vision models
─────────────────────────────────────────────────────────────────────

Each strategy is tried in order until one succeeds.
The visual_vlm fallback uses AI to find elements visually,
making it possible to locate elements even when all metadata changes.
""")

    # Example with Calculator
    print("\n=== Live Demo with Calculator ===\n")

    try:
        app = axterminator.app(name="Calculator")
        print(f"Connected to Calculator (PID: {app.pid})")

        # Find an element - this stores multiple locator strategies
        try:
            element = app.find("5", timeout_ms=2000)
            print("Found button '5'")
            print("\nStored locator strategies for this element:")
            print("  - title: '5'")
            print("  - role: AXButton")
            print("  - position: relative to parent")
            print("  - visual: 'button labeled 5 in calculator'")
            print("\nIf button '5' is renamed or moved, the system will")
            print("automatically try alternative strategies to find it.")
        except RuntimeError:
            print("Button '5' not found - this may vary by macOS version")

    except RuntimeError as e:
        print(f"Could not connect to Calculator: {e}")
        print("Please open Calculator.app first")


if __name__ == "__main__":
    main()
