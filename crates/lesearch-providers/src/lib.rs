//! `LeSearch` agent provider adapters.
//!
//! Each provider wraps an upstream CLI coding agent (Claude Code, Codex, etc.)
//! via piped stdio with structured JSON, exposing a uniform [`AgentProvider`]
//! trait to the daemon. v0.1.0 ships two providers: `claude` and `codex`.

#![doc(html_root_url = "https://docs.rs/lesearch-providers/0.0.1")]

pub mod claude;
pub mod codex;

use std::process::ExitStatus;

use tokio::io::AsyncWriteExt;
use tokio::process::ChildStdin;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// Built-in provider identifiers.
pub const BUILTIN_PROVIDERS: &[&str] = &["claude", "codex"];

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors produced by the provider layer.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    /// Provider binary not found on `$PATH`.
    #[error("provider binary not found: {0}")]
    BinaryNotFound(String),

    /// Failed to spawn the child process.
    #[error("spawn failed: {0}")]
    SpawnFailed(String),

    /// I/O error communicating with the child process.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    /// Failed to parse structured output from the provider.
    #[error("parse error: {0}")]
    Parse(String),

    /// Provider exited unexpectedly.
    #[error("provider exited: {0}")]
    Exited(String),
}

// ---------------------------------------------------------------------------
// Provider manifest
// ---------------------------------------------------------------------------

/// Static metadata about a provider.
#[derive(Debug, Clone)]
pub struct ProviderManifest {
    /// Provider identifier (e.g., `"claude"`, `"codex"`).
    pub name: &'static str,
    /// Human-readable display name.
    pub display_name: &'static str,
    /// Binary name to look up on `$PATH`.
    pub binary: &'static str,
    /// Whether the provider exposes structured tool-call events
    /// (enables strict policy enforcement vs audit-only).
    pub enforcement_mode: EnforcementMode,
}

/// Policy enforcement capability of a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnforcementMode {
    /// Provider emits pre-execution tool events; daemon can intercept.
    Strict,
    /// Provider does not reliably surface tool events; audit-only logging.
    AuditOnly,
}

// ---------------------------------------------------------------------------
// Agent events (provider → daemon)
// ---------------------------------------------------------------------------

/// Events emitted by a running agent, received by the daemon via [`AgentHandle::events`].
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// Streaming text output from the agent.
    StreamChunk {
        /// The text content.
        content: String,
    },

    /// Agent is requesting a tool call.
    ToolCallRequested {
        /// Tool name (e.g., `"Bash"`, `"Read"`, `"Write"`).
        tool_name: String,
        /// Tool arguments (provider-specific shape).
        arguments: serde_json::Value,
        /// Provider-specific tool call id for response routing.
        call_id: String,
    },

    /// Agent finished processing (may still be alive for follow-up).
    TurnComplete {
        /// Optional summary of work performed.
        summary: Option<String>,
    },

    /// Unstructured or provider-specific event.
    Raw {
        /// Event type string from the provider.
        event_type: String,
        /// Raw JSON payload.
        data: serde_json::Value,
    },

    /// Provider process exited.
    Exited {
        /// Process exit status.
        status: ExitStatus,
    },

    /// Error reading from the provider.
    Error {
        /// Error description.
        message: String,
    },
}

// ---------------------------------------------------------------------------
// Agent handle (returned by spawn)
// ---------------------------------------------------------------------------

/// Handle to a running agent subprocess.
///
/// The daemon uses this to send input, receive events, and await termination.
pub struct AgentHandle {
    /// Writable end of the agent's stdin pipe.
    stdin: ChildStdin,
    /// Receives structured events from the agent.
    pub events: mpsc::Receiver<AgentEvent>,
    /// Background task reading from the agent's stdout.
    reader_task: JoinHandle<()>,
}

impl std::fmt::Debug for AgentHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentHandle").finish_non_exhaustive()
    }
}

impl AgentHandle {
    /// Create a new agent handle.
    #[must_use]
    pub const fn new(
        stdin: ChildStdin,
        events: mpsc::Receiver<AgentEvent>,
        reader_task: JoinHandle<()>,
    ) -> Self {
        Self {
            stdin,
            events,
            reader_task,
        }
    }

    /// Send text input to the agent's stdin.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Io`] if the pipe is broken.
    pub async fn send_input(&mut self, text: &str) -> Result<(), ProviderError> {
        self.stdin.write_all(text.as_bytes()).await?;
        self.stdin.flush().await?;
        Ok(())
    }

    /// Abort the background reader task.
    pub fn abort(&self) {
        self.reader_task.abort();
    }
}

// ---------------------------------------------------------------------------
// Agent spawn spec
// ---------------------------------------------------------------------------

/// Parameters for spawning an agent, passed from daemon to provider.
#[derive(Debug, Clone)]
pub struct AgentSpec {
    /// Initial prompt text.
    pub prompt: String,
    /// Working directory for the agent process.
    pub cwd: std::path::PathBuf,
    /// Optional model identifier (provider-specific).
    pub model: Option<String>,
    /// Optional mode (e.g., `"plan"`, `"default"`).
    pub mode: Option<String>,
}

// ---------------------------------------------------------------------------
// Provider trait
// ---------------------------------------------------------------------------

/// Uniform interface for CLI agent providers.
///
/// Each provider implementation knows how to spawn its agent binary,
/// parse its structured output, and translate events into [`AgentEvent`]s.
#[async_trait::async_trait]
pub trait AgentProvider: Send + Sync + 'static {
    /// Static metadata about this provider.
    fn manifest(&self) -> &ProviderManifest;

    /// Spawn a new agent subprocess.
    ///
    /// Returns an [`AgentHandle`] with stdin writer, event receiver,
    /// and background reader task.
    async fn spawn(&self, spec: AgentSpec) -> Result<AgentHandle, ProviderError>;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_providers_list() {
        assert!(BUILTIN_PROVIDERS.contains(&"claude"));
        assert!(BUILTIN_PROVIDERS.contains(&"codex"));
        assert_eq!(BUILTIN_PROVIDERS.len(), 2);
    }

    #[test]
    fn enforcement_mode_equality() {
        assert_eq!(EnforcementMode::Strict, EnforcementMode::Strict);
        assert_ne!(EnforcementMode::Strict, EnforcementMode::AuditOnly);
    }
}
