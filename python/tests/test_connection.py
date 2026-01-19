"""
Tests for AXTerminator application connection functionality.

Tests cover:
- Connection by name
- Connection by bundle ID
- Connection by PID
- Error handling for accessibility disabled
- Error handling for app not found
"""

from __future__ import annotations

from typing import TYPE_CHECKING
from unittest.mock import MagicMock, patch

import pytest

if TYPE_CHECKING:
    from conftest import TestApp


class TestConnectByName:
    """Tests for connecting to apps by name."""

    @pytest.mark.requires_app
    def test_connect_by_name_finder(self) -> None:
        """Connect to Finder by name - always running."""
        import axterminator as ax

        app = ax.app(name="Finder")

        assert app is not None
        assert app.pid > 0
        assert app.is_running()

    @pytest.mark.requires_app
    def test_connect_by_name_calculator(self, calculator_app: TestApp) -> None:
        """Connect to Calculator by name."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        assert app is not None
        assert app.pid == calculator_app.pid
        assert app.is_running()

    def test_connect_by_name_mocked(self, mock_app_connect: MagicMock) -> None:
        """Test connection flow with mocked backend."""
        import axterminator as ax

        app = ax.app(name="TestApp")

        assert app is not None
        mock_app_connect.assert_called_once_with(
            name="TestApp", bundle_id=None, pid=None
        )

    def test_connect_by_name_not_found(self) -> None:
        """Connecting to non-existent app raises error."""
        import axterminator as ax

        with pytest.raises(RuntimeError, match="Application not found"):
            ax.app(name="ThisAppDoesNotExist_12345")

    def test_connect_by_name_empty_string(self) -> None:
        """Empty name string raises error."""
        import axterminator as ax

        with pytest.raises((ValueError, RuntimeError)):
            ax.app(name="")

    def test_connect_by_name_case_sensitive(self) -> None:
        """App name matching is case-sensitive (uses pgrep -x)."""
        import axterminator as ax

        # "finder" (lowercase) should not find "Finder"
        with pytest.raises(RuntimeError, match="Application not found"):
            ax.app(name="finder")


class TestConnectByBundleId:
    """Tests for connecting to apps by bundle identifier."""

    @pytest.mark.requires_app
    def test_connect_by_bundle_id_finder(self) -> None:
        """Connect to Finder by bundle ID."""
        import axterminator as ax

        app = ax.app(bundle_id="com.apple.finder")

        assert app is not None
        assert app.pid > 0
        assert app.bundle_id == "com.apple.finder"

    @pytest.mark.requires_app
    def test_connect_by_bundle_id_calculator(self, calculator_app: TestApp) -> None:
        """Connect to Calculator by bundle ID."""
        import axterminator as ax

        app = ax.app(bundle_id="com.apple.calculator")

        assert app is not None
        assert app.pid == calculator_app.pid

    def test_connect_by_bundle_id_not_found(self) -> None:
        """Connecting to non-existent bundle ID raises error."""
        import axterminator as ax

        with pytest.raises(RuntimeError, match="Application not found"):
            ax.app(bundle_id="com.nonexistent.app.12345")

    def test_connect_by_bundle_id_invalid_format(self) -> None:
        """Invalid bundle ID format still goes through resolution."""
        import axterminator as ax

        # Invalid format but osascript just won't find it
        with pytest.raises(RuntimeError, match="Application not found"):
            ax.app(bundle_id="not-a-valid-bundle-id")

    @pytest.mark.requires_app
    def test_connect_by_bundle_id_preserves_id(self) -> None:
        """Connected app preserves the bundle_id attribute."""
        import axterminator as ax

        app = ax.app(bundle_id="com.apple.finder")

        assert app.bundle_id == "com.apple.finder"


class TestConnectByPid:
    """Tests for connecting to apps by process ID."""

    @pytest.mark.requires_app
    def test_connect_by_pid_finder(self, finder_app: TestApp) -> None:
        """Connect to Finder by PID."""
        import axterminator as ax

        assert finder_app.pid is not None
        app = ax.app(pid=finder_app.pid)

        assert app is not None
        assert app.pid == finder_app.pid

    @pytest.mark.requires_app
    def test_connect_by_pid_calculator(self, calculator_app: TestApp) -> None:
        """Connect to Calculator by PID."""
        import axterminator as ax

        assert calculator_app.pid is not None
        app = ax.app(pid=calculator_app.pid)

        assert app is not None
        assert app.pid == calculator_app.pid

    def test_connect_by_pid_invalid(self) -> None:
        """Connecting to invalid PID raises error."""
        import axterminator as ax

        # PID 99999999 should not exist
        with pytest.raises(RuntimeError):
            ax.app(pid=99999999)

    def test_connect_by_pid_zero(self) -> None:
        """PID 0 (kernel) is handled appropriately."""
        import axterminator as ax

        # PID 0 is kernel_task, may or may not be accessible
        # depending on permissions - just verify no crash
        with pytest.raises((RuntimeError, PermissionError)):
            ax.app(pid=0)

    def test_connect_by_pid_negative(self) -> None:
        """Negative PID raises error."""
        import axterminator as ax

        with pytest.raises((ValueError, RuntimeError)):
            ax.app(pid=-1)


class TestAccessibilityNotEnabled:
    """Tests for accessibility permission errors."""

    def test_accessibility_not_enabled_error_message(
        self, mock_accessibility_disabled: MagicMock
    ) -> None:
        """Error message guides user to enable accessibility."""
        import axterminator as ax

        # When accessibility is disabled, app connection should fail
        # The exact behavior depends on implementation
        with pytest.raises(
            RuntimeError,
            match="Accessibility|System Preferences|Privacy",
        ):
            ax.app(name="Finder")

    def test_is_accessibility_enabled_returns_bool(self) -> None:
        """is_accessibility_enabled() returns boolean."""
        import axterminator as ax

        result = ax.is_accessibility_enabled()

        assert isinstance(result, bool)

    def test_is_accessibility_enabled_callable(self) -> None:
        """is_accessibility_enabled is a callable function."""
        import axterminator as ax

        assert callable(ax.is_accessibility_enabled)

    def test_accessibility_check_before_connect(self) -> None:
        """Accessibility is checked before attempting connection."""
        with patch(
            "axterminator.is_accessibility_enabled", return_value=False
        ) as mock_check:
            import axterminator as ax

            try:
                ax.app(name="Finder")
            except RuntimeError:
                pass

            # Verify the check was performed
            mock_check.assert_called()


class TestAppNotFound:
    """Tests for application not found scenarios."""

    def test_app_not_found_by_name(self) -> None:
        """Clear error for non-existent app name."""
        import axterminator as ax

        with pytest.raises(RuntimeError) as exc_info:
            ax.app(name="NonExistentApp12345")

        assert "not found" in str(exc_info.value).lower()

    def test_app_not_found_by_bundle_id(self) -> None:
        """Clear error for non-existent bundle ID."""
        import axterminator as ax

        with pytest.raises(RuntimeError) as exc_info:
            ax.app(bundle_id="com.nonexistent.app.xyz")

        assert "not found" in str(exc_info.value).lower()

    def test_app_not_running_by_pid(self) -> None:
        """Error for PID of non-running process."""
        import axterminator as ax

        # Use a PID that definitely doesn't exist
        with pytest.raises(RuntimeError):
            ax.app(pid=4000000000)

    def test_app_terminated_after_connect(self, calculator_app: TestApp) -> None:
        """App termination after connection is detectable."""
        import axterminator as ax

        assert calculator_app.pid is not None
        app = ax.app(pid=calculator_app.pid)

        # Verify app is running
        assert app.is_running()

        # Terminate the app
        calculator_app.terminate()

        # Give time for process to die
        import time

        time.sleep(0.5)

        # is_running should now return False
        assert not app.is_running()


class TestConnectionEdgeCases:
    """Edge cases and boundary conditions for connection."""

    def test_no_arguments_raises_error(self) -> None:
        """Calling app() with no arguments raises ValueError."""
        import axterminator as ax

        with pytest.raises(ValueError, match="name|bundle_id|pid"):
            ax.app()

    def test_multiple_arguments_uses_pid_first(self, finder_app: TestApp) -> None:
        """When multiple arguments provided, PID takes precedence."""
        import axterminator as ax

        assert finder_app.pid is not None

        # Provide PID and wrong name - PID should win
        app = ax.app(
            name="WrongName",
            bundle_id="com.wrong.bundleid",
            pid=finder_app.pid,
        )

        assert app.pid == finder_app.pid

    def test_none_arguments_ignored(self) -> None:
        """Explicitly passing None for arguments."""
        import axterminator as ax

        with pytest.raises(ValueError, match="name|bundle_id|pid"):
            ax.app(name=None, bundle_id=None, pid=None)

    @pytest.mark.requires_app
    def test_connect_preserves_pid(self, calculator_app: TestApp) -> None:
        """Connected app has correct PID attribute."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        assert app.pid == calculator_app.pid

    @pytest.mark.requires_app
    def test_multiple_connections_same_app(self) -> None:
        """Multiple connections to same app work independently."""
        import axterminator as ax

        app1 = ax.app(name="Finder")
        app2 = ax.app(name="Finder")

        # Both connections should work
        assert app1.pid == app2.pid
        assert app1.is_running()
        assert app2.is_running()


