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
    "BACKGROUND",
    "FOCUS",
    "__version__",
]
