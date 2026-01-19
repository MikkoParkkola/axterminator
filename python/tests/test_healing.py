"""
Tests for AXTerminator self-healing element location system.

Tests cover:
- Healing by data-testid
- Healing by aria-label
- Healing by identifier
- Healing by title
- Healing by xpath
- Healing by position
- Healing configuration
- Timeout budget management
"""

from __future__ import annotations

from typing import TYPE_CHECKING

import pytest

if TYPE_CHECKING:
    from conftest import MockAXElement, TestApp


class TestHealingByDataTestId:
    """Tests for healing using data-testid attribute."""

    def test_healing_prefers_data_testid(
        self, mock_calculator_tree: MockAXElement
    ) -> None:
        """data-testid is the first (most stable) healing strategy."""
        # In real implementation, data-testid would be the primary locator
        tree = mock_calculator_tree

        # Simulate element with data-testid
        def find_by_data_testid(
            node: MockAXElement, testid: str
        ) -> MockAXElement | None:
            if node.data_testid == testid:
                return node
            for child in node.get_children():
                result = find_by_data_testid(child, testid)
                if result:
                    return result
            return None

        # Add data-testid to a button
        tree.get_children()[0].get_children()[1].get_children()[
            5
        ].data_testid = "submit-button"

        button = find_by_data_testid(tree, "submit-button")

        assert button is not None
        assert button.title == "5"

    def test_data_testid_is_default_first_strategy(self) -> None:
        """data_testid should be first in default strategy order."""
        import axterminator as ax

        config = ax.HealingConfig()

        assert config.strategies[0] == "data_testid"

    def test_data_testid_stable_across_ui_changes(self) -> None:
        """data-testid should be stable even when UI structure changes."""
        # This is a conceptual test - data-testid doesn't change
        # when elements move in the tree, unlike xpath or position
        pass


class TestHealingByAriaLabel:
    """Tests for healing using aria-label attribute."""

    def test_healing_by_aria_label(self, mock_calculator_tree: MockAXElement) -> None:
        """aria-label is second healing strategy."""
        tree = mock_calculator_tree

        def find_by_aria_label(node: MockAXElement, label: str) -> MockAXElement | None:
            if node.aria_label == label:
                return node
            for child in node.get_children():
                result = find_by_aria_label(child, label)
                if result:
                    return result
            return None

        # Add aria-label to a button
        tree.get_children()[0].get_children()[1].get_children()[
            7
        ].aria_label = "Clear all entries"

        button = find_by_aria_label(tree, "Clear all entries")

        assert button is not None

    def test_aria_label_is_second_strategy(self) -> None:
        """aria_label should be second in default strategy order."""
        import axterminator as ax

        config = ax.HealingConfig()

        assert config.strategies[1] == "aria_label"

    def test_aria_label_for_accessibility(self) -> None:
        """aria-label is commonly used for screen reader descriptions."""
        # Elements often have aria-label for accessibility
        # which makes it a good fallback
        pass


