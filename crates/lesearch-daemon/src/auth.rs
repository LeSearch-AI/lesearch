//! Bearer token authentication and Origin/Host allowlist.
//!
//! Provides loopback hardening for the daemon's `WebSocket` endpoint:
//! - Per-session bearer token generated at daemon start
//! - Origin/Host header allowlist to prevent DNS-rebinding attacks

use std::path::{Path, PathBuf};

use axum::extract::{State, WebSocketUpgrade};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use rand::Rng;

use crate::SharedState;

/// Length of the random bearer token in bytes (hex-encoded = 128 chars).
const TOKEN_BYTES: usize = 64;

/// Built-in allowed origins for loopback access.
const BUILTIN_ORIGINS: &[&str] = &["127.0.0.1:6767", "localhost:6767"];

// ---------------------------------------------------------------------------
// Token management
// ---------------------------------------------------------------------------

/// Generate a cryptographically random bearer token (hex-encoded).
#[must_use]
pub fn generate_token() -> String {
    let mut bytes = [0u8; TOKEN_BYTES];
    rand::thread_rng().fill(&mut bytes);
    hex::encode(bytes)
}

/// Load or generate the session bearer token.
///
/// Reads from `$LESEARCH_HOME/keyring/session.token`. If the file doesn't
/// exist, generates a new token and writes it with `0600` permissions.
///
/// # Errors
///
/// Returns an error if the file exists with wrong permissions or on I/O failure.
pub fn load_or_generate_token(home: &Path) -> anyhow::Result<String> {
    let token_path = token_path(home);

    if token_path.exists() {
        // Verify permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let meta = std::fs::metadata(&token_path)?;
            let mode = meta.permissions().mode() & 0o777;
            if mode != 0o600 {
                anyhow::bail!(
                    "session.token has permissions {mode:04o}, expected 0600. \
                     Fix with: chmod 600 {}",
                    token_path.display()
                );
            }
        }

        let token = std::fs::read_to_string(&token_path)?.trim().to_owned();
        if token.is_empty() {
            anyhow::bail!("session.token is empty, delete it and restart the daemon");
        }
        tracing::info!("bearer token loaded from {}", token_path.display());
        return Ok(token);
    }

    // Generate new token
    let token = generate_token();
    if let Some(parent) = token_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&token_path, &token)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&token_path, std::fs::Permissions::from_mode(0o600))?;
    }

    tracing::info!("bearer token generated at {}", token_path.display());
    Ok(token)
}

/// Read the bearer token from disk (used by CLI).
///
/// # Errors
///
/// Returns an error if the token file doesn't exist or can't be read.
pub fn read_token(home: &Path) -> anyhow::Result<String> {
    let path = token_path(home);
    let token = std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", path.display()))?
        .trim()
        .to_owned();
    Ok(token)
}

/// Path to the session token file.
#[must_use]
pub fn token_path(home: &Path) -> PathBuf {
    home.join("keyring").join("session.token")
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Check if a bearer token from the `Authorization` header matches the expected token.
#[must_use]
pub fn validate_bearer(headers: &HeaderMap, expected: &str) -> bool {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|token| token == expected)
}

/// Check if the request's `Origin` or `Host` header is in the allowlist.
///
/// If neither header is present, the request is allowed (direct TCP connections
/// from CLI tools typically don't send Origin/Host).
#[must_use]
pub fn validate_origin(headers: &HeaderMap, additional_origins: &[String]) -> bool {
    // Check Origin header if present
    if let Some(origin) = headers.get(axum::http::header::ORIGIN).and_then(|v| v.to_str().ok()) {
        // Strip scheme prefix for comparison (http://127.0.0.1:6767 → 127.0.0.1:6767)
        let origin_host = origin
            .strip_prefix("http://")
            .or_else(|| origin.strip_prefix("https://"))
            .unwrap_or(origin);

        return is_allowed_origin(origin_host, additional_origins);
    }

    // Check Host header if present
    if let Some(host) = headers.get(axum::http::header::HOST).and_then(|v| v.to_str().ok()) {
        return is_allowed_origin(host, additional_origins);
    }

    // No Origin/Host header — allow (CLI tools don't send these)
    true
}

