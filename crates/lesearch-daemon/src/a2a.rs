//! Read-only A2A 1.0.0 agent card endpoint.
//!
//! Serves `GET /.well-known/agent.json` with the daemon's agent card.
//! v0.1.0 is read-only — no inbound `POST /message:send`.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;

use crate::SharedState;

/// A2A 1.0.0 agent card (subset of the spec for v0.1.0).
#[derive(Debug, Clone, Serialize)]
pub struct AgentCard {
    /// Agent name.
    pub name: String,
    /// Agent description.
    pub description: String,
    /// URL of this card.
    pub url: String,
    /// Protocol version.
    pub version: String,
    /// Capabilities list.
    pub capabilities: AgentCapabilities,
}

/// Supported capabilities advertised in the agent card.
#[derive(Debug, Clone, Serialize)]
pub struct AgentCapabilities {
    /// Whether the agent supports streaming.
    pub streaming: bool,
}

/// `GET /.well-known/agent.json` handler.
#[allow(clippy::unused_async)]
pub async fn agent_card_handler(
    State(state): State<SharedState>,
) -> impl IntoResponse {
    let bind = lesearch_protocol::DEFAULT_BIND;
    let card = AgentCard {
        name: "lesearch-daemon".into(),
        description: format!(
            "LeSearch agent control plane v{}",
            env!("CARGO_PKG_VERSION")
        ),
        url: format!("http://{bind}/.well-known/agent.json"),
        version: state.config.daemon.log_level.clone(),
        capabilities: AgentCapabilities { streaming: true },
    };

    (
        StatusCode::OK,
        [("A2A-Version", "1.0.0"), ("Content-Type", "application/json")],
        Json(card),
    )
}

/// `GET /health` handler.
#[allow(clippy::unused_async)]
pub async fn health_handler() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "ok",
            "version": env!("CARGO_PKG_VERSION"),
            "protocol": crate::protocol_version(),
        })),
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_card_serializes() {
        let card = AgentCard {
            name: "test".into(),
            description: "test daemon".into(),
            url: "http://127.0.0.1:6767/.well-known/agent.json".into(),
            version: "0.0.1".into(),
            capabilities: AgentCapabilities { streaming: true },
        };
        let json = serde_json::to_value(&card).unwrap();
        assert_eq!(json["name"], "test");
        assert_eq!(json["capabilities"]["streaming"], true);
    }
}
