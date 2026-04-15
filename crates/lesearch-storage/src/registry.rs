//! `SQLite` agent and session registry.
//!
//! Stores metadata about agents and sessions in
//! `$LESEARCH_HOME/registry.sqlite`. The full session event data lives
//! in per-agent JSONL files; this registry holds only structural metadata.

use rusqlite::{params, Connection};

/// Agent and session metadata registry backed by `SQLite`.
pub struct Registry {
    conn: Connection,
}

impl std::fmt::Debug for Registry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registry").finish_non_exhaustive()
    }
}

/// Row returned by agent queries.
#[derive(Debug, Clone)]
pub struct AgentRow {
    /// Agent `UUIDv7`.
    pub id: String,
    /// Provider name.
    pub provider: String,
    /// Lifecycle state.
    pub status: String,
    /// Working directory.
    pub cwd: String,
    /// Unix timestamp (seconds).
    pub created_at: i64,
    /// Unix timestamp of last activity.
    pub last_active_at: i64,
    /// Optional AVM policy name.
    pub policy_name: Option<String>,
}

/// Row returned by session queries.
#[derive(Debug, Clone)]
pub struct SessionRow {
    /// Session `UUIDv7`.
    pub id: String,
    /// Owning agent id.
    pub agent_id: String,
    /// Unix timestamp (seconds).
    pub started_at: i64,
    /// Unix timestamp, if finished.
    pub stopped_at: Option<i64>,
    /// Session status string.
    pub status: String,
    /// Path to JSONL file.
    pub file_path: String,
}

impl Registry {
    /// Open (or create) the registry database at `path`.
    ///
    /// Creates the `agents` and `sessions` tables if they don't exist.
    ///
    /// # Errors
    ///
    /// Returns [`crate::StorageError::Sqlite`] on database errors.
    pub fn open(path: &std::path::Path) -> Result<Self, crate::StorageError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS agents (
                id             TEXT PRIMARY KEY,
                provider       TEXT NOT NULL,
                status         TEXT NOT NULL DEFAULT 'initializing',
                cwd            TEXT NOT NULL,
                created_at     INTEGER NOT NULL,
                last_active_at INTEGER NOT NULL,
                config_json    TEXT,
                owner_pubkey   TEXT,
                policy_name    TEXT
            );

