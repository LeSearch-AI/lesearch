//! Codex CLI provider adapter.
//!
//! Spawns `codex exec --json` and parses the JSON event stream.
//! Codex emits newline-delimited JSON events with a `type` field.
//! Policy enforcement is `audit-only` because Codex does not reliably
//! surface pre-execution tool events.

use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

use crate::{
    AgentEvent, AgentHandle, AgentProvider, AgentSpec, EnforcementMode, ProviderError,
    ProviderManifest,
};

/// Codex CLI provider.
#[derive(Debug)]
pub struct CodexProvider {
    manifest: ProviderManifest,
}

impl Default for CodexProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl CodexProvider {
    /// Create a new Codex provider.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            manifest: ProviderManifest {
                name: "codex",
                display_name: "Codex CLI",
                binary: "codex",
                enforcement_mode: EnforcementMode::AuditOnly,
            },
        }
    }
}

#[async_trait::async_trait]
impl AgentProvider for CodexProvider {
    fn manifest(&self) -> &ProviderManifest {
        &self.manifest
    }

    async fn spawn(&self, spec: AgentSpec) -> Result<AgentHandle, ProviderError> {
        let mut cmd = Command::new(self.manifest.binary);

        cmd.arg("exec").arg("--json");

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

        let reader_task = tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();

            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        if line.trim().is_empty() {
                            continue;
                        }
                        let event = parse_codex_event(&line);
                        if tx.send(event).await.is_err() {
                            break;
                        }
                    }
                    Ok(None) => {
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

/// Parse a single JSON-Lines event from Codex's `--json` output.
///
/// Codex events have a `type` field. Key event types:
/// - `message` — streaming text content
/// - `function_call` — tool/function call request
/// - `completed` — turn finished
/// - Other types are passed through as `Raw`.
fn parse_codex_event(line: &str) -> AgentEvent {
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
        "message" => {
            let content = value
                .get("content")
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

        "function_call" => {
            let tool_name = value
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown")
                .to_owned();
            let arguments = value.get("arguments").cloned().unwrap_or_default();
            let call_id = value
                .get("call_id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_owned();

            AgentEvent::ToolCallRequested {
                tool_name,
                arguments,
                call_id,
            }
        }

        "completed" => {
            let summary = value
                .get("summary")
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
        let provider = CodexProvider::new();
        let m = provider.manifest();
        assert_eq!(m.name, "codex");
        assert_eq!(m.binary, "codex");
        assert_eq!(m.enforcement_mode, EnforcementMode::AuditOnly);
    }

    #[test]
    fn parse_message() {
        let line = r#"{"type":"message","content":"hello from codex"}"#;
        let event = parse_codex_event(line);
        match event {
            AgentEvent::StreamChunk { content } => assert_eq!(content, "hello from codex"),
            other => panic!("expected StreamChunk, got {other:?}"),
        }
    }

    #[test]
    fn parse_function_call() {
        let line =
            r#"{"type":"function_call","call_id":"fc_1","name":"shell","arguments":{"cmd":"ls"}}"#;
        let event = parse_codex_event(line);
        match event {
            AgentEvent::ToolCallRequested {
                tool_name,
                arguments,
                call_id,
            } => {
                assert_eq!(tool_name, "shell");
                assert_eq!(call_id, "fc_1");
                assert_eq!(arguments["cmd"], "ls");
            }
            other => panic!("expected ToolCallRequested, got {other:?}"),
        }
    }

    #[test]
    fn parse_completed() {
        let line = r#"{"type":"completed","summary":"done"}"#;
        let event = parse_codex_event(line);
        match event {
            AgentEvent::TurnComplete { summary } => {
                assert_eq!(summary.as_deref(), Some("done"));
            }
            other => panic!("expected TurnComplete, got {other:?}"),
        }
    }

    #[test]
    fn parse_unknown() {
        let line = r#"{"type":"debug","info":"trace"}"#;
        let event = parse_codex_event(line);
        match event {
            AgentEvent::Raw { event_type, .. } => assert_eq!(event_type, "debug"),
            other => panic!("expected Raw, got {other:?}"),
        }
    }
}
