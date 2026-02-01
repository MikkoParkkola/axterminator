"""Tests for recorder module."""

import pytest
from unittest.mock import MagicMock
from axterminator.recorder import Recorder, RecordedAction


class TestRecordedAction:
    """Tests for RecordedAction dataclass."""

    def test_recorded_action_click(self):
        """Test creating a click action."""
        action = RecordedAction(
            action_type="click",
            query="Save",
            element_role="AXButton",
            element_title="Save",
        )
        assert action.action_type == "click"
        assert action.query == "Save"
        assert action.value is None

    def test_recorded_action_type(self):
        """Test creating a type action."""
        action = RecordedAction(
            action_type="type",
            query="textfield",
            value="Hello",
        )
        assert action.action_type == "type"
        assert action.value == "Hello"


class TestRecorder:
    """Tests for Recorder class."""

    def test_recorder_creation(self):
        """Test creating a recorder."""
        mock_app = MagicMock()
        mock_app.name = "TestApp"
        recorder = Recorder(mock_app)
        assert recorder.app == mock_app
        assert len(recorder.actions) == 0
        assert not recorder._recording

    def test_recorder_start_stop(self):
        """Test start and stop recording."""
        mock_app = MagicMock()
        mock_app.name = "TestApp"
        recorder = Recorder(mock_app)

        recorder.start()
        assert recorder._recording
        assert len(recorder.actions) == 0

        recorder.stop()
        assert not recorder._recording

    def test_record_click(self):
        """Test recording a click action."""
        mock_app = MagicMock()
        mock_app.name = "TestApp"
        recorder = Recorder(mock_app)

        recorder.start()
        recorder.record_click("Save")
        recorder.stop()

        assert len(recorder.actions) == 1
        assert recorder.actions[0].action_type == "click"
        assert recorder.actions[0].query == "Save"

    def test_record_type(self):
        """Test recording a type action."""
        mock_app = MagicMock()
        mock_app.name = "TestApp"
        recorder = Recorder(mock_app)

        recorder.start()
        recorder.record_type("textfield", "Hello World")
        recorder.stop()

        assert len(recorder.actions) == 1
        assert recorder.actions[0].action_type == "type"
        assert recorder.actions[0].value == "Hello World"

    def test_record_not_recording(self):
        """Test that actions are not recorded when not recording."""
        mock_app = MagicMock()
        recorder = Recorder(mock_app)

        # Not started
        recorder.record_click("Save")
        assert len(recorder.actions) == 0

    def test_generate_test(self):
        """Test generating test code."""
        mock_app = MagicMock()
        mock_app.name = "Calculator"
        recorder = Recorder(mock_app)

        recorder.start()
        recorder.record_click("5")
        recorder.record_click("+")
        recorder.record_click("3")
        recorder.record_click("=")
        recorder.stop()

        code = recorder.generate_test()

        assert "def test_recorded():" in code
        assert 'axterminator.app(name="Calculator")' in code
        assert 'app.find("5").click()' in code
        assert 'app.find("+").click()' in code

    def test_generate_script(self):
        """Test generating standalone script."""
        mock_app = MagicMock()
        mock_app.name = "Calculator"
        recorder = Recorder(mock_app)

        recorder.start()
        recorder.record_click("5")
        recorder.stop()

        code = recorder.generate_script()

        assert "#!/usr/bin/env python3" in code
        assert "def main():" in code
        assert 'app.find("5").click()' in code
        assert 'if __name__ == "__main__":' in code
