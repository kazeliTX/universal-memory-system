//! SQLite-backed diary storage.
//!
//! Follows the same `Arc<Mutex<Connection>>` + `spawn_blocking` pattern as
//! [`PersonaStore`](crate::store::PersonaStore).

use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use tokio::sync::Mutex;

use umms_core::error::{StorageError, UmmsError};

use crate::diary::{DiaryCategory, DiaryEntry};

/// SQLite-backed diary storage for agent observations about users.
pub struct DiaryStore {
    conn: Arc<Mutex<Connection>>,
}

impl DiaryStore {
    /// Open (or create) a SQLite database at `path` and initialise the schema.
    ///
    /// Pass `":memory:"` for an in-memory database (useful for tests).
    pub fn new(path: impl AsRef<Path>) -> Result<Self, UmmsError> {
        let conn = Connection::open(path.as_ref()).map_err(|e| StorageError::ConnectionFailed {
            backend: "sqlite-diary".into(),
            reason: e.to_string(),
        })?;

        // WAL mode for concurrent readers
        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|e| StorageError::MigrationFailed(e.to_string()))?;

        Self::run_migrations(&conn)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn run_migrations(conn: &Connection) -> Result<(), UmmsError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS diary_entries (
                id TEXT PRIMARY KEY,
                agent_id TEXT NOT NULL,
                category TEXT NOT NULL,
                content TEXT NOT NULL,
                confidence REAL NOT NULL DEFAULT 0.5,
                source_session_id TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_diary_agent ON diary_entries(agent_id);
            CREATE INDEX IF NOT EXISTS idx_diary_category ON diary_entries(agent_id, category);",
        )
        .map_err(|e| StorageError::MigrationFailed(e.to_string()))?;
        Ok(())
    }

    /// Add a new diary entry.
    pub async fn add_entry(&self, entry: &DiaryEntry) -> Result<(), UmmsError> {
        let id = entry.id.clone();
        let agent_id = entry.agent_id.clone();
        let category = entry.category.to_string();
        let content = entry.content.clone();
        let confidence = entry.confidence;
        let source_session_id = entry.source_session_id.clone();
        let created_at = entry.created_at.to_rfc3339();
        let updated_at = entry.updated_at.to_rfc3339();

        let conn = Arc::clone(&self.conn);
        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            conn.execute(
                "INSERT INTO diary_entries
                     (id, agent_id, category, content, confidence,
                      source_session_id, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    id,
                    agent_id,
                    category,
                    content,
                    confidence,
                    source_session_id,
                    created_at,
                    updated_at
                ],
            )
            .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;
            Ok::<(), UmmsError>(())
        })
        .await
        .map_err(|e| UmmsError::Internal(format!("spawn_blocking join error: {e}")))??;

        tracing::debug!(
            id = entry.id,
            agent_id = entry.agent_id,
            "added diary entry"
        );
        Ok(())
    }

    /// Get diary entries for an agent, ordered by most recent first.
    pub async fn get_entries(
        &self,
        agent_id: &str,
        limit: usize,
    ) -> Result<Vec<DiaryEntry>, UmmsError> {
        let agent_id = agent_id.to_owned();
        let conn = Arc::clone(&self.conn);

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            let mut stmt = conn
                .prepare(
                    "SELECT id, agent_id, category, content, confidence,
                            source_session_id, created_at, updated_at
                     FROM diary_entries
                     WHERE agent_id = ?1
                     ORDER BY updated_at DESC
                     LIMIT ?2",
                )
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            let rows = stmt
                .query_map(params![agent_id, limit as i64], parse_row)
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            collect_rows(rows)
        })
        .await
        .map_err(|e| UmmsError::Internal(format!("spawn_blocking join error: {e}")))?
    }

    /// Get diary entries filtered by category.
    pub async fn get_entries_by_category(
        &self,
        agent_id: &str,
        category: &DiaryCategory,
        limit: usize,
    ) -> Result<Vec<DiaryEntry>, UmmsError> {
        let agent_id = agent_id.to_owned();
        let category_str = category.to_string();
        let conn = Arc::clone(&self.conn);

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            let mut stmt = conn
                .prepare(
                    "SELECT id, agent_id, category, content, confidence,
                            source_session_id, created_at, updated_at
                     FROM diary_entries
                     WHERE agent_id = ?1 AND category = ?2
                     ORDER BY updated_at DESC
                     LIMIT ?3",
                )
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            let rows = stmt
                .query_map(params![agent_id, category_str, limit as i64], parse_row)
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            collect_rows(rows)
        })
        .await
        .map_err(|e| UmmsError::Internal(format!("spawn_blocking join error: {e}")))?
    }

    /// Update the content and confidence of an existing entry.
    pub async fn update_entry(
        &self,
        id: &str,
        content: &str,
        confidence: f32,
    ) -> Result<(), UmmsError> {
        let id = id.to_owned();
        let content = content.to_owned();
        let updated_at = Utc::now().to_rfc3339();
        let conn = Arc::clone(&self.conn);

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            let changed = conn
                .execute(
                    "UPDATE diary_entries SET content = ?1, confidence = ?2, updated_at = ?3
                     WHERE id = ?4",
                    params![content, confidence, updated_at, id],
                )
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            if changed == 0 {
                tracing::warn!(id, "diary entry not found for update");
            }
            Ok::<(), UmmsError>(())
        })
        .await
        .map_err(|e| UmmsError::Internal(format!("spawn_blocking join error: {e}")))??;

        Ok(())
    }

    /// Delete a diary entry by ID.
    pub async fn delete_entry(&self, id: &str) -> Result<(), UmmsError> {
        let id_owned = id.to_owned();
        let conn = Arc::clone(&self.conn);

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            conn.execute("DELETE FROM diary_entries WHERE id = ?1", params![id_owned])
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;
            Ok::<(), UmmsError>(())
        })
        .await
        .map_err(|e| UmmsError::Internal(format!("spawn_blocking join error: {e}")))??;

        tracing::debug!(id = id, "deleted diary entry");
        Ok(())
    }

    /// Simple text search over diary content using SQL LIKE.
    pub async fn search_entries(
        &self,
        agent_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<DiaryEntry>, UmmsError> {
        let agent_id = agent_id.to_owned();
        let pattern = format!("%{query}%");
        let conn = Arc::clone(&self.conn);

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            let mut stmt = conn
                .prepare(
                    "SELECT id, agent_id, category, content, confidence,
                            source_session_id, created_at, updated_at
                     FROM diary_entries
                     WHERE agent_id = ?1 AND content LIKE ?2
                     ORDER BY updated_at DESC
                     LIMIT ?3",
                )
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            let rows = stmt
                .query_map(params![agent_id, pattern, limit as i64], parse_row)
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            collect_rows(rows)
        })
        .await
        .map_err(|e| UmmsError::Internal(format!("spawn_blocking join error: {e}")))?
    }
}

