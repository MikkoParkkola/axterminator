//! MCP server implementation for `axterminator`.
//!
//! Phase 1: JSON-RPC 2.0 over stdio transport.
//!   - `initialize` / `initialized` handshake
//!   - `tools/list` — all 12 Phase 1 tools with annotations and output schemas
//!   - `tools/call` — dispatches to Rust core functions
//!   - `ping`
//!
//! Entry point: [`server::run_stdio`].

pub mod annotations;
pub mod protocol;
pub mod server;
pub mod tools;