/// Check if an origin/host value is in the allowlist.
fn is_allowed_origin(value: &str, additional: &[String]) -> bool {
    BUILTIN_ORIGINS.contains(&value)
        || additional.iter().any(|o| o == value)
}

// ---------------------------------------------------------------------------
// WebSocket handler with auth
// ---------------------------------------------------------------------------

/// Authenticated `WebSocket` upgrade handler.
///
/// Checks bearer token and origin allowlist before upgrading.
/// Returns `403` or closes with `1008` on auth failure.
#[allow(clippy::unused_async)]
pub async fn authenticated_ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Response {
    // Origin/Host check first (cheap)
    if !validate_origin(&headers, &state.config.security.additional_origins) {
        tracing::warn!("WebSocket rejected: origin not in allowlist");
        return (
            axum::http::StatusCode::FORBIDDEN,
            [("X-LeSearch-Error", "-32000")],
            "origin not allowed",
        )
            .into_response();
    }

    // Bearer token check
    if !validate_bearer(&headers, &state.bearer_token) {
        tracing::warn!("WebSocket rejected: missing or invalid bearer token");
        return (
            axum::http::StatusCode::FORBIDDEN,
            [("X-LeSearch-Error", "-32000")],
            "unauthorized",
        )
            .into_response();
    }

    ws.on_upgrade(move |socket| crate::ws::handle_socket(socket, state))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderMap, HeaderValue};

    #[test]
    fn generate_token_is_128_hex_chars() {
        let token = generate_token();
        assert_eq!(token.len(), TOKEN_BYTES * 2);
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn validate_bearer_accepts_correct_token() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_static("Bearer abc123"),
        );
        assert!(validate_bearer(&headers, "abc123"));
    }

    #[test]
    fn validate_bearer_rejects_wrong_token() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_static("Bearer wrong"),
        );
        assert!(!validate_bearer(&headers, "abc123"));
    }

    #[test]
    fn validate_bearer_rejects_missing_header() {
        let headers = HeaderMap::new();
        assert!(!validate_bearer(&headers, "abc123"));
    }

    #[test]
    fn validate_origin_allows_loopback() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::ORIGIN,
            HeaderValue::from_static("http://127.0.0.1:6767"),
        );
        assert!(validate_origin(&headers, &[]));
    }

    #[test]
    fn validate_origin_allows_localhost() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::HOST,
            HeaderValue::from_static("localhost:6767"),
        );
        assert!(validate_origin(&headers, &[]));
    }

    #[test]
    fn validate_origin_rejects_evil_origin() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::ORIGIN,
            HeaderValue::from_static("http://evil.com"),
        );
        assert!(!validate_origin(&headers, &[]));
    }

    #[test]
    fn validate_origin_rejects_evil_host() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::HOST,
            HeaderValue::from_static("evil-attacker.local:6767"),
        );
        assert!(!validate_origin(&headers, &[]));
    }

    #[test]
    fn validate_origin_allows_configured_additional() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::ORIGIN,
            HeaderValue::from_static("http://myhost.local:8080"),
        );
        let additional = vec!["myhost.local:8080".to_owned()];
        assert!(validate_origin(&headers, &additional));
    }

    #[test]
    fn validate_origin_allows_no_headers() {
        let headers = HeaderMap::new();
        assert!(validate_origin(&headers, &[]));
    }

    #[test]
    fn token_roundtrip_on_disk() {
        let dir = std::env::temp_dir().join("lesearch-auth-test");
        let _ = std::fs::remove_dir_all(&dir);

        let token = load_or_generate_token(&dir).unwrap();
        assert_eq!(token.len(), TOKEN_BYTES * 2);

        let loaded = read_token(&dir).unwrap();
        assert_eq!(token, loaded);

        // Second load should return same token
        let reloaded = load_or_generate_token(&dir).unwrap();
        assert_eq!(token, reloaded);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
