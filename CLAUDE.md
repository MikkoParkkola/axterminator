# AXTerminator

MCP server + CLI that gives AI agents the ability to see and control macOS applications via the Accessibility API. 27 core tools, up to 34+ with feature flags. Rust, macOS-only.

> `AGENTS.md` in this repo is the public install-facing guide for end-user agents (see `trvl` pattern). This file (`CLAUDE.md`) is for contributing agents. They are intentionally diverged; do not symlink.

## Product Vision

AXTerminator lets an AI agent observe and operate macOS applications **in the background**, with sub-millisecond element access (~379 us), self-healing locators, and structured-output MCP tools. It is the eyes-and-hands of a macOS agent: click, type, query UI state, capture screens, record audio, see camera input, manage virtual desktops. AX-first with vision-fallback means most interactions use the accessibility tree (fast, reliable, semantic), falling back to screenshot+vision only when AX is not available (e.g. Electron web views without proper ARIA).

The boundary is macOS-only. iOS/iPadOS support is tracked but will be screenshot+vision only — the `idevice`/RPPairing surface does not expose AX trees for third-party apps, and UIAutomation would require an on-device test runner which breaks the "mac-only agent, no on-device prereqs" contract.

## Current Status

- **v0.9.1** · Rust stable · macOS 12+ · last MIT + Apache-2.0 dual-license release
- **27 core tools** (default features) plus up to 34+ with `audio`, `camera`, `spaces`, `watch`, `context`, `docker`, `parakeet`, `http-transport` feature flags
- **~1000 tests** on default features; full suite with `--all-features`; CI runs `test --all-features` on macOS
- **Performance**: 379 us per element access, 7-strategy self-healing locators, ObjC FFI for CoreFoundation / CoreGraphics / AVFoundation
- **iOS**: tracked in #43, demoted to screenshot+vision tier (~350 ms USB, ~700 ms WiFi via CoreDeviceProxy)
- **tvOS**: unverified; RSD surface narrower than `idevice`'s tvOS target suggests

## Plan Forward (near-term, technical)