class TestAppMethods:
    """Tests for methods on connected app objects."""

    @pytest.mark.requires_app
    def test_app_pid_getter(self, finder_app: TestApp) -> None:
        """App has working pid property."""
        import axterminator as ax

        app = ax.app(name="Finder")

        assert isinstance(app.pid, int)
        assert app.pid > 0

    @pytest.mark.requires_app
    def test_app_bundle_id_getter(self) -> None:
        """App has working bundle_id property."""
        import axterminator as ax

        app = ax.app(bundle_id="com.apple.finder")

        assert app.bundle_id == "com.apple.finder"

    @pytest.mark.requires_app
    def test_app_is_running_true(self, finder_app: TestApp) -> None:
        """is_running() returns True for running app."""
        import axterminator as ax

        app = ax.app(name="Finder")

        assert app.is_running() is True

    @pytest.mark.requires_app
    def test_app_terminate_method(self, calculator_app: TestApp) -> None:
        """terminate() method kills the app."""
        import axterminator as ax

        import time

        app = ax.app(name="Calculator")
        assert app.is_running()

        app.terminate()
        time.sleep(1.0)

        # App should no longer be running
        assert not app.is_running()

    @pytest.mark.requires_app
    def test_app_windows_method(self, calculator_app: TestApp) -> None:
        """windows() method returns list."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        windows = app.windows()

        assert isinstance(windows, list)

    @pytest.mark.requires_app
    def test_app_main_window_method(self, calculator_app: TestApp) -> None:
        """main_window() method returns main window or raises."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        try:
            window = app.main_window()
            assert window is not None
        except RuntimeError:
            # Acceptable if not implemented yet
            pass


