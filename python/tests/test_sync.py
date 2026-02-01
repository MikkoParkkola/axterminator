"""Tests for synchronization utilities."""

import time
from unittest.mock import MagicMock, patch

import pytest
from unittest.mock import patch
from axterminator.sync import (
    SyncTimeout,
    wait_for_condition,
    wait_for_element,
    wait_for_idle,
    wait_for_value,
    xpc_sync_available,
)


class TestWaitForCondition:
    """Tests for wait_for_condition function."""

    def test_condition_immediately_true(self):
        """Test condition that's immediately true."""
        result = wait_for_condition(lambda: True, timeout_ms=1000)
        assert result is True

    def test_condition_returns_value(self):
        """Test condition returning a value."""
        result = wait_for_condition(lambda: "hello", timeout_ms=1000)
        assert result == "hello"

    def test_condition_becomes_true(self):
        """Test condition that becomes true after some time."""
        start = time.perf_counter()
        counter = [0]

        def delayed_true():
            counter[0] += 1
            return counter[0] >= 3

        result = wait_for_condition(delayed_true, timeout_ms=5000, poll_interval_ms=50)
        assert result is True
        assert counter[0] >= 3

    def test_condition_timeout(self):
        """Test timeout when condition never becomes true."""
        with pytest.raises(SyncTimeout, match="Timed out"):
            wait_for_condition(
                lambda: False,
                timeout_ms=100,
                poll_interval_ms=20,
                description="test condition",
            )

    def test_condition_exception_ignored(self):
        """Test that exceptions in condition are ignored."""
        counter = [0]

        def flaky_condition():
            counter[0] += 1
            if counter[0] < 3:
                raise RuntimeError("Not ready")
            return True

        result = wait_for_condition(flaky_condition, timeout_ms=5000, poll_interval_ms=50)
        assert result is True


class TestWaitForIdle:
    """Tests for wait_for_idle function."""

    def test_wait_for_idle_returns_true(self):
        """Test that wait_for_idle returns True when app settles."""
        mock_app = MagicMock()
        # Patch _get_ui_snapshot to return stable value
        with patch("axterminator.sync._get_ui_snapshot", return_value="stable"):
            result = wait_for_idle(mock_app, timeout_ms=500, stability_count=2)
        assert result is True

    def test_wait_for_idle_respects_timeout(self):
        """Test that wait_for_idle respects timeout."""
        mock_app = MagicMock()
        start = time.perf_counter()
        # Very short timeout
        result = wait_for_idle(mock_app, timeout_ms=100, stability_count=10)
        elapsed = (time.perf_counter() - start) * 1000
        # Should return within reasonable time of timeout
        assert elapsed < 500  # Give some slack


class TestWaitForElement:
    """Tests for wait_for_element function."""

    def test_element_found_immediately(self):
        """Test element found on first try."""
        mock_app = MagicMock()
        mock_element = MagicMock()
        mock_app.find.return_value = mock_element

        result = wait_for_element(mock_app, "button", timeout_ms=1000)
        assert result == mock_element

    def test_element_found_after_retry(self):
        """Test element found after some retries."""
        mock_app = MagicMock()
        mock_element = MagicMock()
        call_count = [0]

        def side_effect(query, timeout_ms):
            call_count[0] += 1
            if call_count[0] < 3:
                raise RuntimeError("Not found")
            return mock_element

        mock_app.find.side_effect = side_effect

        result = wait_for_element(mock_app, "button", timeout_ms=5000, poll_interval_ms=50)
        assert result == mock_element
        assert call_count[0] >= 3

    def test_element_not_found_timeout(self):
        """Test timeout when element never appears."""
        mock_app = MagicMock()
        mock_app.find.side_effect = RuntimeError("Not found")

        result = wait_for_element(mock_app, "button", timeout_ms=200, poll_interval_ms=50)
        assert result is None


class TestWaitForValue:
    """Tests for wait_for_value function."""

    def test_value_matches_immediately(self):
        """Test value matches immediately."""
        mock_element = MagicMock()
        mock_element.value = "expected"

        result = wait_for_value(mock_element, "expected", timeout_ms=1000)
        assert result is True

    def test_value_matches_after_change(self):
        """Test value matches after it changes."""
        mock_element = MagicMock()
        call_count = [0]

        @property
        def changing_value(self):
            call_count[0] += 1
            return "expected" if call_count[0] >= 3 else "other"

        type(mock_element).value = changing_value

        result = wait_for_value(mock_element, "expected", timeout_ms=5000, poll_interval_ms=50)
        assert result is True

    def test_value_never_matches(self):
        """Test timeout when value never matches."""
        mock_element = MagicMock()
        mock_element.value = "wrong"

        result = wait_for_value(mock_element, "expected", timeout_ms=200, poll_interval_ms=50)
        assert result is False


class TestXPCSyncAvailable:
    """Tests for XPC sync availability check."""

    def test_xpc_sync_not_available(self):
        """Test XPC sync is not available (stub returns False)."""
        result = xpc_sync_available()
        assert result is False


class TestSyncTimeout:
    """Tests for SyncTimeout exception."""

    def test_sync_timeout_message(self):
        """Test SyncTimeout has correct message."""
        exc = SyncTimeout("Custom message")
        assert str(exc) == "Custom message"

    def test_sync_timeout_is_exception(self):
        """Test SyncTimeout is an Exception."""
        assert issubclass(SyncTimeout, Exception)
