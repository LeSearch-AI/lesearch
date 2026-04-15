//! `WebSocket` client for communicating with the `LeSearch` daemon.
//!
//! Connects to the daemon's `WebSocket` endpoint and sends/receives
//! JSON-RPC 2.0 messages.

use anyhow::{Context, Result};
use futures::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

/// `WebSocket` client connected to the daemon.
pub struct WsClient {
    sink: futures::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        Message,
    >,
    stream: futures::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
    next_id: i64,
}

impl WsClient {
    /// Connect to the daemon's `WebSocket` endpoint.
    ///
    /// Reads the bearer token from `$LESEARCH_HOME/keyring/session.token`
    /// and sends it as `Authorization: Bearer <token>` on the upgrade request.
    pub async fn connect(addr: &str) -> Result<Self> {
        let url = format!("ws://{addr}/ws");
        let mut request = url.as_str().into_client_request()
            .with_context(|| format!("invalid WebSocket URL: {url}"))?;

        // Read bearer token from LESEARCH_HOME
        if let Ok(token) = read_bearer_token() {
            request.headers_mut().insert(
                "Authorization",
                format!("Bearer {token}").parse().context("invalid token header value")?,
            );
        }

        let (ws, _) = tokio_tungstenite::connect_async(request)
            .await
            .with_context(|| format!("failed to connect to {url}"))?;

        let (sink, stream) = ws.split();
        Ok(Self {
            sink,
            stream,
            next_id: 1,
        })
    }

    /// Send a JSON-RPC request and wait for the response.
    pub async fn call(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let id = self.next_id;
        self.next_id += 1;

        let req = lesearch_protocol::jsonrpc::Request::new(
            lesearch_protocol::jsonrpc::RequestId::Number(id),
            method,
            params,
        );

        let text = serde_json::to_string(&req)?;
        self.sink
            .send(Message::Text(text.into()))
            .await
            .context("send failed")?;

        // Read messages until we get a response matching our id
        while let Some(msg) = self.stream.next().await {
            let msg = msg.context("ws read error")?;
            match msg {
                Message::Text(text) => {
                    let value: serde_json::Value = serde_json::from_str(&text)?;

                    // Check if it's an error response
                    if let Some(error) = value.get("error") {
                        let msg = error
                            .get("message")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("unknown error");
                        anyhow::bail!("RPC error: {msg}");
                    }

                    // Check if it matches our request id
                    if let Some(resp_id) = value.get("id").and_then(serde_json::Value::as_i64) {
                        if resp_id == id {
                            return value
                                .get("result")
                                .cloned()
                                .context("response missing result");
                        }
                    }

                    // Otherwise it's a notification — print it
                    if let Some(method) = value.get("method").and_then(serde_json::Value::as_str) {
                        handle_notification(method, &value);
                    }
                }
                Message::Close(_) => anyhow::bail!("connection closed"),
                _ => {}
            }
        }

        anyhow::bail!("connection closed before response")
    }

    /// Read and print messages until the connection closes.
    pub async fn stream_until_close(&mut self) -> Result<()> {
        while let Some(msg) = self.stream.next().await {
            let msg = msg.context("ws read error")?;
            match msg {
                Message::Text(text) => {
                    let value: serde_json::Value =
                        serde_json::from_str(&text).unwrap_or_default();

                    if let Some(method) = value.get("method").and_then(serde_json::Value::as_str) {
                        handle_notification(method, &value);
                    } else {
                        // Response to something — just print
                        tracing::debug!(%text, "received response");
                    }
                }
                Message::Close(_) => {
                    println!("\n[connection closed]");
                    break;
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// Handle a JSON-RPC notification from the daemon.
fn handle_notification(method: &str, value: &serde_json::Value) {
    let params = value.get("params").cloned().unwrap_or_default();

    match method {
        lesearch_protocol::jsonrpc::method::AGENT_OUTPUT => {
            if let Some(data) = params.get("data").and_then(serde_json::Value::as_str) {
                print!("{data}");
            }
        }
        lesearch_protocol::jsonrpc::method::AGENT_STATUS => {
            if let Some(state) = params.get("state").and_then(serde_json::Value::as_str) {
                println!("[agent state: {state}]");
            }
        }
        lesearch_protocol::jsonrpc::method::AGENT_TOOL_CALL => {
            let tool = params
                .get("tool_call")
                .and_then(|tc| tc.get("tool_name"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("?");
            println!("[tool call: {tool}]");
        }
        lesearch_protocol::jsonrpc::method::AGENT_DONE => {
            let exit = params
                .get("exit_code")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(-1);
            println!("[agent done, exit code: {exit}]");
        }
        _ => {
            tracing::debug!(method, "unknown notification");
        }
    }
}

/// Read bearer token from `$LESEARCH_HOME/keyring/session.token`.
///
/// Falls back to `~/.lesearch/keyring/session.token` if `LESEARCH_HOME` is unset.
fn read_bearer_token() -> Result<String> {
    let home = std::env::var("LESEARCH_HOME").map_or_else(
        |_| {
            std::env::var("HOME").map_or_else(
                |_| std::path::PathBuf::from(".lesearch"),
                |h| std::path::PathBuf::from(h).join(".lesearch"),
            )
        },
        std::path::PathBuf::from,
    );

    let token_path = home.join("keyring").join("session.token");
    let token = std::fs::read_to_string(&token_path)
        .with_context(|| format!("cannot read bearer token from {}", token_path.display()))?
        .trim()
        .to_owned();

    if token.is_empty() {
        anyhow::bail!("bearer token file is empty: {}", token_path.display());
    }

    Ok(token)
}
