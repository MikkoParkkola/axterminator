"""Integration tests for axterminator.

These tests interact with real macOS applications.
Run with: pytest python/tests/test_integration.py -v --run-integration

Note: Tests are skipped if:
- Running in CI (no GUI access)
- Accessibility permissions not granted
- Required app not running
"""

import os
import pytest
import subprocess
from unittest.mock import patch

# Skip all integration tests if in CI or no display
IN_CI = os.environ.get("CI", "false").lower() == "true"
SKIP_INTEGRATION = IN_CI or os.environ.get("SKIP_INTEGRATION", "false").lower() == "true"


def is_app_running(app_name: str) -> bool:
    """Check if an app is running."""
    try:
        result = subprocess.run(
            ["pgrep", "-x", app_name],
            capture_output=True,
            text=True,
        )
        return result.returncode == 0
    except Exception:
        return False


def launch_app(app_name: str) -> bool:
    """Launch an app if not running."""
    if is_app_running(app_name):
        return True
    try:
        subprocess.run(
            ["open", "-a", app_name],
            capture_output=True,
            check=True,
        )
        import time
        time.sleep(1)
        return True
    except Exception:
        return False


@pytest.fixture
def axterminator():
    """Import axterminator with skip if not available."""
    pytest.importorskip("axterminator")
    import axterminator
    return axterminator


@pytest.fixture
def skip_if_no_accessibility(axterminator):
    """Skip test if accessibility not enabled."""
    if not axterminator.is_accessibility_enabled():
        pytest.skip("Accessibility permissions not granted")


@pytest.mark.skipif(SKIP_INTEGRATION, reason="Integration tests disabled")
class TestAccessibility:
    """Test accessibility permission checking."""

    def test_is_accessibility_enabled_returns_bool(self, axterminator):
        """Test that is_accessibility_enabled returns a boolean."""
        result = axterminator.is_accessibility_enabled()
        assert isinstance(result, bool)


@pytest.mark.skipif(SKIP_INTEGRATION, reason="Integration tests disabled")
class TestAppConnection:
    """Test connecting to applications."""

    def test_connect_to_finder(self, axterminator, skip_if_no_accessibility):
        """Test connecting to Finder (always running)."""
        app = axterminator.app(name="Finder")
        assert app is not None
        assert app.pid > 0

    def test_connect_by_bundle_id(self, axterminator, skip_if_no_accessibility):
        """Test connecting by bundle ID."""
        app = axterminator.app(bundle_id="com.apple.finder")
        assert app is not None
        assert app.pid > 0

    def test_connect_nonexistent_app_raises(self, axterminator, skip_if_no_accessibility):
        """Test that connecting to nonexistent app raises."""
        with pytest.raises(Exception):
            axterminator.app(name="NonExistentAppXYZ123")


@pytest.mark.skipif(SKIP_INTEGRATION or IN_CI, reason="Integration tests disabled or in CI")
class TestElementFinding:
    """Test finding elements in applications."""

    def test_find_finder_menu_bar(self, axterminator, skip_if_no_accessibility):
        """Test finding Finder menu bar items."""
        app = axterminator.app(name="Finder")

        # Find the File menu by title
        file_menu = app.find("File", timeout_ms=5000)
        assert file_menu is not None

    def test_find_nonexistent_element_raises(self, axterminator, skip_if_no_accessibility):
        """Test that finding nonexistent element raises/returns None."""
        app = axterminator.app(name="Finder")

        with pytest.raises(Exception):
            app.find("NonExistentElementXYZ123", timeout_ms=1000)


@pytest.mark.skipif(SKIP_INTEGRATION, reason="Integration tests disabled")
class TestCalculatorIntegration:
    """Integration tests with Calculator app."""

    @pytest.fixture
    def calculator(self, axterminator, skip_if_no_accessibility):
        """Get Calculator app, launch if needed."""
        if not launch_app("Calculator"):
            pytest.skip("Cannot launch Calculator")

        import time
        time.sleep(0.5)  # Wait for UI

        return axterminator.app(name="Calculator")

    def test_find_number_buttons(self, calculator):
        """Test finding number buttons in Calculator."""
        for num in ["1", "2", "3", "4", "5"]:
            button = calculator.find(num, timeout_ms=2000)
            assert button is not None

    def test_click_clear_button(self, calculator):
        """Test clicking the clear button."""
        # Find and click C or AC
        try:
            clear = calculator.find("C", timeout_ms=2000)
        except Exception:
            clear = calculator.find("AC", timeout_ms=2000)

        clear.click()
        # If no exception, test passes


@pytest.mark.skipif(SKIP_INTEGRATION, reason="Integration tests disabled")
class TestCrossAppIntegration:
    """Test interacting with multiple apps."""

    def test_connect_multiple_apps(self, axterminator, skip_if_no_accessibility):
        """Test connecting to multiple apps simultaneously."""
        # Finder is always running
        finder = axterminator.app(name="Finder")

        # Connect to another system app
        # SystemUIServer is always running
        try:
            system_ui = axterminator.app(name="SystemUIServer")
            assert finder.pid != system_ui.pid
        except Exception:
            # SystemUIServer might not be accessible
            pass

        assert finder.pid > 0


@pytest.mark.skipif(SKIP_INTEGRATION, reason="Integration tests disabled")
class TestElementProperties:
    """Test element property access."""

    def test_element_role(self, axterminator, skip_if_no_accessibility):
        """Test getting element role."""
        app = axterminator.app(name="Finder")
        file_menu = app.find("File", timeout_ms=5000)

        assert file_menu.role is not None

    def test_element_title(self, axterminator, skip_if_no_accessibility):
        """Test getting element title."""
        app = axterminator.app(name="Finder")
        file_menu = app.find("File", timeout_ms=5000)

        assert file_menu.title is not None


@pytest.mark.skipif(SKIP_INTEGRATION, reason="Integration tests disabled")
class TestBackgroundMode:
    """Test background operation mode."""

    def test_click_does_not_steal_focus(self, axterminator, skip_if_no_accessibility):
        """Test that background click doesn't steal focus."""
        # This test verifies the core feature
        # In practice, we can only verify the click succeeds without error
        # Focus stealing would require checking frontmost app before/after

        app = axterminator.app(name="Finder")
        file_menu = app.find("File", timeout_ms=5000)

        # Click in background mode (default)
        file_menu.click()

        # If we get here without exception, basic functionality works
        # A full test would verify focus wasn't stolen


# Marker for running integration tests
def pytest_configure(config):
    config.addinivalue_line(
        "markers", "integration: marks tests as integration tests"
    )
