//! MCP transport layer — stdio and HTTP/SSE.
//!
//! This module provides two transports:
//!
//! - **stdio** (always available): reads JSON-RPC from stdin, writes to stdout.
//!   This is the primary transport used by Claude Code, Claude Desktop, and all
//!   standard MCP clients. Delegates to [`server::run_stdio`].
//!
//! - **HTTP/SSE** (behind `http-transport` feature): Streamable HTTP transport.
//!   Two endpoints:
//!   - `POST /mcp` — receives a JSON-RPC request, returns a JSON-RPC response.
//!   - `GET  /mcp` — opens a Server-Sent Events (SSE) stream for server-initiated
//!     notifications (log messages, progress, resource updates).
//!
//! # Authentication
//!
//! The HTTP transport always enforces authentication via [`BearerValidator`].
//! See [`auth`][crate::mcp::auth] for the full security model.
//!
//! # Usage
//!
//! ```no_run
//! use axterminator::mcp::transport::{TransportConfig, serve};
//!
//! # #[tokio::main]
//! # async fn main() -> anyhow::Result<()> {
//! // Start stdio transport:
//! serve(TransportConfig::Stdio).await?;
//!
//! # Ok(())
//! # }
//! ```
//!
//! With the `http-transport` feature:
//!
//! ```no_run
//! # #[cfg(feature = "http-transport")]
//! # {
//! use std::net::IpAddr;
//! use axterminator::mcp::auth::AuthConfig;
//! use axterminator::mcp::transport::{HttpConfig, TransportConfig, serve};
//!
//! # #[tokio::main]
//! # async fn main() -> anyhow::Result<()> {
//! let config = HttpConfig {
//!     port: 8741,
//!     bind: "127.0.0.1".parse().unwrap(),
//!     auth: AuthConfig::localhost_only(),
//! };
//! serve(TransportConfig::Http(config)).await?;
//! # Ok(())
//! # }
//! # }
//! ```

// ---------------------------------------------------------------------------
// Transport configuration
// ---------------------------------------------------------------------------

/// Configuration for the chosen MCP transport.
///
/// Construct with one of the factory methods and pass to [`serve`].
#[derive(Debug, Clone)]
pub enum TransportConfig {
    /// Newline-delimited JSON-RPC on stdin/stdout.
    Stdio,
    /// Streamable HTTP + SSE transport (requires `http-transport` feature).
    #[cfg(feature = "http-transport")]
    Http(HttpConfig),
}

/// Configuration for the HTTP/SSE transport.
///
/// Requires the `http-transport` feature.
#[cfg(feature = "http-transport")]
#[derive(Debug, Clone)]
pub struct HttpConfig {
    /// TCP port to listen on.
    pub port: u16,
    /// IP address to bind to. Defaults to `127.0.0.1`.
    pub bind: std::net::IpAddr,
    /// Authentication policy for every HTTP request.
    pub auth: crate::mcp::auth::AuthConfig,
}

#[cfg(feature = "http-transport")]
impl HttpConfig {
    /// Create a localhost-only configuration on the given port.
    ///
    /// Binds to `127.0.0.1` and skips token authentication.
    #[must_use]
    pub fn localhost(port: u16) -> Self {
        Self {
            port,
            bind: std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            auth: crate::mcp::auth::AuthConfig::localhost_only(),
        }
    }

