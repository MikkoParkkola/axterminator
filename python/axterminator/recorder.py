"""Interaction recorder for generating test code.

Records user interactions with an application and generates
pytest-compatible test code.

Usage:
    from axterminator.recorder import Recorder

    app = axterminator.app(name="Calculator")
    recorder = Recorder(app)

    recorder.start()
    # ... perform interactions ...
    recorder.stop()

    print(recorder.generate_test())
"""

from __future__ import annotations

import time
from dataclasses import dataclass, field
from datetime import datetime
from typing import TYPE_CHECKING, List, Optional

if TYPE_CHECKING:
    from axterminator import AXApp, AXElement


@dataclass
class RecordedAction:
    """A single recorded action."""

    action_type: str  # "click", "type", "find"
    query: str
    value: Optional[str] = None  # For type actions
    timestamp: float = field(default_factory=time.time)
    element_role: Optional[str] = None
    element_title: Optional[str] = None


class Recorder:
    """Records interactions with an application.

    Monitors accessibility events and records actions for playback
    or test generation.
    """

    def __init__(self, app: "AXApp"):
        self.app = app
        self.actions: List[RecordedAction] = []
        self._recording = False
        self._last_element: Optional["AXElement"] = None

    def start(self) -> None:
        """Start recording interactions."""
        self._recording = True
        self.actions = []
        print(f"Recording started for {self.app.name or 'app'}")

    def stop(self) -> None:
        """Stop recording interactions."""
        self._recording = False
        print(f"Recording stopped. {len(self.actions)} actions recorded.")

    def record_click(self, query: str, element: Optional["AXElement"] = None) -> None:
        """Record a click action."""
        if not self._recording:
            return

        action = RecordedAction(
            action_type="click",
            query=query,
            element_role=element.role if element else None,
            element_title=element.title if element else None,
        )
        self.actions.append(action)

    def record_type(self, query: str, text: str, element: Optional["AXElement"] = None) -> None:
        """Record a type action."""
        if not self._recording:
            return

        action = RecordedAction(
            action_type="type",
            query=query,
            value=text,
            element_role=element.role if element else None,
            element_title=element.title if element else None,
        )
        self.actions.append(action)

    def record_find(self, query: str, element: Optional["AXElement"] = None) -> None:
        """Record a find action."""
        if not self._recording:
            return

        action = RecordedAction(
            action_type="find",
            query=query,
            element_role=element.role if element else None,
            element_title=element.title if element else None,
        )
        self.actions.append(action)

    def generate_test(self, test_name: str = "test_recorded") -> str:
        """Generate pytest test code from recorded actions.

        Returns:
            Python test code as a string
        """
        app_name = self.app.name or "UnknownApp"
        timestamp = datetime.now().strftime("%Y-%m-%d %H:%M:%S")

        lines = [
            '"""Auto-generated test from axterminator recording.',
            f"",
            f"Recorded: {timestamp}",
            f"Application: {app_name}",
            f"Actions: {len(self.actions)}",
            '"""',
            "",
            "import pytest",
            "import axterminator",
            "",
            "",
            f"def {test_name}():",
            f'    """Test recorded interactions with {app_name}."""',
            f'    app = axterminator.app(name="{app_name}")',
            "",
        ]

        for i, action in enumerate(self.actions):
            comment = ""
            if action.element_title:
                comment = f"  # {action.element_title}"
            elif action.element_role:
                comment = f"  # {action.element_role}"

            if action.action_type == "click":
                lines.append(f'    app.find("{action.query}").click(){comment}')
            elif action.action_type == "type":
                lines.append(f'    app.find("{action.query}").type_text("{action.value}"){comment}')
            elif action.action_type == "find":
                lines.append(f'    element_{i} = app.find("{action.query}"){comment}')
                lines.append(f"    assert element_{i} is not None")

            # Add small delay between actions for readability
            if i < len(self.actions) - 1:
                lines.append("")

        lines.append("")

        return "\n".join(lines)

    def generate_script(self) -> str:
        """Generate standalone Python script from recorded actions.

        Returns:
            Python script code as a string
        """
        app_name = self.app.name or "UnknownApp"
        timestamp = datetime.now().strftime("%Y-%m-%d %H:%M:%S")

        lines = [
            "#!/usr/bin/env python3",
            '"""Auto-generated script from axterminator recording.',
            f"",
            f"Recorded: {timestamp}",
            f"Application: {app_name}",
            f"Actions: {len(self.actions)}",
            '"""',
            "",
            "import axterminator",
            "",
            "",
            "def main():",
            "    # Check accessibility permissions",
            "    if not axterminator.is_accessibility_enabled():",
            '        print("Error: Accessibility permissions required")',
            "        return 1",
            "",
            f'    app = axterminator.app(name="{app_name}")',
            f'    print(f"Connected to {app_name} (PID: {{app.pid}})")',
            "",
        ]

        for action in self.actions:
            comment = ""
            if action.element_title:
                comment = f"  # {action.element_title}"

            if action.action_type == "click":
                lines.append(f'    app.find("{action.query}").click(){comment}')
                lines.append(f'    print("Clicked: {action.query}")')
            elif action.action_type == "type":
                lines.append(f'    app.find("{action.query}").type_text("{action.value}"){comment}')
                lines.append(f'    print("Typed: {action.value}")')
            elif action.action_type == "find":
                lines.append(f'    element = app.find("{action.query}"){comment}')
                lines.append(f'    print(f"Found: {{element.role}} - {{element.title}}")')

            lines.append("")

        lines.extend([
            '    print("Done!")',
            "    return 0",
            "",
            "",
            'if __name__ == "__main__":',
            "    import sys",
            "    sys.exit(main())",
            "",
        ])

        return "\n".join(lines)