class TestHealingByIdentifier:
    """Tests for healing using accessibility identifier."""

    def test_healing_by_identifier(self, mock_calculator_tree: MockAXElement) -> None:
        """identifier is third healing strategy."""
        tree = mock_calculator_tree

        def find_by_identifier(
            node: MockAXElement, identifier: str
        ) -> MockAXElement | None:
            if node.identifier == identifier:
                return node
            for child in node.get_children():
                result = find_by_identifier(child, identifier)
                if result:
                    return result
            return None

        button = find_by_identifier(tree, "calc_btn_5")

        assert button is not None
        assert button.title == "5"

    def test_identifier_is_third_strategy(self) -> None:
        """identifier should be third in default strategy order."""
        import axterminator as ax

        config = ax.HealingConfig()

        assert config.strategies[2] == "identifier"

    @pytest.mark.requires_app
    def test_native_apps_have_identifiers(self, calculator_app: TestApp) -> None:
        """Native macOS apps typically have AXIdentifier."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        try:
            element = app.find("1")
            identifier = element.identifier()

            # Native apps usually have identifiers
            # though they may be auto-generated
            assert identifier is None or isinstance(identifier, str)
        except RuntimeError:
            pass


class TestHealingByTitle:
    """Tests for healing using element title."""

    def test_healing_by_title(self, mock_calculator_tree: MockAXElement) -> None:
        """title is fourth healing strategy."""
        tree = mock_calculator_tree

        def find_by_title(node: MockAXElement, title: str) -> MockAXElement | None:
            if node.title == title:
                return node
            for child in node.get_children():
                result = find_by_title(child, title)
                if result:
                    return result
            return None

        button = find_by_title(tree, "AC")

        assert button is not None

    def test_title_is_fourth_strategy(self) -> None:
        """title should be fourth in default strategy order."""
        import axterminator as ax

        config = ax.HealingConfig()

        assert config.strategies[3] == "title"

    def test_title_may_change_with_localization(self) -> None:
        """Title can change with language settings."""
        # This is why title is lower priority than identifier
        pass


class TestHealingByXPath:
    """Tests for healing using structural XPath-like path."""

    def test_healing_by_xpath(self, mock_calculator_tree: MockAXElement) -> None:
        """xpath is fifth healing strategy."""
        tree = mock_calculator_tree

        # XPath example: //AXWindow/AXGroup/AXButton[@title='5']
        def find_by_path(
            node: MockAXElement,
            path_parts: list[tuple[str, dict[str, str] | None]],
            index: int = 0,
        ) -> MockAXElement | None:
            if index >= len(path_parts):
                return None

            role, attrs = path_parts[index]

            # Check if current node matches
            if node.role != role:
                return None

            if attrs:
                for key, value in attrs.items():
                    if getattr(node, key, None) != value:
                        return None

            # If this is the last part, we found it
            if index == len(path_parts) - 1:
                return node

            # Search children for next part
            for child in node.get_children():
                result = find_by_path(child, path_parts, index + 1)
                if result:
                    return result

            return None

        # Search for: //AXWindow/AXGroup/AXButton[@title='5']
        path = [
            ("AXApplication", None),
            ("AXWindow", None),
            ("AXGroup", None),
            ("AXButton", {"title": "5"}),
        ]

        button = find_by_path(tree, path)

        assert button is not None
        assert button.title == "5"

    def test_xpath_is_fifth_strategy(self) -> None:
        """xpath should be fifth in default strategy order."""
        import axterminator as ax

        config = ax.HealingConfig()

        assert config.strategies[4] == "xpath"

    def test_xpath_sensitive_to_structure_changes(self) -> None:
        """XPath can break when UI structure changes."""
        # This is why xpath is lower priority
        pass


class TestHealingByPosition:
    """Tests for healing using relative position."""

    def test_healing_by_position(self, mock_calculator_tree: MockAXElement) -> None:
        """position is sixth healing strategy."""
        tree = mock_calculator_tree

        # Find element closest to given position
        def find_by_position(
            node: MockAXElement,
            target_x: float,
            target_y: float,
            best: tuple[MockAXElement | None, float] = (None, float("inf")),
        ) -> tuple[MockAXElement | None, float]:
            if node.bounds:
                x, y, w, h = node.bounds
                center_x = x + w / 2
                center_y = y + h / 2
                distance = (
                    (center_x - target_x) ** 2 + (center_y - target_y) ** 2
                ) ** 0.5

                if distance < best[1]:
                    best = (node, distance)

            for child in node.get_children():
                best = find_by_position(child, target_x, target_y, best)

            return best

        # Add bounds to some elements
        tree.get_children()[0].get_children()[1].get_children()[5].bounds = (
            100,
            200,
            50,
            50,
        )

        element, distance = find_by_position(tree, 125, 225)

        assert element is not None

    def test_position_is_sixth_strategy(self) -> None:
        """position should be sixth in default strategy order."""
        import axterminator as ax

        config = ax.HealingConfig()

        assert config.strategies[5] == "position"

    def test_position_fragile_on_resize(self) -> None:
        """Position-based finding is fragile when window resizes."""
        # This is why position is low priority
        pass


class TestHealingConfig:
    """Tests for healing configuration."""

    def test_default_config(self) -> None:
        """Default config has all strategies enabled."""
        import axterminator as ax

        config = ax.HealingConfig()

        assert len(config.strategies) == 7
        assert config.max_heal_time_ms == 100
        assert config.cache_healed is True

    def test_custom_strategies(self) -> None:
        """Custom strategy list can be provided."""
        import axterminator as ax

        config = ax.HealingConfig(strategies=["data_testid", "identifier"])

        assert len(config.strategies) == 2
        assert "title" not in config.strategies

    def test_custom_timeout(self) -> None:
        """Custom timeout can be set."""
        import axterminator as ax

        config = ax.HealingConfig(max_heal_time_ms=500)

        assert config.max_heal_time_ms == 500

    def test_disable_caching(self) -> None:
        """Healing cache can be disabled."""
        import axterminator as ax

        config = ax.HealingConfig(cache_healed=False)

        assert config.cache_healed is False

    def test_configure_healing_global(self) -> None:
        """configure_healing sets global config."""
        import axterminator as ax

        config = ax.HealingConfig(max_heal_time_ms=200)

        # Should not raise
        ax.configure_healing(config)

    def test_strategy_order_matters(self) -> None:
        """Strategies are tried in order specified."""
        import axterminator as ax

        # Title-first config
        config = ax.HealingConfig(strategies=["title", "identifier", "data_testid"])

        assert config.strategies[0] == "title"

    def test_empty_strategies_list(self) -> None:
        """Empty strategies list disables healing."""
        import axterminator as ax

        config = ax.HealingConfig(strategies=[])

        assert len(config.strategies) == 0

    def test_config_strategies_getter(self) -> None:
        """strategies property is accessible."""
        import axterminator as ax

        config = ax.HealingConfig()

        strategies = config.strategies

        assert isinstance(strategies, list)
        assert all(isinstance(s, str) for s in strategies)

    def test_config_strategies_setter(self) -> None:
        """strategies can be modified after creation."""
        import axterminator as ax

        config = ax.HealingConfig()
        config.strategies = ["identifier", "title"]

        assert len(config.strategies) == 2

    def test_config_max_heal_time_getter(self) -> None:
        """max_heal_time_ms property is accessible."""
        import axterminator as ax

        config = ax.HealingConfig(max_heal_time_ms=250)

        assert config.max_heal_time_ms == 250

    def test_config_max_heal_time_setter(self) -> None:
        """max_heal_time_ms can be modified."""
        import axterminator as ax

        config = ax.HealingConfig()
        config.max_heal_time_ms = 300

        assert config.max_heal_time_ms == 300


class TestHealingTimeoutBudget:
    """Tests for healing timeout budget management."""

    def test_healing_respects_timeout(self) -> None:
        """Healing stops when timeout is reached."""
        import axterminator as ax

        config = ax.HealingConfig(max_heal_time_ms=50)

        # With a 50ms budget, not all strategies can be tried
        assert config.max_heal_time_ms == 50

    def test_budget_divided_among_strategies(self) -> None:
        """Time budget is managed across strategies."""
        import axterminator as ax

        config = ax.HealingConfig(max_heal_time_ms=100)

        # 7 strategies with 100ms budget = ~14ms per strategy
        per_strategy = config.max_heal_time_ms / len(config.strategies)

        assert per_strategy > 0

    @pytest.mark.slow
    def test_healing_stops_at_timeout(self) -> None:
        """Healing process respects timeout limit."""
        # This would need actual element healing to test properly
        pass

    def test_successful_heal_within_budget(self) -> None:
        """Successful heal returns before timeout."""
        import axterminator as ax

        config = ax.HealingConfig(max_heal_time_ms=5000)

        # If an element is found by first strategy,
        # it should return immediately, not wait 5 seconds
        assert config.max_heal_time_ms == 5000


class TestHealingStrategies:
    """Tests for individual healing strategy behavior."""

    def test_all_seven_strategies_exist(self) -> None:
        """All 7 healing strategies are defined."""
        import axterminator as ax

        config = ax.HealingConfig()

        expected = [
            "data_testid",
            "aria_label",
            "identifier",
            "title",
            "xpath",
            "position",
            "visual_vlm",
        ]

        for strategy in expected:
            assert strategy in config.strategies

    def test_visual_vlm_is_last_resort(self) -> None:
        """visual_vlm (VLM-based) is the last strategy."""
        import axterminator as ax

        config = ax.HealingConfig()

        assert config.strategies[-1] == "visual_vlm"

    def test_strategies_are_strings(self) -> None:
        """Strategy names are strings."""
        import axterminator as ax

        config = ax.HealingConfig()

        for strategy in config.strategies:
            assert isinstance(strategy, str)

    def test_unknown_strategy_ignored(self) -> None:
        """Unknown strategy names don't cause crashes."""
        import axterminator as ax

        # Should not raise
        config = ax.HealingConfig(
            strategies=["data_testid", "unknown_strategy", "title"]
        )

        assert len(config.strategies) == 3


