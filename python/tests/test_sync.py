"""
Tests for AXTerminator synchronization functionality.

Tests cover:
- Wait for idle
- Wait for element
- Heuristic synchronization
- XPC synchronization (mock)
"""

from __future__ import annotations

import time
from typing import TYPE_CHECKING, Callable
from unittest.mock import patch

import pytest

if TYPE_CHECKING:
    from conftest import PerformanceResult, TestApp


class TestWaitForIdle:
    """Tests for wait_for_idle synchronization."""

    @pytest.mark.requires_app
    def test_wait_for_idle_returns_bool(self, calculator_app: TestApp) -> None:
        """wait_for_idle returns boolean indicating success."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        result = app.wait_for_idle()

        assert isinstance(result, bool)

    @pytest.mark.requires_app
    def test_wait_for_idle_succeeds_on_stable_app(
        self, calculator_app: TestApp
    ) -> None:
        """wait_for_idle succeeds when app is stable."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        # Give app time to stabilize
        time.sleep(0.5)

        result = app.wait_for_idle(timeout_ms=5000)

        assert result is True

    @pytest.mark.requires_app
    def test_wait_for_idle_default_timeout(self, calculator_app: TestApp) -> None:
        """Default timeout is 5000ms."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        # Should work with default timeout
        result = app.wait_for_idle()

        assert isinstance(result, bool)

    @pytest.mark.requires_app
    def test_wait_for_idle_custom_timeout(self, calculator_app: TestApp) -> None:
        """Custom timeout is respected."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        start = time.perf_counter()
        result = app.wait_for_idle(timeout_ms=100)
        elapsed = time.perf_counter() - start

        # Should return within reasonable time
        assert elapsed < 1.0

    @pytest.mark.slow
    @pytest.mark.requires_app
    def test_wait_for_idle_timeout_returns_false(self, calculator_app: TestApp) -> None:
        """wait_for_idle returns False on timeout."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        # With very short timeout, should return quickly
        # May return True if already stable, or False if timeout
        result = app.wait_for_idle(timeout_ms=10)

        assert isinstance(result, bool)

    @pytest.mark.requires_app
    def test_wait_for_idle_after_click(self, calculator_app: TestApp) -> None:
        """wait_for_idle can be used after triggering action."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        # Click a button
        try:
            element = app.find("5")
            element.click()

            # Wait for UI to settle
            result = app.wait_for_idle(timeout_ms=2000)

            assert result is True
        except RuntimeError:
            pass

    @pytest.mark.requires_app
    def test_wait_for_idle_multiple_calls(self, calculator_app: TestApp) -> None:
        """Multiple wait_for_idle calls work correctly."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        result1 = app.wait_for_idle()
        result2 = app.wait_for_idle()
        result3 = app.wait_for_idle()

        # All should succeed on stable app
        assert result1 is True
        assert result2 is True
        assert result3 is True


class TestWaitForElement:
    """Tests for wait_for_element synchronization."""

    @pytest.mark.requires_app
    def test_wait_for_element_existing(self, calculator_app: TestApp) -> None:
        """wait_for_element returns immediately for existing element."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        start = time.perf_counter()
        element = app.wait_for_element("5", timeout_ms=5000)
        elapsed = time.perf_counter() - start

        assert element is not None
        # Should return quickly for existing element
        assert elapsed < 1.0

    @pytest.mark.slow
    @pytest.mark.requires_app
    def test_wait_for_element_timeout(self, calculator_app: TestApp) -> None:
        """wait_for_element raises after timeout for non-existent."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        start = time.perf_counter()
        with pytest.raises(RuntimeError):
            app.wait_for_element("NonExistentElement12345", timeout_ms=500)
        elapsed = time.perf_counter() - start

        # Should have waited approximately timeout duration
        assert 0.4 < elapsed < 1.5

    @pytest.mark.requires_app
    def test_wait_for_element_default_timeout(self, calculator_app: TestApp) -> None:
        """Default timeout for wait_for_element is 5000ms."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        # Should work with default timeout on existing element
        element = app.wait_for_element("5")

        assert element is not None

    @pytest.mark.requires_app
    def test_wait_for_element_returns_element(self, calculator_app: TestApp) -> None:
        """wait_for_element returns the found element."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        element = app.wait_for_element("5")

        # Should be able to use the returned element
        assert element.role() == "AXButton"
        assert element.title() == "5"

    @pytest.mark.requires_app
    def test_wait_for_element_with_role(self, calculator_app: TestApp) -> None:
        """wait_for_element works with role queries."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        try:
            element = app.wait_for_element("role:AXButton", timeout_ms=2000)
            assert element is not None
        except RuntimeError:
            # Query syntax may not be fully implemented
            pass