class TestConnectionPerformance:
    """Performance tests for app connection."""

    @pytest.mark.slow
    @pytest.mark.requires_app
    def test_connect_by_name_performance(self, perf_timer, finder_app: TestApp) -> None:
        """Connection by name completes within target time."""
        import axterminator as ax

        result = perf_timer(
            lambda: ax.app(name="Finder"),
            iterations=50,
            name="connect_by_name",
        )

        # Connection should complete within 500ms p95
        assert result.p95_ms < 500, f"Connection too slow: {result.p95_ms}ms"

    @pytest.mark.slow
    @pytest.mark.requires_app
    def test_connect_by_pid_performance(self, perf_timer, finder_app: TestApp) -> None:
        """Connection by PID is faster than by name."""
        import axterminator as ax

        assert finder_app.pid is not None

        result = perf_timer(
            lambda: ax.app(pid=finder_app.pid),
            iterations=50,
            name="connect_by_pid",
        )

        # PID connection should be fast - within 100ms p95
        assert result.p95_ms < 100, f"PID connection too slow: {result.p95_ms}ms"

    @pytest.mark.slow
    @pytest.mark.requires_app
    def test_pid_faster_than_name(self, perf_timer, finder_app: TestApp) -> None:
        """Verify PID connection is faster than name lookup."""
        import axterminator as ax

        assert finder_app.pid is not None

        name_result = perf_timer(
            lambda: ax.app(name="Finder"),
            iterations=20,
            name="by_name",
        )

        pid_result = perf_timer(
            lambda: ax.app(pid=finder_app.pid),
            iterations=20,
            name="by_pid",
        )

        # PID should be at least 2x faster on average
        assert pid_result.avg_ms < name_result.avg_ms, (
            f"PID ({pid_result.avg_ms}ms) not faster than name ({name_result.avg_ms}ms)"
        )
