//! HTTP transport authentication for the MCP server.
//!
//! This module implements bearer token authentication for the
//! [`http-transport`] feature. The design follows two tenets:
//!
//! 1. **Localhost-only mode** (`--localhost-only`): binds to `127.0.0.1` and
//!    verifies the source IP of every request. No token required.
//!
//! 2. **Bearer token mode** (default for non-localhost): every HTTP request must
//!    carry `Authorization: Bearer <token>`. On first start a 32-byte random
//!    token is generated and printed to stderr. The caller may supply a token
//!    via `--token` or `AXTERMINATOR_HTTP_TOKEN`.
//!
//! # Security rationale
//!
//! This server controls the desktop. Refusing to start without authentication
//! on a non-localhost interface is not optional: a rogue request can click,
//! type, screenshot, and script any application the user has open.
//!
//! # Examples
//!
//! ```rust
//! use axterminator::mcp::auth::{AuthConfig, BearerValidator};
//!
//! let config = AuthConfig::localhost_only();
//! assert!(config.is_localhost_only());
//!
//! let token = "axt_abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";
//! let config = AuthConfig::bearer(token.to_string());
//! let validator = BearerValidator::new(config);
//! // validate_bearer expects the full `Authorization` header value.
//! let header = format!("Bearer {token}");
//! assert!(validator.validate_bearer(&header).is_ok());
//! assert!(validator.validate_bearer("Bearer wrong").is_err());
//! ```

use std::net::IpAddr;

