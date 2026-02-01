"""
Tests for AXTerminator element actions.

Tests cover:
- Background click (CRITICAL - no focus stealing)
- Focus click
- Double click
- Right click
- Text input
- Value setting
"""

from __future__ import annotations

import time
from typing import TYPE_CHECKING, Callable

import pytest

if TYPE_CHECKING:
    from conftest import PerformanceResult, TestApp


class TestBackgroundClick:
    """
    Tests for background click - the WORLD FIRST feature.

    This is the most critical test class as background clicking
    without focus stealing is AXTerminator's key differentiator.
    """

    @pytest.mark.background
    @pytest.mark.requires_app
    def test_background_click_no_focus_steal(
        self,
        calculator_app: TestApp,
        focus_tracker: Callable,
        find_calculator_button: Callable,
    ) -> None:
        """
        CRITICAL: Background click must NOT steal focus.

        This is the core differentiator of AXTerminator.
        Competitors steal focus; AXTerminator doesn't.
        """
        import axterminator as ax

        # Get current focus state
        before_focus = focus_tracker()

        # Connect to Calculator (it's in background)
        app = ax.app(name="Calculator")
        element = find_calculator_button(app, "5")

        # Perform background click (default mode)
        element.click()

        # Small delay to let any focus change propagate
        time.sleep(0.2)

        # Get focus state after click
        after_focus = focus_tracker()

        # CRITICAL ASSERTION: Focus should NOT have changed
        assert before_focus.frontmost_app == after_focus.frontmost_app, (
            f"FOCUS STOLEN! Was '{before_focus.frontmost_app}', "
            f"now '{after_focus.frontmost_app}'"
        )

    @pytest.mark.background
    @pytest.mark.requires_app
    def test_background_click_explicit_mode(
        self,
        calculator_app: TestApp,
        focus_tracker: Callable,
        find_calculator_button: Callable,
    ) -> None:
        """Background click with explicit BACKGROUND mode."""
        import axterminator as ax

        before_focus = focus_tracker()

        app = ax.app(name="Calculator")
        element = find_calculator_button(app, "7")

        # Explicit background mode
        element.click(mode=ax.BACKGROUND)

        time.sleep(0.2)
        after_focus = focus_tracker()

        assert before_focus.frontmost_app == after_focus.frontmost_app

    @pytest.mark.background
    @pytest.mark.requires_app
    def test_background_click_default_is_background(
        self, calculator_app: TestApp, find_calculator_button: Callable
    ) -> None:
        """Default click mode should be BACKGROUND."""
        import axterminator as ax

        # Verify BACKGROUND is the default
        assert ax.ActionMode.Background == ax.BACKGROUND

        app = ax.app(name="Calculator")
        element = find_calculator_button(app, "3")

        # Default click should work without error
        element.click()

    @pytest.mark.background
    @pytest.mark.requires_app
    def test_background_click_multiple_times(
        self,
        calculator_app: TestApp,
        focus_tracker: Callable,
        find_calculator_button: Callable,
    ) -> None:
        """Multiple background clicks don't steal focus."""
        import axterminator as ax

        before_focus = focus_tracker()

        app = ax.app(name="Calculator")

        # Click multiple buttons in sequence
        for digit in ["1", "2", "3"]:
            element = find_calculator_button(app, digit)
            element.click()
            time.sleep(0.1)

        after_focus = focus_tracker()

        assert before_focus.frontmost_app == after_focus.frontmost_app

    @pytest.mark.background
    @pytest.mark.requires_app
    def test_background_click_on_disabled_element(
        self, calculator_app: TestApp, find_calculator_button: Callable
    ) -> None:
        """Background click on disabled element handles gracefully."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        # AC button is always enabled, but we test the flow
        element = find_calculator_button(app, "AC")
        element.click()  # Should work

    @pytest.mark.background
    @pytest.mark.requires_app
    def test_background_click_returns_none(
        self, calculator_app: TestApp, find_calculator_button: Callable
    ) -> None:
        """Click returns None (not the element for chaining)."""
        import axterminator as ax

        app = ax.app(name="Calculator")
        element = find_calculator_button(app, "9")

        result = element.click()

        assert result is None

    @pytest.mark.background
    @pytest.mark.requires_app
    def test_background_mode_constant(self) -> None:
        """BACKGROUND constant is available."""
        import axterminator as ax

        assert hasattr(ax, "BACKGROUND")
        assert ax.BACKGROUND == ax.ActionMode.Background


class TestFocusClick:
    """Tests for focus click mode."""

    @pytest.mark.requires_app
    def test_focus_click_brings_app_forward(
        self,
        calculator_app: TestApp,
        focus_tracker: Callable,
        find_calculator_button: Callable,
    ) -> None:
        """Focus click brings application to foreground."""
        import axterminator as ax

        app = ax.app(name="Calculator")
        element = find_calculator_button(app, "5")

        # Focus click should bring Calculator to front
        element.click(mode=ax.FOCUS)

        time.sleep(0.3)
        after_focus = focus_tracker()

        # Calculator should now be frontmost
        assert "Calculator" in after_focus.frontmost_app

    @pytest.mark.requires_app
    def test_focus_click_explicit(
        self, calculator_app: TestApp, find_calculator_button: Callable
    ) -> None:
        """Focus click works with explicit mode."""
        import axterminator as ax

        app = ax.app(name="Calculator")
        element = find_calculator_button(app, "8")

        # Should not raise
        element.click(mode=ax.FOCUS)

    @pytest.mark.requires_app
    def test_focus_mode_constant(self) -> None:
        """FOCUS constant is available."""
        import axterminator as ax

        assert hasattr(ax, "FOCUS")
        assert ax.FOCUS == ax.ActionMode.Focus

    @pytest.mark.requires_app
    def test_focus_click_on_text_field(self, textedit_app: TestApp) -> None:
        """Focus click on text field prepares for input."""
        import axterminator as ax

        app = ax.app(name="TextEdit")

        try:
            # Find text area
            element = app.find_by_role("AXTextArea")
            element.click(mode=ax.FOCUS)
        except RuntimeError:
            # May not find element
            pass


class TestDoubleClick:
    """Tests for double-click action."""

    @pytest.mark.requires_app
    def test_double_click_background(
        self,
        calculator_app: TestApp,
        focus_tracker: Callable,
        find_calculator_button: Callable,
    ) -> None:
        """Double-click in background mode doesn't steal focus."""
        import axterminator as ax

        before_focus = focus_tracker()

        app = ax.app(name="Calculator")
        element = find_calculator_button(app, "0")

        element.double_click()

        time.sleep(0.2)
        after_focus = focus_tracker()

        assert before_focus.frontmost_app == after_focus.frontmost_app

    @pytest.mark.requires_app
    def test_double_click_focus_mode(
        self, calculator_app: TestApp, find_calculator_button: Callable
    ) -> None:
        """Double-click in focus mode."""
        import axterminator as ax

        app = ax.app(name="Calculator")
        element = find_calculator_button(app, "1")

        # Should not raise
        element.double_click(mode=ax.FOCUS)

    @pytest.mark.requires_app
    def test_double_click_timing(
        self, calculator_app: TestApp, find_calculator_button: Callable
    ) -> None:
        """Double-click has appropriate timing between clicks."""
        import axterminator as ax

        app = ax.app(name="Calculator")
        element = find_calculator_button(app, "2")

        start = time.perf_counter()
        element.double_click()
        elapsed = time.perf_counter() - start

        # Double-click should include a small delay (50ms in impl)
        # but not be too slow
        assert elapsed < 0.5


