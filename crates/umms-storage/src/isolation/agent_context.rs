//! SQLite-backed implementation of [`AgentContextManager`].
//!
//! Persists agent snapshots (L0/L1 cache contents + execution state) so that
//! agent context can be suspended and resumed across switches.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use rusqlite::{params, Connection};
use tokio::sync::Mutex;

use umms_core::error::{StorageError, UmmsError};
use umms_core::traits::{AgentContextManager, AgentSnapshot};
use umms_core::types::AgentId;

/// SQLite-backed agent context manager.
///
/// Stores serialised snapshots in a single `agent_snapshots` table, using
/// `INSERT OR REPLACE` for upsert semantics.
pub struct SqliteAgentContextManager {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteAgentContextManager {
    /// Open (or create) a SQLite database at `path` and initialise the schema.
    ///
    /// Pass `":memory:"` for an in-memory database (useful for tests).
    pub fn new(path: impl AsRef<Path>) -> Result<Self, UmmsError> {
        let conn = Connection::open(path.as_ref()).map_err(|e| {
            StorageError::ConnectionFailed {
                backend: "sqlite-agent-context".into(),
                reason: e.to_string(),
            }
        })?;

        Self::run_migrations(&conn)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn run_migrations(conn: &Connection) -> Result<(), UmmsError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS agent_snapshots (
                agent_id   TEXT PRIMARY KEY,
                l0_data    TEXT NOT NULL,
                l1_data    TEXT NOT NULL,
                state_json TEXT NOT NULL,
                snapshot_at TEXT NOT NULL
            );",
        )
        .map_err(|e| StorageError::MigrationFailed(e.to_string()))?;
        Ok(())
    }
}

#[async_trait]
impl AgentContextManager for SqliteAgentContextManager {
    /// Not directly useful without cache access; returns an empty snapshot.
    ///
    /// Higher-level code (e.g. [`AgentSwitcher`](super::AgentSwitcher)) should
    /// build the `AgentSnapshot` from the cache and call [`save_snapshot`] instead.
    async fn snapshot(&self, agent_id: &AgentId) -> Result<AgentSnapshot, UmmsError> {
        // Try to load an existing snapshot first; otherwise return a fresh empty one.
        if let Some(snap) = self.load_snapshot(agent_id).await? {
            return Ok(snap);
        }
        Ok(AgentSnapshot {
            agent_id: agent_id.clone(),
            l0_entries: Vec::new(),
            l1_entries: Vec::new(),
            state_json: serde_json::Value::Null,
            snapshot_at: Utc::now(),
        })
    }

    async fn save_snapshot(&self, snapshot: &AgentSnapshot) -> Result<(), UmmsError> {
        let agent_id_str = snapshot.agent_id.as_str().to_owned();
        let l0_json = serde_json::to_string(&snapshot.l0_entries).map_err(|e| {
            StorageError::SnapshotFailed {
                agent_id: snapshot.agent_id.clone(),
                reason: format!("failed to serialise L0 entries: {e}"),
            }
        })?;
        let l1_json = serde_json::to_string(&snapshot.l1_entries).map_err(|e| {
            StorageError::SnapshotFailed {
                agent_id: snapshot.agent_id.clone(),
                reason: format!("failed to serialise L1 entries: {e}"),
            }
        })?;
        let state_json = serde_json::to_string(&snapshot.state_json).map_err(|e| {
            StorageError::SnapshotFailed {
                agent_id: snapshot.agent_id.clone(),
                reason: format!("failed to serialise state_json: {e}"),
            }
        })?;
        let snapshot_at = snapshot.snapshot_at.to_rfc3339();

        let conn = Arc::clone(&self.conn);
        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            conn.execute(
                "INSERT OR REPLACE INTO agent_snapshots
                     (agent_id, l0_data, l1_data, state_json, snapshot_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![agent_id_str, l0_json, l1_json, state_json, snapshot_at],
            )
            .map_err(|e| {
                UmmsError::Storage(StorageError::Sqlite(e.to_string()))
            })?;
            Ok::<(), UmmsError>(())
        })
        .await
        .map_err(|e| UmmsError::Internal(format!("spawn_blocking join error: {e}")))??;

