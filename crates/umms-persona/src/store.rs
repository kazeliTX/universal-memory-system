//! SQLite-backed persona storage.
//!
//! Follows the same pattern as `SqliteAgentContextManager`: `Arc<Mutex<Connection>>`
//! with `tokio::task::spawn_blocking` for all DB operations.

use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use tokio::sync::Mutex;

use umms_core::error::{StorageError, UmmsError};
use umms_core::types::AgentId;

use crate::persona::{AgentPersona, AgentRetrievalConfig};

/// SQLite-backed persona storage.
pub struct PersonaStore {
    conn: Arc<Mutex<Connection>>,
}

impl PersonaStore {
    /// Open (or create) a SQLite database at `path` and initialise the schema.
    ///
    /// Pass `":memory:"` for an in-memory database (useful for tests).
    pub fn new(path: impl AsRef<Path>) -> Result<Self, UmmsError> {
        let conn = Connection::open(path.as_ref()).map_err(|e| {
            StorageError::ConnectionFailed {
                backend: "sqlite-persona".into(),
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
            "CREATE TABLE IF NOT EXISTS personas (
                agent_id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                role TEXT NOT NULL DEFAULT '',
                description TEXT NOT NULL DEFAULT '',
                expertise TEXT NOT NULL DEFAULT '[]',
                system_prompt TEXT NOT NULL DEFAULT '',
                retrieval_config TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );",
        )
        .map_err(|e| StorageError::MigrationFailed(e.to_string()))?;
        Ok(())
    }

    /// Save (upsert) a persona. If a persona with the same `agent_id` exists, it is replaced.
    pub async fn save(&self, persona: &AgentPersona) -> Result<(), UmmsError> {
        let agent_id_str = persona.agent_id.as_str().to_owned();
        let name = persona.name.clone();
        let role = persona.role.clone();
        let description = persona.description.clone();
        let expertise_json = serde_json::to_string(&persona.expertise)
            .map_err(|e| UmmsError::Internal(format!("failed to serialize expertise: {e}")))?;
        let system_prompt = persona.system_prompt.clone();
        let retrieval_json = serde_json::to_string(&persona.retrieval_config)
            .map_err(|e| {
                UmmsError::Internal(format!("failed to serialize retrieval_config: {e}"))
            })?;
        let created_at = persona.created_at.to_rfc3339();
        let updated_at = persona.updated_at.to_rfc3339();

        let conn = Arc::clone(&self.conn);
        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            conn.execute(
                "INSERT OR REPLACE INTO personas
                     (agent_id, name, role, description, expertise, system_prompt,
                      retrieval_config, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    agent_id_str,
                    name,
                    role,
                    description,
                    expertise_json,
                    system_prompt,
                    retrieval_json,
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
            agent_id = persona.agent_id.as_str(),
            "saved persona"
        );
        Ok(())
    }

    /// Get a single persona by agent ID.
    pub async fn get(&self, agent_id: &AgentId) -> Result<Option<AgentPersona>, UmmsError> {
        let agent_id_str = agent_id.as_str().to_owned();
        let conn = Arc::clone(&self.conn);

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            let mut stmt = conn
                .prepare(
                    "SELECT agent_id, name, role, description, expertise, system_prompt,
                            retrieval_config, created_at, updated_at
                     FROM personas WHERE agent_id = ?1",
                )
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            let row = stmt
                .query_row(params![agent_id_str], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, String>(8)?,
                    ))
                })
                .optional()
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            match row {
                None => Ok(None),
                Some((aid, name, role, desc, expertise_json, sys_prompt, ret_json, created, updated)) => {
                    let persona = parse_persona_row(
                        aid, name, role, desc, expertise_json, sys_prompt, ret_json, created, updated,
                    )?;
                    Ok(Some(persona))
                }
            }
        })
        .await
        .map_err(|e| UmmsError::Internal(format!("spawn_blocking join error: {e}")))?
    }

    /// List all personas.
    pub async fn list(&self) -> Result<Vec<AgentPersona>, UmmsError> {
        let conn = Arc::clone(&self.conn);

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            let mut stmt = conn
                .prepare(
                    "SELECT agent_id, name, role, description, expertise, system_prompt,
                            retrieval_config, created_at, updated_at
                     FROM personas ORDER BY name ASC",
                )
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, String>(8)?,
                    ))
                })
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            let mut personas = Vec::new();
            for row in rows {
                let (aid, name, role, desc, expertise_json, sys_prompt, ret_json, created, updated) =
                    row.map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;
                let persona = parse_persona_row(
                    aid, name, role, desc, expertise_json, sys_prompt, ret_json, created, updated,
                )?;
                personas.push(persona);
            }
            Ok(personas)
        })
        .await
        .map_err(|e| UmmsError::Internal(format!("spawn_blocking join error: {e}")))?
    }

    /// Delete a persona by agent ID. Returns Ok(()) even if the persona did not exist.
    pub async fn delete(&self, agent_id: &AgentId) -> Result<(), UmmsError> {
        let agent_id_str = agent_id.as_str().to_owned();
        let conn = Arc::clone(&self.conn);

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            conn.execute(
                "DELETE FROM personas WHERE agent_id = ?1",
                params![agent_id_str],
            )
            .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;
            Ok::<(), UmmsError>(())
        })
        .await
        .map_err(|e| UmmsError::Internal(format!("spawn_blocking join error: {e}")))??;

        tracing::debug!(agent_id = agent_id.as_str(), "deleted persona");
        Ok(())
    }
}

