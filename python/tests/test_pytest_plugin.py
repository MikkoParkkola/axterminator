"""Tests for pytest plugin."""

import pytest
from unittest.mock import patch, MagicMock
from axterminator import pytest_plugin


class TestPytestPlugin:
    """Tests for pytest plugin fixtures and markers."""

    def test_is_app_running_true(self):
        """Test _is_app_running when app is running."""
        with patch("subprocess.run") as mock_run:
            mock_run.return_value = MagicMock(returncode=0)
            assert pytest_plugin._is_app_running("Finder") is True

    def test_is_app_running_false(self):
        """Test _is_app_running when app is not running."""
        with patch("subprocess.run") as mock_run:
            mock_run.return_value = MagicMock(returncode=1)
            assert pytest_plugin._is_app_running("NonExistentApp") is False

    def test_get_frontmost_app(self):
        """Test _get_frontmost_app."""
        with patch("subprocess.run") as mock_run:
            mock_run.return_value = MagicMock(
                returncode=0,
                stdout="Terminal\n"
            )
            result = pytest_plugin._get_frontmost_app()
            assert result == "Terminal"

    def test_get_frontmost_app_error(self):
        """Test _get_frontmost_app on error."""
        with patch("subprocess.run") as mock_run:
            mock_run.return_value = MagicMock(returncode=1)
            result = pytest_plugin._get_frontmost_app()
            assert result == ""


class TestFixtures:
    """Tests for plugin fixtures."""

    def test_ax_app_fixture_exists(self):
        """Test ax_app fixture is defined."""
        # Verify the fixture function exists
        assert hasattr(pytest_plugin, "ax_app")
        assert callable(pytest_plugin.ax_app)

    def test_ax_wait_fixture_exists(self):
        """Test ax_wait fixture is defined."""
        # Verify the fixture function exists
        assert hasattr(pytest_plugin, "ax_wait")
        assert callable(pytest_plugin.ax_wait)


class TestMarkers:
    """Tests for plugin markers."""

    def test_pytest_configure_registers_markers(self):
        """Test that markers are registered."""
        mock_config = MagicMock()
        pytest_plugin.pytest_configure(mock_config)

        # Check that markers were added
        calls = mock_config.addinivalue_line.call_args_list
        marker_names = [call[0][1].split(":")[0] for call in calls]

        assert "ax_background" in marker_names
        assert "ax_requires_app(name)" in marker_names
        assert "ax_slow" in marker_names
