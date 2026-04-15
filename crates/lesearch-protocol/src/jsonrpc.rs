//! JSON-RPC 2.0 message types for the `LeSearch` wire protocol.
//!
//! All WebSocket text frames carry a single JSON-RPC 2.0 object. This module
//! provides the envelope types, error codes, and method-name constants.

use serde::{Deserialize, Serialize};

/// JSON-RPC 2.0 version string.
pub const VERSION: &str = "2.0";

/// Standard and LeSearch-specific JSON-RPC error codes.
pub mod error_code {
    // ---- standard JSON-RPC 2.0 ----

    /// Malformed JSON received by the server.
    pub const PARSE_ERROR: i32 = -32700;
    /// The JSON sent is not a valid JSON-RPC 2.0 request.
    pub const INVALID_REQUEST: i32 = -32600;
    /// The method does not exist or is not available.
    pub const METHOD_NOT_FOUND: i32 = -32601;
    /// Invalid method parameters.
    pub const INVALID_PARAMS: i32 = -32602;
    /// Internal JSON-RPC error.
    pub const INTERNAL_ERROR: i32 = -32603;

    // ---- LeSearch extensions ----

    /// Missing or invalid bearer token.
    pub const UNAUTHENTICATED: i32 = -32000;
    /// Valid token but disallowed method.
    pub const UNAUTHORIZED: i32 = -32001;
    /// Provider binary not installed on the host.
    pub const PROVIDER_MISSING: i32 = -32002;
    /// Unknown `agent_id`.
    pub const AGENT_NOT_FOUND: i32 = -32003;
    /// Method called in an incompatible agent state.
    pub const AGENT_STATE_INVALID: i32 = -32004;
    /// AVM rejected the tool call.
    pub const POLICY_DENIED: i32 = -32005;
    /// Storage or session-log I/O failure.
    pub const STORAGE_ERROR: i32 = -32006;
    /// `max_concurrent_agents` or resource ceiling reached.
    pub const RESOURCE_LIMIT: i32 = -32007;
    /// Connection aborting due to transport closure.
    pub const TRANSPORT_CLOSED: i32 = -32008;
}

/// Wire-level method names used in requests and notifications.
pub mod method {
    /// Client → server handshake.
    pub const HANDSHAKE: &str = "server.handshake";

    /// Spawn a new agent.
    pub const AGENT_SPAWN: &str = "agent.spawn";
    /// Send text input to an agent's stdin.
    pub const AGENT_INPUT: &str = "agent.input";
    /// Request graceful agent shutdown.
    pub const AGENT_STOP: &str = "agent.stop";
    /// List running and recent agents.
    pub const AGENT_LIST: &str = "agent.list";

    /// Server → client: raw output chunk (fallback when binary-mux unavailable).
    pub const AGENT_OUTPUT: &str = "agent.output";
    /// Server → client: agent state transition.
    pub const AGENT_STATUS: &str = "agent.status";
    /// Server → client: tool call event.
    pub const AGENT_TOOL_CALL: &str = "agent.tool_call";
    /// Server → client: agent finished.
    pub const AGENT_DONE: &str = "agent.done";

    /// List sessions.
    pub const SESSION_LIST: &str = "session.list";
    /// Search across sessions.
    pub const SESSION_SEARCH: &str = "session.search";
    /// Replay a past session into a new agent.
    pub const SESSION_REPLAY: &str = "session.replay";
    /// Export a session as markdown / JSONL.
    pub const SESSION_EXPORT: &str = "session.export";
}

// ---------------------------------------------------------------------------
// Envelope types
// ---------------------------------------------------------------------------

/// A JSON-RPC 2.0 request identifier (string or integer).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    /// String identifier.
    String(String),
    /// Numeric identifier.
    Number(i64),
}

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::String(s) => f.write_str(s),
            Self::Number(n) => write!(f, "{n}"),
        }
    }
}

/// A JSON-RPC 2.0 request (client → server).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    /// Must be `"2.0"`.
    pub jsonrpc: String,
    /// Request identifier for correlating the response.
    pub id: RequestId,
    /// Dotted method name (e.g., `"agent.spawn"`).
    pub method: String,
    /// Method parameters (always an object).
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub params: serde_json::Value,
}

/// A JSON-RPC 2.0 success response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    /// Must be `"2.0"`.
    pub jsonrpc: String,
    /// Matches the request id.
    pub id: RequestId,
    /// Result payload.
    pub result: serde_json::Value,
}

/// A JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    /// Numeric error code (see [`error_code`]).
    pub code: i32,
    /// Human-readable error message.
    pub message: String,
    /// Optional structured data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// A JSON-RPC 2.0 error response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// Must be `"2.0"`.
    pub jsonrpc: String,
    /// Matches the request id, or `None` for parse errors.
    pub id: Option<RequestId>,
    /// Error object.
    pub error: RpcError,
}

