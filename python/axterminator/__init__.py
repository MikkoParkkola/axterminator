# AXTerminator - World's Most Superior macOS GUI Testing Framework
# Re-export from native Rust extension

from axterminator.axterminator import (
    BACKGROUND,
    FOCUS,
    ActionMode,
    AXApp,
    AXElement,
    HealingConfig,
    __version__,
    app,
    configure_healing,
    is_accessibility_enabled,
)

# VLM integration for visual element detection
from axterminator.vlm import configure_vlm, detect_element_visual

# Synchronization utilities
from axterminator.sync import (
    SyncTimeout,
    wait_for_condition,
    wait_for_element,
    wait_for_idle,
    wait_for_value,
)

# Recording utilities
from axterminator.recorder import Recorder


# Stub for XPC sync availability check (not yet implemented)
def xpc_sync_available() -> bool:
    """Check if XPC synchronization is available (not yet implemented)."""
    return False


__all__ = [
    "ActionMode",
    "AXApp",
    "AXElement",
    "HealingConfig",
    "app",
    "is_accessibility_enabled",
    "configure_healing",
    "configure_vlm",
    "detect_element_visual",
    "xpc_sync_available",
    "BACKGROUND",
    "FOCUS",
    "__version__",
    # Sync utilities
    "wait_for_idle",
    "wait_for_element",
    "wait_for_condition",
    "wait_for_value",
    "SyncTimeout",
    # Recording
    "Recorder",
]