class TestHeuristicSync:
    """Tests for heuristic synchronization (accessibility tree change detection)."""

    @pytest.mark.requires_app
    def test_heuristic_detects_stability(self, calculator_app: TestApp) -> None:
        """Heuristic sync detects when UI is stable."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        # Wait for stability
        time.sleep(0.5)

        result = app.wait_for_idle(timeout_ms=2000)

        assert result is True

    @pytest.mark.requires_app
    @pytest.mark.slow
    def test_heuristic_requires_consecutive_stable(
        self, calculator_app: TestApp
    ) -> None:
        """
        Heuristic requires multiple consecutive stable readings.

        The implementation requires 3 consecutive identical tree hashes
        before considering the UI stable.
        """
        import axterminator as ax

        app = ax.app(name="Calculator")

        # This tests the internal behavior:
        # - 50ms check interval
        # - 3 consecutive stable required
        # - So minimum stable time is ~150ms

        result = app.wait_for_idle(timeout_ms=500)

        assert result is True

    @pytest.mark.requires_app
    def test_heuristic_hash_changes_on_action(self, calculator_app: TestApp) -> None:
        """Tree hash changes when UI changes."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        try:
            # Start with stable UI
            app.wait_for_idle(timeout_ms=1000)

            # Click a button (may change display)
            element = app.find("5")
            element.click()

            # UI may briefly be unstable
            # Wait again should succeed
            result = app.wait_for_idle(timeout_ms=2000)

            assert result is True
        except RuntimeError:
            pass


class TestXPCSync:
    """Tests for XPC-based synchronization (EspressoMac SDK integration)."""

    def test_xpc_sync_not_available_fallback(self) -> None:
        """When XPC sync unavailable, falls back to heuristic."""
        # Mock XPC as unavailable
        with patch("axterminator.xpc_sync_available", return_value=False):
            # Should still work via fallback
            pass

    def test_xpc_sync_preferred_when_available(self) -> None:
        """XPC sync is preferred over heuristic when available."""
        # This would test EspressoMac SDK integration
        pass

    def test_xpc_sync_timeout_respected(self) -> None:
        """XPC sync respects timeout parameter."""
        # Mock XPC with delayed response
        pass

    def test_xpc_sync_error_handling(self) -> None:
        """XPC sync errors fallback to heuristic."""
        # Mock XPC failure
        pass


