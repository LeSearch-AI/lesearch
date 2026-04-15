//! Session management types for the `LeSearch` wire protocol.
//!
//! Covers session metadata, search parameters, export formats, and the
//! CloudEvents-envelope session event used in append-only JSONL logs.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::agent::AgentId;

// ---------------------------------------------------------------------------
// Identity
// ---------------------------------------------------------------------------

/// Unique identifier for a session (`UUIDv7`, time-ordered).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(pub Uuid);

impl SessionId {
    /// Generate a new time-ordered session identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ---------------------------------------------------------------------------
// Session status
// ---------------------------------------------------------------------------

/// Lifecycle status of a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SessionStatus {
    /// Session is active (agent running or idle).
    Active,
    /// Session completed normally.
    Completed,
    /// Session terminated due to an error.
    Failed,
    /// Session was stopped by the user.
    Stopped,
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Active => "active",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
        };
        f.write_str(s)
    }
}

// ---------------------------------------------------------------------------
// Session info (list response)
// ---------------------------------------------------------------------------

/// Summary metadata about a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    /// Session identifier.
    pub id: SessionId,
    /// Owning agent.
    pub agent_id: AgentId,
    /// Provider that ran this session.
    pub provider: String,
    /// Working directory.
    pub cwd: String,
    /// ISO 8601 start time.
    pub started_at: String,
    /// ISO 8601 stop time, if finished.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stopped_at: Option<String>,
    /// Session status.
    pub status: SessionStatus,
    /// Human-readable duration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<String>,
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

/// Parameters for `session.search`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchParams {
    /// Substring query (FTS5 in v0.1.0).
    pub query: String,
    /// Restrict to specific agents.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agent_filter: Vec<AgentId>,
    /// ISO 8601 time range `[start, end]`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_range: Option<(String, String)>,
    /// Maximum results (default 100, max 1000).
    #[serde(default = "default_search_limit")]
    pub limit: u32,
}

const fn default_search_limit() -> u32 {
    100
}

/// A single search hit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    /// Session that contained the match.
    pub session_id: SessionId,
    /// Agent that produced the event.
    pub agent_id: AgentId,
    /// ISO 8601 event timestamp.
    pub timestamp: String,
    /// Event type (e.g., `"lesearch.tool.call.requested.v1"`).
    pub event_type: String,
    /// Matching snippet or full event data.
    pub data: serde_json::Value,
}

/// Result of `session.search`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Matching events.
    pub hits: Vec<SearchHit>,
    /// Total count (may exceed `hits.len()` if truncated by limit).
    pub total: u64,
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

/// Output format for `session.export`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ExportFormat {
    /// Human-readable markdown.
    Markdown,
    /// Raw signed JSONL (preserves signatures).
    Jsonl,
}

impl std::fmt::Display for ExportFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Markdown => f.write_str("markdown"),
            Self::Jsonl => f.write_str("jsonl"),
        }
    }
}

/// Parameters for `session.export`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportParams {
    /// Session to export.
    pub session_id: SessionId,
    /// Desired output format.
    pub format: ExportFormat,
}

/// Parameters for `session.replay`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayParams {
    /// Session to replay.
    pub session_id: SessionId,
}

// ---------------------------------------------------------------------------
// CloudEvent session event
// ---------------------------------------------------------------------------

/// `CloudEvents` v1.0 envelope for a session log entry.
///
/// Each entry in a session JSONL file is one `SessionEvent`, Ed25519-signed
/// and hash-chained to the previous entry.
///
/// See: <https://cloudevents.io/> and `docs/SYSTEM_DESIGN.md` section 4.1.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEvent {
    /// `CloudEvents` spec version. Always `"1.0"`.
    pub specversion: String,

    /// Unique event identifier (`UUIDv7` string).
    pub id: String,

    /// Event source (e.g., `"lesearch://daemon/{daemon_id}"`).
    pub source: String,

    /// Event type (e.g., `"lesearch.session.started.v1"`).
    #[serde(rename = "type")]
    pub ty: String,

    /// Subject (e.g., `"agent:{agent_id}"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,

    /// ISO 8601 / RFC 3339 event timestamp.
    pub time: String,

    /// Content type of `data`. Always `"application/json"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub datacontenttype: Option<String>,

    /// Event payload (shape determined by `type`).
    pub data: serde_json::Value,

    // ---- LeSearch signing extensions (top-level per CloudEvents spec) ----

    /// `"sha256:{hex}"` — hash of the previous record's JCS-canonical bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prevhash: Option<String>,

    /// `"ed25519:{base64}"` — signature over JCS-canonical form of this event
    /// (with `signature` and `pubkey` fields removed before canonicalization).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,

    /// `"ed25519:{base64}"` — public key that produced the signature.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pubkey: Option<String>,
}