            CREATE TABLE IF NOT EXISTS sessions (
                id          TEXT PRIMARY KEY,
                agent_id    TEXT NOT NULL REFERENCES agents(id),
                started_at  INTEGER NOT NULL,
                stopped_at  INTEGER,
                status      TEXT NOT NULL DEFAULT 'active',
                file_path   TEXT NOT NULL
            );
            ",
        )?;
        Ok(Self { conn })
    }

    // -- Agents -----------------------------------------------------------

    /// Register a new agent.
    ///
    /// # Errors
    ///
    /// Returns [`crate::StorageError::Sqlite`] on constraint violations.
    pub fn insert_agent(&self, agent: &AgentRow) -> Result<(), crate::StorageError> {
        self.conn.execute(
            "INSERT INTO agents (id, provider, status, cwd, created_at, last_active_at, policy_name)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                agent.id,
                agent.provider,
                agent.status,
                agent.cwd,
                agent.created_at,
                agent.last_active_at,
                agent.policy_name,
            ],
        )?;
        Ok(())
    }

    /// Update an agent's status and last-active timestamp.
    ///
    /// # Errors
    ///
    /// Returns [`crate::StorageError::Sqlite`] on database errors.
    pub fn update_agent_status(
        &self,
        agent_id: &str,
        status: &str,
        now: i64,
    ) -> Result<(), crate::StorageError> {
        self.conn.execute(
            "UPDATE agents SET status = ?1, last_active_at = ?2 WHERE id = ?3",
            params![status, now, agent_id],
        )?;
        Ok(())
    }

    /// Retrieve a single agent by id.
    ///
    /// # Errors
    ///
    /// Returns [`crate::StorageError::NotFound`] if no matching row exists.
    pub fn get_agent(&self, agent_id: &str) -> Result<AgentRow, crate::StorageError> {
        self.conn
            .query_row(
                "SELECT id, provider, status, cwd, created_at, last_active_at, policy_name
                 FROM agents WHERE id = ?1",
                params![agent_id],
                |row| {
                    Ok(AgentRow {
                        id: row.get(0)?,
                        provider: row.get(1)?,
                        status: row.get(2)?,
                        cwd: row.get(3)?,
                        created_at: row.get(4)?,
                        last_active_at: row.get(5)?,
                        policy_name: row.get(6)?,
                    })
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    crate::StorageError::NotFound(format!("agent {agent_id}"))
                }
                other => crate::StorageError::Sqlite(other),
            })
    }

    /// List all agents, ordered by creation time descending.
    ///
    /// # Errors
    ///
    /// Returns [`crate::StorageError::Sqlite`] on database errors.
    pub fn list_agents(&self) -> Result<Vec<AgentRow>, crate::StorageError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, provider, status, cwd, created_at, last_active_at, policy_name
             FROM agents ORDER BY created_at DESC",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(AgentRow {
                    id: row.get(0)?,
                    provider: row.get(1)?,
                    status: row.get(2)?,
                    cwd: row.get(3)?,
                    created_at: row.get(4)?,
                    last_active_at: row.get(5)?,
                    policy_name: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    // -- Sessions ---------------------------------------------------------

    /// Register a new session.
    ///
    /// # Errors
    ///
    /// Returns [`crate::StorageError::Sqlite`] on constraint violations.
    pub fn insert_session(&self, session: &SessionRow) -> Result<(), crate::StorageError> {
        self.conn.execute(
            "INSERT INTO sessions (id, agent_id, started_at, stopped_at, status, file_path)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                session.id,
                session.agent_id,
                session.started_at,
                session.stopped_at,
                session.status,
                session.file_path,
            ],
        )?;
        Ok(())
    }

    /// Mark a session as finished.
    ///
    /// # Errors
    ///
    /// Returns [`crate::StorageError::Sqlite`] on database errors.
    pub fn finish_session(
        &self,
        session_id: &str,
        status: &str,
        stopped_at: i64,
    ) -> Result<(), crate::StorageError> {
        self.conn.execute(
            "UPDATE sessions SET status = ?1, stopped_at = ?2 WHERE id = ?3",
            params![status, stopped_at, session_id],
        )?;
        Ok(())
    }

    /// List sessions, optionally filtered by agent.
    ///
    /// # Errors
    ///
    /// Returns [`crate::StorageError::Sqlite`] on database errors.
    pub fn list_sessions(
        &self,
        agent_id: Option<&str>,
    ) -> Result<Vec<SessionRow>, crate::StorageError> {
        let (sql, bind): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) = agent_id.map_or_else(
            || -> (&str, Vec<Box<dyn rusqlite::types::ToSql>>) {
                (
                    "SELECT id, agent_id, started_at, stopped_at, status, file_path
                     FROM sessions ORDER BY started_at DESC",
                    vec![],
                )
            },
            |id| {
                (
                    "SELECT id, agent_id, started_at, stopped_at, status, file_path
                     FROM sessions WHERE agent_id = ?1 ORDER BY started_at DESC",
                    vec![Box::new(id.to_owned())],
                )
            },
        );
        let mut stmt = self.conn.prepare(sql)?;
        let params_slice: Vec<&dyn rusqlite::types::ToSql> =
            bind.iter().map(std::convert::AsRef::as_ref).collect();
        let rows = stmt
            .query_map(params_slice.as_slice(), |row| {
                Ok(SessionRow {
                    id: row.get(0)?,
                    agent_id: row.get(1)?,
                    started_at: row.get(2)?,
                    stopped_at: row.get(3)?,
                    status: row.get(4)?,
                    file_path: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Count agents with a given status.
    ///
    /// # Errors
    ///
    /// Returns [`crate::StorageError::Sqlite`] on database errors.
    pub fn count_agents_by_status(&self, status: &str) -> Result<u32, crate::StorageError> {
        let count: u32 = self.conn.query_row(
            "SELECT COUNT(*) FROM agents WHERE status = ?1",
            params![status],
            |row| row.get(0),
        )?;
        Ok(count)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_registry() -> Registry {
        Registry::open(std::path::Path::new(":memory:")).unwrap()
    }

    #[test]
    fn insert_and_get_agent() {
        let reg = temp_registry();
        let agent = AgentRow {
            id: "agent-1".into(),
            provider: "claude".into(),
            status: "initializing".into(),
            cwd: "/tmp".into(),
            created_at: 1_700_000_000,
            last_active_at: 1_700_000_000,
            policy_name: None,
        };
        reg.insert_agent(&agent).unwrap();
        let got = reg.get_agent("agent-1").unwrap();
        assert_eq!(got.provider, "claude");
    }

    #[test]
    fn update_agent_status() {
        let reg = temp_registry();
        reg.insert_agent(&AgentRow {
            id: "a1".into(),
            provider: "codex".into(),
            status: "initializing".into(),
            cwd: "/".into(),
            created_at: 1,
            last_active_at: 1,
            policy_name: None,
        })
        .unwrap();

        reg.update_agent_status("a1", "running", 2).unwrap();
        let got = reg.get_agent("a1").unwrap();
        assert_eq!(got.status, "running");
        assert_eq!(got.last_active_at, 2);
    }

    #[test]
    fn list_agents_ordered() {
        let reg = temp_registry();
        for (id, ts) in [("a", 10), ("b", 20), ("c", 5)] {
            reg.insert_agent(&AgentRow {
                id: id.into(),
                provider: "claude".into(),
                status: "idle".into(),
                cwd: "/".into(),
                created_at: ts,
                last_active_at: ts,
                policy_name: None,
            })
            .unwrap();
        }
        let agents = reg.list_agents().unwrap();
        assert_eq!(agents[0].id, "b"); // newest first
        assert_eq!(agents[2].id, "c");
    }

    #[test]
    fn session_lifecycle() {
        let reg = temp_registry();
        reg.insert_agent(&AgentRow {
            id: "a1".into(),
            provider: "claude".into(),
            status: "idle".into(),
            cwd: "/".into(),
            created_at: 1,
            last_active_at: 1,
            policy_name: None,
        })
        .unwrap();

        reg.insert_session(&SessionRow {
            id: "s1".into(),
            agent_id: "a1".into(),
            started_at: 100,
            stopped_at: None,
            status: "active".into(),
            file_path: "/tmp/s1.jsonl".into(),
        })
        .unwrap();

        let sessions = reg.list_sessions(Some("a1")).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].status, "active");

        reg.finish_session("s1", "completed", 200).unwrap();
        let sessions = reg.list_sessions(None).unwrap();
        assert_eq!(sessions[0].status, "completed");
        assert_eq!(sessions[0].stopped_at, Some(200));
    }

    #[test]
    fn get_missing_agent_returns_not_found() {
        let reg = temp_registry();
        let result = reg.get_agent("nonexistent");
        assert!(result.is_err());
    }
}