/// A JSON-RPC 2.0 notification (server → client, no `id`, no response).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    /// Must be `"2.0"`.
    pub jsonrpc: String,
    /// Method name.
    pub method: String,
    /// Notification parameters.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub params: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Message discriminator
// ---------------------------------------------------------------------------

/// Any incoming JSON-RPC 2.0 message, discriminated by field presence.
#[derive(Debug, Clone)]
pub enum Message {
    /// A request (has `id` + `method`).
    Request(Request),
    /// A success response (has `id` + `result`).
    Response(Response),
    /// An error response (has `id` + `error`).
    Error(ErrorResponse),
    /// A notification (has `method`, no `id`).
    Notification(Notification),
}

impl Message {
    /// Parse a JSON string into the appropriate message variant.
    ///
    /// Discrimination logic (per JSON-RPC 2.0 spec):
    /// - `method` + `id` → [`Request`]
    /// - `result` + `id` → [`Response`]
    /// - `error`  + `id` → [`ErrorResponse`]
    /// - `method` only   → [`Notification`]
    pub fn parse(text: &str) -> Result<Self, crate::ProtocolError> {
        let v: serde_json::Value =
            serde_json::from_str(text).map_err(crate::ProtocolError::Json)?;

        let has_method = v.get("method").is_some();
        let has_id = v.get("id").is_some();
        let has_result = v.get("result").is_some();
        let has_error = v.get("error").is_some();

        if has_method && has_id {
            let req = serde_json::from_value(v).map_err(crate::ProtocolError::Json)?;
            Ok(Self::Request(req))
        } else if has_result && has_id {
            let resp = serde_json::from_value(v).map_err(crate::ProtocolError::Json)?;
            Ok(Self::Response(resp))
        } else if has_error {
            let err = serde_json::from_value(v).map_err(crate::ProtocolError::Json)?;
            Ok(Self::Error(err))
        } else if has_method {
            let notif = serde_json::from_value(v).map_err(crate::ProtocolError::Json)?;
            Ok(Self::Notification(notif))
        } else {
            Err(crate::ProtocolError::InvalidMessage(
                "missing 'method' or 'result'/'error' field".into(),
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// Builder helpers
// ---------------------------------------------------------------------------

impl Request {
    /// Create a new request.
    #[must_use]
    pub fn new(id: RequestId, method: impl Into<String>, params: serde_json::Value) -> Self {
        Self {
            jsonrpc: VERSION.into(),
            id,
            method: method.into(),
            params,
        }
    }
}

impl Response {
    /// Create a success response.
    #[must_use]
    pub fn ok(id: RequestId, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: VERSION.into(),
            id,
            result,
        }
    }
}

impl ErrorResponse {
    /// Create an error response.
    #[must_use]
    pub fn err(id: Option<RequestId>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: VERSION.into(),
            id,
            error: RpcError {
                code,
                message: message.into(),
                data: None,
            },
        }
    }
}

impl Notification {
    /// Create a notification.
    #[must_use]
    pub fn new(method: impl Into<String>, params: serde_json::Value) -> Self {
        Self {
            jsonrpc: VERSION.into(),
            method: method.into(),
            params,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_request() {
        let req = Request::new(
            RequestId::Number(1),
            "agent.spawn",
            serde_json::json!({"provider": "claude"}),
        );
        let json = serde_json::to_string(&req).unwrap();
        let parsed = Message::parse(&json).unwrap();
        assert!(matches!(parsed, Message::Request(r) if r.method == "agent.spawn"));
    }

    #[test]
    fn round_trip_response() {
        let resp = Response::ok(RequestId::String("abc".into()), serde_json::json!({"ok": true}));
        let json = serde_json::to_string(&resp).unwrap();
        let parsed = Message::parse(&json).unwrap();
        assert!(matches!(parsed, Message::Response(_)));
    }

    #[test]
    fn round_trip_error() {
        let err = ErrorResponse::err(
            Some(RequestId::Number(2)),
            error_code::AGENT_NOT_FOUND,
            "no such agent",
        );
        let json = serde_json::to_string(&err).unwrap();
        let parsed = Message::parse(&json).unwrap();
        assert!(matches!(parsed, Message::Error(e) if e.error.code == error_code::AGENT_NOT_FOUND));
    }

    #[test]
    fn round_trip_notification() {
        let notif = Notification::new("agent.status", serde_json::json!({"state": "running"}));
        let json = serde_json::to_string(&notif).unwrap();
        let parsed = Message::parse(&json).unwrap();
        assert!(matches!(parsed, Message::Notification(n) if n.method == "agent.status"));
    }

    #[test]
    fn parse_garbage_returns_error() {
        assert!(Message::parse("not json").is_err());
    }

    #[test]
    fn parse_empty_object_returns_error() {
        assert!(Message::parse(r#"{"jsonrpc":"2.0"}"#).is_err());
    }

    #[test]
    fn request_id_display() {
        assert_eq!(RequestId::Number(42).to_string(), "42");
        assert_eq!(RequestId::String("abc".into()).to_string(), "abc");
    }
}
