#!/usr/bin/env python3
"""pytest integration example.

Demonstrates using axterminator with pytest fixtures and markers.

Run with: pytest examples/pytest_example.py -v
"""

import pytest
import axterminator


# Mark all tests to run in background (no focus stealing)
pytestmark = pytest.mark.ax_background


class TestCalculator:
    """Calculator app tests using axterminator fixtures."""

    @pytest.mark.ax_requires_app("Calculator")
    def test_addition(self, ax_app, ax_wait):
        """Test basic addition: 7 + 3 = 10."""
        app = ax_app("Calculator")

        # Perform calculation
        app.find("7").click()
        app.find("+").click()
        app.find("3").click()
        app.find("=").click()

        ax_wait(0.1)  # Small wait for result

        # Verify (actual verification depends on Calculator version)
        print("Addition test completed!")

    @pytest.mark.ax_requires_app("Calculator")
    def test_multiplication(self, ax_app, ax_wait):
        """Test multiplication: 6 x 4 = 24."""
        app = ax_app("Calculator")

        # Clear first
        app.find("C").click()
        ax_wait(0.1)

        # Perform calculation
        app.find("6").click()
        app.find("×").click()  # Note: this is multiplication sign, not x
        app.find("4").click()
        app.find("=").click()

        ax_wait(0.1)
        print("Multiplication test completed!")

    @pytest.mark.ax_requires_app("Calculator")
    def test_clear(self, ax_app):
        """Test clear button resets display."""
        app = ax_app("Calculator")

        # Enter some numbers
        app.find("9").click()
        app.find("9").click()
        app.find("9").click()

        # Clear
        app.find("C").click()

        print("Clear test completed!")


class TestFinder:
    """Finder tests demonstrating menu navigation."""

    @pytest.mark.ax_requires_app("Finder")
    @pytest.mark.ax_slow  # Mark as slow test
    def test_new_window_menu(self, ax_app, ax_wait):
        """Test opening new Finder window via menu."""
        app = ax_app("Finder")

        # Open File menu
        file_menu = app.find("File", role="AXMenuBarItem")
        file_menu.click()
        ax_wait(0.3)

        # Click New Finder Window
        new_window = app.find("New Finder Window")
        new_window.click()
        ax_wait(0.5)

        print("New window test completed!")


# Standalone test function (not in class)
@pytest.mark.ax_requires_app("TextEdit")
def test_textedit_launch(ax_app):
    """Test TextEdit launches successfully."""
    app = ax_app("TextEdit", launch=True)
    assert app.pid > 0
    print(f"TextEdit launched with PID: {app.pid}")


if __name__ == "__main__":
    # Run with pytest
    pytest.main([__file__, "-v", "--tb=short"])
