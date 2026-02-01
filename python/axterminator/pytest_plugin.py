"""pytest plugin for axterminator.

Provides fixtures and markers for GUI testing.

Usage in conftest.py:
    pytest_plugins = ["axterminator.pytest_plugin"]

Or install as package and it auto-registers.

Fixtures:
    ax_app(name) - Connect to application
    ax_calculator - Pre-connected Calculator app
    ax_finder - Pre-connected Finder app
    ax_check_accessibility - Skip if no permissions

Markers:
    @pytest.mark.ax_background - Verify no focus steal
    @pytest.mark.ax_requires_app(name) - Skip if app not running
"""

from __future__ import annotations

import subprocess
import time
from typing import TYPE_CHECKING, Callable, Generator, Optional

import pytest

if TYPE_CHECKING:
    from axterminator import AXApp


def pytest_configure(config: pytest.Config) -> None:
    """Register custom markers."""
    config.addinivalue_line(
        "markers",
        "ax_background: verify operation doesn't steal focus",
    )
    config.addinivalue_line(
        "markers",
        "ax_requires_app(name): skip if application is not running",
    )
    config.addinivalue_line(
        "markers",
        "ax_slow: test may take longer due to UI interactions",
    )


def pytest_collection_modifyitems(
    config: pytest.Config, items: list[pytest.Item]
) -> None:
    """Process markers before test execution."""
    for item in items:
        # Check ax_requires_app marker
        for marker in item.iter_markers(name="ax_requires_app"):
            app_name = marker.args[0] if marker.args else None
            if app_name and not _is_app_running(app_name):
                item.add_marker(
                    pytest.mark.skip(reason=f"Application '{app_name}' is not running")
                )


def _is_app_running(name: str) -> bool:
    """Check if an application is running."""
    result = subprocess.run(
        ["pgrep", "-x", name],
        capture_output=True,
    )
    return result.returncode == 0


def _get_frontmost_app() -> str:
    """Get the name of the frontmost application."""
    result = subprocess.run(
        [
            "osascript",
            "-e",
            'tell application "System Events" to get name of first application process whose frontmost is true',
        ],
        capture_output=True,
        text=True,
    )
    return result.stdout.strip() if result.returncode == 0 else ""


@pytest.fixture
def ax_check_accessibility() -> None:
    """Skip test if accessibility permissions are not granted."""
    import axterminator

    if not axterminator.is_accessibility_enabled():
        pytest.skip("Accessibility permissions not granted")


@pytest.fixture
def ax_app() -> Callable[[str], "AXApp"]:
    """Factory fixture to connect to applications.

    Usage:
        def test_something(ax_app):
            app = ax_app("Calculator")
            app.find("5").click()
    """
    import axterminator

    def _connect(
        name: Optional[str] = None,
        bundle_id: Optional[str] = None,
        pid: Optional[int] = None,
    ) -> "AXApp":
        return axterminator.app(name=name, bundle_id=bundle_id, pid=pid)

    return _connect


@pytest.fixture
def ax_calculator(ax_check_accessibility: None) -> Generator["AXApp", None, None]:
    """Pre-connected Calculator app fixture.

    Launches Calculator if not running, cleans up after test.
    """
    import axterminator

    # Check if already running
    was_running = _is_app_running("Calculator")

    if not was_running:
        subprocess.Popen(
            ["open", "-a", "Calculator"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        time.sleep(1.0)

    app = axterminator.app(name="Calculator")
    yield app

    # Cleanup: quit if we launched it
    if not was_running:
        subprocess.run(
            ["osascript", "-e", 'tell application "Calculator" to quit'],
            capture_output=True,
        )


@pytest.fixture
def ax_finder(ax_check_accessibility: None) -> "AXApp":
    """Pre-connected Finder app fixture.

    Finder is always running, no cleanup needed.
    """
    import axterminator

    return axterminator.app(name="Finder")


@pytest.fixture
def ax_no_focus_steal() -> Generator[Callable[[], None], None, None]:
    """Context manager to verify no focus steal.

    Usage:
        def test_background(ax_no_focus_steal, ax_app):
            check = ax_no_focus_steal()
            app = ax_app("Calculator")
            app.find("5").click()
            check()  # Verifies focus didn't change
    """
    initial_app = _get_frontmost_app()

    def _check() -> None:
        current_app = _get_frontmost_app()
        if current_app != initial_app:
            pytest.fail(
                f"Focus changed from '{initial_app}' to '{current_app}'"
            )

    yield _check


@pytest.fixture
def ax_wait() -> Callable[[int], None]:
    """Wait fixture for timing-sensitive tests.

    Usage:
        def test_animation(ax_wait, ax_app):
            app.find("button").click()
            ax_wait(500)  # Wait 500ms for animation
    """
    def _wait(ms: int) -> None:
        time.sleep(ms / 1000)

    return _wait


# Auto-register plugin
def pytest_addhooks(pluginmanager: pytest.PytestPluginManager) -> None:
    """Register the plugin."""
    pass  # Markers are registered in pytest_configure
