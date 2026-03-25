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
}
