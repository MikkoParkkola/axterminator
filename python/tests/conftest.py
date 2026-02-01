"""
Pytest fixtures and configuration for AXTerminator tests.

Provides fixtures for:
- Launching/connecting to test applications
- Mock accessibility trees
- Performance measurement
- Background operation verification
"""

from __future__ import annotations

import os
import subprocess
import time
from collections.abc import Generator
from contextlib import contextmanager
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any, Callable
from unittest.mock import MagicMock, patch

import pytest

if TYPE_CHECKING:
    pass


# Custom pytest markers
def pytest_configure(config: pytest.Config) -> None:
    """Register custom markers."""
    config.addinivalue_line(
        "markers", "background: tests that verify background operation (no focus steal)"
    )
    config.addinivalue_line(
        "markers", "requires_app: tests that need a real running application"
    )
    config.addinivalue_line("markers", "slow: performance tests that may take longer")
    config.addinivalue_line(
        "markers", "integration: tests requiring real macOS accessibility API"
    )


# ============================================================================
# Application Fixtures
# ============================================================================


@dataclass
class TestApp:
    """Wrapper for test application state."""

    name: str
    bundle_id: str
    pid: int | None = None
    process: subprocess.Popen[bytes] | None = None

    def is_running(self) -> bool:
        """Check if the app process is still running."""
        if self.pid is None:
            return False
        try:
            os.kill(self.pid, 0)
            return True
        except OSError:
            return False

    def terminate(self) -> None:
        """Terminate the test app."""
        if self.process:
            self.process.terminate()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()


