use std::collections::{HashSet, VecDeque};
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use tokio::sync::Mutex;

use umms_core::error::{StorageError, UmmsError};
use umms_core::traits::KnowledgeGraphStore;
use umms_core::types::*;

/// SQLite-backed knowledge graph store.
///
/// All async trait methods delegate to synchronous `rusqlite` calls wrapped in
/// `tokio::task::spawn_blocking`. The inner `Connection` is protected by a
/// `tokio::sync::Mutex` so only one blocking task accesses it at a time.
pub struct SqliteGraphStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteGraphStore {
    /// Open (or create) a SQLite database at `path` and run migrations.
    ///
    /// Pass `":memory:"` for an in-memory database (useful for tests).
    pub fn new(path: impl AsRef<Path>) -> std::result::Result<Self, UmmsError> {
        let conn = Connection::open(path.as_ref())
            .map_err(|e| StorageError::ConnectionFailed {
                backend: "sqlite-graph".into(),
                reason: e.to_string(),
            })?;

        Self::run_migrations(&conn)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn run_migrations(conn: &Connection) -> std::result::Result<(), UmmsError> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS kg_nodes (
                id TEXT PRIMARY KEY,
                agent_id TEXT,
                node_type TEXT NOT NULL,
                label TEXT NOT NULL,
                properties TEXT,
                importance REAL DEFAULT 0.5,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS kg_edges (
                id TEXT PRIMARY KEY,
                source_id TEXT NOT NULL REFERENCES kg_nodes(id),
                target_id TEXT NOT NULL REFERENCES kg_nodes(id),
                relation TEXT NOT NULL,
                weight REAL DEFAULT 1.0,
                agent_id TEXT,
                created_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_kg_nodes_agent_id ON kg_nodes(agent_id);
            CREATE INDEX IF NOT EXISTS idx_kg_edges_source_id ON kg_edges(source_id);
            CREATE INDEX IF NOT EXISTS idx_kg_edges_target_id ON kg_edges(target_id);
            CREATE INDEX IF NOT EXISTS idx_kg_edges_agent_id ON kg_edges(agent_id);
            ",
        )
        .map_err(|e| StorageError::MigrationFailed(e.to_string()))?;

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers for row ↔ struct conversion
// ---------------------------------------------------------------------------

fn node_type_to_str(t: KgNodeType) -> &'static str {
    match t {
        KgNodeType::Entity => "entity",
        KgNodeType::Concept => "concept",
        KgNodeType::Relation => "relation",
    }
}

fn str_to_node_type(s: &str) -> KgNodeType {
    match s {
        "entity" => KgNodeType::Entity,
        "concept" => KgNodeType::Concept,
        "relation" => KgNodeType::Relation,
        _ => KgNodeType::Entity,
    }
}

fn parse_dt(s: &str) -> DateTime<Utc> {
    s.parse::<DateTime<Utc>>().unwrap_or_else(|_| Utc::now())
}

fn row_to_node(row: &rusqlite::Row<'_>) -> rusqlite::Result<KgNode> {
    let id_str: String = row.get(0)?;
    let agent_str: Option<String> = row.get(1)?;
    let nt_str: String = row.get(2)?;
    let label: String = row.get(3)?;
    let props_str: Option<String> = row.get(4)?;
    let importance: f64 = row.get(5)?;
    let created_str: String = row.get(6)?;
    let updated_str: String = row.get(7)?;

    Ok(KgNode {
        id: NodeId::from_str(&id_str).unwrap_or_else(|_| NodeId::new()),
        agent_id: agent_str.and_then(|s| AgentId::from_str(&s).ok()),
        node_type: str_to_node_type(&nt_str),
        label,
        properties: props_str
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(serde_json::Value::Null),
        #[allow(clippy::cast_possible_truncation)]
        importance: importance as f32,
        created_at: parse_dt(&created_str),
        updated_at: parse_dt(&updated_str),
    })
}

fn row_to_edge(row: &rusqlite::Row<'_>) -> rusqlite::Result<KgEdge> {
    let id_str: String = row.get(0)?;
    let source_str: String = row.get(1)?;
    let target_str: String = row.get(2)?;
    let relation: String = row.get(3)?;
    let weight: f64 = row.get(4)?;
    let agent_str: Option<String> = row.get(5)?;
    let created_str: String = row.get(6)?;

    Ok(KgEdge {
        id: EdgeId::from_str(&id_str).unwrap_or_else(|_| EdgeId::new()),
        source_id: NodeId::from_str(&source_str).unwrap_or_else(|_| NodeId::new()),
        target_id: NodeId::from_str(&target_str).unwrap_or_else(|_| NodeId::new()),
        relation,
        #[allow(clippy::cast_possible_truncation)]
        weight: weight as f32,
        agent_id: agent_str.and_then(|s| AgentId::from_str(&s).ok()),
        created_at: parse_dt(&created_str),
    })
}

// ---------------------------------------------------------------------------
// KnowledgeGraphStore implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl KnowledgeGraphStore for SqliteGraphStore {
    async fn add_node(&self, node: &KgNode) -> umms_core::error::Result<NodeId> {
        let node = node.clone();
        let conn = Arc::clone(&self.conn);

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            conn.execute(
                "INSERT OR REPLACE INTO kg_nodes (id, agent_id, node_type, label, properties, importance, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    node.id.as_str(),
                    node.agent_id.as_ref().map(AgentId::as_str),
                    node_type_to_str(node.node_type),
                    node.label,
                    serde_json::to_string(&node.properties).ok(),
                    f64::from(node.importance),
                    node.created_at.to_rfc3339(),
                    node.updated_at.to_rfc3339(),
                ],
            )
            .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            Ok(node.id)
        })
        .await
        .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?
    }

    async fn add_edge(&self, edge: &KgEdge) -> umms_core::error::Result<EdgeId> {
        let edge = edge.clone();
        let conn = Arc::clone(&self.conn);

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            conn.execute(
                "INSERT OR REPLACE INTO kg_edges (id, source_id, target_id, relation, weight, agent_id, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    edge.id.as_str(),
                    edge.source_id.as_str(),
                    edge.target_id.as_str(),
                    edge.relation,
                    f64::from(edge.weight),
                    edge.agent_id.as_ref().map(AgentId::as_str),
                    edge.created_at.to_rfc3339(),
                ],
            )
            .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            Ok(edge.id)
        })
        .await
        .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?
    }

    async fn find_nodes(
        &self,
        query: &str,
        agent_id: Option<&AgentId>,
        limit: usize,
    ) -> umms_core::error::Result<Vec<KgNode>> {
        let query = format!("%{query}%");
        let agent_id = agent_id.cloned();
        let conn = Arc::clone(&self.conn);

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();

            let (sql, agent_param): (String, Option<String>) = match &agent_id {
                Some(aid) => (
                    "SELECT id, agent_id, node_type, label, properties, importance, created_at, updated_at \
                     FROM kg_nodes WHERE label LIKE ?1 AND (agent_id = ?2 OR agent_id IS NULL) LIMIT ?3"
                        .to_string(),
                    Some(aid.as_str().to_string()),
                ),
                None => (
                    "SELECT id, agent_id, node_type, label, properties, importance, created_at, updated_at \
                     FROM kg_nodes WHERE label LIKE ?1 LIMIT ?2"
                        .to_string(),
                    None,
                ),
            };

            let nodes = if let Some(ref aid) = agent_param {
                let mut stmt = conn.prepare(&sql)
                    .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;
                stmt.query_map(params![query, aid, limit as i64], row_to_node)
                    .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?
                    .filter_map(std::result::Result::ok)
                    .collect()
            } else {
                let mut stmt = conn.prepare(&sql)
                    .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;
                stmt.query_map(params![query, limit as i64], row_to_node)
                    .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?
                    .filter_map(std::result::Result::ok)
                    .collect()
            };

            Ok(nodes)
        })
        .await
        .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?
    }

    async fn get_node(&self, id: &NodeId) -> umms_core::error::Result<Option<KgNode>> {
        let id = id.clone();
        let conn = Arc::clone(&self.conn);

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            let mut stmt = conn
                .prepare(
                    "SELECT id, agent_id, node_type, label, properties, importance, created_at, updated_at \
                     FROM kg_nodes WHERE id = ?1",
                )
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            let node = stmt
                .query_row(params![id.as_str()], row_to_node)
                .ok();

            Ok(node)
        })
        .await
        .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?
    }

    async fn traverse(
        &self,
        start: &NodeId,
        max_hops: usize,
        agent_id: Option<&AgentId>,
    ) -> umms_core::error::Result<(Vec<KgNode>, Vec<KgEdge>)> {
        let start = start.clone();
        let agent_id = agent_id.cloned();
        let conn = Arc::clone(&self.conn);

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();

            let mut visited_ids: HashSet<String> = HashSet::new();
            let mut result_nodes: Vec<KgNode> = Vec::new();
            let mut result_edges: Vec<KgEdge> = Vec::new();
            let mut queue: VecDeque<(String, usize)> = VecDeque::new();

            // Seed the BFS with the start node.
            visited_ids.insert(start.as_str().to_string());
            queue.push_back((start.as_str().to_string(), 0));

            // Load the start node itself.
            {
                let mut stmt = conn
                    .prepare(
                        "SELECT id, agent_id, node_type, label, properties, importance, created_at, updated_at \
                         FROM kg_nodes WHERE id = ?1",
                    )
                    .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

                if let Ok(node) = stmt.query_row(params![start.as_str()], row_to_node) {
                    result_nodes.push(node);
                }
            }

            while let Some((current_id, depth)) = queue.pop_front() {
                if depth >= max_hops {
                    continue;
                }

                // Query edges outgoing and incoming from current node, filtered by agent scope.
                let edge_sql = match &agent_id {
                    Some(_) =>
                        "SELECT id, source_id, target_id, relation, weight, agent_id, created_at \
                         FROM kg_edges \
                         WHERE (source_id = ?1 OR target_id = ?1) \
                           AND (agent_id = ?2 OR agent_id IS NULL)",
                    None =>
                        "SELECT id, source_id, target_id, relation, weight, agent_id, created_at \
                         FROM kg_edges \
                         WHERE (source_id = ?1 OR target_id = ?1)",
                };

                let edges: Vec<KgEdge> = {
                    let mut stmt = conn
                        .prepare(edge_sql)
                        .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

                    let rows = if let Some(ref aid) = agent_id {
                        stmt.query_map(params![current_id, aid.as_str()], row_to_edge)
                    } else {
                        stmt.query_map(params![current_id], row_to_edge)
                    };

                    rows
                        .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?
                        .filter_map(std::result::Result::ok)
                        .collect()
                };

                for edge in edges {
                    // Determine the neighbor node.
                    let neighbor_id = if edge.source_id.as_str() == current_id {
                        edge.target_id.as_str().to_string()
                    } else {
                        edge.source_id.as_str().to_string()
                    };

                    result_edges.push(edge);

                    if visited_ids.contains(&neighbor_id) {
                        continue;
                    }
                    visited_ids.insert(neighbor_id.clone());

                    // Load the neighbor node (respecting agent scope).
                    let node_sql = match &agent_id {
                        Some(_) =>
                            "SELECT id, agent_id, node_type, label, properties, importance, created_at, updated_at \
                             FROM kg_nodes WHERE id = ?1 AND (agent_id = ?2 OR agent_id IS NULL)",
                        None =>
                            "SELECT id, agent_id, node_type, label, properties, importance, created_at, updated_at \
                             FROM kg_nodes WHERE id = ?1",
                    };

                    let mut nstmt = conn
                        .prepare(node_sql)
                        .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

                    let node = if let Some(ref aid) = agent_id {
                        nstmt.query_row(params![neighbor_id, aid.as_str()], row_to_node).ok()
                    } else {
                        nstmt.query_row(params![neighbor_id], row_to_node).ok()
                    };

                    if let Some(node) = node {
                        result_nodes.push(node);
                        queue.push_back((neighbor_id, depth + 1));
                    }
                }
            }

            Ok((result_nodes, result_edges))
        })
        .await
        .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?
    }

    async fn delete_node(&self, id: &NodeId) -> umms_core::error::Result<()> {
        let id = id.clone();
        let conn = Arc::clone(&self.conn);

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();

            // Delete incident edges first.
            conn.execute(
                "DELETE FROM kg_edges WHERE source_id = ?1 OR target_id = ?1",
                params![id.as_str()],
            )
            .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            conn.execute("DELETE FROM kg_nodes WHERE id = ?1", params![id.as_str()])
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            Ok(())
        })
        .await
        .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?
    }

    async fn delete_edge(&self, id: &EdgeId) -> umms_core::error::Result<()> {
        let id = id.clone();
        let conn = Arc::clone(&self.conn);

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            conn.execute("DELETE FROM kg_edges WHERE id = ?1", params![id.as_str()])
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;
            Ok(())
        })
        .await
        .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?
    }

    async fn update_node(
        &self,
        id: &NodeId,
        label: Option<&str>,
        properties: Option<&serde_json::Value>,
        importance: Option<f32>,
    ) -> umms_core::error::Result<()> {
        let id = id.clone();
        let label = label.map(String::from);
        let properties = properties.cloned();
        let conn = Arc::clone(&self.conn);

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            let now = Utc::now().to_rfc3339();

            // Build SET clauses and corresponding parameter values dynamically.
            let mut set_clauses = Vec::new();
            let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

            if let Some(ref lbl) = label {
                set_clauses.push("label = ?");
                param_values.push(Box::new(lbl.clone()));
            }
            if let Some(ref props) = properties {
                set_clauses.push("properties = ?");
                let s = serde_json::to_string(props).unwrap_or_else(|_| "{}".to_string());
                param_values.push(Box::new(s));
            }
            if let Some(imp) = importance {
                set_clauses.push("importance = ?");
                param_values.push(Box::new(f64::from(imp)));
            }

            if set_clauses.is_empty() {
                return Ok(());
            }

            set_clauses.push("updated_at = ?");
            param_values.push(Box::new(now));

            let sql = format!(
                "UPDATE kg_nodes SET {} WHERE id = ?",
                set_clauses.join(", ")
            );

            // Add the WHERE id param.
            param_values.push(Box::new(id.as_str().to_string()));

            let param_refs: Vec<&dyn rusqlite::types::ToSql> =
                param_values.iter().map(|p| p.as_ref()).collect();

            conn.execute(&sql, param_refs.as_slice())
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            Ok(())
        })
        .await
        .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?
    }

    async fn edges_of(&self, node_id: &NodeId) -> umms_core::error::Result<Vec<KgEdge>> {
        let node_id = node_id.clone();
        let conn = Arc::clone(&self.conn);

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            let mut stmt = conn
                .prepare(
                    "SELECT id, source_id, target_id, relation, weight, agent_id, created_at \
                     FROM kg_edges WHERE source_id = ?1 OR target_id = ?1",
                )
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            let edges = stmt
                .query_map(params![node_id.as_str()], row_to_edge)
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?
                .filter_map(std::result::Result::ok)
                .collect();

            Ok(edges)
        })
        .await
        .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?
    }

    async fn nodes_for_agent(
        &self,
        agent_id: &AgentId,
        include_shared: bool,
    ) -> umms_core::error::Result<Vec<KgNode>> {
        let agent_id = agent_id.clone();
        let conn = Arc::clone(&self.conn);

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();

            let sql = if include_shared {
                "SELECT id, agent_id, node_type, label, properties, importance, created_at, updated_at \
                 FROM kg_nodes WHERE agent_id = ?1 OR agent_id IS NULL"
            } else {
                "SELECT id, agent_id, node_type, label, properties, importance, created_at, updated_at \
                 FROM kg_nodes WHERE agent_id = ?1"
            };

            let mut stmt = conn.prepare(sql)
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            let nodes = stmt
                .query_map(params![agent_id.as_str()], row_to_node)
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?
                .filter_map(std::result::Result::ok)
                .collect();

            Ok(nodes)
        })
        .await
        .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?
    }

    async fn merge_nodes(
        &self,
        surviving: &NodeId,
        absorbed: &NodeId,
        merged_properties: serde_json::Value,
    ) -> umms_core::error::Result<Vec<EdgeId>> {
        let surviving = surviving.clone();
        let absorbed = absorbed.clone();
        let conn = Arc::clone(&self.conn);

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            let now = Utc::now().to_rfc3339();
            let props_str = serde_json::to_string(&merged_properties)
                .unwrap_or_else(|_| "{}".to_string());

            // Collect edge IDs that will be redirected (before redirect).
            let mut stmt = conn
                .prepare(
                    "SELECT id FROM kg_edges WHERE source_id = ?1 OR target_id = ?1",
                )
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            let redirected_ids: Vec<EdgeId> = stmt
                .query_map(params![absorbed.as_str()], |row| {
                    let id_str: String = row.get(0)?;
                    Ok(EdgeId::from_str(&id_str).unwrap_or_else(|_| EdgeId::new()))
                })
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?
                .filter_map(std::result::Result::ok)
                .collect();
            drop(stmt);

            // Begin transaction.
            conn.execute_batch("BEGIN IMMEDIATE")
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            let result = (|| -> std::result::Result<(), UmmsError> {
                // (a) Redirect source edges.
                conn.execute(
                    "UPDATE kg_edges SET source_id = ?1 WHERE source_id = ?2",
                    params![surviving.as_str(), absorbed.as_str()],
                )
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

                // (b) Redirect target edges.
                conn.execute(
                    "UPDATE kg_edges SET target_id = ?1 WHERE target_id = ?2",
                    params![surviving.as_str(), absorbed.as_str()],
                )
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

                // (c) Remove duplicate edges (same source+target+relation after redirect).
                // Keep the one with the lowest rowid (i.e. oldest).
                conn.execute(
                    "DELETE FROM kg_edges WHERE rowid NOT IN (
                         SELECT MIN(rowid) FROM kg_edges GROUP BY source_id, target_id, relation
                     )",
                    [],
                )
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

                // Also remove self-loops created by the merge.
                conn.execute(
                    "DELETE FROM kg_edges WHERE source_id = target_id",
                    [],
                )
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

                // (d) Update surviving node properties.
                conn.execute(
                    "UPDATE kg_nodes SET properties = ?1, updated_at = ?2 WHERE id = ?3",
                    params![props_str, now, surviving.as_str()],
                )
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

                // (e) Delete absorbed node.
                conn.execute(
                    "DELETE FROM kg_nodes WHERE id = ?1",
                    params![absorbed.as_str()],
                )
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

                Ok(())
            })();

            match result {
                Ok(()) => {
                    conn.execute_batch("COMMIT")
                        .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;
                    Ok(redirected_ids)
                }
                Err(e) => {
                    let _ = conn.execute_batch("ROLLBACK");
                    Err(e)
                }
            }
        })
        .await
        .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?
    }

    async fn batch_update_edge_weights(
        &self,
        updates: &[(EdgeId, f32)],
    ) -> umms_core::error::Result<()> {
        let updates: Vec<(EdgeId, f32)> = updates.to_vec();
        let conn = Arc::clone(&self.conn);

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();

            conn.execute_batch("BEGIN IMMEDIATE")
                .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

            let result = (|| -> std::result::Result<(), UmmsError> {
                let mut stmt = conn
                    .prepare("UPDATE kg_edges SET weight = ?1 WHERE id = ?2")
                    .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

                for (edge_id, weight) in &updates {
                    stmt.execute(params![f64::from(*weight), edge_id.as_str()])
                        .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;
                }

                Ok(())
            })();

            match result {
                Ok(()) => {
                    conn.execute_batch("COMMIT")
                        .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;
                    Ok(())
                }
                Err(e) => {
                    let _ = conn.execute_batch("ROLLBACK");
                    Err(e)
                }
            }
        })
        .await
        .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?
    }

    async fn find_similar_node_pairs(
        &self,
        agent_id: Option<&AgentId>,
        min_similarity: f32,
        limit: usize,
    ) -> umms_core::error::Result<Vec<(KgNode, KgNode, f32)>> {
        let agent_id = agent_id.cloned();
        let conn = Arc::clone(&self.conn);

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();

            // Load all relevant nodes.
            let sql = match &agent_id {
                Some(_) =>
                    "SELECT id, agent_id, node_type, label, properties, importance, created_at, updated_at \
                     FROM kg_nodes WHERE agent_id = ?1 OR agent_id IS NULL",
                None =>
                    "SELECT id, agent_id, node_type, label, properties, importance, created_at, updated_at \
                     FROM kg_nodes",
            };

            let nodes: Vec<KgNode> = {
                let mut stmt = conn.prepare(sql)
                    .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;

                let rows = if let Some(ref aid) = agent_id {
                    stmt.query_map(params![aid.as_str()], row_to_node)
                } else {
                    stmt.query_map([], row_to_node)
                };

                rows
                    .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?
                    .filter_map(std::result::Result::ok)
                    .collect()
            };

            // Compute pairwise similarity using character bigram Jaccard.
            let bigrams = |s: &str| -> HashSet<(char, char)> {
                let lower: Vec<char> = s.to_lowercase().chars().collect();
                if lower.len() < 2 {
                    return HashSet::new();
                }
                lower.windows(2).map(|w| (w[0], w[1])).collect()
            };

            let node_bigrams: Vec<HashSet<(char, char)>> =
                nodes.iter().map(|n| bigrams(&n.label)).collect();

            let mut pairs: Vec<(usize, usize, f32)> = Vec::new();

            for i in 0..nodes.len() {
                for j in (i + 1)..nodes.len() {
                    let a = &node_bigrams[i];
                    let b = &node_bigrams[j];

                    if a.is_empty() && b.is_empty() {
                        continue;
                    }

                    let intersection = a.intersection(b).count();
                    let union = a.union(b).count();

                    let similarity = if union == 0 {
                        0.0
                    } else {
                        intersection as f32 / union as f32
                    };

                    if similarity >= min_similarity {
                        pairs.push((i, j, similarity));
                    }
                }
            }

            // Sort descending by similarity.
            pairs.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
            pairs.truncate(limit);

            let result = pairs
                .into_iter()
                .map(|(i, j, sim)| (nodes[i].clone(), nodes[j].clone(), sim))
                .collect();

            Ok(result)
        })
        .await
        .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?
    }

    async fn stats(&self, agent_id: Option<&AgentId>) -> umms_core::error::Result<GraphStats> {
        let agent_id = agent_id.cloned();
        let conn = Arc::clone(&self.conn);

        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();

            let (node_count, shared_node_count) = match &agent_id {
                Some(aid) => {
                    let nc: i64 = conn
                        .query_row(
                            "SELECT COUNT(*) FROM kg_nodes WHERE agent_id = ?1 OR agent_id IS NULL",
                            params![aid.as_str()],
                            |row| row.get(0),
                        )
                        .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;
                    let snc: i64 = conn
                        .query_row(
                            "SELECT COUNT(*) FROM kg_nodes WHERE agent_id IS NULL",
                            [],
                            |row| row.get(0),
                        )
                        .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;
                    (nc, snc)
                }
                None => {
                    let nc: i64 = conn
                        .query_row("SELECT COUNT(*) FROM kg_nodes", [], |row| row.get(0))
                        .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;
                    let snc: i64 = conn
                        .query_row(
                            "SELECT COUNT(*) FROM kg_nodes WHERE agent_id IS NULL",
                            [],
                            |row| row.get(0),
                        )
                        .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;
                    (nc, snc)
                }
            };

            let (edge_count, shared_edge_count) = match &agent_id {
                Some(aid) => {
                    let ec: i64 = conn
                        .query_row(
                            "SELECT COUNT(*) FROM kg_edges WHERE agent_id = ?1 OR agent_id IS NULL",
                            params![aid.as_str()],
                            |row| row.get(0),
                        )
                        .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;
                    let sec: i64 = conn
                        .query_row(
                            "SELECT COUNT(*) FROM kg_edges WHERE agent_id IS NULL",
                            [],
                            |row| row.get(0),
                        )
                        .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;
                    (ec, sec)
                }
                None => {
                    let ec: i64 = conn
                        .query_row("SELECT COUNT(*) FROM kg_edges", [], |row| row.get(0))
                        .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;
                    let sec: i64 = conn
                        .query_row(
                            "SELECT COUNT(*) FROM kg_edges WHERE agent_id IS NULL",
                            [],
                            |row| row.get(0),
                        )
                        .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?;
                    (ec, sec)
                }
            };

            Ok(GraphStats {
                node_count: node_count as u64,
                edge_count: edge_count as u64,
                shared_node_count: shared_node_count as u64,
                shared_edge_count: shared_edge_count as u64,
            })
        })
        .await
        .map_err(|e| UmmsError::Storage(StorageError::Sqlite(e.to_string())))?
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_node(id: &str, label: &str, agent_id: Option<&str>) -> KgNode {
        KgNode {
            id: NodeId::from_str(id).unwrap(),
            agent_id: agent_id.map(|a| AgentId::from_str(a).unwrap()),
            node_type: KgNodeType::Entity,
            label: label.to_string(),
            properties: serde_json::json!({}),
            importance: 0.5,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn make_edge(id: &str, src: &str, tgt: &str, relation: &str, agent_id: Option<&str>) -> KgEdge {
        KgEdge {
            id: EdgeId::from_str(id).unwrap(),
            source_id: NodeId::from_str(src).unwrap(),
            target_id: NodeId::from_str(tgt).unwrap(),
            relation: relation.to_string(),
            weight: 1.0,
            agent_id: agent_id.map(|a| AgentId::from_str(a).unwrap()),
            created_at: Utc::now(),
        }
    }

    fn in_memory_store() -> SqliteGraphStore {
        SqliteGraphStore::new(":memory:").expect("in-memory store")
    }

    #[tokio::test]
    async fn add_node_get_node_roundtrip() {
        let store = in_memory_store();
        let node = make_node("node-1", "Rust Language", Some("agent-a"));

        let returned_id = store.add_node(&node).await.unwrap();
        assert_eq!(returned_id.as_str(), "node-1");

        let fetched = store.get_node(&NodeId::from_str("node-1").unwrap()).await.unwrap();
        assert!(fetched.is_some());
        let fetched = fetched.unwrap();
        assert_eq!(fetched.label, "Rust Language");
        assert_eq!(fetched.agent_id.as_ref().unwrap().as_str(), "agent-a");
        assert_eq!(fetched.node_type, KgNodeType::Entity);
    }

    #[tokio::test]
    async fn add_edge_and_verify() {
        let store = in_memory_store();
        store.add_node(&make_node("n1", "Node 1", None)).await.unwrap();
        store.add_node(&make_node("n2", "Node 2", None)).await.unwrap();

        let edge = make_edge("e1", "n1", "n2", "links_to", None);
        let eid = store.add_edge(&edge).await.unwrap();
        assert_eq!(eid.as_str(), "e1");

        // Traverse from n1 should find n2 via the edge.
        let (nodes, edges) = store
            .traverse(&NodeId::from_str("n1").unwrap(), 1, None)
            .await
            .unwrap();

        assert_eq!(nodes.len(), 2); // n1 + n2
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].relation, "links_to");
    }

    #[tokio::test]
    async fn find_nodes_scoped_by_agent() {
        let store = in_memory_store();

        // Shared node (agent_id = None)
        store.add_node(&make_node("shared-1", "Shared Concept", None)).await.unwrap();
        // Agent A private node
        store.add_node(&make_node("a-priv", "Agent A Secret", Some("agent-a"))).await.unwrap();
        // Agent B private node
        store.add_node(&make_node("b-priv", "Agent B Secret", Some("agent-b"))).await.unwrap();

        // Agent A should see shared + own, but not B's.
        let a_nodes = store
            .find_nodes("", Some(&AgentId::from_str("agent-a").unwrap()), 10)
            .await
            .unwrap();
        let a_labels: Vec<&str> = a_nodes.iter().map(|n| n.label.as_str()).collect();
        assert!(a_labels.contains(&"Shared Concept"));
        assert!(a_labels.contains(&"Agent A Secret"));
        assert!(!a_labels.contains(&"Agent B Secret"));

        // Agent B should see shared + own, but not A's.
        let b_nodes = store
            .find_nodes("", Some(&AgentId::from_str("agent-b").unwrap()), 10)
            .await
            .unwrap();
        let b_labels: Vec<&str> = b_nodes.iter().map(|n| n.label.as_str()).collect();
        assert!(b_labels.contains(&"Shared Concept"));
        assert!(b_labels.contains(&"Agent B Secret"));
        assert!(!b_labels.contains(&"Agent A Secret"));

        // No agent filter returns everything.
        let all = store.find_nodes("", None, 10).await.unwrap();
        assert_eq!(all.len(), 3);
    }

    #[tokio::test]
    async fn traverse_two_hops() {
        let store = in_memory_store();

        // Chain: A -> B -> C -> D
        store.add_node(&make_node("a", "A", None)).await.unwrap();
        store.add_node(&make_node("b", "B", None)).await.unwrap();
        store.add_node(&make_node("c", "C", None)).await.unwrap();
        store.add_node(&make_node("d", "D", None)).await.unwrap();

        store.add_edge(&make_edge("e-ab", "a", "b", "r", None)).await.unwrap();
        store.add_edge(&make_edge("e-bc", "b", "c", "r", None)).await.unwrap();
        store.add_edge(&make_edge("e-cd", "c", "d", "r", None)).await.unwrap();

        // 2-hop from A should reach A, B, C but NOT D.
        let (nodes, edges) = store
            .traverse(&NodeId::from_str("a").unwrap(), 2, None)
            .await
            .unwrap();

        let ids: HashSet<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
        assert!(ids.contains("a"));
        assert!(ids.contains("b"));
        assert!(ids.contains("c"));
        assert!(!ids.contains("d"));

        // Edges: e-ab and e-bc should be present.
        assert!(edges.iter().any(|e| e.id.as_str() == "e-ab"));
        assert!(edges.iter().any(|e| e.id.as_str() == "e-bc"));
    }

    #[tokio::test]
    async fn delete_node_cascades_edges() {
        let store = in_memory_store();

        store.add_node(&make_node("x", "X", None)).await.unwrap();
        store.add_node(&make_node("y", "Y", None)).await.unwrap();
        store.add_node(&make_node("z", "Z", None)).await.unwrap();

        store.add_edge(&make_edge("e-xy", "x", "y", "r", None)).await.unwrap();
        store.add_edge(&make_edge("e-yz", "y", "z", "r", None)).await.unwrap();

        // Delete node Y — both edges e-xy and e-yz should also be removed.
        store.delete_node(&NodeId::from_str("y").unwrap()).await.unwrap();

        // Y should be gone.
        let y = store.get_node(&NodeId::from_str("y").unwrap()).await.unwrap();
        assert!(y.is_none());

        // Traversing from X should find only X (no edges left).
        let (nodes, edges) = store
            .traverse(&NodeId::from_str("x").unwrap(), 5, None)
            .await
            .unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].id.as_str(), "x");
        assert!(edges.is_empty());

        // Z should still exist but be isolated.
        let z = store.get_node(&NodeId::from_str("z").unwrap()).await.unwrap();
        assert!(z.is_some());
    }

    // -------------------------------------------------------------------
    // Tests for new methods
    // -------------------------------------------------------------------

    #[tokio::test]
    async fn update_node_changes_only_specified_fields() {
        let store = in_memory_store();
        let node = make_node("upd-1", "Original Label", Some("agent-a"));
        store.add_node(&node).await.unwrap();

        // Update only the label.
        store
            .update_node(
                &NodeId::from_str("upd-1").unwrap(),
                Some("New Label"),
                None,
                None,
            )
            .await
            .unwrap();

        let fetched = store.get_node(&NodeId::from_str("upd-1").unwrap()).await.unwrap().unwrap();
        assert_eq!(fetched.label, "New Label");
        assert_eq!(fetched.importance, 0.5); // unchanged

        // Update only importance.
        store
            .update_node(
                &NodeId::from_str("upd-1").unwrap(),
                None,
                None,
                Some(0.9),
            )
            .await
            .unwrap();

        let fetched = store.get_node(&NodeId::from_str("upd-1").unwrap()).await.unwrap().unwrap();
        assert_eq!(fetched.label, "New Label"); // unchanged from previous update
        assert!((fetched.importance - 0.9).abs() < f32::EPSILON);

        // Update only properties.
        let new_props = serde_json::json!({"key": "value"});
        store
            .update_node(
                &NodeId::from_str("upd-1").unwrap(),
                None,
                Some(&new_props),
                None,
            )
            .await
            .unwrap();

        let fetched = store.get_node(&NodeId::from_str("upd-1").unwrap()).await.unwrap().unwrap();
        assert_eq!(fetched.properties["key"], "value");
        assert_eq!(fetched.label, "New Label"); // still unchanged
    }

    #[tokio::test]
    async fn edges_of_returns_both_incoming_and_outgoing() {
        let store = in_memory_store();
        store.add_node(&make_node("center", "Center", None)).await.unwrap();
        store.add_node(&make_node("left", "Left", None)).await.unwrap();
        store.add_node(&make_node("right", "Right", None)).await.unwrap();

        // left -> center (incoming to center)
        store.add_edge(&make_edge("e-lc", "left", "center", "points_to", None)).await.unwrap();
        // center -> right (outgoing from center)
        store.add_edge(&make_edge("e-cr", "center", "right", "points_to", None)).await.unwrap();

        let edges = store
            .edges_of(&NodeId::from_str("center").unwrap())
            .await
            .unwrap();

        assert_eq!(edges.len(), 2);
        let edge_ids: HashSet<&str> = edges.iter().map(|e| e.id.as_str()).collect();
        assert!(edge_ids.contains("e-lc"));
        assert!(edge_ids.contains("e-cr"));
    }

    #[tokio::test]
    async fn nodes_for_agent_with_and_without_shared() {
        let store = in_memory_store();

        store.add_node(&make_node("shared-1", "Shared", None)).await.unwrap();
        store.add_node(&make_node("priv-a", "Private A", Some("agent-a"))).await.unwrap();
        store.add_node(&make_node("priv-b", "Private B", Some("agent-b"))).await.unwrap();

        // With shared: agent-a sees own + shared.
        let with_shared = store
            .nodes_for_agent(&AgentId::from_str("agent-a").unwrap(), true)
            .await
            .unwrap();
        let ids: HashSet<&str> = with_shared.iter().map(|n| n.id.as_str()).collect();
        assert!(ids.contains("shared-1"));
        assert!(ids.contains("priv-a"));
        assert!(!ids.contains("priv-b"));
        assert_eq!(with_shared.len(), 2);

        // Without shared: agent-a sees only own.
        let without_shared = store
            .nodes_for_agent(&AgentId::from_str("agent-a").unwrap(), false)
            .await
            .unwrap();
        assert_eq!(without_shared.len(), 1);
        assert_eq!(without_shared[0].id.as_str(), "priv-a");
    }

    #[tokio::test]
    async fn merge_nodes_redirects_edges_and_deletes_absorbed() {
        let store = in_memory_store();

        store.add_node(&make_node("survive", "Survivor", None)).await.unwrap();
        store.add_node(&make_node("absorb", "Absorbed", None)).await.unwrap();
        store.add_node(&make_node("other", "Other", None)).await.unwrap();

        // other -> absorb
        store.add_edge(&make_edge("e1", "other", "absorb", "rel", None)).await.unwrap();
        // absorb -> other (outgoing from absorbed)
        store.add_edge(&make_edge("e2", "absorb", "other", "rel2", None)).await.unwrap();

        let redirected = store
            .merge_nodes(
                &NodeId::from_str("survive").unwrap(),
                &NodeId::from_str("absorb").unwrap(),
                serde_json::json!({"merged": true}),
            )
            .await
            .unwrap();

        // Both edges were incident to absorbed, so both should be in redirected list.
        assert_eq!(redirected.len(), 2);

        // Absorbed node should be gone.
        let absorbed = store.get_node(&NodeId::from_str("absorb").unwrap()).await.unwrap();
        assert!(absorbed.is_none());

        // Surviving node should have merged properties.
        let survivor = store.get_node(&NodeId::from_str("survive").unwrap()).await.unwrap().unwrap();
        assert_eq!(survivor.properties["merged"], true);

        // Edges should now reference survive instead of absorb.
        let survivor_edges = store
            .edges_of(&NodeId::from_str("survive").unwrap())
            .await
            .unwrap();
        assert!(!survivor_edges.is_empty());
        for edge in &survivor_edges {
            assert!(
                edge.source_id.as_str() == "survive" || edge.target_id.as_str() == "survive",
                "edge should reference surviving node"
            );
            assert!(
                edge.source_id.as_str() != "absorb" && edge.target_id.as_str() != "absorb",
                "edge should not reference absorbed node"
            );
        }
    }

    #[tokio::test]
    async fn batch_update_edge_weights_changes_weights() {
        let store = in_memory_store();

        store.add_node(&make_node("n1", "N1", None)).await.unwrap();
        store.add_node(&make_node("n2", "N2", None)).await.unwrap();
        store.add_node(&make_node("n3", "N3", None)).await.unwrap();

        store.add_edge(&make_edge("e1", "n1", "n2", "r", None)).await.unwrap();
        store.add_edge(&make_edge("e2", "n2", "n3", "r", None)).await.unwrap();

        // Both start at weight 1.0. Update them.
        store
            .batch_update_edge_weights(&[
                (EdgeId::from_str("e1").unwrap(), 2.5),
                (EdgeId::from_str("e2").unwrap(), 0.1),
            ])
            .await
            .unwrap();

        // Verify via edges_of.
        let edges = store
            .edges_of(&NodeId::from_str("n2").unwrap())
            .await
            .unwrap();

        let e1 = edges.iter().find(|e| e.id.as_str() == "e1").unwrap();
        let e2 = edges.iter().find(|e| e.id.as_str() == "e2").unwrap();
        assert!((e1.weight - 2.5).abs() < f32::EPSILON);
        assert!((e2.weight - 0.1).abs() < f32::EPSILON);
    }

    #[tokio::test]
    async fn find_similar_node_pairs_finds_similar_labels() {
        let store = in_memory_store();

        store.add_node(&make_node("n1", "machine learning", Some("agent-a"))).await.unwrap();
        store.add_node(&make_node("n2", "machine learning algorithms", Some("agent-a"))).await.unwrap();
        store.add_node(&make_node("n3", "quantum physics", Some("agent-a"))).await.unwrap();

        let pairs = store
            .find_similar_node_pairs(
                Some(&AgentId::from_str("agent-a").unwrap()),
                0.3,
                10,
            )
            .await
            .unwrap();

        // "machine learning" and "machine learning algorithms" should be similar.
        // "quantum physics" should not be very similar to either.
        assert!(!pairs.is_empty());

        let top_pair = &pairs[0];
        let labels = [top_pair.0.label.as_str(), top_pair.1.label.as_str()];
        assert!(
            labels.contains(&"machine learning") && labels.contains(&"machine learning algorithms"),
            "top pair should be the two machine learning nodes"
        );
        assert!(top_pair.2 > 0.3);
    }

    #[tokio::test]
    async fn stats_returns_correct_counts() {
        let store = in_memory_store();

        // 2 shared nodes, 1 agent-a node.
        store.add_node(&make_node("s1", "Shared1", None)).await.unwrap();
        store.add_node(&make_node("s2", "Shared2", None)).await.unwrap();
        store.add_node(&make_node("a1", "AgentA", Some("agent-a"))).await.unwrap();

        // 1 shared edge, 1 agent-a edge.
        store.add_edge(&make_edge("es1", "s1", "s2", "r", None)).await.unwrap();
        store.add_edge(&make_edge("ea1", "s1", "a1", "r", Some("agent-a"))).await.unwrap();

        // Stats for agent-a (should see own + shared).
        let s = store
            .stats(Some(&AgentId::from_str("agent-a").unwrap()))
            .await
            .unwrap();
        assert_eq!(s.node_count, 3); // s1, s2, a1
        assert_eq!(s.shared_node_count, 2); // s1, s2
        assert_eq!(s.edge_count, 2); // es1 (shared), ea1 (agent-a)
        assert_eq!(s.shared_edge_count, 1); // es1

        // Stats with no agent filter (all).
        let s_all = store.stats(None).await.unwrap();
        assert_eq!(s_all.node_count, 3);
        assert_eq!(s_all.shared_node_count, 2);
        assert_eq!(s_all.edge_count, 2);
        assert_eq!(s_all.shared_edge_count, 1);
    }
}
