//! `LeSearch` wire protocol.
//!
//! JSON-RPC 2.0 request/response + notifications over a single `WebSocket`
//! connection. Binary frames are reserved for PTY streams; text frames carry
//! JSON-RPC envelopes.
//!
//! # Modules
//!
//! - [`jsonrpc`] — Generic JSON-RPC 2.0 envelope types, error codes, method names.
//! - [`agent`]   — Agent identity, state machine, spawn/stop params, notification events.
//! - [`session`] — Session metadata, search, export, `CloudEvent` session log types.
//! - [`mux`]     — Binary multiplexing framing for PTY data over WebSocket.
//!
//! See `docs/protocol-v0.1.md` in the workspace root for the canonical spec.

#![doc(html_root_url = "https://docs.rs/lesearch-protocol/0.0.1")]

pub mod agent;
pub mod jsonrpc;
pub mod mux;
pub mod session;

/// Protocol version exposed via `server.handshake`. Clients >= 6 months old
/// must keep working; bump the minor version for additive changes only.
pub const PROTOCOL_VERSION: &str = "0.1.0";

/// Default loopback bind address for the daemon.
pub const DEFAULT_BIND: &str = "127.0.0.1:6767";

/// Returns the protocol version string.
#[must_use]
pub const fn version() -> &'static str {
    PROTOCOL_VERSION
}

// ---------------------------------------------------------------------------
// Crate-level error type
// ---------------------------------------------------------------------------

/// Errors arising from protocol parsing or encoding.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ProtocolError {
    /// Incoming text is not a recognizable JSON-RPC 2.0 message.
    #[error("invalid JSON-RPC message: {0}")]
    InvalidMessage(String),

    /// Binary mux frame is shorter than the required header.
    #[error("mux frame too short: expected >= {expected} bytes, got {actual}")]
    MuxFrameTooShort {
        /// Minimum expected size.
        expected: usize,
        /// Actual size received.
        actual: usize,
    },

    /// Binary mux version field is not supported.
    #[error("unsupported mux version: {0}")]
    MuxVersionUnsupported(u16),

    /// JSON serialization / deserialization failure.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_semver_prefix() {
        assert!(version().starts_with("0."));
    }

    #[test]
    fn protocol_error_display() {
        let err = ProtocolError::MuxFrameTooShort {
            expected: 8,
            actual: 3,
        };
        assert!(err.to_string().contains("too short"));
    }
}
