//! FTS5 full-text search index over session events.
//!
//! Uses `SQLite` FTS5 with the `trigram` tokenizer for arbitrary substring
//! matching. The index mirrors content from session JSONL files and is
//! rebuildable from source if lost.

use rusqlite::{params, Connection};

/// FTS5 search index backed by `SQLite`.
pub struct SearchIndex {
    conn: Connection,
}

impl std::fmt::Debug for SearchIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SearchIndex").finish_non_exhaustive()
    }
}

/// A single search result.
#[derive(Debug, Clone)]
pub struct SearchHit {
    /// Session id that contained the match.
    pub session_id: String,
    /// Agent that produced the event.
    pub agent_id: String,
    /// Event type string.
    pub event_type: String,
    /// ISO 8601 event timestamp.
    pub timestamp: String,
    /// Raw event JSON data.
    pub data: String,
}

impl SearchIndex {
    /// Open (or create) the FTS5 index database at `path`.
    ///
    /// # Errors
    ///
    /// Returns [`crate::StorageError::Sqlite`] on database errors.
    pub fn open(path: &std::path::Path) -> Result<Self, crate::StorageError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;

        // The trigram tokenizer enables arbitrary substring matching.
        conn.execute_batch(
            "
            CREATE VIRTUAL TABLE IF NOT EXISTS session_events
            USING fts5(
                session_id,
                agent_id,
                event_type,
                timestamp,
                data,
                tokenize = 'trigram'
            );
            ",
        )?;

        Ok(Self { conn })
    }

    /// Index a single session event.
    ///
    /// # Errors
    ///
    /// Returns [`crate::StorageError::Sqlite`] on insert failures.
    pub fn index_event(
        &self,
        session_id: &str,
        agent_id: &str,
        event_type: &str,
        timestamp: &str,
        data: &str,
    ) -> Result<(), crate::StorageError> {
        self.conn.execute(
            "INSERT INTO session_events (session_id, agent_id, event_type, timestamp, data)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![session_id, agent_id, event_type, timestamp, data],
        )?;
        Ok(())
    }

    /// Substring search across all indexed events.
    ///
    /// Returns up to `limit` matching rows. The query uses FTS5 trigram
    /// matching so any substring works (not just whole words).
    ///
    /// # Errors
    ///
    /// Returns [`crate::StorageError::Sqlite`] on query failures.
    pub fn search(&self, query: &str, limit: u32) -> Result<Vec<SearchHit>, crate::StorageError> {
        // Escape double quotes in query for FTS5
        let escaped = query.replace('"', "\"\"");
        let mut stmt = self.conn.prepare(
            "SELECT session_id, agent_id, event_type, timestamp, data
             FROM session_events
             WHERE session_events MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        )?;

        let rows = stmt
            .query_map(params![format!("\"{escaped}\""), limit], |row| {
                Ok(SearchHit {
                    session_id: row.get(0)?,
                    agent_id: row.get(1)?,
                    event_type: row.get(2)?,
                    timestamp: row.get(3)?,
                    data: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    /// Count total indexed events.
    ///
    /// # Errors
    ///
    /// Returns [`crate::StorageError::Sqlite`] on query failures.
    pub fn count(&self) -> Result<u64, crate::StorageError> {
        let count: u64 = self.conn.query_row(
            "SELECT COUNT(*) FROM session_events",
            [],
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

    fn mem_index() -> SearchIndex {
        SearchIndex::open(std::path::Path::new(":memory:")).unwrap()
    }

    #[test]
    fn index_and_search() {
        let idx = mem_index();
        idx.index_event(
            "s1",
            "a1",
            "lesearch.tool.call.requested.v1",
            "2026-04-14T10:00:00Z",
            r#"{"tool_name":"Bash","arguments":{"command":"cargo test"}}"#,
        )
        .unwrap();

        idx.index_event(
            "s1",
            "a1",
            "lesearch.stream.chunk.v1",
            "2026-04-14T10:00:01Z",
            r#"{"content":"all tests passed"}"#,
        )
        .unwrap();

        // Substring search
        let hits = idx.search("cargo test", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].event_type, "lesearch.tool.call.requested.v1");

        // Search across data
        let hits = idx.search("tests passed", 10).unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn search_no_results() {
        let idx = mem_index();
        let hits = idx.search("nonexistent", 10).unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn count_events() {
        let idx = mem_index();
        assert_eq!(idx.count().unwrap(), 0);

        idx.index_event("s1", "a1", "t", "ts", "data1").unwrap();
        idx.index_event("s2", "a2", "t", "ts", "data2").unwrap();
        assert_eq!(idx.count().unwrap(), 2);
    }
}