use thiserror::Error;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Authentication errors returned by [`BearerValidator`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AuthError {
    /// The `Authorization` header was missing entirely.
    #[error("missing Authorization header")]
    MissingHeader,

    /// The header was present but the scheme was not `Bearer`.
    #[error("unsupported authorization scheme (expected Bearer)")]
    UnsupportedScheme,

    /// The token did not match.
    #[error("invalid bearer token")]
    InvalidToken,

    /// The source IP is not allowed (localhost-only mode).
    #[error("request from non-localhost address {0} rejected (localhost-only mode)")]
    NonLocalhostAddress(IpAddr),

    /// A safe configuration would allow unauthenticated access from non-localhost.
    #[error("refusing to start: --bind 0.0.0.0 requires --token or --localhost-only")]
    UnsafeConfig,
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Selects which authentication mode the HTTP transport enforces.
///
/// Build with [`AuthConfig::localhost_only`] or [`AuthConfig::bearer`].
#[derive(Debug, Clone)]
pub enum AuthConfig {
    /// Accept requests only from `127.0.0.1` / `::1`. No token required.
    LocalhostOnly,
    /// Require a valid bearer token on every request.
    Bearer(String),
}

impl AuthConfig {
    /// Create a localhost-only configuration.
    ///
    /// All requests from non-loopback addresses are rejected with
    /// [`AuthError::NonLocalhostAddress`] without inspecting any header.
    #[must_use]
    pub fn localhost_only() -> Self {
        Self::LocalhostOnly
    }

    /// Create a bearer-token configuration.
    ///
    /// `token` is the expected value of the `Bearer` credential. It must be
    /// long enough to resist brute force (≥32 bytes recommended). Callers
    /// typically obtain this from [`generate_token`].
    #[must_use]
    pub fn bearer(token: String) -> Self {
        Self::Bearer(token)
    }

    /// Returns `true` if this is a localhost-only configuration.
    #[must_use]
    pub fn is_localhost_only(&self) -> bool {
        matches!(self, Self::LocalhostOnly)
    }

    /// Returns `true` if this is a bearer-token configuration.
    #[must_use]
    pub fn is_bearer(&self) -> bool {
        matches!(self, Self::Bearer(_))
    }
}

// ---------------------------------------------------------------------------
// Validator
// ---------------------------------------------------------------------------

/// Stateless validator for bearer tokens and source-IP checks.
///
/// Construct once and clone cheaply into every request handler.
#[derive(Debug, Clone)]
pub struct BearerValidator {
    config: AuthConfig,
}

impl BearerValidator {
    /// Create a new validator from an [`AuthConfig`].
    #[must_use]
    pub fn new(config: AuthConfig) -> Self {
        Self { config }
    }

    /// Validate the raw value of the `Authorization` HTTP header.
    ///
    /// Accepts `Bearer <token>` (case-insensitive scheme).
    ///
    /// # Errors
    ///
    /// Returns [`AuthError`] when the header is absent, has the wrong scheme,
    /// or carries an incorrect token. In localhost-only mode this always
    /// returns `Ok(())` because no token is required.
    pub fn validate_bearer(&self, raw_header: &str) -> Result<(), AuthError> {
        let AuthConfig::Bearer(expected) = &self.config else {
            // Localhost-only — no token check needed.
            return Ok(());
        };

        let Some(credential) = strip_bearer_scheme(raw_header) else {
            return Err(AuthError::UnsupportedScheme);
        };

        // Constant-time comparison to resist timing attacks.
        if constant_time_eq(credential.as_bytes(), expected.as_bytes()) {
            Ok(())
        } else {
            Err(AuthError::InvalidToken)
        }
    }

    /// Validate that `addr` is a loopback address (localhost-only mode).
    ///
    /// In bearer-token mode this is a no-op and always returns `Ok(())`.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::NonLocalhostAddress`] when the config is
    /// localhost-only and `addr` is not a loopback address.
    pub fn validate_source_ip(&self, addr: IpAddr) -> Result<(), AuthError> {
        if matches!(self.config, AuthConfig::LocalhostOnly) && !addr.is_loopback() {
            return Err(AuthError::NonLocalhostAddress(addr));
        }
        Ok(())
    }

    /// Validate an `Option<&str>` authorization header, mapping `None` to
    /// [`AuthError::MissingHeader`].
    ///
    /// Convenience wrapper around [`validate_bearer`][Self::validate_bearer].
    ///
    /// # Errors
    ///
    /// See [`validate_bearer`][Self::validate_bearer].
    pub fn validate_header(&self, header: Option<&str>) -> Result<(), AuthError> {
        match header {
            None => {
                if matches!(self.config, AuthConfig::LocalhostOnly) {
                    Ok(())
                } else {
                    Err(AuthError::MissingHeader)
                }
            }
            Some(h) => self.validate_bearer(h),
        }
    }

    /// Check that a proposed bind configuration is safe.
    ///
    /// Binding to a non-loopback address without a token configured is an
    /// unsafe configuration — callers must use `--token` or
    /// `--localhost-only`.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::UnsafeConfig`] when `bind_addr` is not a
    /// loopback address and the config is localhost-only.
    pub fn check_bind_safety(&self, bind_addr: IpAddr) -> Result<(), AuthError> {
        if !bind_addr.is_loopback() && matches!(self.config, AuthConfig::LocalhostOnly) {
            return Err(AuthError::UnsafeConfig);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Token generation (http-transport feature only)
// ---------------------------------------------------------------------------

/// Generate a cryptographically random bearer token.
///
/// Produces a 32-byte random token hex-encoded with the `axt_` prefix,
/// yielding a 68-character string:
///
/// ```text
/// axt_<64 hex chars>
/// ```
///
/// # Examples
///
/// ```rust
/// use axterminator::mcp::auth::generate_token;
///
/// let token = generate_token();
/// assert!(token.starts_with("axt_"));
/// assert_eq!(token.len(), 68);
/// ```
#[cfg(feature = "http-transport")]
#[must_use]
pub fn generate_token() -> String {
    use rand::RngCore as _;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!("axt_{}", hex::encode_bytes(&bytes))
}

/// Deterministic test helper — creates a fixed-length token without `rand`.
///
/// Used in unit tests that need a well-formed token without the
/// `http-transport` feature.
///
/// # Examples
///
/// ```rust
/// let token = axterminator::mcp::auth::make_test_token("abc");
/// assert!(token.starts_with("axt_"));
/// ```
#[must_use]
pub fn make_test_token(seed: &str) -> String {
    // Pad or truncate seed to exactly 64 hex chars.
    let hex_body = format!("{:0<64}", seed.chars().take(64).collect::<String>());
    format!("axt_{hex_body}")
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Strip the `Bearer ` prefix (case-insensitive), returning the credential.
fn strip_bearer_scheme(header: &str) -> Option<&str> {
    let trimmed = header.trim_start();
    if trimmed.len() < 7 {
        return None;
    }
    let (scheme, rest) = trimmed.split_at(7);
    if scheme.eq_ignore_ascii_case("Bearer ") {
        Some(rest.trim_start())
    } else {
        None
    }
}

/// Constant-time byte comparison.
///
/// Uses XOR-fold so the compiler cannot short-circuit the loop.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let diff: u8 = a.iter().zip(b.iter()).fold(0, |acc, (x, y)| acc | (x ^ y));
    diff == 0
}

// ---------------------------------------------------------------------------
// Hex encoding helper (no dependency on external crate in non-http builds)
// ---------------------------------------------------------------------------

/// Minimal hex encoder used by [`generate_token`].
mod hex {
    #[cfg(feature = "http-transport")]
    pub fn encode_bytes(bytes: &[u8]) -> String {
        bytes.iter().fold(String::new(), |mut s, b| {
            use std::fmt::Write as _;
            let _ = write!(s, "{b:02x}");
            s
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    // -----------------------------------------------------------------------
    // AuthConfig
    // -----------------------------------------------------------------------

    #[test]
    fn localhost_only_config_reports_correctly() {
        // GIVEN: localhost-only config
        let cfg = AuthConfig::localhost_only();
        // THEN: predicates are consistent
        assert!(cfg.is_localhost_only());
        assert!(!cfg.is_bearer());
    }

    #[test]
    fn bearer_config_reports_correctly() {
        // GIVEN: bearer config
        let cfg = AuthConfig::bearer("secret".into());
        // THEN: predicates are consistent
        assert!(!cfg.is_localhost_only());
        assert!(cfg.is_bearer());
    }

    // -----------------------------------------------------------------------
    // strip_bearer_scheme
    // -----------------------------------------------------------------------

    #[test]
    fn strip_bearer_scheme_extracts_credential() {
        // GIVEN: a canonical Bearer header
        // WHEN: stripped
        let result = strip_bearer_scheme("Bearer my-secret-token");
        // THEN: credential is returned
        assert_eq!(result, Some("my-secret-token"));
    }

    #[test]
    fn strip_bearer_scheme_is_case_insensitive() {
        assert_eq!(strip_bearer_scheme("bearer TOKEN"), Some("TOKEN"));
        assert_eq!(strip_bearer_scheme("BEARER TOKEN"), Some("TOKEN"));
    }

    #[test]
    fn strip_bearer_scheme_returns_none_for_basic() {
        assert_eq!(strip_bearer_scheme("Basic dXNlcjpwYXNz"), None);
    }

    #[test]
    fn strip_bearer_scheme_returns_none_for_empty_string() {
        assert_eq!(strip_bearer_scheme(""), None);
    }

    // -----------------------------------------------------------------------
    // constant_time_eq
    // -----------------------------------------------------------------------

    #[test]
    fn constant_time_eq_matches_identical_slices() {
        assert!(constant_time_eq(b"hello", b"hello"));
    }

    #[test]
    fn constant_time_eq_rejects_different_slices() {
        assert!(!constant_time_eq(b"hello", b"world"));
    }

    #[test]
    fn constant_time_eq_rejects_different_lengths() {
        assert!(!constant_time_eq(b"hi", b"hello"));
    }

    #[test]
    fn constant_time_eq_handles_empty_slices() {
        assert!(constant_time_eq(b"", b""));
    }

    // -----------------------------------------------------------------------
    // BearerValidator — bearer mode
    // -----------------------------------------------------------------------

    fn make_validator(token: &str) -> BearerValidator {
        BearerValidator::new(AuthConfig::bearer(token.to_string()))
    }

    #[test]
    fn validate_bearer_accepts_correct_token() {
        // GIVEN: validator with a known token
        let v = make_validator("my-secret");
        // WHEN: correct header
        // THEN: ok
        assert!(v.validate_bearer("Bearer my-secret").is_ok());
    }

    #[test]
    fn validate_bearer_rejects_wrong_token() {
        let v = make_validator("correct");
        assert_eq!(
            v.validate_bearer("Bearer wrong"),
            Err(AuthError::InvalidToken)
        );
    }

    #[test]
    fn validate_bearer_rejects_non_bearer_scheme() {
        let v = make_validator("tok");
        assert_eq!(
            v.validate_bearer("Basic dXNlcjpwYXNz"),
            Err(AuthError::UnsupportedScheme)
        );
    }

    #[test]
    fn validate_header_returns_missing_header_when_none() {
        let v = make_validator("tok");
        assert_eq!(v.validate_header(None), Err(AuthError::MissingHeader));
    }

    #[test]
    fn validate_header_accepts_correct_bearer() {
        let v = make_validator("tok");
        assert!(v.validate_header(Some("Bearer tok")).is_ok());
    }

    // -----------------------------------------------------------------------
    // BearerValidator — localhost-only mode
    // -----------------------------------------------------------------------

    fn localhost_validator() -> BearerValidator {
        BearerValidator::new(AuthConfig::localhost_only())
    }

    #[test]
    fn localhost_only_allows_loopback_ipv4() {
        // GIVEN: localhost-only validator
        let v = localhost_validator();
        // WHEN: source is 127.0.0.1
        let r = v.validate_source_ip(IpAddr::V4(Ipv4Addr::LOCALHOST));
        // THEN: ok
        assert!(r.is_ok());
    }

    #[test]
    fn localhost_only_allows_loopback_ipv6() {
        let v = localhost_validator();
        assert!(v
            .validate_source_ip(IpAddr::V6(Ipv6Addr::LOCALHOST))
            .is_ok());
    }

    #[test]
    fn localhost_only_rejects_external_ip() {
        let v = localhost_validator();
        let ext = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));
        assert_eq!(
            v.validate_source_ip(ext),
            Err(AuthError::NonLocalhostAddress(ext))
        );
    }

    #[test]
    fn localhost_only_header_validation_skips_token_check() {
        // Localhost-only: no token header required.
        let v = localhost_validator();
        assert!(v.validate_header(None).is_ok());
        assert!(v.validate_bearer("Bearer anything").is_ok());
    }

    // -----------------------------------------------------------------------
    // check_bind_safety
    // -----------------------------------------------------------------------

    #[test]
    fn check_bind_safety_rejects_external_bind_without_token() {
        // GIVEN: localhost-only config (no token) + non-loopback bind
        let v = localhost_validator();
        let addr = IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0));
        // THEN: unsafe
        assert_eq!(v.check_bind_safety(addr), Err(AuthError::UnsafeConfig));
    }

    #[test]
    fn check_bind_safety_allows_loopback_without_token() {
        let v = localhost_validator();
        assert!(v.check_bind_safety(IpAddr::V4(Ipv4Addr::LOCALHOST)).is_ok());
    }

    #[test]
    fn check_bind_safety_allows_external_with_token() {
        // GIVEN: bearer mode (token present)
        let v = make_validator("tok");
        let addr = IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0));
        // THEN: safe — token will guard the endpoint
        assert!(v.check_bind_safety(addr).is_ok());
    }

    // -----------------------------------------------------------------------
    // make_test_token
    // -----------------------------------------------------------------------

    #[test]
    fn make_test_token_produces_axt_prefix() {
        let t = make_test_token("abc");
        assert!(t.starts_with("axt_"));
    }

    #[test]
    fn make_test_token_produces_correct_total_length() {
        // "axt_" + 64 chars = 68
        let t = make_test_token("anything");
        assert_eq!(t.len(), 68);
    }

    #[test]
    fn make_test_token_pads_short_seed() {
        let t = make_test_token("x");
        assert_eq!(t.len(), 68);
    }

    // -----------------------------------------------------------------------
    // hex helper
    // -----------------------------------------------------------------------

    #[cfg(feature = "http-transport")]
    #[test]
    fn hex_encode_produces_lowercase_hex() {
        assert_eq!(hex::encode_bytes(&[0x0A, 0xFF, 0x00]), "0aff00");
    }

    #[cfg(feature = "http-transport")]
    #[test]
    fn hex_encode_empty_slice_is_empty_string() {
        assert_eq!(hex::encode_bytes(&[]), "");
    }
}
