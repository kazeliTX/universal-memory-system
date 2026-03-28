//! SQLite-backed storage for prompt configurations and warehouses.
//!
//! Follows the same `Arc<Mutex<Connection>>` + `spawn_blocking` pattern
//! as [`umms_persona::PersonaStore`].

use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use rusqlite::{Connection, params};
use tokio::sync::Mutex;

use umms_core::error::{StorageError, UmmsError};

use super::types::{AgentPromptConfig, PromptWarehouse};

/// SQLite-backed prompt configuration storage.
pub struct PromptStore {
    conn: Arc<Mutex<Connection>>,
}

impl PromptStore {
    /// Open (or create) a SQLite database at `path` and initialise the schema.
    ///
    /// Pass `":memory:"` for an in-memory database (useful for tests).
    pub fn new(path: impl AsRef<Path>) -> Result<Self, UmmsError> {
        let conn = Connection::open(path.as_ref()).map_err(|e| StorageError::ConnectionFailed {
            backend: "sqlite-prompt".into(),
            reason: e.to_string(),
        })?;

        // Enable WAL mode for better concurrent read performance
        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|e| StorageError::Sqlite(e.to_string()))?;

        Self::run_migrations(&conn)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn run_migrations(conn: &Connection) -> Result<(), UmmsError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS agent_prompts (
                agent_id TEXT PRIMARY KEY,
                config_json TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS prompt_warehouses (
                name TEXT PRIMARY KEY,
                is_global INTEGER NOT NULL DEFAULT 0,
                blocks_json TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );",
        )
        .map_err(|e| StorageError::MigrationFailed(e.to_string()))?;
        Ok(())
    }

    /// Save (upsert) a prompt config.
    pub async fn save_prompt_config(&self, config: &AgentPromptConfig) -> Result<(), UmmsError> {
        let agent_id = config.agent_id.clone();
        let config_json = serde_json::to_string(config)
            .map_err(|e| UmmsError::Internal(format!("failed to serialize prompt config: {e}")))?;
        let updated_at = config.updated_at.to_rfc3339();

        let conn = Arc::clone(&self.conn);
        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            conn.execute(
                "INSERT OR REPLACE INTO agent_prompts (agent_id, config_json, updated_at)
                 VALUES (?1, ?2, ?3)",
                params![agent_id, config_json, updated_at],
            )
            .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;
            Ok::<(), UmmsError>(())
        })
        .await
        .map_err(|e| UmmsError::Internal(format!("spawn_blocking join error: {e}")))??;

        tracing::debug!(agent_id = config.agent_id, "saved prompt config");
        Ok(())
    }

    /// Get prompt config for an agent.
    pub async fn get_prompt_config(
        &self,
        agent_id: &str,
    ) -> Result<Option<AgentPromptConfig>, UmmsError> {
        let agent_id = agent_id.to_owned();
        let conn = Arc::clone(&self.conn);

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            let mut stmt = conn
                .prepare("SELECT config_json FROM agent_prompts WHERE agent_id = ?1")
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            let row = stmt
                .query_row(params![agent_id], |row| row.get::<_, String>(0))
                .optional()
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            match row {
                None => Ok(None),
                Some(json) => {
                    let config: AgentPromptConfig = serde_json::from_str(&json).map_err(|e| {
                        UmmsError::Internal(format!("failed to parse prompt config JSON: {e}"))
                    })?;
                    Ok(Some(config))
                }
            }
        })
        .await
        .map_err(|e| UmmsError::Internal(format!("spawn_blocking join error: {e}")))?
    }

    /// List all prompt configs.
    pub async fn list_prompt_configs(&self) -> Result<Vec<AgentPromptConfig>, UmmsError> {
        let conn = Arc::clone(&self.conn);

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            let mut stmt = conn
                .prepare("SELECT config_json FROM agent_prompts ORDER BY agent_id ASC")
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            let rows = stmt
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            let mut configs = Vec::new();
            for row in rows {
                let json =
                    row.map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;
                let config: AgentPromptConfig = serde_json::from_str(&json).map_err(|e| {
                    UmmsError::Internal(format!("failed to parse prompt config JSON: {e}"))
                })?;
                configs.push(config);
            }
            Ok(configs)
        })
        .await
        .map_err(|e| UmmsError::Internal(format!("spawn_blocking join error: {e}")))?
    }

    /// Delete prompt config for an agent.
    pub async fn delete_prompt_config(&self, agent_id: &str) -> Result<(), UmmsError> {
        let agent_id = agent_id.to_owned();
        let agent_id_log = agent_id.clone();
        let conn = Arc::clone(&self.conn);

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            conn.execute(
                "DELETE FROM agent_prompts WHERE agent_id = ?1",
                params![agent_id],
            )
            .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;
            Ok::<(), UmmsError>(())
        })
        .await
        .map_err(|e| UmmsError::Internal(format!("spawn_blocking join error: {e}")))??;

        tracing::debug!(agent_id = agent_id_log, "deleted prompt config");
        Ok(())
    }

    /// Save (upsert) a warehouse.
    pub async fn save_warehouse(&self, warehouse: &PromptWarehouse) -> Result<(), UmmsError> {
        let name = warehouse.name.clone();
        let is_global = i32::from(warehouse.is_global);
        let blocks_json = serde_json::to_string(&warehouse.blocks)
            .map_err(|e| UmmsError::Internal(format!("failed to serialize warehouse: {e}")))?;
        let updated_at = Utc::now().to_rfc3339();

        let conn = Arc::clone(&self.conn);
        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            conn.execute(
                "INSERT OR REPLACE INTO prompt_warehouses (name, is_global, blocks_json, updated_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![name, is_global, blocks_json, updated_at],
            )
            .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;
            Ok::<(), UmmsError>(())
        })
        .await
        .map_err(|e| UmmsError::Internal(format!("spawn_blocking join error: {e}")))??;

        tracing::debug!(name = warehouse.name, "saved warehouse");
        Ok(())
    }

    /// Get a warehouse by name.
    pub async fn get_warehouse(&self, name: &str) -> Result<Option<PromptWarehouse>, UmmsError> {
        let name = name.to_owned();
        let conn = Arc::clone(&self.conn);

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            let mut stmt = conn
                .prepare(
                    "SELECT name, is_global, blocks_json FROM prompt_warehouses WHERE name = ?1",
                )
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            let row = stmt
                .query_row(params![name], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i32>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .optional()
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            match row {
                None => Ok(None),
                Some((name, is_global, blocks_json)) => {
                    let blocks = serde_json::from_str(&blocks_json).map_err(|e| {
                        UmmsError::Internal(format!("failed to parse warehouse JSON: {e}"))
                    })?;
                    Ok(Some(PromptWarehouse {
                        name,
                        blocks,
                        is_global: is_global != 0,
                    }))
                }
            }
        })
        .await
        .map_err(|e| UmmsError::Internal(format!("spawn_blocking join error: {e}")))?
    }

    /// List all warehouses.
    pub async fn list_warehouses(&self) -> Result<Vec<PromptWarehouse>, UmmsError> {
        let conn = Arc::clone(&self.conn);

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            let mut stmt = conn
                .prepare(
                    "SELECT name, is_global, blocks_json FROM prompt_warehouses ORDER BY name ASC",
                )
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i32>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            let mut warehouses = Vec::new();
            for row in rows {
                let (name, is_global, blocks_json) =
                    row.map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;
                let blocks = serde_json::from_str(&blocks_json).map_err(|e| {
                    UmmsError::Internal(format!("failed to parse warehouse JSON: {e}"))
                })?;
                warehouses.push(PromptWarehouse {
                    name,
                    blocks,
                    is_global: is_global != 0,
                });
            }
            Ok(warehouses)
        })
        .await
        .map_err(|e| UmmsError::Internal(format!("spawn_blocking join error: {e}")))?
    }

    /// Delete a warehouse by name.
    pub async fn delete_warehouse(&self, name: &str) -> Result<(), UmmsError> {
        let name = name.to_owned();
        let name_log = name.clone();
        let conn = Arc::clone(&self.conn);

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            conn.execute(
                "DELETE FROM prompt_warehouses WHERE name = ?1",
                params![name],
            )
            .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;
            Ok::<(), UmmsError>(())
        })
        .await
        .map_err(|e| UmmsError::Internal(format!("spawn_blocking join error: {e}")))??;

        tracing::debug!(name = name_log, "deleted warehouse");
        Ok(())
    }

    /// Get the global warehouse, creating it if it doesn't exist.
    pub async fn get_global_warehouse(&self) -> Result<PromptWarehouse, UmmsError> {
        if let Some(wh) = self.get_warehouse("global").await? {
            Ok(wh)
        } else {
            let wh = PromptWarehouse {
                name: "global".into(),
                blocks: vec![],
                is_global: true,
            };
            self.save_warehouse(&wh).await?;
            Ok(wh)
        }
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
    use crate::prompt::types::*;

    fn make_config(agent_id: &str) -> AgentPromptConfig {
        AgentPromptConfig {
            agent_id: agent_id.into(),
            mode: PromptMode::Modular,
            original_prompt: "test prompt".into(),
            blocks: vec![PromptBlock {
                id: "block_1".into(),
                name: "test".into(),
                block_type: BlockType::System,
                content: "hello".into(),
                variants: vec!["hello".into()],
                selected_variant: 0,
                enabled: true,
                order: 0,
            }],
            preset_path: None,
            preset_content: None,
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn save_and_get_prompt_config() {
        let store = PromptStore::new(":memory:").unwrap();
        let config = make_config("agent-1");

        store.save_prompt_config(&config).await.unwrap();
        let loaded = store.get_prompt_config("agent-1").await.unwrap();

        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.agent_id, "agent-1");
        assert_eq!(loaded.blocks.len(), 1);
        assert_eq!(loaded.mode, PromptMode::Modular);
    }

    #[tokio::test]
    async fn get_nonexistent_returns_none() {
        let store = PromptStore::new(":memory:").unwrap();
        let loaded = store.get_prompt_config("ghost").await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn list_prompt_configs_sorted() {
        let store = PromptStore::new(":memory:").unwrap();
        store
            .save_prompt_config(&make_config("b-agent"))
            .await
            .unwrap();
        store
            .save_prompt_config(&make_config("a-agent"))
            .await
            .unwrap();

        let all = store.list_prompt_configs().await.unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].agent_id, "a-agent");
        assert_eq!(all[1].agent_id, "b-agent");
    }

    #[tokio::test]
    async fn delete_prompt_config() {
        let store = PromptStore::new(":memory:").unwrap();
        store
            .save_prompt_config(&make_config("doomed"))
            .await
            .unwrap();
        store.delete_prompt_config("doomed").await.unwrap();

        let loaded = store.get_prompt_config("doomed").await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn warehouse_crud() {
        let store = PromptStore::new(":memory:").unwrap();

        let wh = PromptWarehouse {
            name: "test-wh".into(),
            blocks: vec![PromptBlock {
                id: "b1".into(),
                name: "block".into(),
                block_type: BlockType::Custom,
                content: "content".into(),
                variants: vec!["content".into()],
                selected_variant: 0,
                enabled: true,
                order: 0,
            }],
            is_global: false,
        };

        store.save_warehouse(&wh).await.unwrap();
        let loaded = store.get_warehouse("test-wh").await.unwrap().unwrap();
        assert_eq!(loaded.blocks.len(), 1);
        assert!(!loaded.is_global);

        let all = store.list_warehouses().await.unwrap();
        assert_eq!(all.len(), 1);

        store.delete_warehouse("test-wh").await.unwrap();
        assert!(store.get_warehouse("test-wh").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn global_warehouse_auto_creates() {
        let store = PromptStore::new(":memory:").unwrap();
        let global = store.get_global_warehouse().await.unwrap();
        assert!(global.is_global);
        assert_eq!(global.name, "global");
        assert!(global.blocks.is_empty());

        // Second call returns same
        let global2 = store.get_global_warehouse().await.unwrap();
        assert!(global2.is_global);
    }
}