/// Parse a single row from a diary query.
fn parse_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DiaryEntry> {
    Ok(DiaryEntry {
        id: row.get(0)?,
        agent_id: row.get(1)?,
        category: DiaryCategory::from_str(&row.get::<_, String>(2)?)
            .unwrap_or(DiaryCategory::Context),
        content: row.get(3)?,
        confidence: row.get(4)?,
        source_session_id: row.get(5)?,
        created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(6)?)
            .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc)),
        updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(7)?)
            .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc)),
    })
}

/// Collect rows from a query, converting errors.
fn collect_rows(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<DiaryEntry>>,
) -> Result<Vec<DiaryEntry>, UmmsError> {
    let mut entries = Vec::new();
    for row in rows {
        entries.push(row.map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?);
    }
    Ok(entries)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diary::DiaryCategory;

    fn make_entry(agent_id: &str, category: DiaryCategory, content: &str) -> DiaryEntry {
        DiaryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            agent_id: agent_id.to_owned(),
            category,
            content: content.to_owned(),
            confidence: 0.8,
            source_session_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn add_and_get_entry() {
        let store = DiaryStore::new(":memory:").unwrap();
        let entry = make_entry("agent-1", DiaryCategory::Preference, "User prefers Rust");

        store.add_entry(&entry).await.unwrap();

        let entries = store.get_entries("agent-1", 10).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].content, "User prefers Rust");
        assert_eq!(entries[0].category, DiaryCategory::Preference);
    }

    #[tokio::test]
    async fn get_entries_by_category() {
        let store = DiaryStore::new(":memory:").unwrap();

        store
            .add_entry(&make_entry(
                "agent-1",
                DiaryCategory::Preference,
                "Likes Rust",
            ))
            .await
            .unwrap();
        store
            .add_entry(&make_entry(
                "agent-1",
                DiaryCategory::Expertise,
                "Expert in ML",
            ))
            .await
            .unwrap();
        store
            .add_entry(&make_entry(
                "agent-1",
                DiaryCategory::Preference,
                "Prefers concise",
            ))
            .await
            .unwrap();

        let prefs = store
            .get_entries_by_category("agent-1", &DiaryCategory::Preference, 10)
            .await
            .unwrap();
        assert_eq!(prefs.len(), 2);

        let expertise = store
            .get_entries_by_category("agent-1", &DiaryCategory::Expertise, 10)
            .await
            .unwrap();
        assert_eq!(expertise.len(), 1);
    }

    #[tokio::test]
    async fn update_entry() {
        let store = DiaryStore::new(":memory:").unwrap();
        let entry = make_entry("agent-1", DiaryCategory::Style, "Likes detailed answers");
        let id = entry.id.clone();

        store.add_entry(&entry).await.unwrap();
        store
            .update_entry(&id, "Actually prefers brief answers", 0.9)
            .await
            .unwrap();

        let entries = store.get_entries("agent-1", 10).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].content, "Actually prefers brief answers");
        assert!((entries[0].confidence - 0.9).abs() < f32::EPSILON);
    }

    #[tokio::test]
    async fn delete_entry() {
        let store = DiaryStore::new(":memory:").unwrap();
        let entry = make_entry("agent-1", DiaryCategory::Context, "Working on UMMS");
        let id = entry.id.clone();

        store.add_entry(&entry).await.unwrap();
        assert_eq!(store.get_entries("agent-1", 10).await.unwrap().len(), 1);

        store.delete_entry(&id).await.unwrap();
        assert_eq!(store.get_entries("agent-1", 10).await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn search_entries() {
        let store = DiaryStore::new(":memory:").unwrap();

        store
            .add_entry(&make_entry(
                "agent-1",
                DiaryCategory::Expertise,
                "Expert in Rust programming",
            ))
            .await
            .unwrap();
        store
            .add_entry(&make_entry(
                "agent-1",
                DiaryCategory::Context,
                "Working on Python project",
            ))
            .await
            .unwrap();
        store
            .add_entry(&make_entry(
                "agent-1",
                DiaryCategory::Preference,
                "Prefers Rust over Go",
            ))
            .await
            .unwrap();

        let rust_entries = store.search_entries("agent-1", "Rust", 10).await.unwrap();
        assert_eq!(rust_entries.len(), 2);

        let python_entries = store.search_entries("agent-1", "Python", 10).await.unwrap();
        assert_eq!(python_entries.len(), 1);
    }

    #[tokio::test]
    async fn get_entries_respects_limit() {
        let store = DiaryStore::new(":memory:").unwrap();

        for i in 0..5 {
            store
                .add_entry(&make_entry(
                    "agent-1",
                    DiaryCategory::Pattern,
                    &format!("Pattern {i}"),
                ))
                .await
                .unwrap();
        }

        let entries = store.get_entries("agent-1", 3).await.unwrap();
        assert_eq!(entries.len(), 3);
    }

    #[tokio::test]
    async fn entries_scoped_by_agent() {
        let store = DiaryStore::new(":memory:").unwrap();

        store
            .add_entry(&make_entry("agent-1", DiaryCategory::Style, "Agent 1 note"))
            .await
            .unwrap();
        store
            .add_entry(&make_entry("agent-2", DiaryCategory::Style, "Agent 2 note"))
            .await
            .unwrap();

        let a1 = store.get_entries("agent-1", 10).await.unwrap();
        assert_eq!(a1.len(), 1);
        assert_eq!(a1[0].content, "Agent 1 note");

        let a2 = store.get_entries("agent-2", 10).await.unwrap();
        assert_eq!(a2.len(), 1);
        assert_eq!(a2[0].content, "Agent 2 note");
    }
}
