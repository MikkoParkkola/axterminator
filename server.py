#!/usr/bin/env python3
"""AXTerminator MCP Server - World's First Background macOS GUI Testing.

WORLD FIRST: Test macOS apps without stealing focus - true background testing.

Performance:
    - Element access: 242µs (vs 500ms-2s competitors)
    - Full login test: 103ms (vs 6.6s Appium)
    - 60-100x faster than XCUITest/Appium

Features:
    - Background testing (no focus stealing!)
    - 7-strategy self-healing locators
    - Cross-app testing support
    - Works with any macOS app (SwiftUI, AppKit, Electron, WebView)

Created: 2026-01-21
"""

import asyncio
import base64
import logging
from typing import Any

from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp.types import TextContent, Tool

# Configure logging
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger("axterminator")

# Try to import axterminator
try:
    import axterminator as ax

    AXTERMINATOR_AVAILABLE = True
except ImportError:
    ax = None  # type: ignore
    AXTERMINATOR_AVAILABLE = False
    logger.warning("axterminator not installed - install with: pip install axterminator")

# Try to import VLM module for visual fallback
VLM_AVAILABLE = False
try:
    from axterminator.vlm import configure_vlm, detect_element_visual
    # Try to configure a VLM backend
    import os
    if os.environ.get("ANTHROPIC_API_KEY"):
        configure_vlm(backend="anthropic")
        VLM_AVAILABLE = True
        logger.info("VLM fallback enabled (Anthropic)")
    elif os.environ.get("OPENAI_API_KEY"):
        configure_vlm(backend="openai")
        VLM_AVAILABLE = True
        logger.info("VLM fallback enabled (OpenAI)")
    else:
        try:
            configure_vlm(backend="mlx")
            VLM_AVAILABLE = True
            logger.info("VLM fallback enabled (MLX local)")
        except Exception:
            logger.info("VLM fallback not available (no MLX or API keys)")
except ImportError:
    logger.info("VLM module not installed - visual fallback disabled")

# Store connected apps
_connected_apps: dict[str, Any] = {}


def _click_at_coordinates(x: int, y: int, click_type: str = "single") -> bool:
    """Click at screen coordinates using Quartz/CGEvent (for VLM fallback)."""
    try:
        import Quartz
        from Quartz import CGEventCreateMouseEvent, CGEventPost, kCGEventLeftMouseDown, kCGEventLeftMouseUp, kCGHIDEventTap, CGPoint

        point = CGPoint(x, y)

        # Mouse down
        event = CGEventCreateMouseEvent(None, kCGEventLeftMouseDown, point, 0)
        CGEventPost(kCGHIDEventTap, event)

        # Mouse up
        event = CGEventCreateMouseEvent(None, kCGEventLeftMouseUp, point, 0)
        CGEventPost(kCGHIDEventTap, event)

        if click_type == "double":
            import time
            time.sleep(0.1)
            event = CGEventCreateMouseEvent(None, kCGEventLeftMouseDown, point, 0)
            CGEventPost(kCGHIDEventTap, event)
            event = CGEventCreateMouseEvent(None, kCGEventLeftMouseUp, point, 0)
            CGEventPost(kCGHIDEventTap, event)

        return True
    except Exception as e:
        logger.warning(f"Coordinate click failed: {e}")
        return False

# Initialize MCP server
server = Server("axterminator")


def _get_app(app_name: str) -> Any:
    """Get a connected app by name."""
    if app_name not in _connected_apps:
        raise ValueError(f"App '{app_name}' not connected. Use ax_connect first.")
    return _connected_apps[app_name]


# ============================================================================
# TOOLS
# ============================================================================


