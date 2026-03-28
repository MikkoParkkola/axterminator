//! MCP server implementation for `axterminator`.
//!
//! ## Phase 1 (tools)
//!
//! - `initialize` / `initialized` handshake
//! - `tools/list` — core accessibility tools with annotations and output schemas
//! - `tools/call` — dispatches to Rust core functions
//! - `ping`
//!
//! ## Phase 2 (resources + prompts)
//!
//! - `resources/list` — static resources for system state, guides, and enabled
//!   optional subsystems
//! - `resources/templates/list` — dynamic app-scoped URI templates
//! - `resources/read` — reads system status, running apps, app tree,
//!   app screenshot, and app UI state
//! - `prompts/list` — 4 guided workflow prompts
//! - `prompts/get` — resolves a prompt with caller-supplied arguments
//!
//! ## Phase 3 (extended tools + observability)
//!
//! - `tools/list` — extended GUI, workflow, analysis, and feature-gated tool
//!   families (`audio`, `camera`, `watch`, `spaces`, `docker`, `context`)
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

pub(crate) mod action_safety;
pub(crate) mod analysis_engine;
pub mod annotations;
pub(crate) mod args;
pub mod auth;
pub(crate) mod catalog;
pub mod elicitation;
pub mod logging;
pub mod observer;
pub mod progress;
pub mod prompts;
pub mod protocol;
pub mod resources;
pub(crate) mod resources_read;
pub mod sampling;
pub mod security;
pub mod server;
pub(super) mod server_handlers;
pub mod tools;
pub mod tools_audio;
pub mod tools_camera;
pub mod tools_capture;
pub mod tools_context;
#[cfg(feature = "docker")]
pub mod tools_docker;
pub(crate) mod tools_error;
pub mod tools_extended;
pub mod tools_gui;
pub(crate) mod tools_gui_events;
pub(crate) mod tools_handlers;
pub mod tools_innovation;
pub(crate) mod tools_response;
pub mod tools_spaces;
#[cfg(feature = "watch")]
pub mod tools_watch;
pub(crate) mod tools_workflow;
pub mod transport;
#[cfg(feature = "watch")]
pub(crate) mod watch_channel;
