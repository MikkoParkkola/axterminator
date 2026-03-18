"""
Type stubs for axterminator — macOS GUI Testing Framework.

The core classes (AXApp, AXElement, HealingConfig, ActionMode) are
implemented in Rust via PyO3 and exposed here with accurate signatures.
Python-side utilities (sync, vlm, recorder) are declared in their own
.pyi files alongside the .py sources.
"""

from __future__ import annotations

from typing import Optional

__version__: str

# ---------------------------------------------------------------------------
# ActionMode
# ---------------------------------------------------------------------------

class ActionMode:
    """Action mode for element interactions.

    Controls whether an interaction steals focus from the currently active
    application.

    Example::

        import axterminator as ax

        safari = ax.app(bundle_id="com.apple.Safari")
        safari.find("URL").click()                     # Background (default)
        safari.find("URL").type_text("…", mode=ax.FOCUS)  # Focus required
    """

    Background: ActionMode
    """Perform action without stealing focus (DEFAULT)."""

    Focus: ActionMode
    """Bring the app to the foreground and focus the element before acting."""

    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

# ---------------------------------------------------------------------------
# Module-level constants
# ---------------------------------------------------------------------------

BACKGROUND: ActionMode
"""Alias for ``ActionMode.Background`` — perform action without focus steal."""

FOCUS: ActionMode
"""Alias for ``ActionMode.Focus`` — bring app to foreground first."""

# ---------------------------------------------------------------------------
# AXElement
# ---------------------------------------------------------------------------

class AXElement:
    """Wrapper for a macOS accessibility element (``AXUIElementRef``).

    Obtained via :meth:`AXApp.find`, :meth:`AXApp.find_by_role`,
    :meth:`AXApp.windows`, :meth:`AXApp.main_window`, or
    :meth:`AXElement.find`.

    Example::

        import axterminator as ax

        app = ax.app(bundle_id="com.apple.TextEdit")
        btn = app.find("Save")
        btn.click()                              # Background click
        app.find("filename").type_text("doc")    # Requires FOCUS mode
    """

    def role(self) -> Optional[str]:
        """Return the element's accessibility role, e.g. ``"AXButton"``.

        Returns ``None`` when the attribute is not present or the element
        has been invalidated.
        """
        ...

    def title(self) -> Optional[str]:
        """Return the element's ``AXTitle`` attribute.

        Typically the visible label on buttons, menus, and windows.
        """
        ...

    def value(self) -> Optional[str]:
        """Return the element's ``AXValue`` attribute.

        For text fields this is the current text content; for checkboxes it
        is ``"0"`` or ``"1"``.
        """
        ...

    def description(self) -> Optional[str]:
        """Return the element's ``AXDescription`` attribute."""
        ...

    def label(self) -> Optional[str]:
        """Return the element's ``AXLabel`` attribute.

        Often the same as title for interactive controls, but may differ
        (e.g., icon buttons where the label is a tooltip).
        """
        ...

    def identifier(self) -> Optional[str]:
        """Return the element's ``AXIdentifier`` attribute.

        Developers set this to a stable test-id; prefer this for durable
        selectors over title or position.
        """
        ...

    def enabled(self) -> bool:
        """Return ``True`` when the element accepts user interaction.

        Disabled elements (greyed-out buttons, read-only fields) return
        ``False``.
        """
        ...

    def focused(self) -> bool:
        """Return ``True`` when the element currently has keyboard focus."""
        ...

    def exists(self) -> bool:
        """Return ``True`` when the element is still present in the UI.

        Equivalent to checking whether :meth:`role` returns a non-``None``
        value. Use this as a lightweight liveness check before interacting.
        """
        ...

    def bounds(self) -> Optional[tuple[float, float, float, float]]:
        """Return the element's bounding rectangle as ``(x, y, width, height)``.

        Coordinates are in screen points (top-left origin).  Returns
        ``None`` when the element has no spatial representation (e.g., a
        menu item that is not currently visible).
        """
        ...

    def click(self, mode: Optional[ActionMode] = None) -> None:
        """Send a press action to the element.

        The default mode is :data:`BACKGROUND` — the action is delivered
        directly via the Accessibility API without raising the application
        window, making this a **world-first** focus-stealing-free click.

        Args:
            mode: :data:`BACKGROUND` (default) or :data:`FOCUS`.

        Raises:
            RuntimeError: If the press action fails.

        Example::

            element.click()              # Background, no focus steal
            element.click(mode=ax.FOCUS) # Bring app to front first
        """
        ...

    def double_click(self, mode: Optional[ActionMode] = None) -> None:
        """Send two consecutive press actions with a 50 ms gap.

        Args:
            mode: :data:`BACKGROUND` (default) or :data:`FOCUS`.

        Raises:
            RuntimeError: If either press action fails.
        """
        ...

    def right_click(self, mode: Optional[ActionMode] = None) -> None:
        """Trigger ``AXShowMenu`` to open the contextual menu.

        Args:
            mode: :data:`BACKGROUND` (default) or :data:`FOCUS`.

        Raises:
            RuntimeError: If the show-menu action fails.
        """
        ...

    def type_text(self, text: str, mode: Optional[ActionMode] = None) -> None:
        """Type ``text`` into the element by posting keyboard events.

        Text input **requires** :data:`FOCUS` mode (the default when called
        without an explicit mode argument).  Passing :data:`BACKGROUND`
        explicitly raises ``RuntimeError``.

        Args:
            text: The string to type; supports printable ASCII and common
                  control characters (``\\n``, ``\\t``).
            mode: Must be :data:`FOCUS` (default).

        Raises:
            RuntimeError: If mode is :data:`BACKGROUND` or if event posting
                          fails.

        Example::

            field = app.find("Search")
            field.type_text("hello world")
        """
        ...

    def set_value(self, value: str) -> None:
        """Write ``value`` directly to the element's ``AXValue`` attribute.

        Faster than :meth:`type_text` and works in background mode, but
        only succeeds on elements that expose a settable ``AXValue``
        (e.g., text fields, sliders).

        Args:
            value: New value string.

        Raises:
            RuntimeError: If the attribute is read-only or the write fails.
        """
        ...

    def screenshot(self) -> bytes:
        """Capture a PNG screenshot of this element's bounding rect.

        Returns:
            Raw PNG image data.

        Raises:
            RuntimeError: If the element has no bounds or screencapture
                          fails.
        """
        ...

    def find(
        self, query: str, timeout_ms: Optional[int] = None
    ) -> "AXElement":
        """Search for a descendant element matching ``query``.

        Uses breadth-first search with an optional retry loop.

        Args:
            query: Simple text (matches title/label/identifier), a
                   ``key:value`` string (e.g. ``"role:AXButton
                   title:Save"``), or an XPath-like expression
                   (``"//AXButton[@AXTitle='Save']"``).
            timeout_ms: If given, retry for up to this many milliseconds
                        before raising.

        Returns:
            The first matching descendant element.

        Raises:
            RuntimeError: If no matching element is found within the
                          timeout.
        """
        ...