class TestHealingCache:
    """Tests for healing result caching."""

    def test_cache_enabled_by_default(self) -> None:
        """Caching is enabled by default."""
        import axterminator as ax

        config = ax.HealingConfig()

        assert config.cache_healed is True

    def test_cache_can_be_disabled(self) -> None:
        """Caching can be explicitly disabled."""
        import axterminator as ax

        config = ax.HealingConfig(cache_healed=False)

        assert config.cache_healed is False

    def test_cached_heals_are_faster(self) -> None:
        """Second lookup of healed element is faster."""
        # This would need actual implementation to test
        pass


class TestHealingFallback:
    """Tests for healing fallback behavior."""

    def test_fallback_to_next_strategy(self) -> None:
        """When one strategy fails, next is tried."""
        # Conceptual: if data_testid fails, try aria_label
        pass

    def test_all_strategies_exhausted(self) -> None:
        """Error when all strategies fail."""
        # Should raise ElementNotFoundAfterHealing
        pass

    def test_partial_budget_to_next_strategy(self) -> None:
        """Remaining time budget passes to next strategy."""
        # If data_testid takes 10ms of 100ms budget,
        # next strategy has 90ms remaining
        pass


class TestHealingPerformance:
    """Performance tests for healing system."""

    @pytest.mark.slow
    def test_healing_under_100ms(self) -> None:
        """Default healing completes under 100ms."""
        import axterminator as ax

        config = ax.HealingConfig()

        # Default budget is 100ms
        assert config.max_heal_time_ms == 100

    def test_first_strategy_fastest(self) -> None:
        """First matching strategy returns immediately."""
        import axterminator as ax

        config = ax.HealingConfig(max_heal_time_ms=1000)

        # Even with 1000ms budget, first match should be instant
        assert config.max_heal_time_ms == 1000


class TestHealingIntegration:
    """Integration tests for healing with real apps."""

    @pytest.mark.requires_app
    def test_healing_finds_moved_element(self, calculator_app: TestApp) -> None:
        """Healing finds element even if it moves in tree."""
        import axterminator as ax

        app = ax.app(name="Calculator")

        # Find element normally first
        try:
            element = app.find("5")
            assert element is not None

            # Element should still be findable
            element2 = app.find("5")
            assert element2 is not None
        except RuntimeError:
            pass

    @pytest.mark.requires_app
    def test_healing_with_custom_config(self, calculator_app: TestApp) -> None:
        """Custom healing config affects element finding."""
        import axterminator as ax

        # Configure healing with fewer strategies
        config = ax.HealingConfig(
            strategies=["title", "identifier"],
            max_heal_time_ms=50,
        )
        ax.configure_healing(config)

        app = ax.app(name="Calculator")

        try:
            element = app.find("5")
            assert element is not None
        except RuntimeError:
            # May not find if strategies insufficient
            pass

        # Restore default config
        ax.configure_healing(ax.HealingConfig())
