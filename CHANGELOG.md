# Changelog

All notable changes to AXTerminator will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.2] - 2026-03-18

### Added
- Python type stubs (`.pyi`) for full IDE support — autocomplete, type checking, and inline documentation for all Rust-backed classes (`AXApp`, `AXElement`, `HealingConfig`, `ActionMode`) and module-level functions
- Dead code annotations (`#[allow(dead_code)]`) for planned XPath healing strategy infrastructure in `healing_match.rs`, `router.rs`, `sync.rs`, and `accessibility.rs`

### Fixed
- **Clippy cleanup**: 10 fixes across the codebase — resolved all warnings for clean `cargo clippy` output
- 6 documentation link fixes across module docs (broken intra-doc links, stale references)

### Changed
- Module-level documentation improvements — all public modules now have `//!` doc headers with examples
- Docker browser test targets via Neko containers for CI (#18)
- Electron app skill profiles — 5 built-in profiles + registry (#15)
- Proactive desktop copilot with rule-based context suggestions (#11)
- Cross-app workflow intelligence with pattern detection (#10)
- Workflow recording and blackbox desktop testing (#13, #14)
- Query-based scene understanding with encode-once keystone (#4)
- Semantic element find and Electron DevTools Protocol (#8, #9)
- Triple understanding and durable automation steps (#12, #16)
- Persistent element references with fingerprint-based WeakRef tracking (#7)

## [0.3.1] - 2026-02-01

### Added
- Real-world examples (Safari, Finder, TextEdit workflows)
- Chrome recorder browser extension

## [0.3.0] - 2026-01-19

### Added
- CLI tool (`axterminator inspect`, `axterminator tree`, `axterminator record`)
- Action recording and playback
- pytest plugin (`pytest-axterminator`)
- Ollama local VLM backend
- MCP server for Claude Code integration

## [0.2.1] - 2026-01-18

### Added
- PyPI publishing workflow
- XPC sync module
- API documentation

## [0.2.0] - 2026-01-17

### Added
- Gemini VLM backend
- VLM examples
- MLX/Claude/GPT-4V visual element detection

## [0.1.0] - 2026-01-10

### Added
- Initial release
- Background GUI testing (no focus stealing)
- 7-strategy self-healing locators
- Python bindings via PyO3
- macOS Accessibility API integration
- ~379 us element access (measured on M1 MacBook Pro)
