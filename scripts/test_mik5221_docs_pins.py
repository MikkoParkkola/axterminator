#!/usr/bin/env python3
"""MIK-5221: docs pip pins, dependabot pip coverage, openssl floor."""

from __future__ import annotations

import re
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEPENDABOT = ROOT / ".github" / "dependabot.yml"
DOCS_YML = ROOT / ".github" / "workflows" / "docs.yml"
REQUIREMENTS = ROOT / "docs" / "requirements.txt"
CARGO_LOCK = ROOT / "Cargo.lock"

FLOORS = {
    "pillow": "12.2.0",
    "urllib3": "2.7.0",
    "python-multipart": "0.0.27",
    "PyJWT": "2.13.0",
    "pyasn1": "0.6.3",
    "cryptography": "46.0.5",
    "protobuf": "5.29.6",
    "GitPython": "3.1.59",
    "idna": "3.15",
}

ENTRYPOINTS = ("mkdocs-material", "mkdocstrings", "pymdown-extensions")


def _ecosystems(text: str) -> set[str]:
    return set(re.findall(r"package-ecosystem:\s*[\"']?([A-Za-z0-9-]+)", text))


def _parse_req(text: str) -> dict[str, str]:
    out: dict[str, str] = {}
    for raw in text.splitlines():
        line = raw.split("#", 1)[0].strip()
        if not line:
            continue
        m = re.match(r"([A-Za-z0-9_.-]+(?:\[[^\]]+\])?)\s*>=\s*([0-9][0-9A-Za-z._-]*)", line)
        if m:
            name = m.group(1).split("[", 1)[0]
            out[name] = m.group(2)
    return out


def _ver_tuple(v: str) -> tuple[int, ...]:
    parts = []
    for p in v.split("."):
        m = re.match(r"(\d+)", p)
        parts.append(int(m.group(1)) if m else 0)
    return tuple(parts)


class TestMik5221DocsPins(unittest.TestCase):
    def test_dependabot_covers_pip_and_cargo(self) -> None:
        """MIK-5221.AC.1"""
        text = DEPENDABOT.read_text(encoding="utf-8")
        ecos = _ecosystems(text)
        self.assertIn("pip", ecos)
        self.assertIn("cargo", ecos)
        self.assertRegex(text, r'directory:\s*["\']?/docs["\']?')

    def test_docs_requirements_pin_floors_and_workflow_installs_them(self) -> None:
        """MIK-5221.AC.2"""
        self.assertTrue(REQUIREMENTS.is_file(), "docs/requirements.txt must exist")
        req = _parse_req(REQUIREMENTS.read_text(encoding="utf-8"))
        for name, floor in FLOORS.items():
            self.assertIn(name, req, f"{name} missing from docs/requirements.txt")
            self.assertGreaterEqual(
                _ver_tuple(req[name]),
                _ver_tuple(floor),
                f"{name} {req[name]} is below {floor}",
            )
        text = REQUIREMENTS.read_text(encoding="utf-8")
        for ep in ENTRYPOINTS:
            self.assertIn(ep, text)
        docs = DOCS_YML.read_text(encoding="utf-8")
        self.assertIn("docs/requirements.txt", docs)
        self.assertNotRegex(
            docs,
            r"pip install mkdocs-material mkdocstrings\[python\] pymdown-extensions\s*$",
            "docs.yml must not install the docs toolchain unpinned",
        )

    def test_openssl_lock_stays_at_or_above_0_10_79(self) -> None:
        """MIK-5221.AC.3"""
        lock = CARGO_LOCK.read_text(encoding="utf-8")
        self.assertRegex(
            lock,
            r'name = "openssl"\nversion = "0\.10\.(79|[89][0-9]|[1-9][0-9]{2})"',
        )


if __name__ == "__main__":
    sys.exit(0 if unittest.main(verbosity=2) else 1)