# ---------------------------------------------------------------------------
# AXApp
# ---------------------------------------------------------------------------

class AXApp:
    """Application handle providing the main entry point for GUI automation.

    Create one via the :func:`app` factory function rather than
    constructing this class directly.

    Example::

        import axterminator as ax

        safari = ax.app(bundle_id="com.apple.Safari")
        safari.find("New Tab").click()
    """

    @property
    def pid(self) -> int:
        """Process ID of the connected application."""
        ...

    @property
    def bundle_id(self) -> Optional[str]:
        """Bundle identifier, e.g. ``"com.apple.Safari"``.

        ``None`` when the app was connected by name or PID only.
        """
        ...

    def is_running(self) -> bool:
        """Return ``True`` when the application process is still alive."""
        ...

    def find(self, query: str, timeout_ms: Optional[int] = None) -> AXElement:
        """Find a UI element by query string.

        Args:
            query: Simple text, ``key:value`` pairs, or XPath-like syntax.
            timeout_ms: Retry duration in milliseconds before raising.

        Returns:
            First matching element.

        Raises:
            RuntimeError: No element found within timeout.

        Example::

            btn = app.find("Save")
            btn = app.find("role:AXButton title:Save")
            btn = app.find("//AXButton[@AXTitle='Save']")
        """
        ...

    def find_by_role(
        self,
        role: str,
        title: Optional[str] = None,
        identifier: Optional[str] = None,
        label: Optional[str] = None,
    ) -> AXElement:
        """Find an element by accessibility role with optional attribute filters.

        Args:
            role: AX role string, e.g. ``"AXButton"``, ``"AXTextField"``.
            title: Optional ``AXTitle`` constraint.
            identifier: Optional ``AXIdentifier`` constraint.
            label: Optional ``AXLabel`` constraint.

        Returns:
            First matching element.

        Raises:
            RuntimeError: No matching element found.
        """
        ...

    def wait_for_element(
        self, query: str, timeout_ms: int = 5000
    ) -> AXElement:
        """Poll until a matching element appears or timeout expires.

        Equivalent to :meth:`find` with a ``timeout_ms`` argument but
        with an explicit default of 5 000 ms for readability.

        Args:
            query: Element query string.
            timeout_ms: Maximum wait time (default 5 000 ms).

        Returns:
            The element once it appears.

        Raises:
            RuntimeError: Element did not appear within timeout.
        """
        ...

    def wait_for_idle(self, timeout_ms: int = 5000) -> bool:
        """Block until the application becomes idle.

        Uses the EspressoMac SDK when available; falls back to a
        heuristic that watches for AX-tree stability.

        Args:
            timeout_ms: Maximum wait time (default 5 000 ms).

        Returns:
            ``True`` if idle was reached, ``False`` on timeout.
        """
        ...

    def is_idle(self) -> bool:
        """Return ``True`` if the application is currently idle (non-blocking)."""
        ...

    def screenshot(self) -> bytes:
        """Capture a PNG screenshot of the application's main window.

        Returns:
            Raw PNG image data.

        Raises:
            RuntimeError: If screencapture fails.
        """
        ...

    def windows(self) -> list[AXElement]:
        """Return all open windows of the application.

        Returns:
            List of window elements (may be empty if the app has no
            visible windows).

        Raises:
            RuntimeError: If the accessibility attribute read fails.
        """
        ...

    def main_window(self) -> AXElement:
        """Return the application's main (frontmost) window.

        Raises:
            RuntimeError: If the main window cannot be determined.
        """
        ...

    def terminate(self) -> None:
        """Send SIGTERM to the application process.

        Raises:
            RuntimeError: If the kill command fails.
        """
        ...