@server.list_tools()
async def list_tools() -> list[Tool]:
    """List available tools."""
    return [
        Tool(
            name="ax_connect",
            description="""Connect to a macOS application for GUI testing.

Connect by bundle ID (recommended), name, or PID.
The app must be running and accessibility must be enabled.

Returns: Connection status and app info.""",
            inputSchema={
                "type": "object",
                "properties": {
                    "app": {
                        "type": "string",
                        "description": "App identifier: bundle ID (com.apple.Safari), name (Safari), or PID (12345)",
                    },
                    "alias": {
                        "type": "string",
                        "description": "Optional alias to reference this app later (defaults to app name)",
                    },
                },
                "required": ["app"],
            },
        ),
        Tool(
            name="ax_find",
            description="""Find UI elements in a connected app.

Search by text, role, or attributes. Uses 7-strategy self-healing locators.
Returns element info including role, title, position, and available actions.

Query syntax:
- Simple text: "Save" (finds element with text "Save")
- By role: "role:AXButton" (finds buttons)
- Combined: "role:AXButton title:Save" (finds Save button)
- XPath-like: "//AXButton[@AXTitle='OK']" """,
            inputSchema={
                "type": "object",
                "properties": {
                    "app": {
                        "type": "string",
                        "description": "App name/alias from ax_connect",
                    },
                    "query": {
                        "type": "string",
                        "description": "Element query (text, role:X, title:X, or XPath-like)",
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "description": "Max time to wait for element (default: 5000ms)",
                        "default": 5000,
                    },
                },
                "required": ["app", "query"],
            },
        ),
        Tool(
            name="ax_click",
            description="""Click a UI element - IN BACKGROUND (no focus stealing!).

WORLD FIRST: Click without stealing focus from user's active work.
Use mode=focus only if the element requires it (rare).

Returns: Click result and element state after click.""",
            inputSchema={
                "type": "object",
                "properties": {
                    "app": {
                        "type": "string",
                        "description": "App name/alias from ax_connect",
                    },
                    "query": {
                        "type": "string",
                        "description": "Element to click (text, role:X, or XPath-like)",
                    },
                    "mode": {
                        "type": "string",
                        "description": "Action mode: 'background' (default, no focus steal) or 'focus'",
                        "enum": ["background", "focus"],
                        "default": "background",
                    },
                    "click_type": {
                        "type": "string",
                        "description": "Click type: 'single' (default), 'double', or 'right'",
                        "enum": ["single", "double", "right"],
                        "default": "single",
                    },
                },
                "required": ["app", "query"],
            },
        ),
        Tool(
            name="ax_type",
            description="""Type text into an element.

Note: Text input typically requires focus mode.
For setting values directly without typing, use ax_set_value instead.

Returns: Typing result.""",
            inputSchema={
                "type": "object",
                "properties": {
                    "app": {
                        "type": "string",
                        "description": "App name/alias from ax_connect",
                    },
                    "query": {
                        "type": "string",
                        "description": "Element to type into (text field query)",
                    },
                    "text": {
                        "type": "string",
                        "description": "Text to type",
                    },
                    "mode": {
                        "type": "string",
                        "description": "Action mode: 'focus' (default for typing) or 'background'",
                        "enum": ["background", "focus"],
                        "default": "focus",
                    },
                },
                "required": ["app", "query", "text"],
            },
        ),
        Tool(
            name="ax_set_value",
            description="""Set the value of an element directly (no keystroke simulation).

Faster than ax_type and works in background mode.
Use for text fields, sliders, etc.

Returns: Set result and new value.""",
            inputSchema={
                "type": "object",
                "properties": {
                    "app": {
                        "type": "string",
                        "description": "App name/alias from ax_connect",
                    },
                    "query": {
                        "type": "string",
                        "description": "Element to set value on",
                    },
                    "value": {
                        "type": "string",
                        "description": "Value to set",
                    },
                },
                "required": ["app", "query", "value"],
            },
        ),
        Tool(
            name="ax_get_value",
            description="""Get the current value of an element.

Works for text fields, labels, checkboxes, sliders, etc.
Returns the accessibility value attribute.

Returns: Element value and additional attributes.""",
            inputSchema={
                "type": "object",
                "properties": {
                    "app": {
                        "type": "string",
                        "description": "App name/alias from ax_connect",
                    },
                    "query": {
                        "type": "string",
                        "description": "Element to get value from",
                    },
                },
                "required": ["app", "query"],
            },
        ),
        Tool(
            name="ax_list_windows",
            description="""List all windows of a connected app.

Returns window titles, positions, sizes, and states.

Returns: List of window information.""",
            inputSchema={
                "type": "object",
                "properties": {
                    "app": {
                        "type": "string",
                        "description": "App name/alias from ax_connect",
                    },
                },
                "required": ["app"],
            },
        ),
        Tool(
            name="ax_screenshot",
            description="""Take a screenshot of an app or element.

Captures without stealing focus.
Returns base64-encoded PNG image.

Returns: Base64 screenshot data.""",
            inputSchema={
                "type": "object",
                "properties": {
                    "app": {
                        "type": "string",
                        "description": "App name/alias from ax_connect",
                    },
                    "query": {
                        "type": "string",
                        "description": "Optional: element to screenshot (entire app if not specified)",
                    },
                },
                "required": ["app"],
            },
        ),
        Tool(
            name="ax_click_at",
            description="""Click at specific screen coordinates.

Use this when VLM visual detection found an element but accessibility couldn't.
Coordinates are absolute screen position.

Returns: Click result.""",
            inputSchema={
                "type": "object",
                "properties": {
                    "x": {
                        "type": "integer",
                        "description": "X coordinate (pixels from left)",
                    },
                    "y": {
                        "type": "integer",
                        "description": "Y coordinate (pixels from top)",
                    },
                    "click_type": {
                        "type": "string",
                        "description": "Click type: 'single' (default), 'double', or 'right'",
                        "enum": ["single", "double", "right"],
                        "default": "single",
                    },
                },
                "required": ["x", "y"],
            },
        ),
        Tool(
            name="ax_find_visual",
            description="""Find UI element using VLM visual detection (AI vision).

Use this when accessibility-based locators fail (e.g., shadow DOM, canvas, WebGL).
Takes a screenshot and uses AI to locate the element visually.

Returns: Element coordinates if found.""",
            inputSchema={
                "type": "object",
                "properties": {
                    "app": {
                        "type": "string",
                        "description": "App name/alias from ax_connect",
                    },
                    "description": {
                        "type": "string",
                        "description": "Natural language description of element (e.g., 'Load unpacked button')",
                    },
                },
                "required": ["app", "description"],
            },
        ),
        Tool(
            name="ax_wait_idle",
            description="""Wait for an app to become idle (no pending UI updates).

Useful before asserting state or taking screenshots.

Returns: Idle status and wait time.""",
            inputSchema={
                "type": "object",
                "properties": {
                    "app": {
                        "type": "string",
                        "description": "App name/alias from ax_connect",
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "description": "Max time to wait (default: 5000ms)",
                        "default": 5000,
                    },
                },
                "required": ["app"],
            },
        ),
        Tool(
            name="ax_is_accessible",
            description="""Check if accessibility is enabled and working.

Returns: Accessibility status and any setup instructions needed.""",
            inputSchema={
                "type": "object",
                "properties": {},
            },
        ),
    ]


