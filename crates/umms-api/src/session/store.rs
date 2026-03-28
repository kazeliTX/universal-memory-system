//! SQLite-backed session storage.
//!
//! Follows the same pattern as `PersonaStore`: `Arc<Mutex<Connection>>`
//! with `tokio::task::spawn_blocking` for all DB operations.
//! Messages are stored as a JSON blob in a single column.

use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use tokio::sync::Mutex;

use umms_core::error::{StorageError, UmmsError};

use super::{ChatMessage, ChatSession, ChatSessionSummary};

/// SQLite-backed session storage.
pub struct SessionStore {
    conn: Arc<Mutex<Connection>>,
}

impl SessionStore {
    /// Open (or create) a SQLite database at `path` and initialise the schema.
    ///
    /// Pass `":memory:"` for an in-memory database (useful for tests).
    pub fn new(path: impl AsRef<Path>) -> Result<Self, UmmsError> {
        let conn = Connection::open(path.as_ref()).map_err(|e| StorageError::ConnectionFailed {
            backend: "sqlite-session".into(),
            reason: e.to_string(),
        })?;

        // Enable WAL mode for concurrent access
        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|e| StorageError::Sqlite(e.to_string()))?;

        Self::run_migrations(&conn)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn run_migrations(conn: &Connection) -> Result<(), UmmsError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                agent_id TEXT NOT NULL,
                title TEXT NOT NULL DEFAULT '',
                messages TEXT NOT NULL DEFAULT '[]',
                metadata TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_sessions_agent_id ON sessions(agent_id);
            CREATE INDEX IF NOT EXISTS idx_sessions_updated_at ON sessions(updated_at);",
        )
        .map_err(|e| StorageError::MigrationFailed(e.to_string()))?;
        Ok(())
    }

    /// Save (upsert) a session. If a session with the same `id` exists, it is replaced.
    pub async fn save_session(&self, session: &ChatSession) -> Result<(), UmmsError> {
        let id = session.id.clone();
        let agent_id = session.agent_id.clone();
        let title = session.title.clone();
        let messages_json = serde_json::to_string(&session.messages)
            .map_err(|e| UmmsError::Internal(format!("failed to serialize messages: {e}")))?;
        let metadata_json = serde_json::to_string(&session.metadata)
            .map_err(|e| UmmsError::Internal(format!("failed to serialize metadata: {e}")))?;
        let created_at = session.created_at.to_rfc3339();
        let updated_at = session.updated_at.to_rfc3339();

        let conn = Arc::clone(&self.conn);
        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            conn.execute(
                "INSERT OR REPLACE INTO sessions
                     (id, agent_id, title, messages, metadata, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    id,
                    agent_id,
                    title,
                    messages_json,
                    metadata_json,
                    created_at,
                    updated_at
                ],
            )
            .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;
            Ok::<(), UmmsError>(())
        })
        .await
        .map_err(|e| UmmsError::Internal(format!("spawn_blocking join error: {e}")))??;

        tracing::debug!(session_id = session.id, "saved session");
        Ok(())
    }

    /// Get a single session by ID, including full message history.
    pub async fn get_session(&self, id: &str) -> Result<Option<ChatSession>, UmmsError> {
        let id = id.to_owned();
        let conn = Arc::clone(&self.conn);

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            let mut stmt = conn
                .prepare(
                    "SELECT id, agent_id, title, messages, metadata, created_at, updated_at
                     FROM sessions WHERE id = ?1",
                )
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            let row = stmt
                .query_row(params![id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                    ))
                })
                .optional()
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            match row {
                None => Ok(None),
                Some((
                    id,
                    agent_id,
                    title,
                    messages_json,
                    metadata_json,
                    created_str,
                    updated_str,
                )) => {
                    let session = parse_session_row(
                        id,
                        agent_id,
                        title,
                        messages_json,
                        metadata_json,
                        created_str,
                        updated_str,
                    )?;
                    Ok(Some(session))
                }
            }
        })
        .await
        .map_err(|e| UmmsError::Internal(format!("spawn_blocking join error: {e}")))?
    }

    /// List sessions, optionally filtered by agent_id.
    /// Returns summaries without full message content, ordered by updated_at desc.
    pub async fn list_sessions(
        &self,
        agent_id: Option<&str>,
    ) -> Result<Vec<ChatSessionSummary>, UmmsError> {
        let agent_id = agent_id.map(ToOwned::to_owned);
        let conn = Arc::clone(&self.conn);

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();

            let (sql, param): (String, Option<String>) = match agent_id {
                Some(ref aid) => (
                    "SELECT id, agent_id, title, messages, created_at, updated_at
                     FROM sessions WHERE agent_id = ?1
                     ORDER BY updated_at DESC"
                        .to_owned(),
                    Some(aid.clone()),
                ),
                None => (
                    "SELECT id, agent_id, title, messages, created_at, updated_at
                     FROM sessions ORDER BY updated_at DESC"
                        .to_owned(),
                    None,
                ),
            };

            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            let rows: Vec<(String, String, String, String, String, String)> =
                if let Some(ref p) = param {
                    stmt.query_map(params![p], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                            row.get::<_, String>(5)?,
                        ))
                    })
                    .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?
                } else {
                    stmt.query_map([], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                            row.get::<_, String>(5)?,
                        ))
                    })
                    .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?
                };

            let mut summaries = Vec::new();
            for (id, agent_id, title, messages_json, created_str, updated_str) in rows {
                let messages: Vec<ChatMessage> =
                    serde_json::from_str(&messages_json).unwrap_or_default();

                let last_message_preview = messages
                    .last()
                    .map(|m| {
                        let preview: String = m.content.chars().take(100).collect();
                        preview
                    })
                    .unwrap_or_default();

                let created_at = DateTime::parse_from_rfc3339(&created_str)
                    .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc));
                let updated_at = DateTime::parse_from_rfc3339(&updated_str)
                    .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc));

                summaries.push(ChatSessionSummary {
                    id,
                    agent_id,
                    title,
                    message_count: messages.len(),
                    last_message_preview,
                    created_at,
                    updated_at,
                });
            }

            Ok(summaries)
        })
        .await
        .map_err(|e| UmmsError::Internal(format!("spawn_blocking join error: {e}")))?
    }

    /// Delete a session by ID. Returns Ok(()) even if the session did not exist.
    pub async fn delete_session(&self, id: &str) -> Result<(), UmmsError> {
        let id = id.to_owned();
        let conn = Arc::clone(&self.conn);

        let id_clone = id.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            conn.execute("DELETE FROM sessions WHERE id = ?1", params![id_clone])
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;
            Ok::<(), UmmsError>(())
        })
        .await
        .map_err(|e| UmmsError::Internal(format!("spawn_blocking join error: {e}")))??;

        tracing::debug!(session_id = id, "deleted session");
        Ok(())
    }

    /// Update only the title of a session.
    pub async fn update_title(&self, id: &str, title: &str) -> Result<(), UmmsError> {
        let id = id.to_owned();
        let title = title.to_owned();
        let now = Utc::now().to_rfc3339();
        let conn = Arc::clone(&self.conn);

        let id_for_block = id.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            let rows = conn
                .execute(
                    "UPDATE sessions SET title = ?1, updated_at = ?2 WHERE id = ?3",
                    params![title, now, id_for_block],
                )
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            if rows == 0 {
                return Err(UmmsError::Internal(format!(
                    "session not found: {id_for_block}"
                )));
            }
            Ok::<(), UmmsError>(())
        })
        .await
        .map_err(|e| UmmsError::Internal(format!("spawn_blocking join error: {e}")))??;

        tracing::debug!(session_id = id, "updated session title");
        Ok(())
    }
}

