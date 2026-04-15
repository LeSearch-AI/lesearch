//! `WebSocket` handler for the daemon.
//!
//! Upgrades HTTP connections to `WebSocket`, reads JSON-RPC text frames,
//! routes them through [`crate::router`], and sends back responses.

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures::stream::StreamExt;
use futures::SinkExt;

use crate::SharedState;

/// Axum handler that upgrades to `WebSocket`.
#[allow(clippy::unused_async)]
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<SharedState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// Handle an individual `WebSocket` connection.
#[allow(clippy::cognitive_complexity)]
async fn handle_socket(socket: WebSocket, state: SharedState) {
    let (mut sender, mut receiver) = socket.split();

    tracing::info!("websocket client connected");

    while let Some(msg) = receiver.next().await {
        let msg = match msg {
            Ok(msg) => msg,
            Err(e) => {
                tracing::warn!("websocket receive error: {e}");
                break;
            }
        };

        match msg {
            Message::Text(text) => {
                if let Some(response) = crate::router::route_message(&state, &text).await {
                    if sender.send(Message::Text(response.into())).await.is_err() {
                        break;
                    }
                }
            }
            Message::Close(_) => break,
            // Binary frames reserved for mux protocol (future)
            Message::Binary(_) | Message::Ping(_) | Message::Pong(_) => {}
        }
    }

    tracing::info!("websocket client disconnected");
}
