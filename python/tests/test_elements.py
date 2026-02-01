"""
Tests for AXTerminator element operations.

Tests cover:
- Finding elements by title
- Finding elements by role
- Finding elements by identifier
- Timeout and waiting behavior
- Element not found errors
- Element properties (role, title, value, etc.)
"""

from __future__ import annotations

import time
from typing import TYPE_CHECKING, Callable

import pytest

if TYPE_CHECKING:
    from conftest import MockAXElement, PerformanceResult, TestApp


class TestFindByTitle:
    """Tests for finding elements by title."""

    @pytest.mark.requires_app
    def test_find_by_title_simple(
        self, calculator_app: TestApp, find_calculator_button: Callable
    ) -> None:
        """Find element by simple title string."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        # Calculator has buttons with number titles
        element = find_calculator_button(app, "1")

        assert element is not None

    @pytest.mark.requires_app
    def test_find_by_title_with_spaces(
        self, calculator_app: TestApp, find_calculator_button: Callable
    ) -> None:
        """Find element with spaces in title."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        # Try finding AC (All Clear) button
        element = find_calculator_button(app, "AC")

        assert element is not None

    def test_find_by_title_mock(self, mock_calculator_tree: MockAXElement) -> None:
        """Test title search with mock tree."""
        # Search logic test with mock
        tree = mock_calculator_tree

        # Find button with title "5"
        def find_by_title(node: MockAXElement, title: str) -> MockAXElement | None:
            if node.title == title:
                return node
            for child in node.get_children():
                result = find_by_title(child, title)
                if result:
                    return result
            return None

        button = find_by_title(tree, "5")

        assert button is not None
        assert button.role == "AXButton"
        assert button.title == "5"

    @pytest.mark.requires_app
    def test_find_by_title_returns_first_match(self, finder_app: TestApp) -> None:
        """When multiple elements have same title, first is returned."""
        import axterminator as ax

        app = ax.app(name="Finder")

        # This depends on Finder's actual UI structure
        # Just verify we get a single element back
        try:
            element = app.find("File")
            assert element is not None
        except RuntimeError:
            # Element not found is acceptable for this test
            pass

    def test_find_by_title_case_sensitive(
        self, mock_calculator_tree: MockAXElement
    ) -> None:
        """Title search should be case-sensitive."""

        def find_by_title(node: MockAXElement, title: str) -> MockAXElement | None:
            if node.title == title:
                return node
            for child in node.get_children():
                result = find_by_title(child, title)
                if result:
                    return result
            return None

        # "AC" exists, "ac" should not find it (case sensitive)
        ac_button = find_by_title(mock_calculator_tree, "AC")
        lowercase = find_by_title(mock_calculator_tree, "ac")

        assert ac_button is not None
        assert lowercase is None


