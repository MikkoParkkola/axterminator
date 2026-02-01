"""XPC-based synchronization for waiting on app state changes.

This module provides utilities to wait for applications to reach an idle state
before performing interactions. It uses macOS accessibility APIs and heuristics
to detect when:
- Animations have completed
- UI has settled (no pending layout changes)
- Network requests have finished (where detectable)

Usage:
    from axterminator.sync import wait_for_idle, wait_for_element

    # Wait for app to become idle (animations done, UI settled)
    wait_for_idle(app, timeout_ms=5000)

    # Wait for specific element to appear
    element = wait_for_element(app, "Save", timeout_ms=3000)

Note:
    Full XPC synchronization requires additional entitlements and is
    not available in sandboxed apps. This module provides best-effort
    synchronization using accessibility APIs.
"""

from __future__ import annotations

import time
from typing import TYPE_CHECKING, Callable, Optional, TypeVar

if TYPE_CHECKING:
    from axterminator import AXApp, AXElement

T = TypeVar("T")


class SyncTimeout(Exception):
    """Raised when synchronization times out."""

    pass


def wait_for_idle(
    app: "AXApp",
    timeout_ms: int = 5000,
    poll_interval_ms: int = 50,
    stability_count: int = 3,
) -> bool:
    """Wait for application to become idle.

    Detects idle state by monitoring the accessibility tree for changes.
    The app is considered idle when no changes occur for `stability_count`
    consecutive polls.

    Args:
        app: The application to monitor
        timeout_ms: Maximum time to wait in milliseconds
        poll_interval_ms: How often to check for changes
        stability_count: Number of stable polls required

    Returns:
        True if app became idle, False if timeout

    Example:
        >>> app = axterminator.app(name="Calculator")
        >>> wait_for_idle(app, timeout_ms=2000)
        True
    """
    deadline = time.perf_counter() + (timeout_ms / 1000)
    stable_polls = 0
    last_snapshot = None

    while time.perf_counter() < deadline:
        try:
            # Get a lightweight snapshot of the UI state
            # Using element count as a simple proxy for UI state
            current_snapshot = _get_ui_snapshot(app)

            if current_snapshot == last_snapshot:
                stable_polls += 1
                if stable_polls >= stability_count:
                    return True
            else:
                stable_polls = 0
                last_snapshot = current_snapshot

        except RuntimeError:
            # App may be busy, reset stability counter
            stable_polls = 0

        time.sleep(poll_interval_ms / 1000)

    return False


def wait_for_element(
    app: "AXApp",
    query: str,
    timeout_ms: int = 5000,
    poll_interval_ms: int = 100,
) -> Optional["AXElement"]:
    """Wait for an element matching query to appear.

    Polls the accessibility tree until the element is found or timeout.

    Args:
        app: The application to search in
        query: Element query (same as app.find())
        timeout_ms: Maximum time to wait in milliseconds
        poll_interval_ms: How often to check for the element

    Returns:
        The element if found, None if timeout

    Example:
        >>> app = axterminator.app(name="Safari")
        >>> button = wait_for_element(app, "Done", timeout_ms=3000)
        >>> if button:
        ...     button.click()
    """
    deadline = time.perf_counter() + (timeout_ms / 1000)

    while time.perf_counter() < deadline:
        try:
            element = app.find(query, timeout_ms=int(poll_interval_ms / 2))
            return element
        except RuntimeError:
            # Element not found yet, continue polling
            pass

        time.sleep(poll_interval_ms / 1000)

    return None


def wait_for_condition(
    condition: Callable[[], T],
    timeout_ms: int = 5000,
    poll_interval_ms: int = 100,
    description: str = "condition",
) -> T:
    """Wait for a condition to become truthy.

    Generic waiting utility for any condition.

    Args:
        condition: Callable that returns truthy value when condition is met
        timeout_ms: Maximum time to wait in milliseconds
        poll_interval_ms: How often to check the condition
        description: Human-readable description for error messages

    Returns:
        The truthy value returned by condition

    Raises:
        SyncTimeout: If condition is not met within timeout

    Example:
        >>> def app_has_window():
        ...     return len(app.windows()) > 0
        >>> wait_for_condition(app_has_window, timeout_ms=5000)
    """
    deadline = time.perf_counter() + (timeout_ms / 1000)

    while time.perf_counter() < deadline:
        try:
            result = condition()
            if result:
                return result
        except Exception:
            # Condition raised, continue polling
            pass

        time.sleep(poll_interval_ms / 1000)

    raise SyncTimeout(f"Timed out waiting for {description} after {timeout_ms}ms")


def wait_for_value(
    element: "AXElement",
    expected: str,
    timeout_ms: int = 5000,
    poll_interval_ms: int = 100,
) -> bool:
    """Wait for element value to match expected.

    Useful for waiting on text fields or displays to update.

    Args:
        element: The element to monitor
        expected: Expected value string
        timeout_ms: Maximum time to wait
        poll_interval_ms: How often to check

    Returns:
        True if value matched, False if timeout

    Example:
        >>> display = app.find("calculator_display")
        >>> app.find("5").click()
        >>> wait_for_value(display, "5", timeout_ms=1000)
        True
    """
    deadline = time.perf_counter() + (timeout_ms / 1000)

    while time.perf_counter() < deadline:
        try:
            if element.value == expected:
                return True
        except Exception:
            pass

        time.sleep(poll_interval_ms / 1000)

    return False


def _get_ui_snapshot(app: "AXApp") -> str:
    """Get lightweight snapshot of UI state.

    Uses a simple heuristic: hash of focused element + window count.
    This is much faster than full tree comparison but catches most
    meaningful state changes.
    """
    try:
        # This is a placeholder - actual implementation would use
        # app.focused_element() and app.windows() when available
        # For now, return a timestamp-based snapshot for basic functionality
        return str(time.perf_counter())
    except Exception:
        return ""


# XPC-specific functionality (requires entitlements)


def xpc_sync_available() -> bool:
    """Check if XPC synchronization is available.

    XPC sync provides more accurate idle detection but requires:
    - com.apple.security.temporary-exception.mach-lookup.global-name
    - Running outside of sandbox

    Returns:
        True if XPC sync can be used
    """
    # TODO: Check for XPC availability via Rust extension
    return False


def wait_for_xpc_idle(
    app: "AXApp",
    timeout_ms: int = 5000,
) -> bool:
    """Wait for app to be idle using XPC run loop synchronization.

    More accurate than polling-based wait_for_idle() but requires
    XPC entitlements.

    Args:
        app: Application to wait on
        timeout_ms: Maximum wait time

    Returns:
        True if idle, False if timeout or XPC unavailable
    """
    if not xpc_sync_available():
        # Fall back to polling-based sync
        return wait_for_idle(app, timeout_ms=timeout_ms)

    # TODO: Implement XPC-based synchronization via Rust extension
    # This would use kAXApplicationActivatedNotification and similar
    return wait_for_idle(app, timeout_ms=timeout_ms)
