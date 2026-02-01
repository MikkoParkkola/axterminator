"""pytest configuration for axterminator tests."""

from __future__ import annotations

import os
import subprocess
import time
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any, Callable

import pytest

if TYPE_CHECKING:
    pass


# Skip integration tests in CI or when explicitly disabled
IN_CI = os.environ.get("CI", "false").lower() == "true"
SKIP_INTEGRATION = IN_CI or os.environ.get("SKIP_INTEGRATION", "false").lower() == "true"


def pytest_addoption(parser):
    """Add custom command line options."""
    parser.addoption(
        "--run-integration",
        action="store_true",
        default=False,
        help="Run integration tests with real apps",
    )


def pytest_configure(config):
    """Register custom markers."""
    config.addinivalue_line(
        "markers", "integration: marks tests as integration tests (require --run-integration)"
    )
    config.addinivalue_line(
        "markers", "slow: marks tests as slow running"
    )
    config.addinivalue_line(
        "markers", "background: marks tests for background click functionality"
    )
    config.addinivalue_line(
        "markers", "requires_app: marks tests that require a running app"
    )


def pytest_collection_modifyitems(config, items):
    """Skip integration tests unless --run-integration is passed."""
    if config.getoption("--run-integration"):
        return

    skip_integration = pytest.mark.skip(reason="need --run-integration option to run")
    skip_requires_app = pytest.mark.skip(reason="need --run-integration option to run app tests")

    for item in items:
        if "integration" in item.keywords:
            item.add_marker(skip_integration)
        if "requires_app" in item.keywords:
            item.add_marker(skip_requires_app)


# --- Helper Classes ---

@dataclass
class FocusState:
    """Current focus state."""
    frontmost_app: str
    frontmost_pid: int


@dataclass
class PerformanceResult:
    """Performance measurement result."""
    name: str
    iterations: int
    total_ms: float
    avg_ms: float
    min_ms: float
    max_ms: float
    p50_ms: float
    p95_ms: float
    p99_ms: float


@dataclass
class TestApp:
    """Test app wrapper."""
    name: str
    pid: int

    def terminate(self) -> None:
        """Terminate the app."""
        try:
            subprocess.run(["pkill", "-x", self.name], capture_output=True)
        except Exception:
            pass


# --- Helper Functions ---

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


def get_app_pid(app_name: str) -> int:
    """Get PID of running app."""
    try:
        result = subprocess.run(
            ["pgrep", "-x", app_name],
            capture_output=True,
            text=True,
        )
        if result.returncode == 0:
            return int(result.stdout.strip().split()[0])
    except Exception:
        pass
    return 0


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
        time.sleep(1)
        return True
    except Exception:
        return False


def get_frontmost_app() -> FocusState:
    """Get the frontmost application."""
    try:
        script = '''
        tell application "System Events"
            set frontApp to first application process whose frontmost is true
            return {name of frontApp, unix id of frontApp}
        end tell
        '''
        result = subprocess.run(
            ["osascript", "-e", script],
            capture_output=True,
            text=True,
        )
        if result.returncode == 0:
            parts = result.stdout.strip().split(", ")
            if len(parts) == 2:
                return FocusState(parts[0], int(parts[1]))
    except Exception:
        pass
    return FocusState("Unknown", 0)


# --- Fixtures ---

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


@pytest.fixture
def calculator_app() -> TestApp:
    """Get Calculator app, launch if needed."""
    if SKIP_INTEGRATION:
        pytest.skip("Integration tests disabled")

    if not launch_app("Calculator"):
        pytest.skip("Cannot launch Calculator")

    time.sleep(0.5)
    pid = get_app_pid("Calculator")
    return TestApp("Calculator", pid)


@pytest.fixture
def textedit_app() -> TestApp:
    """Get TextEdit app, launch if needed."""
    if SKIP_INTEGRATION:
        pytest.skip("Integration tests disabled")

    if not launch_app("TextEdit"):
        pytest.skip("Cannot launch TextEdit")

    time.sleep(0.5)
    pid = get_app_pid("TextEdit")
    return TestApp("TextEdit", pid)