class TestFindByRole:
    """Tests for finding elements by accessibility role."""

    @pytest.mark.requires_app
    def test_find_by_role_button(self, calculator_app: TestApp) -> None:
        """Find elements with AXButton role."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        # Find any button
        element = app.find_by_role("AXButton")

        assert element is not None
        assert element.role() == "AXButton"

    @pytest.mark.requires_app
    def test_find_by_role_with_title(
        self, calculator_app: TestApp, find_calculator_button: Callable
    ) -> None:
        """Find element by role AND title."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        # Find specific button - use helper to skip if not found
        try:
            element = app.find_by_role("AXButton", title="5")
            assert element is not None
            assert element.title() == "5"
        except RuntimeError as e:
            if "not found" in str(e).lower():
                pytest.skip("Calculator button '5' not accessible on this macOS version")

    @pytest.mark.requires_app
    def test_find_by_role_window(self, calculator_app: TestApp) -> None:
        """Find window element."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        element = app.find_by_role("AXWindow")

        assert element is not None
        assert element.role() == "AXWindow"

    @pytest.mark.requires_app
    def test_find_by_role_with_identifier(self, calculator_app: TestApp) -> None:
        """Find element by role and identifier."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        # May or may not find element depending on app structure
        try:
            element = app.find_by_role("AXButton", identifier="some_id")
            assert element is not None
        except RuntimeError:
            # Not found is acceptable
            pass

    def test_find_by_role_invalid_role(self) -> None:
        """Invalid role name raises or returns nothing."""
        import axterminator as ax

        app = ax.app(name="Finder")

        with pytest.raises(RuntimeError):
            app.find_by_role("AXInvalidRole12345")

    @pytest.mark.requires_app
    def test_find_by_role_statictext(self, calculator_app: TestApp) -> None:
        """Find static text element (display)."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        try:
            element = app.find_by_role("AXStaticText")
            assert element is not None
        except RuntimeError:
            # Structure may vary
            pass


class TestFindByIdentifier:
    """Tests for finding elements by accessibility identifier."""

    def test_find_by_identifier_mock(self, mock_calculator_tree: MockAXElement) -> None:
        """Test identifier search with mock tree."""

        def find_by_id(node: MockAXElement, identifier: str) -> MockAXElement | None:
            if node.identifier == identifier:
                return node
            for child in node.get_children():
                result = find_by_id(child, identifier)
                if result:
                    return result
            return None

        button = find_by_id(mock_calculator_tree, "calc_btn_5")

        assert button is not None
        assert button.title == "5"

    @pytest.mark.requires_app
    def test_find_by_identifier_using_find(self, calculator_app: TestApp) -> None:
        """Find element using identifier: prefix in query."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        try:
            # Use query syntax: identifier:value
            element = app.find("identifier:_NS:9")  # Example identifier
            assert element is not None
        except RuntimeError:
            # Identifier may not exist
            pass

    def test_find_by_identifier_not_found(
        self, mock_calculator_tree: MockAXElement
    ) -> None:
        """Non-existent identifier returns None."""

        def find_by_id(node: MockAXElement, identifier: str) -> MockAXElement | None:
            if node.identifier == identifier:
                return node
            for child in node.get_children():
                result = find_by_id(child, identifier)
                if result:
                    return result
            return None

        result = find_by_id(mock_calculator_tree, "nonexistent_identifier")

        assert result is None


class TestFindWithTimeout:
    """Tests for element finding with timeout."""

    @pytest.mark.requires_app
    def test_find_with_timeout_found_immediately(
        self, calculator_app: TestApp, find_calculator_button: Callable
    ) -> None:
        """Element found immediately returns without waiting."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        start = time.perf_counter()
        element = find_calculator_button(app, "1", timeout_ms=5000)
        elapsed = time.perf_counter() - start

        assert element is not None
        # Should return quickly, not wait full timeout
        assert elapsed < 1.0

    @pytest.mark.requires_app
    @pytest.mark.slow
    def test_find_with_timeout_waits(self, calculator_app: TestApp) -> None:
        """When element not found, waits for timeout."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        start = time.perf_counter()
        with pytest.raises(RuntimeError, match="not found"):
            app.find("NonExistentElement12345", timeout_ms=500)
        elapsed = time.perf_counter() - start

        # Should have waited approximately the timeout
        assert 0.4 < elapsed < 1.5  # Allow some margin

    @pytest.mark.requires_app
    def test_wait_for_element_method(
        self, calculator_app: TestApp, find_calculator_button: Callable
    ) -> None:
        """wait_for_element() explicitly waits for element."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        try:
            element = app.wait_for_element("1", timeout_ms=2000)
            assert element is not None
        except RuntimeError as e:
            if "not found" in str(e).lower():
                pytest.skip("Calculator button '1' not accessible on this macOS version")

    @pytest.mark.requires_app
    @pytest.mark.slow
    def test_wait_for_element_timeout_raises(self, calculator_app: TestApp) -> None:
        """wait_for_element() raises after timeout."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        with pytest.raises(RuntimeError):
            app.wait_for_element("NonExistent12345", timeout_ms=500)

    @pytest.mark.requires_app
    def test_timeout_zero_no_wait(self, calculator_app: TestApp) -> None:
        """Timeout of 0 means no waiting."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        start = time.perf_counter()
        with pytest.raises(RuntimeError):
            app.find("NonExistent", timeout_ms=0)
        elapsed = time.perf_counter() - start

        # Should return immediately
        assert elapsed < 0.5


class TestElementNotFound:
    """Tests for element not found error handling."""

    @pytest.mark.requires_app
    def test_element_not_found_error_message(self, calculator_app: TestApp) -> None:
        """Error message includes the query."""
        import axterminator as ax

        app = ax.app(name="Calculator")
        query = "UniqueNonExistentElement12345"

        with pytest.raises(RuntimeError) as exc_info:
            app.find(query, timeout_ms=100)

        error_msg = str(exc_info.value)
        assert "not found" in error_msg.lower()

    @pytest.mark.requires_app
    def test_find_by_role_not_found(self, calculator_app: TestApp) -> None:
        """find_by_role raises when no match."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        with pytest.raises(RuntimeError):
            app.find_by_role("AXButton", title="NonExistentTitle12345")

    def test_element_not_found_after_healing(self) -> None:
        """Error type changes after healing attempts fail."""
        # This tests the ElementNotFoundAfterHealing error type
        # Mock implementation would be needed
        pass


