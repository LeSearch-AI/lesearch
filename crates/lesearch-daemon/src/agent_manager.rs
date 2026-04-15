//! Agent lifecycle manager.
//!
//! Tracks running agents, handles spawn/stop/input/list operations,
//! and bridges agent events to the session writer.

use std::collections::HashMap;
use std::path::PathBuf;

use lesearch_protocol::agent::AgentId;
use lesearch_providers::{AgentEvent, AgentHandle, AgentSpec};
use tokio::sync::mpsc;

use crate::DaemonError;

// ---------------------------------------------------------------------------
// Running agent record
// ---------------------------------------------------------------------------

/// Metadata and handle for a running agent.
pub struct RunningAgent {
    /// Agent identifier.
    pub id: AgentId,
    /// Provider name (e.g., `"claude"`).
    pub provider: String,
    /// Current lifecycle state string.
    pub status: String,
    /// Working directory.
    pub cwd: PathBuf,
    /// Session id for this run.
    pub session_id: String,
    /// Handle to the child process (present while running).
    handle: Option<AgentHandle>,
}

impl std::fmt::Debug for RunningAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunningAgent")
            .field("id", &self.id)
            .field("provider", &self.provider)
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

// ---------------------------------------------------------------------------
// Agent manager
// ---------------------------------------------------------------------------

/// Manages the lifecycle of all agents.
#[derive(Debug)]
pub struct AgentManager {
    agents: HashMap<String, RunningAgent>,
    /// Channel to forward agent events to the session writer.
    event_tx: mpsc::Sender<(String, AgentEvent)>,
    /// `$LESEARCH_HOME` root for creating per-agent directories.
    home: PathBuf,
}

impl AgentManager {
    /// Create a new agent manager.
    ///
    /// `event_tx` sends `(session_id, AgentEvent)` tuples to the session
    /// writer background task. `home` is `$LESEARCH_HOME` for agent isolation dirs.
    #[must_use]
    pub fn new(event_tx: mpsc::Sender<(String, AgentEvent)>, home: PathBuf) -> Self {
        Self {
            agents: HashMap::new(),
            event_tx,
            home,
        }
    }

    /// Prepare a spawn: allocate IDs and create isolation dirs.
    ///
    /// Returns `(agent_id, session_id, spec)` without calling the provider.
    /// This is designed to be called under the lock, then the lock is released,
    /// the provider spawns the process, and [`register_agent`] is called to
    /// insert the result — avoiding holding the mutex across `.await`.
    pub fn prepare_spawn(
        &self,
        prompt: String,
        cwd: PathBuf,
        model: Option<String>,
        mode: Option<String>,
    ) -> Result<(AgentId, String, AgentSpec), DaemonError> {
        let agent_id = AgentId::new();
        let session_id = uuid::Uuid::now_v7().to_string();

        // Per-agent directory isolation (plain dirs + OS perms)
        let agent_dir = self.home.join("agents").join(agent_id.to_string());
        let agent_fs = agent_dir.join("fs");
        std::fs::create_dir_all(&agent_fs).map_err(|e| {
            DaemonError::Agent(format!("failed to create agent dir: {e}"))
        })?;

        // Use agent's cwd if specified, otherwise the isolated dir
        let effective_cwd = if cwd.as_os_str().is_empty() { agent_fs } else { cwd };

        let spec = AgentSpec {
            prompt,
            cwd: effective_cwd,
            model,
            mode,
        };

        Ok((agent_id, session_id, spec))
    }

    /// Register a spawned agent after the provider has created the handle.
    ///
    /// Call this under the lock after releasing it for `provider.spawn()`.
    pub fn register_agent(
        &mut self,
        agent_id: AgentId,
        session_id: String,
        provider_name: &str,
        cwd: PathBuf,
        handle: AgentHandle,
    ) {
        let agent = RunningAgent {
            id: agent_id,
            provider: provider_name.to_owned(),
            status: "running".into(),
            cwd,
            session_id: session_id.clone(),
            handle: Some(handle),
        };

        self.agents.insert(agent_id.to_string(), agent);

        // Start background event forwarding task
        self.start_event_reader(agent_id, session_id);

        tracing::info!(%agent_id, provider = provider_name, "agent spawned");
    }

