//! Agent lifecycle types for the `LeSearch` wire protocol.
//!
//! Defines agent identity, state machine, spawn parameters, and the event
//! types emitted by the daemon as server → client notifications.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Identity
// ---------------------------------------------------------------------------

/// Unique identifier for an agent instance (`UUIDv7`, time-ordered).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentId(pub Uuid);

impl AgentId {
    /// Generate a new time-ordered agent identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl Default for AgentId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ---------------------------------------------------------------------------
// State machine
// ---------------------------------------------------------------------------

/// Agent lifecycle state.
///
/// ```text
/// initializing → idle → running → idle → closed
///                  ↓        ↓                ↓
///                error    error            error
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AgentState {
    /// Provider subprocess is starting.
    Initializing,
    /// Ready for input; not currently processing.
    Idle,
    /// Actively processing a prompt.
    Running,
    /// Terminated normally.
    Closed,
    /// Terminated due to an error.
    Error,
}

impl std::fmt::Display for AgentState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Initializing => "initializing",
            Self::Idle => "idle",
            Self::Running => "running",
            Self::Closed => "closed",
            Self::Error => "error",
        };
        f.write_str(s)
    }
}

// ---------------------------------------------------------------------------
// Handshake
// ---------------------------------------------------------------------------

/// Parameters for `server.handshake` (client → server).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeParams {
    /// Human-readable client name (e.g., `"lesearch-cli"`).
    pub client_name: String,
    /// Client software version.
    pub client_version: String,
    /// Capabilities the client supports (e.g., `["binary-mux"]`).
    #[serde(default)]
    pub client_capabilities: Vec<String>,
    /// Optional bearer token for authenticated sessions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bearer_token: Option<String>,
}

/// Result of `server.handshake`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeResult {
    /// Wire protocol version (semver, e.g., `"0.1.0"`).
    pub protocol_version: String,
    /// Daemon software version.
    pub daemon_version: String,
    /// Capabilities the server supports.
    pub server_capabilities: Vec<String>,
    /// The daemon's own A2A agent identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Spawn
// ---------------------------------------------------------------------------

/// Parameters for `agent.spawn`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnParams {
    /// Provider identifier (e.g., `"claude"`, `"codex"`).
    pub provider: String,
    /// Initial prompt text.
    pub prompt: String,
    /// Working directory (must resolve on the host).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Optional git worktree path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree: Option<String>,
    /// Provider-specific model identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Provider-specific mode (e.g., `"plan"`, `"default"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// Named AVM policy profile.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_profile: Option<String>,
}

/// Binary-mux stream identifiers assigned at spawn time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamIds {
    /// Stream id for stdout.
    pub stdout: u16,
    /// Stream id for stdin.
    pub stdin: u16,
    /// Stream id for stderr.
    pub stderr: u16,
}

/// Result of `agent.spawn`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnResult {
    /// Newly created agent identifier.
    pub agent_id: AgentId,
    /// A2A agent card URL for this agent.
    pub a2a_card_url: String,
    /// Session identifier for the new run.
    pub session_id: String,
    /// Binary-mux stream identifiers.
    pub streams: StreamIds,
}

// ---------------------------------------------------------------------------
// Input / Stop
// ---------------------------------------------------------------------------

/// Parameters for `agent.input`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputParams {
    /// Target agent.
    pub agent_id: AgentId,
    /// Text to send to the agent's stdin.
    pub text: String,
}

/// Parameters for `agent.stop`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopParams {
    /// Target agent.
    pub agent_id: AgentId,
}

// ---------------------------------------------------------------------------
// Agent info (list response)
// ---------------------------------------------------------------------------

/// Summary information about an agent, returned by `agent.spawn` result and
/// list operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    /// Agent identifier.
    pub id: AgentId,
    /// Provider name.
    pub provider: String,
    /// Current lifecycle state.
    pub status: AgentState,
    /// Working directory.
    pub cwd: String,
    /// ISO 8601 timestamp when the agent was created.
    pub started: String,
    /// Human-readable runtime duration (e.g., `"2m 30s"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime: Option<String>,
}

// ---------------------------------------------------------------------------
// Notification event types (server → client)
// ---------------------------------------------------------------------------

/// Payload for `agent.output` notification (fallback text mode).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentOutputEvent {
    /// Agent identifier.
    pub agent_id: AgentId,
    /// Raw output bytes, base64-encoded.
    pub data: String,
    /// Stream origin: `"stdout"` or `"stderr"`.
    pub stream: String,
}

/// Payload for `agent.status` notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatusEvent {
    /// Agent identifier.
    pub agent_id: AgentId,
    /// New state.
    pub state: AgentState,
    /// ISO 8601 timestamp.
    pub timestamp: String,
}

/// A tool call made by an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Tool name (e.g., `"Bash"`, `"Read"`, `"Write"`).
    pub tool_name: String,
    /// Tool arguments (provider-specific shape).
    pub arguments: serde_json::Value,
}

/// Policy decision for a tool call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ToolCallDecision {
    /// Tool call is permitted.
    Allow,
    /// Tool call is blocked by policy.
    Deny,
    /// Tool call is permitted but logged for audit.
    AuditOnly,
}

/// Payload for `agent.tool_call` notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentToolCallEvent {
    /// Agent identifier.
    pub agent_id: AgentId,
    /// The tool call.
    pub tool_call: ToolCall,
    /// Policy decision (present when AVM is active).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<ToolCallDecision>,
    /// Reason for deny, if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Payload for `agent.done` notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDoneEvent {
    /// Agent identifier.
    pub agent_id: AgentId,
    /// Process exit code.
    pub exit_code: i32,
    /// Optional summary of work performed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_id_roundtrip() {
        let id = AgentId::new();
        let json = serde_json::to_string(&id).unwrap();
        let parsed: AgentId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn agent_state_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&AgentState::Initializing).unwrap(),
            r#""initializing""#
        );
        assert_eq!(
            serde_json::to_string(&AgentState::Running).unwrap(),
            r#""running""#
        );
    }

    #[test]
    fn spawn_params_minimal() {
        let params = SpawnParams {
            provider: "claude".into(),
            prompt: "hello".into(),
            cwd: None,
            worktree: None,
            model: None,
            mode: None,
            policy_profile: None,
        };
        let json = serde_json::to_value(&params).unwrap();
        assert_eq!(json["provider"], "claude");
        // Optional fields should be absent
        assert!(json.get("cwd").is_none());
    }

    #[test]
    fn tool_call_decision_roundtrip() {
        let d = ToolCallDecision::Deny;
        let json = serde_json::to_string(&d).unwrap();
        assert_eq!(json, r#""deny""#);
        let parsed: ToolCallDecision = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, d);
    }
}