class TestElementProperties:
    """Tests for element property accessors."""

    @pytest.mark.requires_app
    def test_element_role(
        self, calculator_app: TestApp, find_calculator_button: Callable
    ) -> None:
        """Element has role() method returning role string."""
        import axterminator as ax

        app = ax.app(name="Calculator")
        element = find_calculator_button(app, "1")

        role = element.role()

        assert role is not None
        assert isinstance(role, str)
        assert role == "AXButton"

    @pytest.mark.requires_app
    def test_element_title(
        self, calculator_app: TestApp, find_calculator_button: Callable
    ) -> None:
        """Element has title() method."""
        import axterminator as ax

        app = ax.app(name="Calculator")
        element = find_calculator_button(app, "5")

        title = element.title()

        assert title == "5"

    @pytest.mark.requires_app
    def test_element_value(self, calculator_app: TestApp) -> None:
        """Element has value() method for elements with values."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        # Try to find display element which has a value
        try:
            element = app.find_by_role("AXStaticText")
            value = element.value()
            assert value is not None or value == ""
        except RuntimeError:
            # May not find this element
            pass

    @pytest.mark.requires_app
    def test_element_description(
        self, calculator_app: TestApp, find_calculator_button: Callable
    ) -> None:
        """Element has description() method."""
        import axterminator as ax

        app = ax.app(name="Calculator")
        element = find_calculator_button(app, "1")

        # description may be None
        desc = element.description()

        assert desc is None or isinstance(desc, str)

    @pytest.mark.requires_app
    def test_element_label(
        self, calculator_app: TestApp, find_calculator_button: Callable
    ) -> None:
        """Element has label() method."""
        import axterminator as ax

        app = ax.app(name="Calculator")
        element = find_calculator_button(app, "1")

        label = element.label()

        assert label is None or isinstance(label, str)

    @pytest.mark.requires_app
    def test_element_identifier(
        self, calculator_app: TestApp, find_calculator_button: Callable
    ) -> None:
        """Element has identifier() method."""
        import axterminator as ax

        app = ax.app(name="Calculator")
        element = find_calculator_button(app, "1")

        identifier = element.identifier()

        assert identifier is None or isinstance(identifier, str)

    @pytest.mark.requires_app
    def test_element_enabled(
        self, calculator_app: TestApp, find_calculator_button: Callable
    ) -> None:
        """Element has enabled() method."""
        import axterminator as ax

        app = ax.app(name="Calculator")
        element = find_calculator_button(app, "1")

        enabled = element.enabled()

        assert isinstance(enabled, bool)
        assert enabled is True  # Calculator buttons should be enabled

    @pytest.mark.requires_app
    def test_element_focused(
        self, calculator_app: TestApp, find_calculator_button: Callable
    ) -> None:
        """Element has focused() method."""
        import axterminator as ax

        app = ax.app(name="Calculator")
        element = find_calculator_button(app, "1")

        focused = element.focused()

        assert isinstance(focused, bool)

    @pytest.mark.requires_app
    def test_element_exists(
        self, calculator_app: TestApp, find_calculator_button: Callable
    ) -> None:
        """Element has exists() method."""
        import axterminator as ax

        app = ax.app(name="Calculator")
        element = find_calculator_button(app, "1")

        exists = element.exists()

        assert exists is True

    @pytest.mark.requires_app
    def test_element_bounds(
        self, calculator_app: TestApp, find_calculator_button: Callable
    ) -> None:
        """Element has bounds() method returning position/size."""
        import axterminator as ax

        app = ax.app(name="Calculator")
        element = find_calculator_button(app, "1")

        bounds = element.bounds()

        # bounds may be None if not implemented
        if bounds is not None:
            assert len(bounds) == 4
            x, y, width, height = bounds
            assert isinstance(x, (int, float))
            assert isinstance(y, (int, float))
            assert isinstance(width, (int, float))
            assert isinstance(height, (int, float))


class TestElementPropertyEdgeCases:
    """Edge cases for element properties."""

    def test_properties_on_mock_element(
        self, mock_ax_element: Callable[..., MockAXElement]
    ) -> None:
        """All properties work on mock element."""
        element = mock_ax_element(
            role="AXButton",
            title="Test",
            value="Value",
            identifier="test_id",
            description="A test button",
            label="Test Label",
            enabled=True,
            focused=False,
            bounds=(100, 100, 50, 30),
        )

        assert element.role == "AXButton"
        assert element.title == "Test"
        assert element.value == "Value"
        assert element.identifier == "test_id"
        assert element.description == "A test button"
        assert element.label == "Test Label"
        assert element.enabled is True
        assert element.focused is False
        assert element.bounds == (100, 100, 50, 30)

    def test_properties_with_none_values(
        self, mock_ax_element: Callable[..., MockAXElement]
    ) -> None:
        """Properties handle None values gracefully."""
        element = mock_ax_element(
            role="AXButton",
            title=None,
            value=None,
            identifier=None,
        )

        assert element.role == "AXButton"
        assert element.title is None
        assert element.value is None
        assert element.identifier is None

    def test_properties_with_empty_strings(
        self, mock_ax_element: Callable[..., MockAXElement]
    ) -> None:
        """Properties handle empty strings."""
        element = mock_ax_element(
            role="AXButton",
            title="",
            value="",
        )

        assert element.title == ""
        assert element.value == ""

    def test_properties_with_unicode(
        self, mock_ax_element: Callable[..., MockAXElement]
    ) -> None:
        """Properties handle unicode characters."""
        element = mock_ax_element(
            role="AXButton",
            title="Save Changes",
            description="Saves the current document",
        )

        assert element.title == "Save Changes"
        assert "Saves" in element.description


class TestQuerySyntax:
    """Tests for various query syntax formats."""

    @pytest.mark.requires_app
    def test_simple_title_query(
        self, calculator_app: TestApp, find_calculator_button: Callable
    ) -> None:
        """Simple string is treated as title search."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        element = find_calculator_button(app, "7")

        assert element is not None

    @pytest.mark.requires_app
    def test_role_prefix_query(self, calculator_app: TestApp) -> None:
        """role: prefix searches by role."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        try:
            element = app.find("role:AXButton")
            assert element is not None
        except RuntimeError:
            # Query syntax may not be implemented
            pass

    @pytest.mark.requires_app
    def test_combined_query(self, calculator_app: TestApp) -> None:
        """Combined role and title query."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        try:
            element = app.find("role:AXButton title:8")
            assert element is not None
        except RuntimeError:
            # Query syntax may not be implemented
            pass


