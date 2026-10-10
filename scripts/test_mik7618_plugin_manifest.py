#!/usr/bin/env python3
"""MIK-7618: Claude plugin manifest matches the published release."""

from __future__ import annotations

import json
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CARGO = ROOT / "Cargo.toml"
PLUGIN = ROOT / "plugin" / ".claude-plugin" / "plugin.json"
MCP = ROOT / "plugin" / ".mcp.json"
README = ROOT / "plugin" / "README.md"

LICENSE = "PolyForm-Noncommercial-1.0.0"
LAUNCHER = ("npx", ["-y", "axterminator@{version}", "mcp", "serve"])


def cargo_version() -> str:
    """Version of the package table, not a dependency."""
    text = CARGO.read_text(encoding="utf-8")
    package = text.split("[package]", 1)[1].split("[", 1)[0]
    match = re.search(r'(?m)^version\s*=\s*"([^"]+)"', package)
    if match is None:
        raise AssertionError("Cargo.toml [package] has no version")
    return match.group(1)


def words(text: str) -> list[str]:
    return re.findall(r"[A-Za-z0-9]+(?:'[A-Za-z0-9]+)?", text)


class TestMik7618PluginManifest(unittest.TestCase):
    def test_manifest_version_matches_the_release(self) -> None:
        """MIK-7618.AC2: plugin.json, the npm pin, and Cargo.toml are one version."""
        release = cargo_version()
        plugin = json.loads(PLUGIN.read_text(encoding="utf-8"))
        mcp = json.loads(MCP.read_text(encoding="utf-8"))
        server = mcp["mcpServers"]["axterminator"]
        self.assertEqual(server["command"], LAUNCHER[0])
        self.assertEqual(
            server["args"],
            [part.format(version=release) for part in LAUNCHER[1]],
        )
        self.assertEqual(plugin["version"], release)
        self.assertNotIn("@latest", json.dumps(mcp))

    def test_plugin_folder_lists_license_platform_and_permission(self) -> None:
        """MIK-7618.AC1: license, a 40-word README, macOS-only, permission steps."""
        plugin = json.loads(PLUGIN.read_text(encoding="utf-8"))
        self.assertEqual(plugin["license"], LICENSE)
        self.assertEqual(plugin["name"], "axterminator")
        readme = README.read_text(encoding="utf-8")
        self.assertGreaterEqual(len(words(readme)), 40)
        self.assertIn("macOS", readme)
        self.assertIn("Claude Code", readme)
        self.assertIn("local Cowork", readme)
        self.assertIn("Accessibility", readme)
        self.assertIn("System Settings", readme)
        self.assertIn("Privacy & Security", readme)


if __name__ == "__main__":
    unittest.main()
