"""Tests for CLI module."""

import pytest
from unittest.mock import patch, MagicMock
from axterminator.cli import main


class TestCLI:
    """Tests for CLI commands."""

    def test_no_command_shows_help(self, capsys):
        """Test that no command shows help."""
        result = main([])
        assert result == 0

    def test_version(self, capsys):
        """Test version command."""
        with pytest.raises(SystemExit) as exc_info:
            main(["--version"])
        assert exc_info.value.code == 0

    def test_check_enabled(self, capsys):
        """Test check command when accessibility enabled."""
        with patch("axterminator.is_accessibility_enabled", return_value=True):
            result = main(["check"])
        assert result == 0
        captured = capsys.readouterr()
        assert "ENABLED" in captured.out

    def test_check_disabled(self, capsys):
        """Test check command when accessibility disabled."""
        with patch("axterminator.is_accessibility_enabled", return_value=False):
            result = main(["check"])
        assert result == 1
        captured = capsys.readouterr()
        assert "DISABLED" in captured.out

    def test_find_success(self, capsys):
        """Test find command success."""
        mock_element = MagicMock()
        mock_element.role = "AXButton"
        mock_element.title = "Save"
        mock_element.value = None

        mock_app = MagicMock()
        mock_app.find.return_value = mock_element

        with patch("axterminator.app", return_value=mock_app):
            result = main(["find", "TestApp", "Save"])

        assert result == 0
        captured = capsys.readouterr()
        assert "Found" in captured.out

    def test_find_not_found(self, capsys):
        """Test find command when element not found."""
        mock_app = MagicMock()
        mock_app.find.side_effect = RuntimeError("Element not found")

        with patch("axterminator.app", return_value=mock_app):
            result = main(["find", "TestApp", "NonExistent"])

        assert result == 1

    def test_click_success(self, capsys):
        """Test click command success."""
        mock_element = MagicMock()
        mock_element.role = "AXButton"
        mock_element.title = "Save"

        mock_app = MagicMock()
        mock_app.find.return_value = mock_element

        with patch("axterminator.app", return_value=mock_app):
            with patch("axterminator.BACKGROUND", "BACKGROUND"):
                result = main(["click", "TestApp", "Save"])

        assert result == 0
        mock_element.click.assert_called_once()

    def test_type_success(self, capsys):
        """Test type command success."""
        mock_element = MagicMock()
        mock_element.role = "AXTextField"

        mock_app = MagicMock()
        mock_app.find.return_value = mock_element

        with patch("axterminator.app", return_value=mock_app):
            result = main(["type", "TestApp", "textfield", "Hello World"])

        assert result == 0
        mock_element.type_text.assert_called_once_with("Hello World")