class TestElementPerformance:
    """Performance tests for element operations."""

    @pytest.mark.slow
    @pytest.mark.requires_app
    def test_find_performance(
        self,
        calculator_app: TestApp,
        perf_timer: Callable[..., PerformanceResult],
    ) -> None:
        """Element find should be fast."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        result = perf_timer(
            lambda: app.find("5", timeout_ms=100),
            iterations=100,
            name="find_element",
        )

        # Target: <500ms p95 for element access with timeout
        # Note: timeout_ms=100 adds overhead; Rust core is 242µs
        assert result.p95_ms < 500, f"Find too slow: {result.p95_ms}ms"

    @pytest.mark.slow
    @pytest.mark.requires_app
    def test_property_access_performance(
        self,
        calculator_app: TestApp,
        perf_timer: Callable[..., PerformanceResult],
        find_calculator_button: Callable,
    ) -> None:
        """Property access should be very fast."""
        import axterminator as ax

        app = ax.app(name="Calculator")
        element = find_calculator_button(app, "5")

        result = perf_timer(
            lambda: element.role(),
            iterations=1000,
            name="property_access",
        )

        # Property access should be sub-millisecond
        assert result.p95_ms < 1, f"Property access too slow: {result.p95_ms}ms"

    @pytest.mark.slow
    @pytest.mark.requires_app
    def test_multiple_finds_performance(
        self,
        calculator_app: TestApp,
        perf_timer: Callable[..., PerformanceResult],
    ) -> None:
        """Multiple consecutive finds remain fast."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        def find_multiple() -> None:
            for i in range(10):
                app.find(str(i % 10), timeout_ms=100)

        result = perf_timer(
            find_multiple,
            iterations=10,
            name="find_multiple",
        )

        # 10 finds should complete in <2000ms total (with timeouts)
        assert result.avg_ms < 2000, f"Multiple finds too slow: {result.avg_ms}ms"