- **Release cadence**: v0.9.1 unblocked for aarch64-apple-darwin (#50); continue clippy-1.95 drift cleanup
- **iOS screenshot-vision path**: #43 — implement when "mac-only agent, no on-device prereqs" can be preserved
- **Dependabot cadence**: frequent automated PRs; rebase-and-ship once CI is green
- **Companion bundles**: axterminator auto-loads alongside mcp-gateway + nab + hebb in botnaut-client

## Decisions Locked (do not re-litigate)

| Decision | Rationale | Do not |
|---|---|---|
| **macOS-only for AX-first path** | AX API is the entire performance + reliability story | Introduce Linux/Windows AX support that is not a thin shim |
| **AX-first with vision fallback** (not vision-first) | AX is faster, more reliable, semantic; vision is the escape hatch; see [ADR-0001](docs/architecture/decisions/ADR-0001-ax-first-with-vision-fallback.md) | Make vision the default pipeline |
| **Background interaction** (no mouse-takeover required) | Parallel agent work; user keeps using their machine | Require focus-stealing; avoid mouse-driven automation |
| **Accessibility permissions required for terminal/host** | macOS security model; no workaround | Add code paths that work around Privacy & Security panel |
| **No on-device prereqs for iOS path** | Keeps "mac-only agent" contract intact | Require iOS UIAutomation test runner install |
| **Session state via `OnceLock<Mutex>` singleton** (`McpSession`) | Shared state across tool handlers; tests take `session_test_lock()` | Add parallel singletons; avoid per-tool mutable globals |
| **Feature flags for optional modules** (audio / camera / spaces / watch / context / docker / parakeet / http-transport) | Keep default build lean; opt-in heavy deps | Promote feature-gated modules to default without explicit trade-off |
| **No unsafe without `// SAFETY:`** | Discipline for ObjC FFI + CF bridging | Add unsafe blocks without the comment |
| **License: community + commercial for future releases** | Free personal/research/noncommercial OSS/free public-good use with attribution; business use by written commercial license | Revert to permissive licensing or accept meaningful external contributions without commercial relicensing permission |

## Anti-Patterns (things agents get wrong in this repo)

- **Implementing iOS via `idevice` UIAutomation** — breaks "mac-only agent, no on-device prereqs" contract. Screenshot+vision only per #43.
- **Making vision the default path** — AX-first is the performance moat; vision is fallback, not equivalent.
- **Forgetting `destructiveHint` on automation tools** — axterminator literally drives the user's machine. See MIK-2986: err toward `destructiveHint=true` when uncertain. Any tool that types, clicks, modifies system state must carry it.
- **Sharing `McpSession` across tests without the test lock** — tests break intermittently. Acquire `session_test_lock()` from `tools_capture`.
- **Adding ObjC FFI without updating `build.rs`** — camera/audio `.m` files only compile when the matching feature is enabled; the build script gates this.
- **Skipping `live_` test naming convention** — CI skips these explicitly; tests that hit real macOS APIs must use the prefix.

## Guidance for Agents

- **Before editing a tool schema**: check MCP 2025-11-25 annotation matrix in MIK-2986; every tool needs `readOnlyHint` / `destructiveHint` / `idempotentHint` / `openWorldHint` plus `title`.
- **When adding a feature-gated module**: mirror `src/audio/` pattern — feature flag in `Cargo.toml`, `cfg`-gated module, `.m` file compilation in `build.rs`, integration test behind `live_` prefix.
- **Cross-app interaction**: use `src/app.rs` for app connection and `src/element.rs` for element abstraction — do not bypass to raw `accessibility.rs` FFI.
- **Self-healing locators**: the 7-strategy system in `src/healing.rs` / `healing_match.rs` is the primary stability mechanism; do not bypass it with hard-coded selectors.
- **Build + test parity with CI**: `cargo fmt --check && cargo clippy --all-features -- -D warnings -A unexpected_cfgs && cargo test --all-features -- --skip live_`

## Where to Look

| You want to… | Read |
|---|---|
| Onboard a human user / end-user agent | `AGENTS.md` (public install-facing) + `README.md` |
| MCP tool definitions | `src/mcp/tools_*.rs` |
| Accessibility FFI | `src/accessibility.rs` + `src/element.rs` |
| Self-healing locators | `src/healing.rs` + `src/healing_match.rs` |
| Scene graph / UI patterns | `src/intent.rs` + `src/intent_matching.rs` |
| Benchmarks | `benches/` |
| AX-first vs vision-first decision | `docs/architecture/decisions/ADR-0001-ax-first-with-vision-fallback.md` |
| Known limitations | README §Known Limitations + #43 (iOS) + #42 (AX-first rationale) |

## Build & Test

```bash
cargo build --release --features cli          # Binary
cargo build --release --all-features          # All features (audio, camera, spaces, etc.)
cargo test                                     # Default features (~1000 tests)
cargo test --all-features                      # Full suite
cargo fmt --check && cargo clippy --all-features -- -D warnings -A unexpected_cfgs
```

CI runs: `fmt --check`, `clippy --all-features`, `test --all-features`, `check --all-features` on macOS.

CI skips tests that use the `live_` prefix (`cargo test --all-features -- --skip live_`).

## Architecture

- `src/lib.rs` -- crate root, module declarations
- `src/bin/axterminator.rs` -- CLI (clap) + MCP server entry point
- `src/mcp/` -- MCP protocol, server, tools, resources, transport
  - `server.rs` / `server_handlers.rs` -- JSON-RPC dispatch
  - `tools_gui.rs` -- core GUI interaction tools (find, click, type, scroll)
  - `tools_innovation.rs` -- analyze, record, audit, workflow tools
  - `tools_capture.rs` -- screenshot, session capture
  - `tools_audio.rs` / `tools_camera.rs` / `tools_spaces.rs` -- feature-gated
  - `protocol.rs` -- MCP message types, capabilities
  - `annotations.rs` -- tool annotation constants (readOnly, destructive, etc.)
  - `elicitation.rs` -- confirmation prompts for destructive actions
  - `transport.rs` -- stdio + optional HTTP transport
- `src/app.rs` -- macOS app connection, element search
- `src/accessibility.rs` -- AXUIElement FFI wrappers
- `src/element.rs` -- UI element abstraction
- `src/healing.rs` / `healing_match.rs` -- 7-strategy self-healing locators
- `src/intent.rs` / `intent_matching.rs` -- scene graph, UI pattern detection
- `src/copilot.rs` -- AI copilot state tracking
- `src/spaces.rs` -- virtual desktop management (feature-gated)
- `src/audio/` / `src/camera/` / `src/watch/` -- optional modules
- `tests/` -- integration tests
- `benches/` -- Criterion benchmarks

## Feature Flags

`cli` (default), `audio`, `camera`, `spaces`, `http-transport`, `watch`, `context`, `docker`, `parakeet`

## Key Patterns

- All MCP tools are pure functions: `handle_ax_*(params: &Value) -> ToolResult`
- Session state via singleton (`McpSession`) protected by `OnceLock<Mutex>`
- Tests sharing session state must acquire `session_test_lock()` from `tools_capture`
- ObjC FFI for CoreFoundation, CoreGraphics, AVFoundation (camera_objc.m, sck_audio_objc.m)
- `build.rs` compiles .m files when camera feature enabled

## Lint Suppression Notes

- `unexpected_cfgs = "allow"` in Cargo.toml -- objc 0.2 `sel_impl` macro compatibility
- `assertions_on_constants` allowed in annotation tests -- intentional const field validation
- `dead_code` on `session_test_lock` -- used cross-module by test code clippy can't see