    /// Send text input to a running agent.
    pub async fn send_input(
        &mut self,
        agent_id: &str,
        text: &str,
    ) -> Result<(), DaemonError> {
        let agent = self
            .agents
            .get_mut(agent_id)
            .ok_or_else(|| DaemonError::AgentNotFound(agent_id.into()))?;

        let handle = agent
            .handle
            .as_mut()
            .ok_or_else(|| DaemonError::Agent("agent has no active handle".into()))?;

        handle
            .send_input(text)
            .await
            .map_err(|e| DaemonError::Agent(format!("input failed: {e}")))?;

        Ok(())
    }

    /// Stop a running agent.
    pub fn stop(&mut self, agent_id: &str) -> Result<(), DaemonError> {
        let agent = self
            .agents
            .get_mut(agent_id)
            .ok_or_else(|| DaemonError::AgentNotFound(agent_id.into()))?;

        if let Some(handle) = agent.handle.take() {
            handle.abort();
        }

        agent.status = "closed".into();
        tracing::info!(agent_id, "agent stopped");
        Ok(())
    }

    /// List all agents (running and completed).
    #[must_use]
    pub fn list(&self) -> Vec<AgentSummary> {
        self.agents
            .values()
            .map(|a| AgentSummary {
                id: a.id.to_string(),
                provider: a.provider.clone(),
                status: a.status.clone(),
                cwd: a.cwd.display().to_string(),
                session_id: a.session_id.clone(),
            })
            .collect()
    }

    /// Get a single agent summary.
    #[must_use]
    pub fn get(&self, agent_id: &str) -> Option<AgentSummary> {
        self.agents.get(agent_id).map(|a| AgentSummary {
            id: a.id.to_string(),
            provider: a.provider.clone(),
            status: a.status.clone(),
            cwd: a.cwd.display().to_string(),
            session_id: a.session_id.clone(),
        })
    }

    /// Start a background task to drain events from the agent handle
    /// and forward them to the session writer.
    fn start_event_reader(&mut self, agent_id: AgentId, session_id: String) {
        let agent_key = agent_id.to_string();
        let Some(agent) = self.agents.get_mut(&agent_key) else {
            return;
        };
        let Some(handle) = agent.handle.as_mut() else {
            return;
        };

        // Take the receiver out of the handle to move into the task
        // We replace it with a dummy channel
        let (_, dummy_rx) = mpsc::channel(1);
        let mut events = std::mem::replace(&mut handle.events, dummy_rx);
        let tx = self.event_tx.clone();

        tokio::spawn(async move {
            while let Some(event) = events.recv().await {
                let is_exit = matches!(event, AgentEvent::Exited { .. });
                if tx.send((session_id.clone(), event)).await.is_err() {
                    break;
                }
                if is_exit {
                    break;
                }
            }
            tracing::debug!(%agent_id, "event reader finished");
        });
    }
}

/// Summary of an agent for API responses.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentSummary {
    /// Agent id string.
    pub id: String,
    /// Provider name.
    pub provider: String,
    /// Lifecycle status.
    pub status: String,
    /// Working directory.
    pub cwd: String,
    /// Session id.
    pub session_id: String,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_manager_list_empty() {
        let (tx, _rx) = mpsc::channel(16);
        let manager = AgentManager::new(tx, std::path::PathBuf::from("/tmp"));
        assert!(manager.list().is_empty());
    }

    #[test]
    fn agent_summary_serializes() {
        let summary = AgentSummary {
            id: "abc".into(),
            provider: "claude".into(),
            status: "running".into(),
            cwd: "/tmp".into(),
            session_id: "s1".into(),
        };
        let json = serde_json::to_value(&summary).unwrap();
        assert_eq!(json["provider"], "claude");
    }
}
