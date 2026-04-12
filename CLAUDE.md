# AXTerminator

MCP server + CLI that gives AI agents the ability to see and control macOS applications via the Accessibility API. 27 core tools, up to 34+ with feature flags. Rust, macOS-only.

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