        tracing::debug!(
            agent_id = snapshot.agent_id.as_str(),
            "saved agent snapshot"
        );
        Ok(())
    }

    async fn load_snapshot(
        &self,
        agent_id: &AgentId,
    ) -> Result<Option<AgentSnapshot>, UmmsError> {
        let agent_id_owned = agent_id.clone();
        let agent_id_str = agent_id.as_str().to_owned();
        let conn = Arc::clone(&self.conn);

        let result = tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            let mut stmt = conn
                .prepare(
                    "SELECT l0_data, l1_data, state_json, snapshot_at
                     FROM agent_snapshots WHERE agent_id = ?1",
                )
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            let row = stmt
                .query_row(params![agent_id_str], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                })
                .optional()
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            match row {
                None => Ok(None),
                Some((l0_data, l1_data, state_json, snapshot_at)) => {
                    let l0_entries = serde_json::from_str(&l0_data).map_err(|e| {
                        UmmsError::Storage(StorageError::SnapshotFailed {
                            agent_id: agent_id_owned.clone(),
                            reason: format!("failed to deserialise L0 entries: {e}"),
                        })
                    })?;
                    let l1_entries = serde_json::from_str(&l1_data).map_err(|e| {
                        UmmsError::Storage(StorageError::SnapshotFailed {
                            agent_id: agent_id_owned.clone(),
                            reason: format!("failed to deserialise L1 entries: {e}"),
                        })
                    })?;
                    let state_json_val =
                        serde_json::from_str(&state_json).map_err(|e| {
                            UmmsError::Storage(StorageError::SnapshotFailed {
                                agent_id: agent_id_owned.clone(),
                                reason: format!("failed to deserialise state_json: {e}"),
                            })
                        })?;
                    let snapshot_at_dt =
                        chrono::DateTime::parse_from_rfc3339(&snapshot_at)
                            .map_err(|e| {
                                UmmsError::Storage(StorageError::SnapshotFailed {
                                    agent_id: agent_id_owned.clone(),
                                    reason: format!("failed to parse snapshot_at: {e}"),
                                })
                            })?
                            .with_timezone(&chrono::Utc);

                    Ok(Some(AgentSnapshot {
                        agent_id: agent_id_owned,
                        l0_entries,
                        l1_entries,
                        state_json: state_json_val,
                        snapshot_at: snapshot_at_dt,
                    }))
                }
            }
        })
        .await
        .map_err(|e| UmmsError::Internal(format!("spawn_blocking join error: {e}")))?;

        result
    }

    /// Full agent switch is delegated to [`AgentSwitcher`](super::AgentSwitcher)
    /// which holds both the cache and this context manager. This method is a
    /// no-op stub; call `AgentSwitcher::switch` instead.
    async fn switch(&self, _from: &AgentId, _to: &AgentId) -> Result<(), UmmsError> {
        Err(UmmsError::Internal(
            "switch() requires cache access; use AgentSwitcher::switch() instead".into(),
        ))
    }
}

/// Extension trait so `query_row` returns `Option` on no rows.
trait OptionalRow<T> {
    fn optional(self) -> std::result::Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalRow<T> for std::result::Result<T, rusqlite::Error> {
    fn optional(self) -> std::result::Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    use umms_core::traits::AgentContextManager;
    use umms_core::types::{MemoryEntryBuilder, MemoryLayer, Modality};

    fn agent(name: &str) -> AgentId {
        AgentId::from_str(name).unwrap()
    }

    fn make_snapshot(agent_id: &AgentId) -> AgentSnapshot {
        let l0 = MemoryEntryBuilder::new(agent_id.clone(), Modality::Text)
            .layer(MemoryLayer::SensoryBuffer)
            .content_text("l0-data")
            .build();
        let l1 = MemoryEntryBuilder::new(agent_id.clone(), Modality::Text)
            .layer(MemoryLayer::WorkingMemory)
            .content_text("l1-data")
            .build();
        AgentSnapshot {
            agent_id: agent_id.clone(),
            l0_entries: vec![l0],
            l1_entries: vec![l1],
            state_json: serde_json::json!({"step": 3}),
            snapshot_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn save_and_load_roundtrip() {
        let mgr = SqliteAgentContextManager::new(":memory:").unwrap();
        let a = agent("agent-a");
        let snap = make_snapshot(&a);

        mgr.save_snapshot(&snap).await.unwrap();
        let loaded = mgr.load_snapshot(&a).await.unwrap();

        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.agent_id, a);
        assert_eq!(loaded.l0_entries.len(), 1);
        assert_eq!(
            loaded.l0_entries[0].content_text.as_deref(),
            Some("l0-data")
        );
        assert_eq!(loaded.l1_entries.len(), 1);
        assert_eq!(
            loaded.l1_entries[0].content_text.as_deref(),
            Some("l1-data")
        );
        assert_eq!(loaded.state_json, serde_json::json!({"step": 3}));
    }

    #[tokio::test]
    async fn load_nonexistent_returns_none() {
        let mgr = SqliteAgentContextManager::new(":memory:").unwrap();
        let a = agent("ghost");
        let loaded = mgr.load_snapshot(&a).await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn save_overwrites_previous_snapshot() {
        let mgr = SqliteAgentContextManager::new(":memory:").unwrap();
        let a = agent("agent-a");

        // Save first version.
        let snap1 = make_snapshot(&a);
        mgr.save_snapshot(&snap1).await.unwrap();

        // Save second version with different state.
        let mut snap2 = make_snapshot(&a);
        snap2.state_json = serde_json::json!({"step": 99});
        snap2.l0_entries.clear();
        mgr.save_snapshot(&snap2).await.unwrap();

        let loaded = mgr.load_snapshot(&a).await.unwrap().unwrap();
        assert_eq!(loaded.state_json, serde_json::json!({"step": 99}));
        assert!(loaded.l0_entries.is_empty());
    }
}
