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
    use std::sync::{Arc, Mutex};
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
    use tokio_stream::StreamExt as _;
    use tokio_stream::wrappers::BroadcastStream;
    use tracing::{debug, error, info, warn};

    use crate::mcp::auth::{AuthError, BearerValidator};
    use crate::mcp::protocol::{JsonRpcRequest, JsonRpcResponse, RequestId, RpcError};

    /// Maximum SSE clients per server instance.
    const SSE_CHANNEL_CAPACITY: usize = 64;

    /// Idle SSE keepalive interval.
    const SSE_KEEPALIVE: Duration = Duration::from_secs(15);

    /// Shared state injected into every request handler.
    pub(super) struct AppState {
        validator: BearerValidator,
        /// Persistent MCP session state shared by all HTTP requests.
        ///
        /// MCP lifecycle, connected apps, tasks, workflows, and subscriptions
        /// all live inside `ServerHandle`; recreating it per request breaks the
        /// initialize -> initialized -> tools/call sequence used by clients.
        server: Mutex<crate::mcp::server::ServerHandle>,
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
            Self {
                validator,
                server: Mutex::new(crate::mcp::server::ServerHandle::new()),
                sse_tx,
            }
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
        let maybe_resp = match state.server.lock() {
            Ok(mut server) => server.handle(&rpc_req, &mut sink),
            Err(e) => {
                error!("MCP HTTP server state lock poisoned: {e}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
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
        let mcp_route = post(post_mcp).get(get_mcp_sse).fallback(method_not_allowed);

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

    #[cfg(test)]
    mod tests {
        use std::net::{IpAddr, Ipv4Addr};

        use axum::body::to_bytes;
        use serde_json::json;

        use crate::mcp::auth::AuthConfig;

        use super::*;

        async fn post_json(state: Arc<AppState>, body: Value) -> (StatusCode, Value) {
            let peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 49152);
            let response = post_mcp(
                ConnectInfo(peer),
                State(state),
                HeaderMap::new(),
                Json(body),
            )
            .await;
            let status = response.status();
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let value = if body.is_empty() {
                Value::Null
            } else {
                serde_json::from_slice(&body).unwrap()
            };
            (status, value)
        }

        #[tokio::test]
        async fn post_mcp_reuses_server_state_across_requests() {
            let validator = BearerValidator::new(AuthConfig::localhost_only());
            let state = Arc::new(AppState::new(validator));

            let (status, init) = post_json(
                Arc::clone(&state),
                json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "initialize",
                    "params": {
                        "protocolVersion": "2025-11-05",
                        "capabilities": {},
                        "clientInfo": {"name": "http-test", "version": "1"}
                    }
                }),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(init["result"]["protocolVersion"], "2025-11-25");

            let (status, notification) = post_json(
                Arc::clone(&state),
                json!({
                    "jsonrpc": "2.0",
                    "method": "notifications/initialized"
                }),
            )
            .await;
            assert_eq!(status, StatusCode::NO_CONTENT);
            assert_eq!(notification, Value::Null);

            let (status, tools) = post_json(
                Arc::clone(&state),
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "method": "tools/list"
                }),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            assert!(
                tools["result"]["tools"]
                    .as_array()
                    .is_some_and(|tools| !tools.is_empty())
            );
        }

        async fn post_json_with_headers(
            state: Arc<AppState>,
            headers: HeaderMap,
            body: Value,
        ) -> (StatusCode, Vec<u8>, Value) {
            let peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 49152);
            let response = post_mcp(ConnectInfo(peer), State(state), headers, Json(body)).await;
            let status = response.status();
            let bytes = to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec();
            let value = if bytes.is_empty() {
                Value::Null
            } else {
                serde_json::from_slice(&bytes).unwrap()
            };
            (status, bytes, value)
        }

        fn fresh() -> Arc<AppState> {
            Arc::new(AppState::new(BearerValidator::new(
                AuthConfig::localhost_only(),
            )))
        }

        fn hdr(pairs: &[(&str, &[u8])]) -> HeaderMap {
            let mut map = HeaderMap::new();
            for (name, value) in pairs {
                map.insert(
                    axum::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                    axum::http::HeaderValue::from_bytes(value).unwrap(),
                );
            }
            map
        }

        fn agree(version: &str, method: &str) -> HeaderMap {
            hdr(&[
                ("mcp-protocol-version", version.as_bytes()),
                ("mcp-method", method.as_bytes()),
            ])
        }

        fn rpc(id: i64, method: &str, params: Value) -> Value {
            json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
        }

        fn meta(version: &str) -> Value {
            json!({
                "io.modelcontextprotocol/protocolVersion": version,
                "io.modelcontextprotocol/clientCapabilities": {}
            })
        }

        fn list_body(version: &str) -> Value {
            rpc(7, "tools/list", json!({"_meta": meta(version)}))
        }

        fn call_body() -> Value {
            rpc(
                7,
                "tools/call",
                json!({
                    "_meta": meta("2026-07-28"),
                    "name": "ax_list_apps",
                    "arguments": {}
                }),
            )
        }

        fn keys_of(v: &Value) -> Vec<&str> {
            let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
            keys.sort_unstable();
            keys
        }

        fn assert_modern_list(v: &Value) {
            assert_eq!(v["result"]["resultType"], "complete");
            assert!(v.get("error").is_none(), "{v}");
            assert_eq!(v["result"]["ttlMs"], 0);
            assert!(v["result"]["ttlMs"].is_number());
            assert_eq!(v["result"]["cacheScope"], "public");
            let tools = v["result"]["tools"].as_array().unwrap();
            assert!(tools.iter().any(|tool| tool["name"] == "ax_list_apps"));
            assert_eq!(
                keys_of(&v["result"]),
                ["cacheScope", "resultType", "tools", "ttlMs"]
            );
        }

        fn assert_legacy_list(v: &Value) {
            assert!(v.get("error").is_none(), "{v}");
            assert_eq!(keys_of(&v["result"]), ["tools"]);
            let tools = v["result"]["tools"].as_array().unwrap();
            assert!(tools.iter().any(|tool| tool["name"] == "ax_list_apps"));
        }

        fn assert_apps(text: &str) {
            assert!(!text.contains("Server not yet initialized"));
            let parsed: Value = serde_json::from_str(text).unwrap();
            assert_eq!(keys_of(&parsed), ["apps"]);
            let apps = parsed["apps"].as_array().unwrap();
            assert!(!apps.is_empty());
            for app in apps {
                assert_eq!(keys_of(app), ["name", "pid"]);
                assert!(app["name"].is_string());
                assert!(app["pid"].is_number());
            }
        }

        fn assert_call(v: &Value) {
            assert_eq!(v["result"]["resultType"], "complete");
            assert!(v.get("error").is_none(), "{v}");
            assert_eq!(keys_of(&v["result"]), ["content", "isError", "resultType"]);
            assert!(v["result"].get("ttlMs").is_none(), "{v}");
            assert!(v["result"].get("cacheScope").is_none(), "{v}");
            assert_eq!(v["result"]["isError"], false);
            let content = v["result"]["content"].as_array().unwrap();
            assert_eq!(content.len(), 1);
            assert_eq!(content[0]["type"], "text");
            assert_apps(content[0]["text"].as_str().unwrap());
        }

        fn assert_discover(v: &Value, capabilities: &Value) {
            assert_eq!(v["result"]["resultType"], "complete");
            assert!(v.get("error").is_none(), "{v}");
            assert_eq!(v["result"]["ttlMs"], 0);
            assert_eq!(v["result"]["cacheScope"], "public");
            assert_eq!(
                v["result"]["supportedVersions"],
                json!(["2026-07-28", "2025-11-25"])
            );
            assert_eq!(&v["result"]["capabilities"], capabilities);
        }

        async fn donor_capabilities() -> Value {
            let state = fresh();
            let (_, _, v) = post_json_with_headers(
                state,
                HeaderMap::new(),
                rpc(
                    1,
                    "initialize",
                    json!({
                        "protocolVersion": "2025-11-25",
                        "capabilities": {},
                        "clientInfo": {"name": "test", "version": "1"}
                    }),
                ),
            )
            .await;
            assert!(v.get("error").is_none(), "{v}");
            v["result"]["capabilities"].clone()
        }

        async fn prime(state: &Arc<AppState>, prove_list: bool) {
            let (_, _, init) = post_json_with_headers(
                Arc::clone(state),
                HeaderMap::new(),
                rpc(
                    1,
                    "initialize",
                    json!({
                        "protocolVersion": "2025-11-05",
                        "capabilities": {"sampling": {}},
                        "clientInfo": {"name": "test", "version": "1"}
                    }),
                ),
            )
            .await;
            assert!(init.get("error").is_none(), "{init}");
            let (status, bytes, _) = post_json_with_headers(
                Arc::clone(state),
                HeaderMap::new(),
                json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            )
            .await;
            assert_eq!(status, StatusCode::NO_CONTENT);
            assert!(bytes.is_empty());
            if prove_list {
                let (status, _, listed) = post_json_with_headers(
                    Arc::clone(state),
                    HeaderMap::new(),
                    json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
                )
                .await;
                assert_eq!(status, StatusCode::OK);
                assert!(listed.get("error").is_none(), "{listed}");
            }
        }

        async fn init_only(state: &Arc<AppState>, version: &str) {
            let (status, _, v) = post_json_with_headers(
                Arc::clone(state),
                HeaderMap::new(),
                rpc(
                    1,
                    "initialize",
                    json!({
                        "protocolVersion": version,
                        "capabilities": {},
                        "clientInfo": {"name": "test", "version": "1"}
                    }),
                ),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            assert!(v.get("error").is_none(), "{v}");
            assert_eq!(v["result"]["protocolVersion"], "2025-11-25");
            assert!(v["result"].get("resultType").is_none(), "{v}");
            let (status, bytes, _) = post_json_with_headers(
                Arc::clone(state),
                HeaderMap::new(),
                json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            )
            .await;
            assert_eq!(status, StatusCode::NO_CONTENT);
            assert!(bytes.is_empty());
        }

        #[tokio::test]
        async fn mcp2026_h1_initialize_client_2026() {
            let (status, _, v) = post_json_with_headers(
                fresh(),
                HeaderMap::new(),
                rpc(
                    1,
                    "initialize",
                    json!({
                        "protocolVersion": "2026-07-28",
                        "capabilities": {},
                        "clientInfo": {"name": "test", "version": "1"}
                    }),
                ),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            assert!(v.get("error").is_none(), "{v}");
            assert_eq!(v["result"]["protocolVersion"], "2025-11-25");
            assert!(v["result"].get("resultType").is_none(), "{v}");
        }

        #[tokio::test]
        async fn mcp2026_h2_initialize_client_2025_11_25() {
            let state = fresh();
            init_only(&state, "2025-11-25").await;
        }

        #[tokio::test]
        async fn mcp2026_h3_initialize_client_2025_11_05() {
            let state = fresh();
            init_only(&state, "2025-11-05").await;
        }

        #[tokio::test]
        async fn mcp2026_h5_legacy_list_ignores_header() {
            let state = fresh();
            let (_, _, init) = post_json_with_headers(
                Arc::clone(&state),
                HeaderMap::new(),
                rpc(
                    1,
                    "initialize",
                    json!({
                        "protocolVersion": "2025-11-25",
                        "capabilities": {},
                        "clientInfo": {"name": "test", "version": "1"}
                    }),
                ),
            )
            .await;
            assert!(init.get("error").is_none(), "{init}");
            let (status, bytes, _) = post_json_with_headers(
                Arc::clone(&state),
                HeaderMap::new(),
                json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            )
            .await;
            assert_eq!(status, StatusCode::NO_CONTENT);
            assert!(bytes.is_empty());
            let (status, _, proof) = post_json_with_headers(
                Arc::clone(&state),
                HeaderMap::new(),
                json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            assert!(proof.get("error").is_none(), "{proof}");
            let (status, _, v) = post_json_with_headers(
                state,
                hdr(&[("mcp-protocol-version", b"2025-11-25")]),
                json!({"jsonrpc": "2.0", "id": 3, "method": "tools/list"}),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            assert_legacy_list(&v);
            assert_ne!(v["error"]["code"], -32020);
        }

        #[tokio::test]
        async fn mcp2026_h5b_legacy_list_after_2026_initialize() {
            let state = fresh();
            let (_, _, init) = post_json_with_headers(
                Arc::clone(&state),
                HeaderMap::new(),
                rpc(
                    1,
                    "initialize",
                    json!({
                        "protocolVersion": "2026-07-28",
                        "capabilities": {},
                        "clientInfo": {"name": "test", "version": "1"}
                    }),
                ),
            )
            .await;
            assert!(init.get("error").is_none(), "{init}");
            let _ = post_json_with_headers(
                Arc::clone(&state),
                HeaderMap::new(),
                json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            )
            .await;
            let (_, _, proof) = post_json_with_headers(
                Arc::clone(&state),
                HeaderMap::new(),
                json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
            )
            .await;
            assert!(proof.get("error").is_none(), "{proof}");
            let (status, _, v) = post_json_with_headers(
                state,
                hdr(&[("mcp-protocol-version", b"2025-11-25")]),
                json!({"jsonrpc": "2.0", "id": 3, "method": "tools/list"}),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            assert_legacy_list(&v);
        }

        #[tokio::test]
        async fn mcp2026_h6_modern_list_cold() {
            let (status, _, v) = post_json_with_headers(
                fresh(),
                agree("2026-07-28", "tools/list"),
                list_body("2026-07-28"),
            )
            .await;
            assert_modern_list(&v);
            assert_eq!(status, StatusCode::OK);
        }

        #[tokio::test]
        async fn mcp2026_h7_modern_list_primed_matches_cold() {
            let (cold_status, _, cold) = post_json_with_headers(
                fresh(),
                agree("2026-07-28", "tools/list"),
                list_body("2026-07-28"),
            )
            .await;
            let primed_state = fresh();
            prime(&primed_state, true).await;
            let (primed_status, _, primed) = post_json_with_headers(
                primed_state,
                agree("2026-07-28", "tools/list"),
                list_body("2026-07-28"),
            )
            .await;
            assert_eq!(primed_status, StatusCode::OK);
            assert_eq!(cold_status, StatusCode::OK);
            assert_eq!(primed["result"]["resultType"], "complete");
            assert_modern_list(&cold);
            assert_eq!(primed["result"]["tools"], cold["result"]["tools"]);
        }

        #[tokio::test]
        async fn mcp2026_h8_missing_capabilities_primed() {
            let state = fresh();
            prime(&state, true).await;
            let body = rpc(
                7,
                "tools/list",
                json!({
                    "_meta": {"io.modelcontextprotocol/protocolVersion": "2026-07-28"}
                }),
            );
            let (status, _, v) =
                post_json_with_headers(state, agree("2026-07-28", "tools/list"), body).await;
            assert_eq!(v["error"]["code"], -32602);
            assert_ne!(v["error"]["code"], -32020);
            assert_eq!(status, StatusCode::BAD_REQUEST);
        }

        #[tokio::test]
        async fn mcp2026_h9_header_meta_disagree() {
            let (status, _, v) = post_json_with_headers(
                fresh(),
                agree("2025-11-25", "tools/list"),
                list_body("2026-07-28"),
            )
            .await;
            assert_eq!(v["error"]["code"], -32020);
            assert_ne!(v["error"]["code"], -32022);
            assert_eq!(status, StatusCode::BAD_REQUEST);
        }

        #[tokio::test]
        async fn mcp2026_h10_discover_cold() {
            let capabilities = donor_capabilities().await;
            let (status, _, v) = post_json_with_headers(
                fresh(),
                agree("2026-07-28", "server/discover"),
                rpc(7, "server/discover", json!({"_meta": meta("2026-07-28")})),
            )
            .await;
            assert_discover(&v, &capabilities);
            assert_eq!(status, StatusCode::OK);
        }

        #[tokio::test]
        async fn mcp2026_h11_discover_primed_matches_cold() {
            let capabilities = donor_capabilities().await;
            let (cold_status, _, cold) = post_json_with_headers(
                fresh(),
                agree("2026-07-28", "server/discover"),
                rpc(7, "server/discover", json!({"_meta": meta("2026-07-28")})),
            )
            .await;
            let primed_state = fresh();
            prime(&primed_state, false).await;
            let (primed_status, _, primed) = post_json_with_headers(
                primed_state,
                agree("2026-07-28", "server/discover"),
                rpc(7, "server/discover", json!({"_meta": meta("2026-07-28")})),
            )
            .await;
            assert_eq!(primed_status, StatusCode::OK);
            assert_eq!(cold_status, StatusCode::OK);
            assert_eq!(primed["result"]["resultType"], "complete");
            assert_discover(&cold, &capabilities);
            assert_discover(&primed, &capabilities);
            assert_eq!(primed["result"], cold["result"]);
        }

        #[tokio::test]
        async fn mcp2026_h12_unsupported_cold() {
            let body = list_body("1900-01-01");
            let (status, _, v) =
                post_json_with_headers(fresh(), agree("1900-01-01", "tools/list"), body).await;
            assert_eq!(v["error"]["code"], -32022);
            assert_ne!(v["error"]["code"], -32020);
            assert_ne!(v["error"]["code"], -32601);
            assert_eq!(v["error"]["data"]["requested"], "1900-01-01");
            assert_eq!(
                v["error"]["data"]["supported"],
                json!(["2026-07-28", "2025-11-25"])
            );
            assert_eq!(status, StatusCode::BAD_REQUEST);
        }

        #[tokio::test]
        async fn mcp2026_h13_unsupported_primed() {
            let state = fresh();
            prime(&state, true).await;
            let (status, _, v) = post_json_with_headers(
                state,
                agree("1900-01-01", "tools/list"),
                list_body("1900-01-01"),
            )
            .await;
            assert_eq!(v["error"]["code"], -32022);
            assert_eq!(status, StatusCode::BAD_REQUEST);
        }

        #[tokio::test]
        async fn mcp2026_h14_call_cold() {
            let headers = hdr(&[
                ("mcp-protocol-version", b"2026-07-28"),
                ("mcp-method", b"tools/call"),
                ("mcp-name", b"ax_list_apps"),
            ]);
            let (status, _, v) = post_json_with_headers(fresh(), headers, call_body()).await;
            assert_call(&v);
            assert_eq!(status, StatusCode::OK);
        }

        #[tokio::test]
        async fn mcp2026_h15_call_primed_keeps_shape() {
            let headers = || {
                hdr(&[
                    ("mcp-protocol-version", b"2026-07-28"),
                    ("mcp-method", b"tools/call"),
                    ("mcp-name", b"ax_list_apps"),
                ])
            };
            let (cold_status, _, cold) =
                post_json_with_headers(fresh(), headers(), call_body()).await;
            let primed_state = fresh();
            prime(&primed_state, false).await;
            let (primed_status, _, primed) =
                post_json_with_headers(primed_state, headers(), call_body()).await;
            assert_eq!(primed_status, StatusCode::OK);
            assert_eq!(cold_status, StatusCode::OK);
            assert_eq!(primed["result"]["resultType"], "complete");
            assert_call(&cold);
            assert_call(&primed);
        }

        #[tokio::test]
        async fn mcp2026_h16_modern_list_at_2025_11_25() {
            let (status, _, v) = post_json_with_headers(
                fresh(),
                agree("2025-11-25", "tools/list"),
                list_body("2025-11-25"),
            )
            .await;
            assert_modern_list(&v);
            assert_eq!(status, StatusCode::OK);
        }

        #[tokio::test]
        async fn mcp2026_r2_partial_meta_primed() {
            let state = fresh();
            prime(&state, true).await;
            let body = rpc(
                7,
                "tools/list",
                json!({
                    "_meta": {"io.modelcontextprotocol/clientCapabilities": {}}
                }),
            );
            let (status, _, v) =
                post_json_with_headers(state, agree("2026-07-28", "tools/list"), body).await;
            assert_eq!(v["error"]["code"], -32602);
            assert_eq!(status, StatusCode::BAD_REQUEST);
        }

        #[tokio::test]
        async fn mcp2026_r3_forbidden_byte_on_version() {
            let mut version = b"2026-07-28".to_vec();
            version.extend_from_slice(&[0xC2, 0x80]);
            let headers = hdr(&[
                ("mcp-protocol-version", &version),
                ("mcp-method", b"tools/list"),
            ]);
            let body = rpc(
                7,
                "tools/list",
                json!({
                    "_meta": {
                        "io.modelcontextprotocol/protocolVersion": "2026-07-28\u{0080}",
                        "io.modelcontextprotocol/clientCapabilities": {}
                    }
                }),
            );
            let (status, _, v) = post_json_with_headers(fresh(), headers, body).await;
            assert_eq!(v["error"]["code"], -32020);
            assert_ne!(v["error"]["code"], -32022);
            assert_eq!(status, StatusCode::BAD_REQUEST);
        }

        #[tokio::test]
        async fn mcp2026_r4_forbidden_byte_on_unrelated_header() {
            let mut headers = agree("2026-07-28", "tools/list");
            headers.insert(
                axum::http::HeaderName::from_static("x-unrelated"),
                axum::http::HeaderValue::from_bytes(&[0x80]).unwrap(),
            );
            let (status, _, v) =
                post_json_with_headers(fresh(), headers, list_body("2026-07-28")).await;
            assert_eq!(v["error"]["code"], -32020);
            assert_eq!(status, StatusCode::BAD_REQUEST);
        }

        #[tokio::test]
        async fn mcp2026_r5a_encoded_name_matches() {
            let headers = hdr(&[
                ("mcp-protocol-version", b"2026-07-28"),
                ("mcp-method", b"tools/call"),
                ("mcp-name", b"=?base64?YXhfbGlzdF9hcHBz?="),
            ]);
            let (status, _, v) = post_json_with_headers(fresh(), headers, call_body()).await;
            assert_call(&v);
            assert_eq!(status, StatusCode::OK);
        }

        #[tokio::test]
        async fn mcp2026_r5b_encoded_name_disagrees() {
            let headers = hdr(&[
                ("mcp-protocol-version", b"2026-07-28"),
                ("mcp-method", b"tools/call"),
                ("mcp-name", b"=?base64?b3RoZXI=?="),
            ]);
            let (status, _, v) = post_json_with_headers(fresh(), headers, call_body()).await;
            assert_eq!(v["error"]["code"], -32020);
            assert_eq!(status, StatusCode::BAD_REQUEST);
        }

        #[tokio::test]
        async fn mcp2026_r5c_sentinel_does_not_decode() {
            let headers = hdr(&[
                ("mcp-protocol-version", b"2026-07-28"),
                ("mcp-method", b"tools/call"),
                ("mcp-name", b"=?base64?*?="),
            ]);
            let (status, _, v) = post_json_with_headers(fresh(), headers, call_body()).await;
            assert_eq!(v["error"]["code"], -32020);
            assert_eq!(status, StatusCode::BAD_REQUEST);
        }

        #[tokio::test]
        async fn mcp2026_r5d_plain_name_disagrees() {
            let headers = hdr(&[
                ("mcp-protocol-version", b"2026-07-28"),
                ("mcp-method", b"tools/call"),
                ("mcp-name", b"other"),
            ]);
            let (status, _, v) = post_json_with_headers(fresh(), headers, call_body()).await;
            assert_eq!(v["error"]["code"], -32020);
            assert_eq!(status, StatusCode::BAD_REQUEST);
        }

        #[tokio::test]
        async fn mcp2026_r6_unknown_method_cold_is_404() {
            let body = rpc(7, "no/such", json!({"_meta": meta("2026-07-28")}));
            let (status, _, v) =
                post_json_with_headers(fresh(), agree("2026-07-28", "no/such"), body).await;
            assert_eq!(v["error"]["code"], -32601);
            assert_eq!(status, StatusCode::NOT_FOUND);
        }

        #[tokio::test]
        async fn mcp2026_r6b_unknown_method_primed_is_404() {
            let state = fresh();
            prime(&state, false).await;
            let body = rpc(7, "no/such", json!({"_meta": meta("2026-07-28")}));
            let (status, _, v) =
                post_json_with_headers(state, agree("2026-07-28", "no/such"), body).await;
            assert_eq!(status, StatusCode::NOT_FOUND);
            assert_eq!(v["error"]["code"], -32601);
        }

        #[tokio::test]
        async fn mcp2026_r8_null_capabilities_is_present() {
            let body = rpc(
                7,
                "tools/list",
                json!({
                    "_meta": {
                        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                        "io.modelcontextprotocol/clientCapabilities": null
                    }
                }),
            );
            let (status, _, v) =
                post_json_with_headers(fresh(), agree("2026-07-28", "tools/list"), body).await;
            assert_modern_list(&v);
            assert_eq!(status, StatusCode::OK);
        }

        #[tokio::test]
        async fn mcp2026_r9_method_header_absent() {
            let (status, _, v) = post_json_with_headers(
                fresh(),
                hdr(&[("mcp-protocol-version", b"2026-07-28")]),
                list_body("2026-07-28"),
            )
            .await;
            assert_eq!(v["error"]["code"], -32020);
            assert_eq!(status, StatusCode::BAD_REQUEST);
        }

        #[tokio::test]
        async fn mcp2026_r10_name_header_absent() {
            let (status, _, v) =
                post_json_with_headers(fresh(), agree("2026-07-28", "tools/call"), call_body())
                    .await;
            assert_eq!(v["error"]["code"], -32020);
            assert_eq!(status, StatusCode::BAD_REQUEST);
        }

        #[tokio::test]
        async fn mcp2026_r11_method_header_disagrees() {
            let (status, _, v) = post_json_with_headers(
                fresh(),
                agree("2026-07-28", "ping"),
                list_body("2026-07-28"),
            )
            .await;
            assert_eq!(v["error"]["code"], -32020);
            assert_ne!(v["error"]["code"], -32602);
            assert_eq!(status, StatusCode::BAD_REQUEST);
        }

        #[tokio::test]
        async fn mcp2026_r12_protocol_header_absent() {
            let (status, _, v) = post_json_with_headers(
                fresh(),
                hdr(&[("mcp-method", b"tools/list")]),
                list_body("2026-07-28"),
            )
            .await;
            assert_eq!(v["error"]["code"], -32020);
            assert_eq!(status, StatusCode::BAD_REQUEST);
        }

        #[tokio::test]
        async fn mcp2026_r13_empty_protocol_header() {
            let (status, _, v) = post_json_with_headers(
                fresh(),
                hdr(&[("mcp-protocol-version", b""), ("mcp-method", b"tools/list")]),
                list_body("2026-07-28"),
            )
            .await;
            assert_eq!(v["error"]["code"], -32020);
            assert_eq!(status, StatusCode::BAD_REQUEST);
        }

        #[tokio::test]
        async fn mcp2026_r14_extra_name_on_list() {
            let headers = hdr(&[
                ("mcp-protocol-version", b"2026-07-28"),
                ("mcp-method", b"tools/list"),
                ("mcp-name", b"extra"),
            ]);
            let (status, _, v) =
                post_json_with_headers(fresh(), headers, list_body("2026-07-28")).await;
            assert_modern_list(&v);
            assert_eq!(status, StatusCode::OK);
        }

        #[tokio::test]
        async fn mcp2026_n1_notification_stays_empty() {
            let body = json!({
                "jsonrpc": "2.0",
                "method": "tools/list",
                "params": {"_meta": {"io.modelcontextprotocol/protocolVersion": "2026-07-28"}}
            });
            let (status, bytes, value) =
                post_json_with_headers(fresh(), agree("2026-07-28", "tools/list"), body).await;
            assert_eq!(status, StatusCode::NO_CONTENT);
            assert!(bytes.is_empty());
            assert!(value.is_null());
        }

        #[tokio::test]
        async fn mcp2026_n2_initialized_still_advances_phase() {
            let state = fresh();
            let (_, _, init) = post_json_with_headers(
                Arc::clone(&state),
                HeaderMap::new(),
                rpc(
                    1,
                    "initialize",
                    json!({
                        "protocolVersion": "2025-11-25",
                        "capabilities": {},
                        "clientInfo": {"name": "test", "version": "1"}
                    }),
                ),
            )
            .await;
            assert!(init.get("error").is_none(), "{init}");
            let (_, _, early) = post_json_with_headers(
                Arc::clone(&state),
                HeaderMap::new(),
                json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
            )
            .await;
            assert_eq!(early["error"]["code"], -32600);
            let (status, bytes, _) = post_json_with_headers(
                Arc::clone(&state),
                HeaderMap::new(),
                json!({
                    "jsonrpc": "2.0",
                    "method": "notifications/initialized",
                    "params": {"_meta": meta("2026-07-28")}
                }),
            )
            .await;
            assert_eq!(status, StatusCode::NO_CONTENT);
            assert!(bytes.is_empty());
            let (status, _, listed) = post_json_with_headers(
                state,
                HeaderMap::new(),
                json!({"jsonrpc": "2.0", "id": 3, "method": "tools/list"}),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            assert_legacy_list(&listed);
        }

        #[tokio::test]
        async fn mcp2026_n3_ping_ignores_modern_meta() {
            let body = rpc(1, "ping", json!({"_meta": meta("2026-07-28")}));
            let (status, _, v) = post_json_with_headers(fresh(), HeaderMap::new(), body).await;
            assert_eq!(status, StatusCode::OK);
            assert!(v.get("error").is_none(), "{v}");
            assert_eq!(v["result"], json!({}));
            assert!(v["result"].get("resultType").is_none(), "{v}");
        }

        #[tokio::test]
        async fn mcp2026_n4_resources_list_keeps_phase_gate() {
            let body = rpc(1, "resources/list", json!({"_meta": meta("2026-07-28")}));
            let (status, _, v) =
                post_json_with_headers(fresh(), agree("2026-07-28", "resources/list"), body).await;
            assert_eq!(v["error"]["code"], -32600);
            let message = v["error"]["message"].as_str().unwrap_or("");
            assert!(message.contains("Server not yet initialized"), "{v}");
            assert_eq!(status, StatusCode::OK);
            assert_ne!(status, StatusCode::BAD_REQUEST);
            assert_ne!(status, StatusCode::NOT_FOUND);
        }

        #[tokio::test]
        async fn mcp2026_n5_primed_resources_list_stays_legacy() {
            let state = fresh();
            prime(&state, false).await;
            let body = rpc(3, "resources/list", json!({"_meta": meta("2026-07-28")}));
            let (status, _, v) =
                post_json_with_headers(state, agree("2026-07-28", "resources/list"), body).await;
            assert_eq!(status, StatusCode::OK);
            assert!(v.get("error").is_none(), "{v}");
            assert!(v["result"]["resources"].is_array(), "{v}");
            assert!(v["result"].get("resultType").is_none(), "{v}");
            assert!(v["result"].get("ttlMs").is_none(), "{v}");
            assert!(v["result"].get("cacheScope").is_none(), "{v}");
        }

        #[tokio::test]
        async fn mcp2026_n7_legacy_unknown_stays_200() {
            let state = fresh();
            prime(&state, false).await;
            let (status, _, v) = post_json_with_headers(
                state,
                HeaderMap::new(),
                json!({"jsonrpc": "2.0", "id": 3, "method": "no/such"}),
            )
            .await;
            assert_eq!(v["error"]["code"], -32601);
            assert_eq!(status, StatusCode::OK);
            assert_ne!(status, StatusCode::NOT_FOUND);
        }

        #[tokio::test]
        async fn mcp2026_n8_legacy_call_missing_name_stays_200() {
            let state = fresh();
            prime(&state, false).await;
            let (status, _, v) = post_json_with_headers(
                state,
                HeaderMap::new(),
                json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {}}),
            )
            .await;
            assert_eq!(v["error"]["code"], -32602);
            assert_eq!(status, StatusCode::OK);
            assert_ne!(status, StatusCode::BAD_REQUEST);
        }

        #[tokio::test]
        async fn mcp2026_n9_tasks_list_ignores_meta() {
            let state = fresh();
            prime(&state, false).await;
            let body = rpc(3, "tasks/list", json!({"_meta": meta("2026-07-28")}));
            let (status, _, v) = post_json_with_headers(state, HeaderMap::new(), body).await;
            assert_eq!(status, StatusCode::OK);
            assert!(v.get("error").is_none(), "{v}");
            assert!(v["result"]["tasks"].is_array(), "{v}");
            assert!(v["result"].get("resultType").is_none(), "{v}");
            assert_ne!(v["error"]["code"], -32020);
        }

        #[tokio::test]
        async fn mcp2026_n10_subscribe_ignores_meta() {
            let state = fresh();
            prime(&state, false).await;
            let body = rpc(
                3,
                "resources/subscribe",
                json!({
                    "_meta": meta("2026-07-28"),
                    "uri": "axterminator://system/status"
                }),
            );
            let (status, _, v) = post_json_with_headers(state, HeaderMap::new(), body).await;
            assert_eq!(status, StatusCode::OK);
            assert!(v.get("error").is_none(), "{v}");
            assert_ne!(v["error"]["code"], -32601);
            assert_ne!(v["error"]["code"], -32020);
        }

        #[tokio::test]
        async fn mcp2026_n11_unsubscribe_ignores_meta() {
            let state = fresh();
            prime(&state, false).await;
            let body = rpc(
                3,
                "resources/unsubscribe",
                json!({
                    "_meta": meta("2026-07-28"),
                    "uri": "axterminator://system/status"
                }),
            );
            let (status, _, v) = post_json_with_headers(state, HeaderMap::new(), body).await;
            assert_eq!(status, StatusCode::OK);
            assert!(v.get("error").is_none(), "{v}");
            assert_ne!(v["error"]["code"], -32601);
            assert_ne!(v["error"]["code"], -32020);
        }

        #[tokio::test]
        async fn mcp2026_n12_task_result_unknown_id() {
            let state = fresh();
            prime(&state, false).await;
            let body = rpc(
                3,
                "tasks/result",
                json!({
                    "_meta": meta("2026-07-28"),
                    "taskId": "no-such-task"
                }),
            );
            let (status, _, v) = post_json_with_headers(state, HeaderMap::new(), body).await;
            assert_eq!(v["error"]["code"], -32602);
            assert_eq!(status, StatusCode::OK);
            assert_ne!(status, StatusCode::BAD_REQUEST);
        }

        #[tokio::test]
        async fn mcp2026_n13_task_cancel_unknown_id() {
            let state = fresh();
            prime(&state, false).await;
            let body = rpc(
                3,
                "tasks/cancel",
                json!({
                    "_meta": meta("2026-07-28"),
                    "taskId": "no-such-task"
                }),
            );
            let (status, _, v) = post_json_with_headers(state, HeaderMap::new(), body).await;
            assert_eq!(v["error"]["code"], -32602);
            assert_eq!(status, StatusCode::OK);
            assert_ne!(status, StatusCode::BAD_REQUEST);
        }

        #[tokio::test]
        async fn mcp2026_n14_progress_token_stays_legacy() {
            let state = fresh();
            prime(&state, true).await;
            let body = rpc(4, "tools/list", json!({"_meta": {"progressToken": "1"}}));
            let (status, _, v) = post_json_with_headers(
                state,
                hdr(&[("mcp-protocol-version", b"2025-11-25")]),
                body,
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            assert_legacy_list(&v);
        }
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