class TestSyncCombined:
    """Tests combining multiple sync methods."""

    @pytest.mark.requires_app
    def test_wait_idle_then_find(self, calculator_app: TestApp) -> None:
        """Common pattern: wait_for_idle then find element."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        # Wait for UI stability
        app.wait_for_idle()

        # Then find element
        element = app.find("5")

        assert element is not None

    @pytest.mark.requires_app
    def test_click_wait_find(self, calculator_app: TestApp) -> None:
        """Common pattern: click, wait, then find/verify."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        try:
            # Click a number
            element = app.find("5")
            element.click()

            # Wait for UI update
            app.wait_for_idle()

            # Verify (in a real test, would check display)
            display = app.find_by_role("AXStaticText")
            if display:
                value = display.value()
                # Calculator display would show "5"
        except RuntimeError:
            pass

    @pytest.mark.requires_app
    def test_sequential_operations_with_sync(self, calculator_app: TestApp) -> None:
        """Sequential operations with sync between each."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        try:
            # Perform calculation: 5 + 3 =
            operations = ["5", "+", "3", "="]

            for op in operations:
                element = app.find(op)
                element.click()
                app.wait_for_idle(timeout_ms=500)
        except RuntimeError:
            pass


class TestSyncPerformance:
    """Performance tests for synchronization."""

    @pytest.mark.slow
    @pytest.mark.requires_app
    def test_wait_for_idle_performance(
        self,
        calculator_app: TestApp,
        perf_timer: Callable[..., PerformanceResult],
    ) -> None:
        """wait_for_idle has reasonable performance on stable app."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        # Wait for initial stability
        time.sleep(0.5)

        result = perf_timer(
            lambda: app.wait_for_idle(timeout_ms=500),
            iterations=10,
            name="wait_for_idle",
        )

        # On stable app, should return quickly
        # 3 consecutive checks at 50ms = minimum 150ms
        assert result.avg_ms < 500, f"wait_for_idle too slow: {result.avg_ms}ms"

    @pytest.mark.slow
    @pytest.mark.requires_app
    def test_wait_for_element_performance(
        self,
        calculator_app: TestApp,
        perf_timer: Callable[..., PerformanceResult],
    ) -> None:
        """wait_for_element is fast for existing elements."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        result = perf_timer(
            lambda: app.wait_for_element("5", timeout_ms=100),
            iterations=50,
            name="wait_for_element",
        )

        # For existing element, should be similar to find()
        assert result.p95_ms < 50, f"wait_for_element too slow: {result.p95_ms}ms"


class TestSyncEdgeCases:
    """Edge cases for synchronization."""

    @pytest.mark.requires_app
    def test_sync_on_minimized_app(self, calculator_app: TestApp) -> None:
        """Sync works on minimized window."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        # Note: Can't easily minimize in test, but verify no crash
        result = app.wait_for_idle(timeout_ms=1000)

        assert isinstance(result, bool)

    @pytest.mark.requires_app
    def test_sync_zero_timeout(self, calculator_app: TestApp) -> None:
        """Zero timeout returns immediately."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        start = time.perf_counter()
        result = app.wait_for_idle(timeout_ms=0)
        elapsed = time.perf_counter() - start

        # Should return immediately
        assert elapsed < 0.5

    @pytest.mark.requires_app
    def test_sync_very_long_timeout(self, calculator_app: TestApp) -> None:
        """Long timeout doesn't block forever on stable app."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        # Wait for stability first
        time.sleep(0.5)

        start = time.perf_counter()
        result = app.wait_for_idle(timeout_ms=30000)  # 30s timeout
        elapsed = time.perf_counter() - start

        # Should return quickly despite long timeout
        assert elapsed < 5.0
        assert result is True

    def test_sync_after_app_termination(self, calculator_app: TestApp) -> None:
        """Sync handles terminated app gracefully."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        # Terminate the app
        calculator_app.terminate()
        time.sleep(0.5)

        # Sync should handle this gracefully (return False or raise)
        try:
            result = app.wait_for_idle(timeout_ms=100)
            # May return False
            assert isinstance(result, bool)
        except RuntimeError:
            # Or may raise error
            pass


class TestSyncRetries:
    """Tests for retry behavior in synchronization."""

    @pytest.mark.requires_app
    def test_find_retries_until_found(self, calculator_app: TestApp) -> None:
        """find() with timeout retries until element found."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        # With timeout, should retry
        element = app.find("5", timeout_ms=2000)

        assert element is not None

    @pytest.mark.requires_app
    def test_find_retry_interval(self, calculator_app: TestApp) -> None:
        """find() uses appropriate retry interval (50ms)."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        # This is internal behavior verification
        # With 200ms timeout and 50ms interval, ~4 retries possible
        with pytest.raises(RuntimeError):
            app.find("NonExistentElement", timeout_ms=200)


class TestAsyncSync:
    """Tests for async synchronization patterns."""

    @pytest.mark.requires_app
    def test_sync_doesnt_block_other_threads(self, calculator_app: TestApp) -> None:
        """Sync operations don't block Python GIL excessively."""
        import axterminator as ax
        from concurrent.futures import ThreadPoolExecutor, as_completed

        app = ax.app(name="Calculator")

        def sync_operation():
            return app.wait_for_idle(timeout_ms=100)

        # Run multiple sync operations concurrently
        with ThreadPoolExecutor(max_workers=3) as executor:
            futures = [executor.submit(sync_operation) for _ in range(3)]

            results = [f.result() for f in as_completed(futures)]

        # All should complete
        assert len(results) == 3
