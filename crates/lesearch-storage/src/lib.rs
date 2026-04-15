//! `LeSearch` storage layer.
//!
//! Provides configuration, cryptographic signing, metadata registry,
//! append-only session logs, and full-text search over session events.

#![doc(html_root_url = "https://docs.rs/lesearch-storage/0.0.1")]

pub mod config;
pub mod keyring;
pub mod registry;
pub mod search;
pub mod session_log;

/// Default filesystem layout root under `$HOME`.
pub const DEFAULT_HOME: &str = ".lesearch";

// TODO(FR-10): Extract `StorageBackend` trait so registry/session_log/search
//   can be swapped (e.g., AgentFS backend behind feature flag). Defer to
//   follow-up PR before release-gate cert.
// TODO(FR-39): Add signed segment manifests at log rotation boundaries.
//   Per-line hash chain is implemented; segment manifests (start_hash,
//   end_hash, event range, Ed25519-signed) are not yet emitted.

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors produced by the storage layer.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// Filesystem I/O failure.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    /// `SQLite` error (registry or FTS5 index).
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// JSON serialization / deserialization failure.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),

    /// Signing or canonicalization error.
    #[error("signing: {0}")]
    Signing(String),

    /// Signature or hash-chain verification failed.
    #[error("verification failed: {0}")]
    VerificationFailed(String),

    /// Configuration file error.
    #[error("config: {0}")]
    Config(String),

    /// Requested entity not found.
    #[error("not found: {0}")]
    NotFound(String),
}
