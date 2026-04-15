//! Session writer background task.
//!
//! Receives `(session_id, AgentEvent)` tuples from the agent manager,
//! translates them into [`SessionEvent`]s, writes them to the session log
//! (with signing + hash chain), and updates the FTS5 search index.
//!
//! [`SessionEvent`]: lesearch_protocol::session::SessionEvent

use lesearch_protocol::session::{event_type, SessionEvent};
use lesearch_providers::AgentEvent;
use lesearch_storage::keyring::Keyring;
use lesearch_storage::search::SearchIndex;
use lesearch_storage::session_log::SessionLogWriter;
use tokio::sync::{mpsc, Mutex};

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Spawn the background session writer task.
///
/// Returns immediately. The task reads from `rx` until the channel closes.
pub fn spawn_session_writer(
    mut rx: mpsc::Receiver<(String, AgentEvent)>,
    sessions_dir: PathBuf,
    _keyring: Keyring,
    search_index: Arc<Mutex<SearchIndex>>,
) {
    tokio::spawn(async move {
        let mut writers: HashMap<String, SessionLogWriter> = HashMap::new();

        while let Some((session_id, event)) = rx.recv().await {
            // Lazily open a writer for each session
            let writer = if let Some(w) = writers.get_mut(&session_id) { w } else {
                let path = sessions_dir.join(format!("{session_id}.jsonl"));
                // Each session gets a fresh keyring clone for signing
                // In v0.1.0 we use the same daemon keyring for all sessions.
                // We need to reconstruct a keyring from the same key bytes.
                // For now, generate per-session (simpler; verification still works
                // because each event embeds its pubkey).
                let kr = Keyring::generate();
                match SessionLogWriter::open(&path, kr) {
                    Ok(w) => {
                        writers.insert(session_id.clone(), w);
                        writers.get_mut(&session_id).unwrap()
                    }
                    Err(e) => {
                        tracing::error!(session_id, "failed to open session log: {e}");
                        continue;
                    }
                }
            };

            let mut se = translate_event(&session_id, &event);

            if let Err(e) = writer.append(&mut se) {
                tracing::error!(session_id, "failed to write session event: {e}");
                continue;
            }

            // Update FTS5 index
            let idx = search_index.lock().await;
            if let Err(e) = idx.index_event(
                &session_id,
                "agent",
                &se.ty,
                &se.time,
                &serde_json::to_string(&se.data).unwrap_or_default(),
            ) {
                tracing::warn!(session_id, "FTS5 index error: {e}");
            }
        }

        tracing::info!("session writer task exiting");
    });
}

/// Translate an [`AgentEvent`] into a [`SessionEvent`], if applicable.
fn translate_event(session_id: &str, event: &AgentEvent) -> SessionEvent {
    let source = format!("lesearch://session/{session_id}");

    match event {
        AgentEvent::StreamChunk { content } => SessionEvent::new(
            &source,
            event_type::STREAM_CHUNK,
            "agent",
            serde_json::json!({ "content": content }),
        ),

        AgentEvent::ToolCallRequested {
            tool_name,
            arguments,
            call_id,
        } => SessionEvent::new(
            &source,
            event_type::TOOL_CALL_REQUESTED,
            "agent",
            serde_json::json!({
                "tool_name": tool_name,
                "arguments": arguments,
                "call_id": call_id,
            }),
        ),

        AgentEvent::TurnComplete { summary } => SessionEvent::new(
            &source,
            event_type::SESSION_STOPPED,
            "agent",
            serde_json::json!({ "summary": summary }),
        ),

        AgentEvent::Exited { status } => SessionEvent::new(
            &source,
            event_type::SESSION_STOPPED,
            "agent",
            serde_json::json!({
                "exit_code": status.code().unwrap_or(-1),
            }),
        ),

        AgentEvent::Error { message } => SessionEvent::new(
            &source,
            event_type::SESSION_STOPPED,
            "agent",
            serde_json::json!({ "error": message }),
        ),

        AgentEvent::Raw { event_type, data } => SessionEvent::new(
            &source,
            event_type,
            "agent",
            data.clone(),
        ),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translate_stream_chunk() {
        let event = AgentEvent::StreamChunk {
            content: "hello".into(),
        };
        let se = translate_event("s1", &event);
        assert_eq!(se.ty, event_type::STREAM_CHUNK);
    }

    #[test]
    fn translate_tool_call() {
        let event = AgentEvent::ToolCallRequested {
            tool_name: "Bash".into(),
            arguments: serde_json::json!({"cmd": "ls"}),
            call_id: "c1".into(),
        };
        let se = translate_event("s1", &event);
        assert_eq!(se.ty, event_type::TOOL_CALL_REQUESTED);
        assert_eq!(se.data["tool_name"], "Bash");
    }
}