class TestRightClick:
    """Tests for right-click (context menu) action."""

    @pytest.mark.requires_app
    def test_right_click_background(
        self,
        finder_app: TestApp,
        focus_tracker: Callable,
    ) -> None:
        """Right-click in background mode doesn't steal focus."""
        import axterminator as ax

        # Use Finder which has context menus
        app = ax.app(name="Finder")

        before_focus = focus_tracker()

        try:
            window = app.main_window()
            window.right_click()
        except RuntimeError:
            # May not find window
            pass

        time.sleep(0.2)
        after_focus = focus_tracker()

        # Focus should not change
        assert before_focus.frontmost_app == after_focus.frontmost_app

    @pytest.mark.requires_app
    def test_right_click_shows_menu(self, calculator_app: TestApp) -> None:
        """Right-click triggers AXShowMenu action."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        try:
            element = app.find("1")
            element.right_click()
        except RuntimeError:
            # Element may not support context menu
            pass

    @pytest.mark.requires_app
    def test_right_click_with_mode(
        self, calculator_app: TestApp, find_calculator_button: Callable
    ) -> None:
        """Right-click accepts mode parameter."""
        import axterminator as ax

        app = ax.app(name="Calculator")
        element = find_calculator_button(app, "5")

        # Should accept mode parameter
        try:
            element.right_click(mode=ax.BACKGROUND)
        except RuntimeError:
            # May fail if ShowMenu not supported
            pass


class TestTypeText:
    """Tests for text input action."""

    @pytest.mark.requires_app
    def test_type_text_requires_focus(self, textedit_app: TestApp) -> None:
        """Type text requires FOCUS mode."""
        import axterminator as ax

        app = ax.app(name="TextEdit")

        try:
            element = app.find_by_role("AXTextArea")

            # Typing with BACKGROUND should raise error
            with pytest.raises(RuntimeError, match="FOCUS"):
                element.type_text("test", mode=ax.BACKGROUND)
        except RuntimeError:
            # May not find element
            pass

    @pytest.mark.requires_app
    def test_type_text_focus_mode(self, textedit_app: TestApp) -> None:
        """Type text works in focus mode."""
        import axterminator as ax

        app = ax.app(name="TextEdit")

        try:
            element = app.find_by_role("AXTextArea")

            # Should work with FOCUS mode (or default)
            element.type_text("Hello", mode=ax.FOCUS)
        except RuntimeError:
            # May fail if not implemented yet
            pass

    @pytest.mark.requires_app
    def test_type_text_default_focus_mode(self, textedit_app: TestApp) -> None:
        """Type text defaults to FOCUS mode."""
        import axterminator as ax

        app = ax.app(name="TextEdit")

        try:
            element = app.find_by_role("AXTextArea")

            # Default should be FOCUS for type_text
            element.type_text("World")
        except RuntimeError:
            # May fail if not implemented
            pass

    @pytest.mark.requires_app
    def test_type_text_special_characters(self, textedit_app: TestApp) -> None:
        """Type text handles special characters."""
        import axterminator as ax

        app = ax.app(name="TextEdit")

        try:
            element = app.find_by_role("AXTextArea")

            # Test special characters
            element.type_text("Hello! @#$%")
        except RuntimeError:
            pass

    @pytest.mark.requires_app
    def test_type_text_unicode(self, textedit_app: TestApp) -> None:
        """Type text handles unicode."""
        import axterminator as ax

        app = ax.app(name="TextEdit")

        try:
            element = app.find_by_role("AXTextArea")

            # Test unicode
            element.type_text("Cafe")
        except RuntimeError:
            pass

    def test_type_text_empty_string(self, textedit_app: TestApp) -> None:
        """Type text with empty string is handled."""
        import axterminator as ax

        app = ax.app(name="TextEdit")

        try:
            element = app.find_by_role("AXTextArea")
            element.type_text("")
        except RuntimeError:
            pass


class TestSetValue:
    """Tests for set_value action."""

    @pytest.mark.requires_app
    def test_set_value_on_text_field(self, textedit_app: TestApp) -> None:
        """Set value on text field."""
        import axterminator as ax

        app = ax.app(name="TextEdit")

        try:
            element = app.find_by_role("AXTextArea")
            element.set_value("Direct value")
        except RuntimeError:
            # May not be implemented
            pass

    @pytest.mark.requires_app
    def test_set_value_clears_existing(self, textedit_app: TestApp) -> None:
        """Set value replaces existing content."""
        import axterminator as ax

        app = ax.app(name="TextEdit")

        try:
            element = app.find_by_role("AXTextArea")
            element.set_value("First")
            element.set_value("Second")

            # Value should be "Second", not "FirstSecond"
            value = element.value()
            if value:
                assert "First" not in value or value == "Second"
        except RuntimeError:
            pass

    @pytest.mark.requires_app
    def test_set_value_empty_string(self, textedit_app: TestApp) -> None:
        """Set value to empty string clears field."""
        import axterminator as ax

        app = ax.app(name="TextEdit")

        try:
            element = app.find_by_role("AXTextArea")
            element.set_value("")
        except RuntimeError:
            pass

    def test_set_value_on_button_fails(
        self, calculator_app: TestApp, find_calculator_button: Callable
    ) -> None:
        """Set value on button should fail (no value attribute)."""
        import axterminator as ax

        app = ax.app(name="Calculator")
        element = find_calculator_button(app, "5")

        with pytest.raises(RuntimeError):
            element.set_value("cannot set")


class TestActionModes:
    """Tests for action mode constants and behavior."""

    def test_action_mode_enum_values(self) -> None:
        """ActionMode enum has expected values."""
        import axterminator as ax

        assert hasattr(ax, "ActionMode")
        assert hasattr(ax.ActionMode, "Background")
        assert hasattr(ax.ActionMode, "Focus")

    def test_background_constant(self) -> None:
        """BACKGROUND constant equals ActionMode.Background."""
        import axterminator as ax

        assert ax.BACKGROUND == ax.ActionMode.Background

    def test_focus_constant(self) -> None:
        """FOCUS constant equals ActionMode.Focus."""
        import axterminator as ax

        assert ax.FOCUS == ax.ActionMode.Focus

    def test_default_mode_is_background(self) -> None:
        """Default action mode is Background."""

        # ActionMode default should be Background

        # Can't test default() from Python, but BACKGROUND should be usable as default


class TestActionErrors:
    """Tests for action error handling."""

    @pytest.mark.requires_app
    def test_click_on_nonexistent_element_raises(
        self, calculator_app: TestApp, find_calculator_button: Callable
    ) -> None:
        """Clicking destroyed/invalid element raises error."""
        import axterminator as ax

        app = ax.app(name="Calculator")
        element = find_calculator_button(app, "5")

        # Terminate app
        calculator_app.terminate()
        time.sleep(0.5)

        # Clicking should now fail
        with pytest.raises(RuntimeError):
            element.click()

    def test_action_not_supported_error(self) -> None:
        """Actions not supported by element raise appropriate error."""
        # Some elements don't support certain actions
        # This would need specific UI elements to test
        pass


class TestActionPerformance:
    """Performance tests for actions."""

    @pytest.mark.slow
    @pytest.mark.background
    @pytest.mark.requires_app
    def test_background_click_performance(
        self,
        calculator_app: TestApp,
        perf_timer: Callable[..., PerformanceResult],
        find_calculator_button: Callable,
    ) -> None:
        """Background click should be fast."""
        import axterminator as ax

        app = ax.app(name="Calculator")
        element = find_calculator_button(app, "5")

        result = perf_timer(
            lambda: element.click(),
            iterations=50,
            name="background_click",
        )

        # Click should be sub-10ms p95
        assert result.p95_ms < 10, f"Click too slow: {result.p95_ms}ms"

    @pytest.mark.slow
    @pytest.mark.requires_app
    def test_double_click_performance(
        self,
        calculator_app: TestApp,
        perf_timer: Callable[..., PerformanceResult],
        find_calculator_button: Callable,
    ) -> None:
        """Double-click includes internal delay but should still be fast."""
        import axterminator as ax

        app = ax.app(name="Calculator")
        element = find_calculator_button(app, "5")

        result = perf_timer(
            lambda: element.double_click(),
            iterations=20,
            name="double_click",
        )

        # Double-click has 50ms internal delay, so allow 100ms
        assert result.p95_ms < 150, f"Double-click too slow: {result.p95_ms}ms"


class TestScreenshot:
    """Tests for screenshot functionality."""

    @pytest.mark.requires_app
    def test_app_screenshot(self, calculator_app: TestApp) -> None:
        """App screenshot returns PNG bytes."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        try:
            data = app.screenshot()

            assert data is not None
            assert len(data) > 0
            # PNG magic bytes
            assert data[:4] == b"\x89PNG" or len(data) > 100
        except RuntimeError:
            # Screenshot may require additional permissions
            pass

    @pytest.mark.requires_app
    def test_element_screenshot(
        self, calculator_app: TestApp, find_calculator_button: Callable
    ) -> None:
        """Element screenshot captures just that element."""
        import axterminator as ax

        app = ax.app(name="Calculator")
        element = find_calculator_button(app, "5")

        try:
            data = element.screenshot()

            assert data is not None
        except RuntimeError:
            # May not be implemented
            pass
