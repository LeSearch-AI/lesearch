//! Claude Code provider adapter.
//!
//! Spawns `claude` with `--output-format stream-json --input-format stream-json`
//! and parses the structured JSON-Lines event stream. Claude Code emits events
//! like `assistant`, `result`, and tool-use blocks that we translate into
//! [`AgentEvent`]s.

use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

use crate::{
    AgentEvent, AgentHandle, AgentProvider, AgentSpec, EnforcementMode, ProviderError,
    ProviderManifest,
};

/// Claude Code provider.
#[derive(Debug)]
pub struct ClaudeProvider {
    manifest: ProviderManifest,
}

impl Default for ClaudeProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ClaudeProvider {
    /// Create a new Claude Code provider.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            manifest: ProviderManifest {
                name: "claude",
                display_name: "Claude Code",
                binary: "claude",
                enforcement_mode: EnforcementMode::Strict,
            },
        }
    }
}

#[async_trait::async_trait]
impl AgentProvider for ClaudeProvider {
    fn manifest(&self) -> &ProviderManifest {
        &self.manifest
    }

    async fn spawn(&self, spec: AgentSpec) -> Result<AgentHandle, ProviderError> {
        let mut cmd = Command::new(self.manifest.binary);

        cmd.arg("--print")
            .arg("--output-format")
            .arg("stream-json")
            .arg("--input-format")
            .arg("stream-json")
            .arg("--verbose");

        if let Some(ref model) = spec.model {
            cmd.arg("--model").arg(model);
        }

        cmd.arg(&spec.prompt);

        cmd.current_dir(&spec.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ProviderError::BinaryNotFound(self.manifest.binary.to_owned())
            } else {
                ProviderError::SpawnFailed(format!("{}: {e}", self.manifest.binary))
            }
        })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| ProviderError::SpawnFailed("failed to capture stdin".into()))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ProviderError::SpawnFailed("failed to capture stdout".into()))?;

        let (tx, rx) = mpsc::channel(256);

        // Background task: read JSON-Lines from stdout, parse, emit AgentEvents
        let reader_task = tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();

            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        if line.trim().is_empty() {
                            continue;
                        }
                        let event = parse_claude_event(&line);
                        if tx.send(event).await.is_err() {
                            break; // receiver dropped
                        }
                    }
                    Ok(None) => {
                        // stdout closed — process exiting
                        let status = child.wait().await.unwrap_or_else(|_| {
                            std::process::ExitStatus::default()
                        });
                        let _ = tx.send(AgentEvent::Exited { status }).await;
                        break;
                    }
                    Err(e) => {
                        let _ = tx
                            .send(AgentEvent::Error {
                                message: format!("read error: {e}"),
                            })
                            .await;
                        break;
                    }
                }
            }
        });

        Ok(AgentHandle::new(stdin, rx, reader_task))
    }
}

/// Parse a single JSON-Lines event from Claude Code's `stream-json` output.
///
/// Claude Code emits events with a `type` field. Key event types:
/// - `assistant` — streaming text with `content_block` containing `text`
/// - `tool_use` — tool call request
/// - `result` — final result / turn complete
/// - Other types are passed through as `Raw`.
fn parse_claude_event(line: &str) -> AgentEvent {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
        return AgentEvent::Raw {
            event_type: "unparseable".into(),
            data: serde_json::Value::String(line.to_owned()),
        };
    };

    let event_type = value
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");

    match event_type {
        "assistant" => {
            // Extract text from content blocks
            let content = value
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(serde_json::Value::as_array)
                .map(|blocks| {
                    blocks
                        .iter()
                        .filter_map(|b| {
                            if b.get("type").and_then(serde_json::Value::as_str) == Some("text") {
                                b.get("text").and_then(serde_json::Value::as_str)
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("")
                })
                .unwrap_or_default();

            if content.is_empty() {
                AgentEvent::Raw {
                    event_type: event_type.into(),
                    data: value,
                }
            } else {
                AgentEvent::StreamChunk { content }
            }
        }

        "content_block_delta" => {
            let content = value
                .get("delta")
                .and_then(|d| d.get("text"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_owned();

            if content.is_empty() {
                AgentEvent::Raw {
                    event_type: event_type.into(),
                    data: value,
                }
            } else {
                AgentEvent::StreamChunk { content }
            }
        }

        "tool_use" => {
            let tool_name = value
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown")
                .to_owned();
            let arguments = value.get("input").cloned().unwrap_or_default();
            let call_id = value
                .get("id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_owned();

            AgentEvent::ToolCallRequested {
                tool_name,
                arguments,
                call_id,
            }
        }

        "result" => {
            let summary = value
                .get("result")
                .and_then(serde_json::Value::as_str)
                .map(String::from);
            AgentEvent::TurnComplete { summary }
        }

        _ => AgentEvent::Raw {
            event_type: event_type.into(),
            data: value,
        },
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_is_correct() {
        let provider = ClaudeProvider::new();
        let m = provider.manifest();
        assert_eq!(m.name, "claude");
        assert_eq!(m.binary, "claude");
        assert_eq!(m.enforcement_mode, EnforcementMode::Strict);
    }

    #[test]
    fn parse_stream_chunk() {
        let line = r#"{"type":"content_block_delta","delta":{"text":"hello world"}}"#;
        let event = parse_claude_event(line);
        match event {
            AgentEvent::StreamChunk { content } => assert_eq!(content, "hello world"),
            other => panic!("expected StreamChunk, got {other:?}"),
        }
    }

    #[test]
    fn parse_tool_use() {
        let line = r#"{"type":"tool_use","id":"call_123","name":"Bash","input":{"command":"ls"}}"#;
        let event = parse_claude_event(line);
        match event {
            AgentEvent::ToolCallRequested {
                tool_name,
                arguments,
                call_id,
            } => {
                assert_eq!(tool_name, "Bash");
                assert_eq!(call_id, "call_123");
                assert_eq!(arguments["command"], "ls");
            }
            other => panic!("expected ToolCallRequested, got {other:?}"),
        }
    }

    #[test]
    fn parse_result() {
        let line = r#"{"type":"result","result":"all done"}"#;
        let event = parse_claude_event(line);
        match event {
            AgentEvent::TurnComplete { summary } => {
                assert_eq!(summary.as_deref(), Some("all done"));
            }
            other => panic!("expected TurnComplete, got {other:?}"),
        }
    }

    #[test]
    fn parse_unknown_event() {
        let line = r#"{"type":"system","message":"hello"}"#;
        let event = parse_claude_event(line);
        match event {
            AgentEvent::Raw { event_type, .. } => assert_eq!(event_type, "system"),
            other => panic!("expected Raw, got {other:?}"),
        }
    }

    #[test]
    fn parse_garbage_line() {
        let event = parse_claude_event("not json at all");
        match event {
            AgentEvent::Raw { event_type, .. } => assert_eq!(event_type, "unparseable"),
            other => panic!("expected Raw, got {other:?}"),
        }
    }
}
