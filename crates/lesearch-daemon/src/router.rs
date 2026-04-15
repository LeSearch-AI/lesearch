//! JSON-RPC message router.
//!
//! Dispatches incoming JSON-RPC requests to the agent manager and
//! returns responses. Used by the `WebSocket` handler.

use lesearch_protocol::jsonrpc::{self, Message};
use serde_json::Value;

use crate::{DaemonError, SharedState};

/// Route a single JSON-RPC message and return an optional response.
///
/// Returns `None` for notifications (no response expected).
pub async fn route_message(state: &SharedState, raw: &str) -> Option<String> {
    let Ok(msg) = Message::parse(raw) else {
        let resp = jsonrpc::ErrorResponse::err(
            None,
            lesearch_protocol::jsonrpc::error_code::PARSE_ERROR,
            "parse error",
        );
        return Some(serde_json::to_string(&resp).unwrap_or_default());
    };

    match msg {
        Message::Request(req) => {
            let result = handle_request(state, &req.method, &req.params).await;
            match result {
                Ok(value) => {
                    let resp = jsonrpc::Response::ok(req.id, value);
                    Some(serde_json::to_string(&resp).unwrap_or_default())
                }
                Err(e) => {
                    let resp = jsonrpc::ErrorResponse::err(
                        Some(req.id),
                        lesearch_protocol::jsonrpc::error_code::INTERNAL_ERROR,
                        e.to_string(),
                    );
                    Some(serde_json::to_string(&resp).unwrap_or_default())
                }
            }
        }
        Message::Notification(_) | Message::Response(_) | Message::Error(_) => None,
    }
}

/// Handle a JSON-RPC request by method name.
async fn handle_request(
    state: &SharedState,
    method: &str,
    params: &Value,
) -> Result<Value, DaemonError> {
    match method {
        lesearch_protocol::jsonrpc::method::HANDSHAKE => handle_handshake(),
        lesearch_protocol::jsonrpc::method::AGENT_SPAWN => {
            handle_agent_spawn(state, params).await
        }
        lesearch_protocol::jsonrpc::method::AGENT_INPUT => {
            handle_agent_input(state, params).await
        }
        lesearch_protocol::jsonrpc::method::AGENT_STOP => {
            handle_agent_stop(state, params).await
        }
        lesearch_protocol::jsonrpc::method::AGENT_LIST => handle_agent_list(state).await,
        lesearch_protocol::jsonrpc::method::SESSION_SEARCH => {
            handle_session_search(state, params).await
        }
        _ => Err(DaemonError::Protocol(format!("unknown method: {method}"))),
    }
}

fn handle_handshake() -> Result<Value, DaemonError> {
    let result = lesearch_protocol::agent::HandshakeResult {
        protocol_version: crate::protocol_version().to_owned(),
        daemon_version: env!("CARGO_PKG_VERSION").to_owned(),
        server_capabilities: vec!["binary-mux".into(), "session-search".into()],
        agent_id: None,
    };
    serde_json::to_value(result).map_err(|e| DaemonError::Protocol(e.to_string()))
}

async fn handle_agent_spawn(state: &SharedState, params: &Value) -> Result<Value, DaemonError> {
    let spawn_params: lesearch_protocol::agent::SpawnParams =
        serde_json::from_value(params.clone())
            .map_err(|e| DaemonError::Protocol(format!("invalid spawn params: {e}")))?;

    let provider = state
        .providers
        .get(&spawn_params.provider)
        .ok_or_else(|| DaemonError::UnknownProvider(spawn_params.provider.clone()))?;

    let cwd = spawn_params
        .cwd.map_or_else(|| std::env::current_dir().unwrap_or_else(|_| "/tmp".into()), std::path::PathBuf::from);

    let agent_id = state
        .agent_manager
        .lock()
        .await
        .spawn(
            provider.as_ref(),
            spawn_params.prompt,
            cwd,
            spawn_params.model,
            spawn_params.mode,
        )
        .await?;

    Ok(serde_json::json!({ "agent_id": agent_id.to_string() }))
}

async fn handle_agent_input(state: &SharedState, params: &Value) -> Result<Value, DaemonError> {
    let input_params: lesearch_protocol::agent::InputParams =
        serde_json::from_value(params.clone())
            .map_err(|e| DaemonError::Protocol(format!("invalid input params: {e}")))?;

    state
        .agent_manager
        .lock()
        .await
        .send_input(&input_params.agent_id.to_string(), &input_params.text)
        .await?;

    Ok(serde_json::json!({"ok": true}))
}

async fn handle_agent_stop(state: &SharedState, params: &Value) -> Result<Value, DaemonError> {
    let stop_params: lesearch_protocol::agent::StopParams =
        serde_json::from_value(params.clone())
            .map_err(|e| DaemonError::Protocol(format!("invalid stop params: {e}")))?;

    state
        .agent_manager
        .lock()
        .await
        .stop(&stop_params.agent_id.to_string())?;

    Ok(serde_json::json!({"ok": true}))
}

async fn handle_session_search(
    state: &SharedState,
    params: &Value,
) -> Result<Value, DaemonError> {
    let query = params
        .get("query")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");

    let limit = params
        .get("limit")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(50);

    #[allow(clippy::cast_possible_truncation)]
    let hits = state
        .search_index
        .lock()
        .await
        .search(query, limit as u32)?;

    let results: Vec<Value> = hits
        .iter()
        .map(|h| {
            serde_json::json!({
                "session_id": h.session_id,
                "agent_id": h.agent_id,
                "event_type": h.event_type,
                "timestamp": h.timestamp,
                "data": h.data,
            })
        })
        .collect();

    Ok(serde_json::json!({ "hits": results }))
}

async fn handle_agent_list(state: &SharedState) -> Result<Value, DaemonError> {
    let agents = state.agent_manager.lock().await.list();
    serde_json::to_value(agents).map_err(|e| DaemonError::Protocol(e.to_string()))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    #[test]
    fn method_constants_exist() {
        assert!(!lesearch_protocol::jsonrpc::method::HANDSHAKE.is_empty());
        assert!(!lesearch_protocol::jsonrpc::method::AGENT_SPAWN.is_empty());
        assert!(!lesearch_protocol::jsonrpc::method::AGENT_LIST.is_empty());
    }
}