@pytest.fixture
def finder_app() -> TestApp:
    """Get Finder app (always running)."""
    if SKIP_INTEGRATION:
        pytest.skip("Integration tests disabled")

    pid = get_app_pid("Finder")
    return TestApp("Finder", pid)


@pytest.fixture
def focus_tracker() -> Callable[[], FocusState]:
    """Return a function to track current focus state."""
    return get_frontmost_app


@pytest.fixture
def find_calculator_button() -> Callable:
    """Return a function to find Calculator buttons."""
    def _find(app: Any, label: str) -> Any:
        return app.find(label, timeout_ms=2000)
    return _find


@pytest.fixture
def mock_app_connect():
    """Mock for app connection - skip tests requiring this."""
    pytest.skip("Mock fixtures don't work with Rust bindings - use integration tests")


@pytest.fixture
def mock_element():
    """Mock for element - skip tests requiring this."""
    pytest.skip("Mock fixtures don't work with Rust bindings - use integration tests")


@pytest.fixture
def mock_locator():
    """Mock for locator - skip tests requiring this."""
    pytest.skip("Mock fixtures don't work with Rust bindings - use integration tests")


# --- Mock Classes for unit testing ---

@dataclass
class MockAXElement:
    """Mock accessibility element for unit testing."""
    role: str = "AXButton"
    title: str = ""
    value: str = ""
    identifier: str = ""
    data_testid: str = ""
    aria_label: str = ""
    xpath: str = ""
    description: str = ""
    label: str = ""
    enabled: bool = True
    focused: bool = False
    bounds: tuple = None
    children: list = None

    def __post_init__(self):
        if self.children is None:
            self.children = []

    def get_children(self) -> list:
        return self.children


@pytest.fixture
def mock_ax_element() -> Callable[..., MockAXElement]:
    """Factory fixture to create mock AX elements."""
    def _create(**kwargs) -> MockAXElement:
        return MockAXElement(**kwargs)
    return _create


@pytest.fixture
def mock_calculator_tree() -> MockAXElement:
    """Create a mock Calculator accessibility tree.

    Structure matches test expectations:
    - Application
      - Window
        - Group (toolbar)
          - Buttons [0-13]
    """
    # Build a simplified Calculator tree
    # Use identifiers matching test expectations (calc_btn_X format)
    buttons = [MockAXElement(role="AXButton", title=str(i), identifier=f"calc_btn_{i}") for i in range(10)]
    buttons.append(MockAXElement(role="AXButton", title="+", identifier="calc_btn_plus"))
    buttons.append(MockAXElement(role="AXButton", title="-", identifier="calc_btn_minus"))
    buttons.append(MockAXElement(role="AXButton", title="=", identifier="calc_btn_equals"))
    buttons.append(MockAXElement(role="AXButton", title="AC", identifier="calc_btn_ac"))

    # Nested structure to match test expectations:
    # tree.get_children()[0].get_children()[1].get_children()[5] = button "5"
    group = MockAXElement(role="AXGroup", title="Buttons", children=buttons)
    toolbar = MockAXElement(role="AXToolbar", title="Toolbar", children=[])
    window = MockAXElement(role="AXWindow", title="Calculator", children=[toolbar, group])
    return MockAXElement(role="AXApplication", title="Calculator", children=[window])


@pytest.fixture
def perf_timer() -> Callable[..., PerformanceResult]:
    """Return a performance timing function."""
    def _timer(
        func: Callable,
        iterations: int = 100,
        name: str = "operation",
    ) -> PerformanceResult:
        times = []
        for _ in range(iterations):
            start = time.perf_counter()
            func()
            elapsed = (time.perf_counter() - start) * 1000  # ms
            times.append(elapsed)

        times.sort()
        return PerformanceResult(
            name=name,
            iterations=iterations,
            total_ms=sum(times),
            avg_ms=sum(times) / len(times),
            min_ms=min(times),
            max_ms=max(times),
            p50_ms=times[len(times) // 2],
            p95_ms=times[int(len(times) * 0.95)],
            p99_ms=times[int(len(times) * 0.99)],
        )
    return _timer
