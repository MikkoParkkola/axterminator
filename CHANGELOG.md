# Changelog

All notable changes to AXTerminator will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.5.0] - 2026-03-19

### Added
- **MCP server rewrite in Rust** — 13 files, 8163 LOC replacing Python server.py
  - 12 core tools + 18 extended tools (30 total)
  - 5 resources (apps, tree, screenshot, state, displays)
  - 4 guided prompts (test-app, navigate-to, extract-data, accessibility-audit)
  - 9 elicitation scenarios for destructive/ambiguous operations
  - Tool annotations (readOnlyHint, destructiveHint, idempotentHint, openWorldHint)
  - Bearer token auth + localhost-only for HTTP transport
  - Structured MCP logging + progress notifications
- **Audio capture** (`audio` feature) — CoreAudio microphone/system capture, SFSpeechRecognizer transcription, NSSpeechSynthesizer TTS
- **Camera + gesture detection** (`camera` feature) — AVFoundation single-frame capture, Vision hand pose detection with 7 gesture types
- **Virtual desktop management** (`spaces` feature) — CGSSpace private API: list/create/move/switch/destroy
- **Multi-monitor support** — negative coordinates, Retina scaling
- Homebrew formula
- Wiki (7 pages)
- SECURITY.md, CODE_OF_CONDUCT.md, issue/PR templates

### Fixed
- Gesture recognition: TIP vs MCP knuckle comparison, confidence filtering (#25)
- Speech recognition: TCC permission request + 30s timeout (#26)
- Speech recognition: pump CFRunLoop for SFSpeechRecognizer callback delivery (#27)
- Speech recognition: capture at native 48kHz sample rate — was writing 48kHz
  data with 16kHz WAV header, causing 3x slowdown and unrecognizable audio (#27)
- Camera permission request when status undetermined
- PyO3 extension-module decoupled from CLI binary via `python-ext` feature flag
- bytes 1.11.0 integer overflow (RUSTSEC-2026-0007)
- Dangerous unwrap() on missing JSON-RPC id, NaN confidence, CString user input
- Removed Pickle virtual audio drivers intercepting default input device

### Security
- pyo3 0.22 → 0.24.2 (RUSTSEC-2025-0020: buffer overflow in PyString)
- lru 0.12 → 0.16.3 (RUSTSEC-2026-0002: IterMut unsoundness)

### Changed
- README rewritten: MCP server first, no superlatives, honest claims only
- Audio module split from 1567 LOC monolith to 5-file module (max 436 LOC)
- MCP tools_extended.rs (2640 LOC) split into 6 focused modules
- 5 additional modules split to meet 800 LOC quality gate
- camera.rs refactored to camera/ module directory
- Issue/PR templates updated for MCP-first workflow
- Gateway config points to release binary
- Published v0.5.0 to crates.io

## [0.4.0] - 2026-03-19

### Added
- **Query-based scene understanding** with encode-once keystone architecture (#4)
- **Two-stage screen intent extraction** pipeline (#5)
- **Copilot-readable state injection** (`useCopilotReadable`) (#6)
- **Persistent element references** with fingerprint-based WeakRef tracking (#7)
- **Semantic element find** and Electron DevTools Protocol integration (#8, #9)
- **Cross-app workflow intelligence** with pattern detection and automation (#10)
- **Proactive desktop copilot** with rule-based context suggestions (#11)
- **Triple understanding** and durable automation steps (#12, #16)
- **Workflow recording** and blackbox desktop testing (#13, #14)
- **Electron app skill profiles** — 5 built-in profiles + extensible registry (#15)
- **Docker browser test targets** via Neko containers for CI (#18)
- MkDocs documentation site with benchmarks
- Comprehensive tool comparison table (vs XCUITest, Appium, PyAutoGUI, Maestro)
- Python type stubs (`.pyi`) for full IDE support
- mediar-ai/terminator acknowledgement in README

### Fixed
- MCP server: VLM fallback and method calls on AXElement
- CI: formatting and skip accessibility-dependent tests
- Clippy: 10 fixes across the codebase for clean `cargo clippy` output
- 6 documentation link fixes (broken intra-doc links, stale references)

### Changed
- Updated OpenAI model references to current models (GPT-4o, GPT-5)
- README rewritten with verified benchmark numbers and factual claims
- Module-level documentation improvements with `//!` doc headers and examples
- Publication cleanup: factual claims, honest benchmarks, no superlatives

### Refactored
- Extracted `copilot_extract.rs` and `healing_match.rs` to fix LOC violations
- Registered new modules in `lib.rs`

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