    /// Create a bearer-token authenticated configuration.
    ///
    /// Suitable for non-localhost binds once a token has been generated.
    #[must_use]
    pub fn with_bearer(port: u16, bind: std::net::IpAddr, token: String) -> Self {
        Self {
            port,
            bind,
            auth: crate::mcp::auth::AuthConfig::bearer(token),
        }
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Start the MCP server with the given transport configuration.
///
/// - `TransportConfig::Stdio` delegates to [`crate::mcp::server::run_stdio`].
/// - `TransportConfig::Http(cfg)` starts an axum HTTP server (requires the
///   `http-transport` feature).
///
/// Blocks until the transport closes (stdin EOF for stdio; Ctrl-C or error
/// for HTTP).
///
/// # Errors
///
/// Returns an error if the transport fails to start or encounters an
/// unrecoverable I/O error.
pub async fn serve(config: TransportConfig) -> anyhow::Result<()> {
    match config {
        TransportConfig::Stdio => serve_stdio(),
        #[cfg(feature = "http-transport")]
        TransportConfig::Http(cfg) => serve_http(cfg).await,
    }
}

/// Run the stdio transport synchronously.
///
/// Wraps [`crate::mcp::server::run_stdio`] so it can be called from the
/// `async` [`serve`] function.
fn serve_stdio() -> anyhow::Result<()> {
    crate::mcp::server::run_stdio()
}

// ---------------------------------------------------------------------------
// HTTP transport (http-transport feature)
// ---------------------------------------------------------------------------

#[cfg(feature = "http-transport")]
mod http {
    use std::convert::Infallible;
    use std::net::SocketAddr;
    use std::sync::Arc;
    use std::time::Duration;

    use axum::extract::{ConnectInfo, State};
    use axum::http::{HeaderMap, StatusCode};
    // axum 0.8: `NoContent` is the idiomatic zero-allocation 204 response type.
    use axum::response::{IntoResponse, NoContent, Response, Sse};
    // Only `post` is imported — GET is registered via `MethodRouter::get`
    // (the method, not the routing function), so `axum::routing::get` is unused.
    use axum::routing::post;
    use axum::{Json, Router};
    use serde_json::Value;
    use tokio::sync::broadcast;
    use tokio_stream::wrappers::BroadcastStream;
    use tokio_stream::StreamExt as _;
    use tracing::{debug, error, info, warn};

    use crate::mcp::auth::{AuthError, BearerValidator};
    use crate::mcp::protocol::{JsonRpcRequest, JsonRpcResponse, RequestId, RpcError};

    /// Maximum SSE clients per server instance.
    const SSE_CHANNEL_CAPACITY: usize = 64;

    /// Idle SSE keepalive interval.
    const SSE_KEEPALIVE: Duration = Duration::from_secs(15);

    /// Shared state injected into every request handler.
    #[derive(Clone)]
    pub(super) struct AppState {
        validator: BearerValidator,
        /// Broadcast channel for server-initiated notifications.
        sse_tx: broadcast::Sender<SseEvent>,
    }

    /// A single SSE event sent to connected clients.
    #[derive(Debug, Clone)]
    pub(super) struct SseEvent {
        /// `event:` field — e.g. `"notification"`.
        pub event: String,
        /// `data:` field — JSON string.
        pub data: String,
    }

    impl AppState {
        pub fn new(validator: BearerValidator) -> Self {
            let (sse_tx, _) = broadcast::channel(SSE_CHANNEL_CAPACITY);
            Self { validator, sse_tx }
        }
    }

    // -----------------------------------------------------------------------
    // Auth middleware helper
    // -----------------------------------------------------------------------

    /// Extract and validate the `Authorization` header and source IP.
    ///
    /// Returns `Ok(())` or an HTTP `401 Unauthorized` response.
    ///
    /// # Errors
    ///
    /// Returns a `Response` with status 401 when the source IP or bearer token
    /// fails validation.
    // `Response` is an axum type we don't control; boxing it adds indirection
    // with no benefit here since check_auth is only called from handlers.
    #[allow(clippy::result_large_err)]
    fn check_auth(
        headers: &HeaderMap,
        peer: SocketAddr,
        validator: &BearerValidator,
    ) -> Result<(), Response> {
        // Source-IP check (localhost-only mode).
        if let Err(e) = validator.validate_source_ip(peer.ip()) {
            warn!(%peer, "rejected non-localhost request: {e}");
            return Err(unauthorized("Non-localhost request rejected"));
        }

        // Bearer token check.
        let raw = headers.get("Authorization").and_then(|v| v.to_str().ok());
        if let Err(e) = validator.validate_header(raw) {
            let msg = match e {
                AuthError::MissingHeader => "Authorization header required",
                AuthError::UnsupportedScheme => "Unsupported authorization scheme",
                AuthError::InvalidToken => "Invalid bearer token",
                _ => "Authorization failed",
            };
            warn!(%peer, "auth failure: {e}");
            return Err(unauthorized(msg));
        }

        Ok(())
    }

    fn unauthorized(msg: &'static str) -> Response {
        (
            StatusCode::UNAUTHORIZED,
            [("WWW-Authenticate", "Bearer")],
            msg,
        )
            .into_response()
    }

    // -----------------------------------------------------------------------
    // POST /mcp — JSON-RPC handler
    // -----------------------------------------------------------------------

    /// Handle a single JSON-RPC request over HTTP.
    ///
    /// Reads the JSON body, dispatches to the MCP server, and returns the
    /// JSON-RPC response. Authentication is checked before dispatch.
    pub(super) async fn post_mcp(
        ConnectInfo(peer): ConnectInfo<SocketAddr>,
        State(state): State<Arc<AppState>>,
        headers: HeaderMap,
        Json(body): Json<Value>,
    ) -> Response {
        if let Err(resp) = check_auth(&headers, peer, &state.validator) {
            return resp;
        }

        debug!(%peer, "POST /mcp");

        let rpc_req: JsonRpcRequest = match serde_json::from_value(body) {
            Ok(r) => r,
            Err(e) => {
                let resp = JsonRpcResponse::err(
                    RequestId::Number(0),
                    RpcError::new(RpcError::PARSE_ERROR, format!("Parse error: {e}")),
                );
                return Json(serde_json::to_value(&resp).unwrap_or(Value::Null)).into_response();
            }
        };

        let mut sink = Vec::<u8>::new();
        let maybe_resp = {
            // Server state is not shared across HTTP requests in Phase 4 —
            // each request gets its own ephemeral server instance. Stateful
            // sessions (connected apps persisting across requests) are a
            // Phase 5 feature. This keeps Phase 4 correct and simple.
            let mut server = crate::mcp::server::ServerHandle::new();
            server.handle(&rpc_req, &mut sink)
        };

        // Forward any SSE notifications that were written to the sink.
        if !sink.is_empty() {
            if let Ok(notifications) = String::from_utf8(sink) {
                for line in notifications.lines() {
                    if !line.is_empty() {
                        let _ = state.sse_tx.send(SseEvent {
                            event: "notification".into(),
                            data: line.to_owned(),
                        });
                    }
                }
            }
        }

        match maybe_resp {
            Some(resp) => match serde_json::to_value(&resp) {
                Ok(v) => Json(v).into_response(),
                Err(e) => {
                    error!("response serialization failed: {e}");
                    StatusCode::INTERNAL_SERVER_ERROR.into_response()
                }
            },
            // Notification — no response body.
            // axum 0.8: `NoContent` is the idiomatic zero-allocation 204 type.
            None => NoContent.into_response(),
        }
    }

    // -----------------------------------------------------------------------
    // GET /mcp — SSE stream
    // -----------------------------------------------------------------------

    /// Open an SSE stream for server-initiated notifications.
    ///
    /// Clients subscribe once and receive `notifications/message`,
    /// `notifications/progress`, and `notifications/resources/updated` events
    /// as they are broadcast.
    pub(super) async fn get_mcp_sse(
        ConnectInfo(peer): ConnectInfo<SocketAddr>,
        State(state): State<Arc<AppState>>,
        headers: HeaderMap,
    ) -> Response {
        if let Err(resp) = check_auth(&headers, peer, &state.validator) {
            return resp;
        }

        info!(%peer, "SSE client connected");

        let rx = state.sse_tx.subscribe();
        let stream = BroadcastStream::new(rx).filter_map(|result| {
            result.ok().map(|ev| {
                Ok::<_, Infallible>(
                    axum::response::sse::Event::default()
                        .event(ev.event)
                        .data(ev.data),
                )
            })
        });

        Sse::new(stream)
            .keep_alive(
                axum::response::sse::KeepAlive::new()
                    .interval(SSE_KEEPALIVE)
                    .text("keep-alive"),
            )
            .into_response()
    }

    // -----------------------------------------------------------------------
    // 405 fallback
    // -----------------------------------------------------------------------

    /// Return 405 Method Not Allowed with an explicit `Allow` header.
    ///
    /// Attached as the `MethodRouter::fallback` on the `/mcp` route so that
    /// clients using DELETE, PUT, PATCH, etc. receive a precise 405 instead of
    /// the default bare 405 with no body.
    ///
    /// axum 0.8 routes GET+POST on a single `MethodRouter`; when another HTTP
    /// method is used the framework calls this fallback and automatically
    /// appends `Allow: GET, POST` to the response via the tower service layer.
    async fn method_not_allowed() -> Response {
        (
            StatusCode::METHOD_NOT_ALLOWED,
            [("Allow", "GET, POST")],
            "/mcp only accepts GET (SSE stream) and POST (JSON-RPC)",
        )
            .into_response()
    }

    // -----------------------------------------------------------------------
    // Server startup
    // -----------------------------------------------------------------------

    /// Start the HTTP/SSE MCP server.
    ///
    /// Binds to `cfg.bind:cfg.port`, serves `POST /mcp` and `GET /mcp`, and
    /// blocks until the process receives SIGINT (Ctrl-C).
    pub(super) async fn start(cfg: super::HttpConfig) -> anyhow::Result<()> {
        use anyhow::Context as _;

        let validator = BearerValidator::new(cfg.auth.clone());

        // Safety check before binding — refuse unsafe configs.
        validator
            .check_bind_safety(cfg.bind)
            .context("unsafe server configuration")?;

        // Print startup banner.
        print_startup_banner(&cfg, &cfg.auth);

        let state = Arc::new(AppState::new(validator));
        let addr = SocketAddr::new(cfg.bind, cfg.port);

        // axum 0.8: merge GET+POST onto one MethodRouter so the framework
        // emits 405 (not 404) for any other method on /mcp.  The custom
        // `fallback` enriches the 405 with a human-readable body.
        let mcp_route = post(post_mcp)
            .get(get_mcp_sse)
            .fallback(method_not_allowed);

        let app = Router::new()
            .route("/mcp", mcp_route)
            .with_state(state)
            .into_make_service_with_connect_info::<SocketAddr>();

        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .with_context(|| format!("Failed to bind to {addr}"))?;

        info!(%addr, "MCP HTTP server listening");

        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal())
            .await
            .context("HTTP server error")?;

        info!("MCP HTTP server stopped");
        Ok(())
    }

    fn print_startup_banner(cfg: &super::HttpConfig, auth: &crate::mcp::auth::AuthConfig) {
        eprintln!("MCP HTTP server starting");
        eprintln!("  Address : http://{}:{}/mcp", cfg.bind, cfg.port);
        match auth {
            crate::mcp::auth::AuthConfig::LocalhostOnly => {
                eprintln!("  Auth    : localhost-only (no token required)");
            }
            crate::mcp::auth::AuthConfig::Bearer(token) => {
                eprintln!("  Auth    : Bearer token");
                eprintln!("  Token   : {token}");
                eprintln!();
                eprintln!("  Add this to your MCP client config:");
                eprintln!("    Authorization: Bearer {token}");
            }
        }
        eprintln!();
    }

    async fn shutdown_signal() {
        let _ = tokio::signal::ctrl_c().await;
        info!("shutdown signal received");
    }
}

// ---------------------------------------------------------------------------
// serve_http — re-export
// ---------------------------------------------------------------------------

#[cfg(feature = "http-transport")]
async fn serve_http(cfg: HttpConfig) -> anyhow::Result<()> {
    http::start(cfg).await
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_config_stdio_variant_exists() {
        // GIVEN / WHEN / THEN: TransportConfig::Stdio is constructible
        let _cfg = TransportConfig::Stdio;
    }

    #[cfg(feature = "http-transport")]
    mod http_tests {
        use super::super::*;
        use std::net::{IpAddr, Ipv4Addr};

        #[test]
        fn http_config_localhost_binds_to_loopback() {
            // GIVEN: localhost config
            let cfg = HttpConfig::localhost(8741);
            // THEN: bind address is loopback
            assert!(cfg.bind.is_loopback());
            assert_eq!(cfg.port, 8741);
            assert!(cfg.auth.is_localhost_only());
        }

        #[test]
        fn http_config_with_bearer_stores_token() {
            // GIVEN: bearer config
            let cfg =
                HttpConfig::with_bearer(9000, IpAddr::V4(Ipv4Addr::LOCALHOST), "axt_tok".into());
            // THEN: auth mode is bearer
            assert!(cfg.auth.is_bearer());
            assert_eq!(cfg.port, 9000);
        }

        #[tokio::test]
        async fn serve_refuses_unsafe_config() {
            // GIVEN: bind 0.0.0.0 without a token (localhost-only)
            use crate::mcp::auth::AuthConfig;
            let cfg = HttpConfig {
                port: 19999,
                bind: IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
                auth: AuthConfig::localhost_only(),
            };
            // WHEN: serve_http called
            let result = serve_http(cfg).await;
            // THEN: error because unsafe
            assert!(result.is_err());
            let msg = result.unwrap_err().to_string();
            assert!(
                msg.contains("unsafe") || msg.contains("configuration"),
                "{msg}"
            );
        }
    }
}
