//! `LeSearch` daemon core.
//!
//! Hosts the agent manager, JSON-RPC router, `WebSocket` endpoint,
//! session writer, and read-only A2A agent card. Binds `127.0.0.1:6767`
//! by default.

#![doc(html_root_url = "https://docs.rs/lesearch-daemon/0.0.1")]

pub mod a2a;
pub mod agent_manager;
pub mod auth;
pub mod router;
pub mod session_writer;
pub mod ws;

use std::collections::HashMap;
use std::sync::Arc;

use lesearch_providers::AgentProvider;
use lesearch_storage::config::LesearchConfig;
use lesearch_storage::keyring::Keyring;
use lesearch_storage::registry::Registry;
use lesearch_storage::search::SearchIndex;
use tokio::sync::Mutex;

/// Shared mutex type used for cross-task access.
pub type SharedMutex<T> = Arc<Mutex<T>>;

/// Returns the compiled protocol version this daemon speaks.
#[must_use]
pub const fn protocol_version() -> &'static str {
    lesearch_protocol::version()
}

// ---------------------------------------------------------------------------
// Shared daemon state
// ---------------------------------------------------------------------------

/// Shared application state, held in an `Arc` and passed to all handlers.
pub struct DaemonState {
    /// Daemon configuration.
    pub config: LesearchConfig,
    /// Ed25519 keyring for session signing.
    pub keyring: Keyring,
    /// Agent/session metadata registry.
    pub registry: Mutex<Registry>,
    /// FTS5 search index (shared with session writer).
    pub search_index: SharedMutex<SearchIndex>,
    /// Registered providers keyed by name.
    pub providers: HashMap<String, Box<dyn AgentProvider>>,
    /// Agent manager (manages running agents).
    pub agent_manager: Mutex<agent_manager::AgentManager>,
    /// Per-session bearer token for WebSocket authentication.
    pub bearer_token: String,
}

impl std::fmt::Debug for DaemonState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DaemonState")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

/// Type alias for the shared state handle.
pub type SharedState = Arc<DaemonState>;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors produced by the daemon.
#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    /// Agent manager error.
    #[error("agent: {0}")]
    Agent(String),

    /// Provider error.
    #[error("provider: {0}")]
    Provider(#[from] lesearch_providers::ProviderError),

    /// Storage error.
    #[error("storage: {0}")]
    Storage(#[from] lesearch_storage::StorageError),

    /// Protocol error.
    #[error("protocol: {0}")]
    Protocol(String),

    /// Unknown provider requested.
    #[error("unknown provider: {0}")]
    UnknownProvider(String),

    /// Agent not found.
    #[error("agent not found: {0}")]
    AgentNotFound(String),
}