impl SessionEvent {
    /// `CloudEvents` spec version.
    pub const SPEC_VERSION: &str = "1.0";

    /// Create a new unsigned event with no hash chain link.
    ///
    /// Signing and hash-chaining are performed by the storage layer
    /// before the event is appended to the session log.
    #[must_use]
    pub fn new(
        source: impl Into<String>,
        ty: impl Into<String>,
        subject: impl Into<String>,
        data: serde_json::Value,
    ) -> Self {
        Self {
            specversion: Self::SPEC_VERSION.into(),
            id: Uuid::now_v7().to_string(),
            source: source.into(),
            ty: ty.into(),
            subject: Some(subject.into()),
            time: time::OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
            datacontenttype: Some("application/json".into()),
            data,
            prevhash: None,
            signature: None,
            pubkey: None,
        }
    }
}

/// Well-known session event type strings.
pub mod event_type {
    /// Agent session has started.
    pub const SESSION_STARTED: &str = "lesearch.session.started.v1";
    /// User submitted a prompt.
    pub const PROMPT_SUBMITTED: &str = "lesearch.prompt.submitted.v1";
    /// Raw output stream chunk.
    pub const STREAM_CHUNK: &str = "lesearch.stream.chunk.v1";
    /// Agent requested a tool call.
    pub const TOOL_CALL_REQUESTED: &str = "lesearch.tool.call.requested.v1";
    /// Policy decision on a tool call.
    pub const TOOL_CALL_DECIDED: &str = "lesearch.tool.call.decided.v1";
    /// Tool call completed with a result.
    pub const TOOL_CALL_RESULT: &str = "lesearch.tool.call.result.v1";
    /// Agent requested permission from the user.
    pub const PERMISSION_REQUESTED: &str = "lesearch.permission.requested.v1";
    /// User decided on a permission request.
    pub const PERMISSION_DECIDED: &str = "lesearch.permission.decided.v1";
    /// Agent session has stopped.
    pub const SESSION_STOPPED: &str = "lesearch.session.stopped.v1";
}

// ---------------------------------------------------------------------------
// Event data payloads (convenience structs)
// ---------------------------------------------------------------------------

/// Data payload for [`event_type::SESSION_STARTED`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStartedData {
    /// Provider name.
    pub provider: String,
    /// Initial prompt.
    pub prompt: String,
    /// Working directory.
    pub cwd: String,
}

/// Data payload for [`event_type::STREAM_CHUNK`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunkData {
    /// Output content (UTF-8 text).
    pub content: String,
    /// Stream origin: `"stdout"` or `"stderr"`.
    #[serde(default = "default_stream")]
    pub stream: String,
}

fn default_stream() -> String {
    "stdout".into()
}

/// Data payload for [`event_type::TOOL_CALL_REQUESTED`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRequestedData {
    /// Tool name.
    pub tool_name: String,
    /// Tool arguments.
    pub arguments: serde_json::Value,
}

/// Data payload for [`event_type::TOOL_CALL_DECIDED`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallDecidedData {
    /// Tool name.
    pub tool_name: String,
    /// Policy decision.
    pub decision: crate::agent::ToolCallDecision,
    /// Reason for the decision.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Data payload for [`event_type::SESSION_STOPPED`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStoppedData {
    /// Process exit code.
    pub exit_code: i32,
    /// Reason for stopping.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_id_roundtrip() {
        let id = SessionId::new();
        let json = serde_json::to_string(&id).unwrap();
        let parsed: SessionId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn session_event_new_has_required_fields() {
        let evt = SessionEvent::new(
            "lesearch://daemon/test",
            event_type::SESSION_STARTED,
            "agent:test-agent",
            serde_json::json!({"provider": "claude", "prompt": "hi", "cwd": "/tmp"}),
        );
        assert_eq!(evt.specversion, "1.0");
        assert_eq!(evt.ty, event_type::SESSION_STARTED);
        assert!(evt.signature.is_none());
        assert!(evt.prevhash.is_none());
    }

    #[test]
    fn session_event_serializes_type_as_type() {
        let evt = SessionEvent::new(
            "src",
            "test.v1",
            "sub",
            serde_json::Value::Null,
        );
        let json = serde_json::to_value(&evt).unwrap();
        // The field should be "type" not "ty" on the wire
        assert!(json.get("type").is_some());
        assert!(json.get("ty").is_none());
    }

    #[test]
    fn export_format_roundtrip() {
        let f = ExportFormat::Markdown;
        let json = serde_json::to_string(&f).unwrap();
        assert_eq!(json, r#""markdown""#);
        let parsed: ExportFormat = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, f);
    }

    #[test]
    fn search_params_defaults() {
        let json = r#"{"query": "TODO"}"#;
        let params: SearchParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.limit, 100);
        assert!(params.agent_filter.is_empty());
    }
}