/// Parse a raw row tuple into a `ChatSession`.
#[allow(clippy::needless_pass_by_value)]
fn parse_session_row(
    id: String,
    agent_id: String,
    title: String,
    messages_json: String,
    metadata_json: String,
    created_at_str: String,
    updated_at_str: String,
) -> Result<ChatSession, UmmsError> {
    let messages: Vec<ChatMessage> = serde_json::from_str(&messages_json)
        .map_err(|e| UmmsError::Internal(format!("failed to parse messages JSON: {e}")))?;
    let metadata: serde_json::Value = serde_json::from_str(&metadata_json)
        .map_err(|e| UmmsError::Internal(format!("failed to parse metadata JSON: {e}")))?;
    let created_at: DateTime<Utc> = DateTime::parse_from_rfc3339(&created_at_str)
        .map_err(|e| UmmsError::Internal(format!("failed to parse created_at: {e}")))?
        .with_timezone(&Utc);
    let updated_at: DateTime<Utc> = DateTime::parse_from_rfc3339(&updated_at_str)
        .map_err(|e| UmmsError::Internal(format!("failed to parse updated_at: {e}")))?
        .with_timezone(&Utc);

    Ok(ChatSession {
        id,
        agent_id,
        title,
        messages,
        created_at,
        updated_at,
        metadata,
    })
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
    use crate::session::{ChatMessage, ChatSourceRecord};

    fn make_session(id: &str, agent_id: &str, title: &str) -> ChatSession {
        ChatSession {
            id: id.to_owned(),
            agent_id: agent_id.to_owned(),
            title: title.to_owned(),
            messages: vec![
                ChatMessage {
                    role: "user".to_owned(),
                    content: "Hello".to_owned(),
                    timestamp: Utc::now(),
                    sources: vec![],
                    latency_ms: None,
                },
                ChatMessage {
                    role: "assistant".to_owned(),
                    content: "Hi there!".to_owned(),
                    timestamp: Utc::now(),
                    sources: vec![ChatSourceRecord {
                        memory_id: "mem-1".to_owned(),
                        score: 0.85,
                        content_preview: "Some memory".to_owned(),
                    }],
                    latency_ms: Some(150),
                },
            ],
            created_at: Utc::now(),
            updated_at: Utc::now(),
            metadata: serde_json::json!({}),
        }
    }

    #[tokio::test]
    async fn save_and_get_roundtrip() {
        let store = SessionStore::new(":memory:").unwrap();
        let session = make_session("sess-1", "agent-a", "Test Chat");

        store.save_session(&session).await.unwrap();
        let loaded = store.get_session("sess-1").await.unwrap();

        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.id, "sess-1");
        assert_eq!(loaded.agent_id, "agent-a");
        assert_eq!(loaded.title, "Test Chat");
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.messages[0].role, "user");
        assert_eq!(loaded.messages[1].sources.len(), 1);
        assert_eq!(loaded.messages[1].latency_ms, Some(150));
    }

    #[tokio::test]
    async fn get_nonexistent_returns_none() {
        let store = SessionStore::new(":memory:").unwrap();
        let loaded = store.get_session("ghost").await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn list_sessions_returns_summaries() {
        let store = SessionStore::new(":memory:").unwrap();
        store
            .save_session(&make_session("s1", "agent-a", "Chat 1"))
            .await
            .unwrap();
        store
            .save_session(&make_session("s2", "agent-a", "Chat 2"))
            .await
            .unwrap();
        store
            .save_session(&make_session("s3", "agent-b", "Chat 3"))
            .await
            .unwrap();

        // List all
        let all = store.list_sessions(None).await.unwrap();
        assert_eq!(all.len(), 3);

        // List filtered by agent
        let agent_a = store.list_sessions(Some("agent-a")).await.unwrap();
        assert_eq!(agent_a.len(), 2);
        assert!(agent_a.iter().all(|s| s.agent_id == "agent-a"));

        // Summaries have message counts
        assert_eq!(agent_a[0].message_count, 2);
        assert!(!agent_a[0].last_message_preview.is_empty());
    }

    #[tokio::test]
    async fn delete_session_removes_it() {
        let store = SessionStore::new(":memory:").unwrap();
        store
            .save_session(&make_session("doomed", "agent-a", "Doomed"))
            .await
            .unwrap();

        store.delete_session("doomed").await.unwrap();
        let loaded = store.get_session("doomed").await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn delete_nonexistent_is_ok() {
        let store = SessionStore::new(":memory:").unwrap();
        let result = store.delete_session("ghost").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn update_title_works() {
        let store = SessionStore::new(":memory:").unwrap();
        store
            .save_session(&make_session("s1", "agent-a", "Original"))
            .await
            .unwrap();

        store.update_title("s1", "Renamed").await.unwrap();
        let loaded = store.get_session("s1").await.unwrap().unwrap();
        assert_eq!(loaded.title, "Renamed");
    }

    #[tokio::test]
    async fn update_title_nonexistent_fails() {
        let store = SessionStore::new(":memory:").unwrap();
        let result = store.update_title("ghost", "Nope").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn save_overwrites_existing() {
        let store = SessionStore::new(":memory:").unwrap();
        let mut session = make_session("s1", "agent-a", "Original");
        store.save_session(&session).await.unwrap();

        session.title = "Updated".to_owned();
        session.messages.push(ChatMessage {
            role: "user".to_owned(),
            content: "Another message".to_owned(),
            timestamp: Utc::now(),
            sources: vec![],
            latency_ms: None,
        });
        store.save_session(&session).await.unwrap();

        let loaded = store.get_session("s1").await.unwrap().unwrap();
        assert_eq!(loaded.title, "Updated");
        assert_eq!(loaded.messages.len(), 3);
    }
}
