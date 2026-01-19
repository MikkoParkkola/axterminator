"""
AXTerminator test suite.

Test organization:
- test_connection.py - Application connection tests
- test_elements.py - Element finding and properties
- test_actions.py - Element actions (click, type, etc.)
- test_healing.py - Self-healing element location
- test_sync.py - Synchronization and waiting

Markers:
- @pytest.mark.background - Tests verifying background operation
- @pytest.mark.requires_app - Tests needing real running application
- @pytest.mark.slow - Performance tests that take longer
- @pytest.mark.integration - Tests requiring real macOS accessibility
"""
