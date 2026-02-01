#!/usr/bin/env python3
"""Command-line interface for axterminator.

Usage:
    axterminator list-apps              # List running applications
    axterminator tree <app>             # Show accessibility tree
    axterminator find <app> <query>     # Find element
    axterminator click <app> <query>    # Click element
    axterminator type <app> <query> <text>  # Type into element
    axterminator record <app>           # Record interactions
"""

from __future__ import annotations

import argparse
import json
import sys
from typing import Optional

import axterminator


def cmd_list_apps(args: argparse.Namespace) -> int:
    """List running applications."""
    import subprocess

    result = subprocess.run(
        ["osascript", "-e", 'tell application "System Events" to get name of every process whose background only is false'],
        capture_output=True,
        text=True,
    )

    if result.returncode != 0:
        print(f"Error: {result.stderr}", file=sys.stderr)
        return 1

    apps = result.stdout.strip().split(", ")
    for app in sorted(apps):
        print(app)

    return 0


def cmd_tree(args: argparse.Namespace) -> int:
    """Show accessibility tree for an application."""
    try:
        app = axterminator.app(name=args.app)
        # For now, just show we connected
        print(f"Connected to {args.app} (PID: {app.pid})")
        print("Tree visualization not yet implemented in core")
        return 0
    except RuntimeError as e:
        print(f"Error: {e}", file=sys.stderr)
        return 1


def cmd_find(args: argparse.Namespace) -> int:
    """Find an element."""
    try:
        app = axterminator.app(name=args.app)
        element = app.find(args.query, timeout_ms=args.timeout)
        print(f"Found: {element.role} - {element.title or element.value or '(no title)'}")
        return 0
    except RuntimeError as e:
        print(f"Not found: {e}", file=sys.stderr)
        return 1


def cmd_click(args: argparse.Namespace) -> int:
    """Click an element."""
    try:
        app = axterminator.app(name=args.app)
        element = app.find(args.query, timeout_ms=args.timeout)
        mode = axterminator.FOCUS if args.foreground else axterminator.BACKGROUND
        element.click(mode=mode)
        print(f"Clicked: {element.title or element.role}")
        return 0
    except RuntimeError as e:
        print(f"Error: {e}", file=sys.stderr)
        return 1


def cmd_type(args: argparse.Namespace) -> int:
    """Type text into an element."""
    try:
        app = axterminator.app(name=args.app)
        element = app.find(args.query, timeout_ms=args.timeout)
        element.type_text(args.text)
        print(f"Typed '{args.text}' into {element.role}")
        return 0
    except RuntimeError as e:
        print(f"Error: {e}", file=sys.stderr)
        return 1


def cmd_record(args: argparse.Namespace) -> int:
    """Record interactions with an application."""
    try:
        from axterminator.recorder import Recorder

        app = axterminator.app(name=args.app)
        recorder = Recorder(app)

        print(f"Recording interactions with {args.app}...")
        print("Press Ctrl+C to stop and generate test code.")
        print()

        try:
            recorder.start()
            # Keep running until interrupted
            import time
            while True:
                time.sleep(0.1)
        except KeyboardInterrupt:
            print("\n\nStopping recording...")
            recorder.stop()

            # Generate test code
            code = recorder.generate_test()
            print("\n# Generated test code:")
            print(code)

            if args.output:
                with open(args.output, "w") as f:
                    f.write(code)
                print(f"\nSaved to {args.output}")

        return 0
    except RuntimeError as e:
        print(f"Error: {e}", file=sys.stderr)
        return 1


def cmd_check(args: argparse.Namespace) -> int:
    """Check accessibility permissions."""
    if axterminator.is_accessibility_enabled():
        print("Accessibility: ENABLED")
        return 0
    else:
        print("Accessibility: DISABLED")
        print("Go to System Preferences > Privacy & Security > Accessibility")
        print("and add your terminal/IDE to the allowed list.")
        return 1


def main(argv: Optional[list[str]] = None) -> int:
    """Main CLI entry point."""
    parser = argparse.ArgumentParser(
        prog="axterminator",
        description="macOS GUI testing from the command line",
    )
    parser.add_argument(
        "--version",
        action="version",
        version=f"axterminator {axterminator.__version__}",
    )

    subparsers = parser.add_subparsers(dest="command", help="Commands")

    # list-apps
    subparsers.add_parser("list-apps", help="List running applications")

    # check
    subparsers.add_parser("check", help="Check accessibility permissions")

    # tree
    tree_parser = subparsers.add_parser("tree", help="Show accessibility tree")
    tree_parser.add_argument("app", help="Application name")

    # find
    find_parser = subparsers.add_parser("find", help="Find an element")
    find_parser.add_argument("app", help="Application name")
    find_parser.add_argument("query", help="Element query")
    find_parser.add_argument("--timeout", type=int, default=5000, help="Timeout in ms")

    # click
    click_parser = subparsers.add_parser("click", help="Click an element")
    click_parser.add_argument("app", help="Application name")
    click_parser.add_argument("query", help="Element query")
    click_parser.add_argument("--timeout", type=int, default=5000, help="Timeout in ms")
    click_parser.add_argument("--foreground", action="store_true", help="Bring app to foreground")

    # type
    type_parser = subparsers.add_parser("type", help="Type text into element")
    type_parser.add_argument("app", help="Application name")
    type_parser.add_argument("query", help="Element query")
    type_parser.add_argument("text", help="Text to type")
    type_parser.add_argument("--timeout", type=int, default=5000, help="Timeout in ms")

    # record
    record_parser = subparsers.add_parser("record", help="Record interactions")
    record_parser.add_argument("app", help="Application name")
    record_parser.add_argument("-o", "--output", help="Output file for generated test")

    args = parser.parse_args(argv)

    if not args.command:
        parser.print_help()
        return 0

    commands = {
        "list-apps": cmd_list_apps,
        "check": cmd_check,
        "tree": cmd_tree,
        "find": cmd_find,
        "click": cmd_click,
        "type": cmd_type,
        "record": cmd_record,
    }

    return commands[args.command](args)


if __name__ == "__main__":
    sys.exit(main())