/// Parse a raw row tuple into an `AgentPersona`.
fn parse_persona_row(
    agent_id_str: String,
    name: String,
    role: String,
    description: String,
    expertise_json: String,
    system_prompt: String,
    retrieval_json: String,
    created_at_str: String,
    updated_at_str: String,
) -> Result<AgentPersona, UmmsError> {
    let agent_id = AgentId::from_str(&agent_id_str)
        .map_err(|e| UmmsError::Internal(format!("invalid agent_id in DB: {e}")))?;
    let expertise: Vec<String> = serde_json::from_str(&expertise_json)
        .map_err(|e| UmmsError::Internal(format!("failed to parse expertise JSON: {e}")))?;
    let retrieval_config: AgentRetrievalConfig = serde_json::from_str(&retrieval_json)
        .map_err(|e| UmmsError::Internal(format!("failed to parse retrieval_config JSON: {e}")))?;
    let created_at: DateTime<Utc> = DateTime::parse_from_rfc3339(&created_at_str)
        .map_err(|e| UmmsError::Internal(format!("failed to parse created_at: {e}")))?
        .with_timezone(&Utc);
    let updated_at: DateTime<Utc> = DateTime::parse_from_rfc3339(&updated_at_str)
        .map_err(|e| UmmsError::Internal(format!("failed to parse updated_at: {e}")))?
        .with_timezone(&Utc);

    Ok(AgentPersona {
        agent_id,
        name,
        role,
        description,
        expertise,
        system_prompt,
        retrieval_config,
        created_at,
        updated_at,
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
    use crate::persona::AgentRetrievalConfig;

    fn make_persona(id: &str, name: &str) -> AgentPersona {
        AgentPersona {
            agent_id: AgentId::from_str(id).unwrap(),
            name: name.to_owned(),
            role: "Engineer".to_owned(),
            description: "A test persona".to_owned(),
            expertise: vec!["rust".to_owned(), "testing".to_owned()],
            system_prompt: "You are helpful.".to_owned(),
            retrieval_config: AgentRetrievalConfig::default(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn save_and_get_roundtrip() {
        let store = PersonaStore::new(":memory:").unwrap();
        let persona = make_persona("test-agent", "Test Agent");

        store.save(&persona).await.unwrap();
        let loaded = store
            .get(&AgentId::from_str("test-agent").unwrap())
            .await
            .unwrap();

        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.name, "Test Agent");
        assert_eq!(loaded.role, "Engineer");
        assert_eq!(loaded.expertise, vec!["rust", "testing"]);
    }

    #[tokio::test]
    async fn get_nonexistent_returns_none() {
        let store = PersonaStore::new(":memory:").unwrap();
        let loaded = store
            .get(&AgentId::from_str("ghost").unwrap())
            .await
            .unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn list_returns_all_sorted() {
        let store = PersonaStore::new(":memory:").unwrap();
        store.save(&make_persona("b-agent", "Bravo")).await.unwrap();
        store.save(&make_persona("a-agent", "Alpha")).await.unwrap();
        store.save(&make_persona("c-agent", "Charlie")).await.unwrap();

        let all = store.list().await.unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].name, "Alpha");
        assert_eq!(all[1].name, "Bravo");
        assert_eq!(all[2].name, "Charlie");
    }

    #[tokio::test]
    async fn save_overwrites_existing() {
        let store = PersonaStore::new(":memory:").unwrap();
        let mut persona = make_persona("test-agent", "Original");
        store.save(&persona).await.unwrap();

        persona.name = "Updated".to_owned();
        persona.expertise = vec!["python".to_owned()];
        store.save(&persona).await.unwrap();

        let loaded = store
            .get(&AgentId::from_str("test-agent").unwrap())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded.name, "Updated");
        assert_eq!(loaded.expertise, vec!["python"]);
    }

    #[tokio::test]
    async fn delete_removes_persona() {
        let store = PersonaStore::new(":memory:").unwrap();
        let persona = make_persona("doomed", "Doomed");
        store.save(&persona).await.unwrap();

        store
            .delete(&AgentId::from_str("doomed").unwrap())
            .await
            .unwrap();
        let loaded = store
            .get(&AgentId::from_str("doomed").unwrap())
            .await
            .unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn delete_nonexistent_is_ok() {
        let store = PersonaStore::new(":memory:").unwrap();
        let result = store
            .delete(&AgentId::from_str("ghost").unwrap())
            .await;
        assert!(result.is_ok());
    }
}