# ---------------------------------------------------------------------------
# HealingConfig
# ---------------------------------------------------------------------------

class HealingConfig:
    """Configuration for the self-healing element location system.

    The healing system tries up to seven strategies in order when the
    primary locator fails.  You can customise which strategies are
    attempted and how long to spend.

    Example::

        import axterminator as ax

        config = ax.HealingConfig(
            strategies=["data_testid", "aria_label", "title"],
            max_heal_time_ms=200,
            cache_healed=True,
        )
        ax.configure_healing(config)
    """

    strategies: list[str]
    """Ordered list of strategy names to attempt.

    Valid values (in default order):
    ``"data_testid"``, ``"aria_label"``, ``"identifier"``,
    ``"title"``, ``"xpath"``, ``"position"``, ``"visual_vlm"``.
    """

    max_heal_time_ms: int
    """Maximum total time budget for healing a single locator, in
    milliseconds. Default: ``100``."""

    cache_healed: bool
    """Whether to cache successful heals so future lookups skip retrying
    failed strategies. Default: ``True``."""

    def __init__(
        self,
        strategies: Optional[list[str]] = None,
        max_heal_time_ms: int = 100,
        cache_healed: bool = True,
    ) -> None:
        """Create a new :class:`HealingConfig`.

        Args:
            strategies: Strategy names in preference order.  When
                ``None`` all seven strategies are enabled in their
                default order.
            max_heal_time_ms: Time cap for the full healing pass.
            cache_healed: Enable strategy-result caching.
        """
        ...

# ---------------------------------------------------------------------------
# Module-level functions
# ---------------------------------------------------------------------------

def app(
    name: Optional[str] = None,
    bundle_id: Optional[str] = None,
    pid: Optional[int] = None,
) -> AXApp:
    """Connect to a running application and return an :class:`AXApp` handle.

    Exactly one of ``name``, ``bundle_id``, or ``pid`` must be provided.
    Bundle ID is the most reliable selector because application names can
    change with locale or version.

    Args:
        name: Application name as shown in Activity Monitor (e.g.
              ``"Safari"``).
        bundle_id: Reverse-DNS bundle ID (e.g. ``"com.apple.Safari"``).
        pid: Unix process ID.

    Returns:
        Connected application handle.

    Raises:
        ValueError: If none of the three selectors are provided.
        RuntimeError: If the application is not running or the
                      accessibility element cannot be created.

    Example::

        import axterminator as ax

        # Preferred: bundle ID is locale-independent
        safari = ax.app(bundle_id="com.apple.Safari")

        # By name (locale-dependent)
        finder = ax.app(name="Finder")

        # By PID
        my_app = ax.app(pid=12345)
    """
    ...

def is_accessibility_enabled() -> bool:
    """Return ``True`` when the process has macOS accessibility permissions.

    If this returns ``False`` open **System Settings → Privacy & Security →
    Accessibility** and grant permission to your terminal or test runner.

    Example::

        import axterminator as ax

        if not ax.is_accessibility_enabled():
            raise RuntimeError("Accessibility permission required")
    """
    ...

def configure_healing(config: HealingConfig) -> None:
    """Install ``config`` as the global healing configuration.

    Call once at test-suite startup before any :class:`AXApp` instances
    are created.

    Args:
        config: Healing configuration to apply globally.

    Raises:
        RuntimeError: If the configuration cannot be applied.
    """
    ...

# ---------------------------------------------------------------------------
# Python-side re-exports (sync, vlm, recorder)
# ---------------------------------------------------------------------------

# These are implemented in pure Python — see sync.py, vlm.py, recorder.py.
# Stubs live here so ``import axterminator; reveal_type(axterminator.wait_for_idle)``
# works without importing the submodules explicitly.

from axterminator.sync import (  # noqa: E402
    SyncTimeout as SyncTimeout,
    wait_for_condition as wait_for_condition,
    wait_for_element as wait_for_element,
    wait_for_idle as wait_for_idle,
    wait_for_value as wait_for_value,
)
from axterminator.vlm import (  # noqa: E402
    configure_vlm as configure_vlm,
    detect_element_visual as detect_element_visual,
)
from axterminator.recorder import Recorder as Recorder  # noqa: E402

def xpc_sync_available() -> bool:
    """Return ``True`` if XPC synchronization is available.

    XPC sync provides more accurate idle detection than polling but
    requires additional entitlements.  This always returns ``False``
    in the current release.
    """
    ...