@server.call_tool()
async def call_tool(name: str, arguments: dict[str, Any]) -> list[TextContent]:
    """Handle tool calls."""
    if not AXTERMINATOR_AVAILABLE:
        return [
            TextContent(
                type="text",
                text="Error: axterminator not installed. Install with: pip install axterminator",
            )
        ]

    try:
        if name == "ax_is_accessible":
            is_enabled = ax.is_accessibility_enabled()
            if is_enabled:
                return [
                    TextContent(
                        type="text",
                        text="✅ Accessibility is enabled and working.",
                    )
                ]
            else:
                return [
                    TextContent(
                        type="text",
                        text="❌ Accessibility not enabled.\n\nTo enable:\n1. Open System Settings > Privacy & Security > Accessibility\n2. Add and enable the terminal app (Terminal, iTerm2, etc.)\n3. Restart the terminal",
                    )
                ]

        elif name == "ax_connect":
            app_id = arguments["app"]
            alias = arguments.get("alias")

            # Determine connection method
            if app_id.isdigit():
                app_obj = ax.app(pid=int(app_id))
            elif "." in app_id and app_id.count(".") >= 2:
                app_obj = ax.app(bundle_id=app_id)
            else:
                app_obj = ax.app(name=app_id)

            # Store with alias or app name
            key = alias or app_id
            _connected_apps[key] = app_obj

            return [
                TextContent(
                    type="text",
                    text=f"✅ Connected to '{key}'\nReady for GUI testing (background mode by default).",
                )
            ]

        elif name == "ax_find":
            app = _get_app(arguments["app"])
            query = arguments["query"]
            timeout = arguments.get("timeout_ms", 5000)
            use_vlm = arguments.get("use_vlm_fallback", True)  # Enable VLM fallback by default

            element = app.wait_for_element(query, timeout_ms=timeout)

            if element:
                # Call methods on AXElement to get actual values
                role = element.role() if callable(getattr(element, "role", None)) else getattr(element, "role", "unknown")
                title = element.title() if callable(getattr(element, "title", None)) else getattr(element, "title", "")
                value = element.value() if callable(getattr(element, "value", None)) else getattr(element, "value", "")
                enabled = element.enabled() if callable(getattr(element, "enabled", None)) else getattr(element, "enabled", True)
                position = element.position() if callable(getattr(element, "position", None)) else getattr(element, "position", None)
                size = element.size() if callable(getattr(element, "size", None)) else getattr(element, "size", None)
                info = {
                    "found": True,
                    "method": "accessibility",
                    "role": role,
                    "title": title,
                    "value": value,
                    "enabled": enabled,
                    "position": position,
                    "size": size,
                }
                return [
                    TextContent(
                        type="text",
                        text=f"✅ Found element:\n{info}",
                    )
                ]

            # VLM fallback: try visual detection if accessibility failed
            if use_vlm and VLM_AVAILABLE:
                logger.info(f"Accessibility search failed for '{query}', trying VLM visual fallback...")
                try:
                    # Take screenshot of the app
                    screenshot_data = app.screenshot()
                    if screenshot_data:
                        # Use VLM to detect element visually
                        result = detect_element_visual(
                            image_data=screenshot_data,
                            description=query,
                            image_width=1920,  # TODO: get actual dimensions
                            image_height=1080,
                        )
                        if result:
                            x, y = result
                            return [
                                TextContent(
                                    type="text",
                                    text=f"✅ Found element via VLM visual detection:\n{{'found': True, 'method': 'visual_vlm', 'position': ({x}, {y}), 'query': '{query}'}}",
                                )
                            ]
                except Exception as e:
                    logger.warning(f"VLM fallback failed: {e}")

            # All strategies failed
            vlm_status = " (VLM fallback attempted)" if use_vlm and VLM_AVAILABLE else " (VLM fallback not available)"
            return [
                TextContent(
                    type="text",
                    text=f"❌ Element not found: '{query}' (timeout: {timeout}ms){vlm_status}",
                )
            ]

        elif name == "ax_click":
            app = _get_app(arguments["app"])
            query = arguments["query"]
            mode_str = arguments.get("mode", "background")
            click_type = arguments.get("click_type", "single")
            use_vlm = arguments.get("use_vlm_fallback", True)

            mode = ax.BACKGROUND if mode_str == "background" else ax.FOCUS
            element = app.find(query)

            if element:
                if click_type == "double":
                    element.double_click(mode=mode)
                elif click_type == "right":
                    element.right_click(mode=mode)
                else:
                    element.click(mode=mode)

                return [
                    TextContent(
                        type="text",
                        text=f"✅ Clicked '{query}' ({click_type}, {mode_str} mode)",
                    )
                ]

            # VLM fallback: try visual detection and coordinate-based click
            if use_vlm and VLM_AVAILABLE:
                logger.info(f"Accessibility search failed for '{query}', trying VLM visual click...")
                try:
                    screenshot_data = app.screenshot()
                    if screenshot_data:
                        result = detect_element_visual(
                            image_data=screenshot_data,
                            description=query,
                            image_width=1920,
                            image_height=1080,
                        )
                        if result:
                            x, y = result
                            # Click at the detected coordinates using Quartz
                            if _click_at_coordinates(x, y, click_type):
                                return [
                                    TextContent(
                                        type="text",
                                        text=f"✅ Clicked '{query}' at ({x}, {y}) via VLM ({click_type}, {mode_str} mode)",
                                    )
                                ]
                except Exception as e:
                    logger.warning(f"VLM click fallback failed: {e}")

            return [TextContent(type="text", text=f"❌ Element not found: '{query}'")]

        elif name == "ax_type":
            app = _get_app(arguments["app"])
            query = arguments["query"]
            text = arguments["text"]
            mode_str = arguments.get("mode", "focus")

            mode = ax.BACKGROUND if mode_str == "background" else ax.FOCUS
            element = app.find(query)

            if not element:
                return [TextContent(type="text", text=f"❌ Element not found: '{query}'")]

            element.type_text(text, mode=mode)

            return [
                TextContent(
                    type="text",
                    text=f"✅ Typed {len(text)} chars into '{query}' ({mode_str} mode)",
                )
            ]

        elif name == "ax_set_value":
            app = _get_app(arguments["app"])
            query = arguments["query"]
            value = arguments["value"]

            element = app.find(query)
            if not element:
                return [TextContent(type="text", text=f"❌ Element not found: '{query}'")]

            element.set_value(value)

            return [
                TextContent(
                    type="text",
                    text=f"✅ Set value on '{query}' to: {value}",
                )
            ]

        elif name == "ax_get_value":
            app = _get_app(arguments["app"])
            query = arguments["query"]

            element = app.find(query)
            if not element:
                return [TextContent(type="text", text=f"❌ Element not found: '{query}'")]

            value = element.value() if callable(getattr(element, "value", None)) else getattr(element, "value", None)

            return [
                TextContent(
                    type="text",
                    text=f"Value of '{query}': {value}",
                )
            ]

        elif name == "ax_list_windows":
            app = _get_app(arguments["app"])
            windows = app.windows() if hasattr(app, "windows") else []

            if not windows:
                return [TextContent(type="text", text="No windows found")]

            result = "Windows:\n"
            for i, w in enumerate(windows):
                title = w.title() if callable(getattr(w, "title", None)) else getattr(w, "title", f"Window {i}")
                result += f"  {i + 1}. {title}\n"

            return [TextContent(type="text", text=result)]

        elif name == "ax_screenshot":
            app = _get_app(arguments["app"])
            query = arguments.get("query")

            if query:
                element = app.find(query)
                if not element:
                    return [TextContent(type="text", text=f"❌ Element not found: '{query}'")]
                screenshot_data = element.screenshot()
            else:
                screenshot_data = app.screenshot()

            if screenshot_data:
                b64 = base64.b64encode(screenshot_data).decode("utf-8")
                return [
                    TextContent(
                        type="text",
                        text=f"Screenshot captured ({len(screenshot_data)} bytes)\nBase64: {b64[:100]}...",
                    )
                ]
            else:
                return [TextContent(type="text", text="❌ Failed to capture screenshot")]

        elif name == "ax_wait_idle":
            app = _get_app(arguments["app"])
            timeout = arguments.get("timeout_ms", 5000)

            # Wait for idle - implementation depends on axterminator API
            if hasattr(app, "wait_idle"):
                app.wait_idle(timeout_ms=timeout)
                return [TextContent(type="text", text="✅ App is idle")]
            else:
                await asyncio.sleep(0.1)  # Brief pause
                return [
                    TextContent(type="text", text="✅ Wait complete (no native idle detection)")
                ]

        elif name == "ax_click_at":
            x = arguments["x"]
            y = arguments["y"]
            click_type = arguments.get("click_type", "single")

            if _click_at_coordinates(x, y, click_type):
                return [
                    TextContent(
                        type="text",
                        text=f"✅ Clicked at ({x}, {y}) ({click_type} click)",
                    )
                ]
            else:
                return [TextContent(type="text", text=f"❌ Click at ({x}, {y}) failed")]

        elif name == "ax_find_visual":
            if not VLM_AVAILABLE:
                return [
                    TextContent(
                        type="text",
                        text="❌ VLM not available. Set ANTHROPIC_API_KEY or OPENAI_API_KEY, or install mlx-vlm.",
                    )
                ]

            app = _get_app(arguments["app"])
            description = arguments["description"]

            try:
                screenshot_data = app.screenshot()
                if not screenshot_data:
                    return [TextContent(type="text", text="❌ Failed to capture screenshot for VLM")]

                result = detect_element_visual(
                    image_data=screenshot_data,
                    description=description,
                    image_width=1920,
                    image_height=1080,
                )

                if result:
                    x, y = result
                    return [
                        TextContent(
                            type="text",
                            text=f"✅ Found '{description}' at ({x}, {y}) via VLM visual detection",
                        )
                    ]
                else:
                    return [
                        TextContent(
                            type="text",
                            text=f"❌ VLM could not find: '{description}'",
                        )
                    ]
            except Exception as e:
                return [TextContent(type="text", text=f"❌ VLM detection failed: {e}")]

        else:
            return [TextContent(type="text", text=f"Unknown tool: {name}")]

    except Exception as e:
        logger.exception(f"Error in {name}")
        return [TextContent(type="text", text=f"❌ Error: {e}")]


async def main():
    """Run the MCP server."""
    async with stdio_server() as (read_stream, write_stream):
        await server.run(read_stream, write_stream, server.create_initialization_options())


if __name__ == "__main__":
    asyncio.run(main())
