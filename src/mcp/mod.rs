//! MCP server implementation for `axterminator`.
//!
//! ## Phase 1 (tools)
//!
//! - `initialize` / `initialized` handshake
//! - `tools/list` — 12 Phase 1 tools with annotations and output schemas
//! - `tools/call` — dispatches to Rust core functions
//! - `ping`
//!
//! ## Phase 2 (resources + prompts)
//!
//! - `resources/list` — 2 static resources
//! - `resources/templates/list` — 3 URI templates
//! - `resources/read` — reads system status, running apps, app tree,
//!   app screenshot, and app UI state
//! - `prompts/list` — 4 guided workflow prompts
//! - `prompts/get` — resolves a prompt with caller-supplied arguments
//!
//! ## Phase 3 (extended tools + observability)
//!
//! - `tools/list` — 7 additional tools (`ax_scroll`, `ax_key_press`,
//!   `ax_get_attributes`, `ax_get_tree`, `ax_list_apps`, `ax_drag`, `ax_assert`)
//! - `notifications/message` — structured per-tool-call log notifications
//! - `notifications/progress` — incremental progress for long-running tools
//!
//! ## Phase 4 (HTTP transport + auth + elicitation)
//!
//! - [`transport`] — stdio and Streamable HTTP/SSE transports
//! - [`auth`] — bearer token and localhost-only authentication for HTTP
//! - [`elicitation`] — server-initiated user questions (4 key scenarios)
//!
//! Entry points: [`server::run_stdio`] (stdio), [`transport::serve`] (any).

pub mod annotations;
pub mod auth;
pub mod elicitation;
pub mod logging;
pub mod progress;
pub mod prompts;
pub mod protocol;
pub mod resources;
pub(crate) mod resources_read;
pub mod server;
pub(super) mod server_handlers;
pub mod tools;
pub mod tools_audio;
pub(crate) mod tools_handlers;
pub mod tools_camera;
pub mod tools_extended;
pub mod tools_gui;
pub(crate) mod tools_gui_events;
pub mod tools_spaces;
pub mod transport;