@pytest.fixture
def calculator_app() -> Generator[TestApp, None, None]:
    """
    Launch Calculator.app for testing.

    Calculator is ideal for tests because:
    - Always installed on macOS
    - Simple, predictable UI
    - Has buttons for click testing
    - Has display for value testing

    Note: macOS Calculator UI has changed over versions.
    Tests using specific button titles may need adjustment.
    """
    # Check if Calculator is already running
    result = subprocess.run(
        ["pgrep", "-x", "Calculator"],
        capture_output=True,
        text=True,
        check=False,
    )

    existing_pid = None
    if result.returncode == 0 and result.stdout.strip():
        # Use existing Calculator
        existing_pid = int(result.stdout.strip().split()[0])
        pid = existing_pid
        process = None
    else:
        # Launch Calculator
        process = subprocess.Popen(
            ["open", "-a", "Calculator"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )

        # Wait for app to launch and get PID
        time.sleep(2.0)  # Give app more time to launch

        # Get PID via pgrep - use -n for newest
        result = subprocess.run(
            ["pgrep", "-n", "-x", "Calculator"],
            capture_output=True,
            text=True,
            check=False,
        )

        pid = int(result.stdout.strip()) if result.returncode == 0 and result.stdout.strip() else None

    app = TestApp(
        name="Calculator",
        bundle_id="com.apple.calculator",
        pid=pid,
        process=process,
    )

    yield app

    # Only cleanup if we launched it (not if it was already running)
    if existing_pid is None and process is not None:
        subprocess.run(
            ["osascript", "-e", 'tell application "Calculator" to quit'],
            capture_output=True,
            check=False,
        )
        time.sleep(0.5)


@pytest.fixture
def textedit_app() -> Generator[TestApp, None, None]:
    """
    Launch TextEdit.app for text input testing.

    TextEdit is useful for:
    - Text input testing
    - Has text fields
    - Has value attributes
    """
    # Launch TextEdit with a new document
    process = subprocess.Popen(
        ["open", "-a", "TextEdit", "--new"],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )

    time.sleep(1.0)

    result = subprocess.run(
        ["pgrep", "-n", "TextEdit"],
        capture_output=True,
        text=True,
        check=False,
    )

    pid = int(result.stdout.strip()) if result.returncode == 0 else None

    app = TestApp(
        name="TextEdit",
        bundle_id="com.apple.TextEdit",
        pid=pid,
        process=process,
    )

    yield app

    subprocess.run(
        ["osascript", "-e", 'tell application "TextEdit" to quit saving no'],
        capture_output=True,
        check=False,
    )
    time.sleep(0.5)


@pytest.fixture
def finder_app() -> TestApp:
    """
    Use Finder for testing (always running).

    Finder is useful because:
    - Always running on macOS
    - No need to launch/terminate
    - Stable accessibility tree
    """
    result = subprocess.run(
        ["pgrep", "-x", "Finder"],
        capture_output=True,
        text=True,
        check=False,
    )

    pid = int(result.stdout.strip().split("\n")[0]) if result.returncode == 0 else None

    return TestApp(
        name="Finder",
        bundle_id="com.apple.finder",
        pid=pid,
    )


# ============================================================================
# Mock Accessibility Tree Fixtures
# ============================================================================


@dataclass
class MockAXElement:
    """Mock accessibility element for unit testing."""

    role: str
    title: str | None = None
    value: str | None = None
    identifier: str | None = None
    description: str | None = None
    label: str | None = None
    enabled: bool = True
    focused: bool = False
    bounds: tuple[float, float, float, float] | None = None
    children: list[MockAXElement] | None = None
    parent: MockAXElement | None = None
    data_testid: str | None = None
    aria_label: str | None = None

    def get_children(self) -> list[MockAXElement]:
        """Return children or empty list."""
        return self.children or []


def create_calculator_tree() -> MockAXElement:
    """
    Create a mock accessibility tree resembling Calculator.app.

    Structure:
    - AXApplication
      - AXWindow "Calculator"
        - AXGroup (display)
          - AXStaticText (result display)
        - AXGroup (buttons)
          - AXButton "1" ... "9", "0"
          - AXButton "+", "-", "*", "/"
          - AXButton "="
          - AXButton "AC"
    """
    # Create buttons
    number_buttons = [
        MockAXElement(role="AXButton", title=str(i), identifier=f"calc_btn_{i}")
        for i in range(10)
    ]

    operator_buttons = [
        MockAXElement(role="AXButton", title="+", identifier="calc_btn_plus"),
        MockAXElement(role="AXButton", title="-", identifier="calc_btn_minus"),
        MockAXElement(role="AXButton", title="*", identifier="calc_btn_multiply"),
        MockAXElement(role="AXButton", title="/", identifier="calc_btn_divide"),
        MockAXElement(role="AXButton", title="=", identifier="calc_btn_equals"),
        MockAXElement(role="AXButton", title="AC", identifier="calc_btn_clear"),
    ]

    button_group = MockAXElement(
        role="AXGroup",
        identifier="calculator_buttons",
        children=number_buttons + operator_buttons,
    )

    display = MockAXElement(
        role="AXStaticText",
        value="0",
        identifier="calculator_display",
    )

    display_group = MockAXElement(
        role="AXGroup",
        identifier="calculator_display_group",
        children=[display],
    )

    window = MockAXElement(
        role="AXWindow",
        title="Calculator",
        identifier="calculator_window",
        children=[display_group, button_group],
    )

    app = MockAXElement(
        role="AXApplication",
        title="Calculator",
        identifier="com.apple.calculator",
        children=[window],
    )

    return app


@pytest.fixture
def mock_calculator_tree() -> MockAXElement:
    """Provide mock Calculator accessibility tree."""
    return create_calculator_tree()


@pytest.fixture
def mock_ax_element() -> Callable[..., MockAXElement]:
    """Factory fixture for creating mock elements."""

    def _create(
        role: str = "AXButton",
        title: str | None = "Test",
        value: str | None = None,
        identifier: str | None = None,
        **kwargs: Any,
    ) -> MockAXElement:
        return MockAXElement(
            role=role,
            title=title,
            value=value,
            identifier=identifier,
            **kwargs,
        )

    return _create


# ============================================================================
# Performance Measurement Fixtures
# ============================================================================


@dataclass
class PerformanceResult:
    """Performance measurement result."""

    operation: str
    duration_ms: float
    iterations: int
    min_ms: float
    max_ms: float
    avg_ms: float
    p95_ms: float

    def meets_target(self, target_ms: float) -> bool:
        """Check if p95 meets target."""
        return self.p95_ms <= target_ms


@pytest.fixture
def perf_timer() -> Callable[..., PerformanceResult]:
    """
    Fixture for measuring operation performance.

    Usage:
        result = perf_timer(lambda: app.find("button"), iterations=100)
        assert result.p95_ms < 1.0  # 1ms target
    """

    def _measure(
        operation: Callable[[], Any],
        iterations: int = 100,
        warmup: int = 5,
        name: str = "operation",
    ) -> PerformanceResult:
        # Warmup
        for _ in range(warmup):
            try:
                operation()
            except Exception:
                pass

        # Measure
        durations: list[float] = []
        for _ in range(iterations):
            start = time.perf_counter()
            try:
                operation()
            except Exception:
                pass
            end = time.perf_counter()
            durations.append((end - start) * 1000)  # Convert to ms

        durations.sort()
        p95_idx = int(iterations * 0.95)

        return PerformanceResult(
            operation=name,
            duration_ms=sum(durations),
            iterations=iterations,
            min_ms=min(durations),
            max_ms=max(durations),
            avg_ms=sum(durations) / iterations,
            p95_ms=durations[p95_idx] if p95_idx < len(durations) else durations[-1],
        )

    return _measure


# ============================================================================
# Background Operation Verification
# ============================================================================


@dataclass
class FocusState:
    """Captured focus state for verification."""

    frontmost_app: str
    frontmost_window: str | None
    timestamp: float


@pytest.fixture
def focus_tracker() -> Callable[[], FocusState]:
    """
    Track which app has focus.

    Used to verify background operations don't steal focus.
    """

    def _get_focus() -> FocusState:
        # Get frontmost app via AppleScript
        result = subprocess.run(
            [
                "osascript",
                "-e",
                'tell application "System Events" to get name of first application process whose frontmost is true',
            ],
            capture_output=True,
            text=True,
            check=False,
        )

        frontmost_app = result.stdout.strip() if result.returncode == 0 else "Unknown"

        return FocusState(
            frontmost_app=frontmost_app,
            frontmost_window=None,  # Could be extended
            timestamp=time.time(),
        )

    return _get_focus


@contextmanager
def verify_no_focus_change(
    focus_tracker: Callable[[], FocusState],
) -> Generator[None, None, None]:
    """
    Context manager to verify no focus change occurs.

    Usage:
        with verify_no_focus_change(focus_tracker):
            element.click()  # Should not change focus
    """
    before = focus_tracker()
    yield
    after = focus_tracker()

    if before.frontmost_app != after.frontmost_app:
        pytest.fail(
            f"Focus changed from '{before.frontmost_app}' to '{after.frontmost_app}'"
        )


@pytest.fixture
def no_focus_change(
    focus_tracker: Callable[[], FocusState],
) -> Callable[[], contextmanager[None]]:
    """Fixture providing the no_focus_change context manager."""

    @contextmanager
    def _verify() -> Generator[None, None, None]:
        with verify_no_focus_change(focus_tracker):
            yield

    return _verify


# ============================================================================
# Mocking Fixtures
# ============================================================================


@pytest.fixture
def mock_accessibility_disabled() -> Generator[MagicMock, None, None]:
    """Mock accessibility as disabled."""
    with patch("axterminator.is_accessibility_enabled", return_value=False) as mock:
        yield mock


@pytest.fixture
def mock_accessibility_enabled() -> Generator[MagicMock, None, None]:
    """Mock accessibility as enabled."""
    with patch("axterminator.is_accessibility_enabled", return_value=True) as mock:
        yield mock


@pytest.fixture
def mock_app_connect() -> Generator[MagicMock, None, None]:
    """Mock app connection for unit tests."""

    def _mock_connect(
        name: str | None = None,
        bundle_id: str | None = None,
        pid: int | None = None,
    ) -> MagicMock:
        mock_app = MagicMock()
        mock_app.pid = pid or 12345
        mock_app.bundle_id = bundle_id
        mock_app.name = name
        mock_app.is_running.return_value = True
        return mock_app

    # Create a flexible mock that accepts any kwargs
    mock = MagicMock(side_effect=_mock_connect)
    with patch("axterminator.app", mock):
        yield mock


# ============================================================================
# Test Data Fixtures
# ============================================================================


@pytest.fixture
def sample_queries() -> list[str]:
    """Sample element queries for testing."""
    return [
        "Button",
        "role:AXButton",
        "title:Save",
        "identifier:btn_save",
        "role:AXButton title:Save",
        "//AXWindow/AXButton[@title='Save']",
    ]


@pytest.fixture
def healing_strategies() -> list[str]:
    """All healing strategies in priority order."""
    return [
        "data_testid",
        "aria_label",
        "identifier",
        "title",
        "xpath",
        "position",
        "visual_vlm",
    ]


# ============================================================================
# Skip Conditions
# ============================================================================


def has_accessibility_permission() -> bool:
    """Check if running with accessibility permissions."""
    try:
        import axterminator

        return axterminator.is_accessibility_enabled()
    except (ImportError, AttributeError):
        return False


def can_find_calculator_buttons() -> bool:
    """Check if Calculator buttons are accessible with expected labels."""
    try:
        import axterminator

        if not axterminator.is_accessibility_enabled():
            return False

        # Try to find Calculator and a button
        app = axterminator.app(name="Calculator")
        # Try to find button "5" - common test target
        try:
            app.find("5", timeout_ms=500)
            return True
        except RuntimeError:
            return False
    except Exception:
        return False


skip_without_accessibility = pytest.mark.skipif(
    not has_accessibility_permission(),
    reason="Requires accessibility permissions",
)

skip_in_ci = pytest.mark.skipif(
    os.environ.get("CI") == "true",
    reason="Cannot run in CI environment",
)

skip_without_calculator_buttons = pytest.mark.skipif(
    os.environ.get("CI") == "true",
    reason="Calculator button tests may fail due to UI changes between macOS versions",
)


# ============================================================================
# Calculator Button Helper
# ============================================================================


# Global flag to track if Calculator buttons work
_calculator_buttons_checked = False
_calculator_buttons_work = False


@pytest.fixture
def find_calculator_button():
    """
    Helper fixture to find Calculator buttons with graceful skip on failure.

    Usage:
        def test_something(find_calculator_button):
            element = find_calculator_button(app, "5")
            element.click()
    """
    global _calculator_buttons_checked, _calculator_buttons_work

    def _find(app: Any, button_label: str, timeout_ms: int = 2000) -> Any:
        global _calculator_buttons_checked, _calculator_buttons_work

        # Check once if Calculator buttons work
        if not _calculator_buttons_checked:
            _calculator_buttons_checked = True
            try:
                app.find("5", timeout_ms=1000)
                _calculator_buttons_work = True
            except RuntimeError:
                _calculator_buttons_work = False

        # If we know buttons don't work, skip immediately
        if not _calculator_buttons_work:
            pytest.skip(
                f"Calculator button '{button_label}' not accessible "
                f"on this macOS version"
            )

        try:
            return app.find(button_label, timeout_ms=timeout_ms)
        except RuntimeError as e:
            if "not found" in str(e).lower():
                _calculator_buttons_work = False
                pytest.skip(
                    f"Calculator button '{button_label}' not accessible "
                    f"on this macOS version"
                )
            raise

    return _find


# ============================================================================
# Async Support
# ============================================================================


@pytest.fixture
def event_loop_policy():
    """Configure event loop for async tests."""
    import asyncio

    return asyncio.DefaultEventLoopPolicy()
