//! CozoDB-backed knowledge graph store.
//!
//! Uses CozoDB's embedded Datalog engine with Sled storage backend for
//! persistent storage. All async trait methods delegate to synchronous
//! `run_script` calls wrapped in `tokio::task::spawn_blocking`. The
//! `DbInstance` is `Send + Sync` so it can be shared via `Arc`.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use cozo::{DataValue, DbInstance, ScriptMutability};

use umms_core::error::{StorageError, UmmsError};
use umms_core::traits::KnowledgeGraphStore;
use umms_core::types::*;

// ---------------------------------------------------------------------------
// Error helpers
// ---------------------------------------------------------------------------

fn cozo_err(e: impl std::fmt::Display) -> UmmsError {
    UmmsError::Storage(StorageError::ConnectionFailed {
        backend: "cozo-graph".into(),
        reason: e.to_string(),
    })
}

fn join_err(e: impl std::fmt::Display) -> UmmsError {
    UmmsError::Storage(StorageError::ConnectionFailed {
        backend: "cozo-graph".into(),
        reason: format!("spawn_blocking join error: {e}"),
    })
}

// ---------------------------------------------------------------------------
// Param builder
// ---------------------------------------------------------------------------

fn to_params(pairs: &[(&str, DataValue)]) -> BTreeMap<String, DataValue> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), v.clone()))
        .collect()
}

// ---------------------------------------------------------------------------
// Row conversion helpers
// ---------------------------------------------------------------------------

fn dv_to_string(dv: &DataValue) -> String {
    match dv {
        DataValue::Str(s) => s.to_string(),
        _ => String::new(),
    }
}

fn dv_to_opt_string(dv: &DataValue) -> Option<String> {
    match dv {
        DataValue::Str(s) if s.is_empty() => None,
        DataValue::Str(s) => Some(s.to_string()),
        DataValue::Null => None,
        _ => None,
    }
}

fn dv_to_f32(dv: &DataValue) -> f32 {
    dv.get_float().map_or(0.0, |f| f as f32)
}

fn parse_dt(s: &str) -> DateTime<Utc> {
    s.parse::<DateTime<Utc>>().unwrap_or_else(|_| Utc::now())
}

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

fn row_to_node(row: &[DataValue]) -> Option<KgNode> {
    if row.len() < 8 {
        return None;
    }
    Some(KgNode {
        id: NodeId::from_str(&dv_to_string(&row[0])).unwrap_or_else(|_| NodeId::new()),
        agent_id: dv_to_opt_string(&row[1]).and_then(|s| AgentId::from_str(&s).ok()),
        node_type: str_to_node_type(&dv_to_string(&row[2])),
        label: dv_to_string(&row[3]),
        properties: {
            let s = dv_to_string(&row[4]);
            if s.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::from_str(&s).unwrap_or(serde_json::Value::Null)
            }
        },
        importance: dv_to_f32(&row[5]),
        created_at: parse_dt(&dv_to_string(&row[6])),
        updated_at: parse_dt(&dv_to_string(&row[7])),
    })
}

fn row_to_edge(row: &[DataValue]) -> Option<KgEdge> {
    if row.len() < 7 {
        return None;
    }
    Some(KgEdge {
        id: EdgeId::from_str(&dv_to_string(&row[0])).unwrap_or_else(|_| EdgeId::new()),
        source_id: NodeId::from_str(&dv_to_string(&row[1])).unwrap_or_else(|_| NodeId::new()),
        target_id: NodeId::from_str(&dv_to_string(&row[2])).unwrap_or_else(|_| NodeId::new()),
        relation: dv_to_string(&row[3]),
        weight: dv_to_f32(&row[4]),
        agent_id: dv_to_opt_string(&row[5]).and_then(|s| AgentId::from_str(&s).ok()),
        created_at: parse_dt(&dv_to_string(&row[6])),
    })
}

// ---------------------------------------------------------------------------
// CozoGraphStore
// ---------------------------------------------------------------------------

/// CozoDB-backed knowledge graph store.
///
/// Uses CozoDB's embedded Datalog engine with Sled persistence for durable
/// storage. Graph traversals leverage CozoDB's native recursive Datalog
/// evaluation.
pub struct CozoGraphStore {
    db: Arc<DbInstance>,
}

impl CozoGraphStore {
    /// Open (or create) a CozoDB database backed by Sled at `path`.
    ///
    /// Pass `""` for an in-memory database (useful for tests).
    pub fn new(path: impl AsRef<Path>) -> std::result::Result<Self, UmmsError> {
        let path_str = path.as_ref().to_str().unwrap_or("");

        let db = if path_str.is_empty() {
            DbInstance::new("mem", "", "").map_err(|e| StorageError::ConnectionFailed {
                backend: "cozo-graph".into(),
                reason: e.to_string(),
            })?
        } else {
            // Ensure parent directory exists.
            if let Some(parent) = path.as_ref().parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    StorageError::ConnectionFailed {
                        backend: "cozo-graph".into(),
                        reason: format!("cannot create directory: {e}"),
                    }
                })?;
            }
            DbInstance::new("sled", path_str, "").map_err(|e| {
                StorageError::ConnectionFailed {
                    backend: "cozo-graph".into(),
                    reason: e.to_string(),
                }
            })?
        };

        let store = Self { db: Arc::new(db) };
        store.ensure_relations()?;
        Ok(store)
    }

    /// Create relations (tables) if they don't already exist.
    fn ensure_relations(&self) -> std::result::Result<(), UmmsError> {
        let res = self.db.run_script(
            r"
            :create nodes {
                id: String
                =>
                agent_id: String,
                node_type: String,
                label: String,
                properties: String,
                importance: Float,
                created_at: String,
                updated_at: String
            }
            ",
            Default::default(),
            ScriptMutability::Mutable,
        );
        if let Err(e) = &res {
            let msg = e.to_string();
            if !msg.contains("already exists") && !msg.contains("conflicts with an existing one") {
                return Err(cozo_err(e));
            }
        }

        let res = self.db.run_script(
            r"
            :create edges {
                id: String
                =>
                source_id: String,
                target_id: String,
                relation: String,
                weight: Float,
                agent_id: String,
                created_at: String
            }
            ",
            Default::default(),
            ScriptMutability::Mutable,
        );
        if let Err(e) = &res {
            let msg = e.to_string();
            if !msg.contains("already exists") && !msg.contains("conflicts with an existing one") {
                return Err(cozo_err(e));
            }
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// KnowledgeGraphStore implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl KnowledgeGraphStore for CozoGraphStore {
    async fn add_node(&self, node: &KgNode) -> umms_core::error::Result<NodeId> {
        let node = node.clone();
        let db = Arc::clone(&self.db);

        tokio::task::spawn_blocking(move || {
            let agent_id_str = node
                .agent_id
                .as_ref()
                .map_or_else(String::new, |a| a.as_str().to_owned());
            let props_str =
                serde_json::to_string(&node.properties).unwrap_or_else(|_| "{}".to_owned());

            let params = to_params(&[
                ("id", DataValue::Str(node.id.as_str().into())),
                ("agent_id", DataValue::Str(agent_id_str.into())),
                ("node_type", DataValue::Str(node_type_to_str(node.node_type).into())),
                ("label", DataValue::Str(node.label.clone().into())),
                ("properties", DataValue::Str(props_str.into())),
                ("importance", DataValue::from(f64::from(node.importance))),
                ("created_at", DataValue::Str(node.created_at.to_rfc3339().into())),
                ("updated_at", DataValue::Str(node.updated_at.to_rfc3339().into())),
            ]);

            db.run_script(
                r"
                ?[id, agent_id, node_type, label, properties, importance, created_at, updated_at] <-
                    [[$id, $agent_id, $node_type, $label, $properties, $importance, $created_at, $updated_at]]
                :put nodes {id => agent_id, node_type, label, properties, importance, created_at, updated_at}
                ",
                params,
                ScriptMutability::Mutable,
            )
            .map_err(cozo_err)?;

            Ok(node.id)
        })
        .await
        .map_err(join_err)?
    }

    async fn add_edge(&self, edge: &KgEdge) -> umms_core::error::Result<EdgeId> {
        let edge = edge.clone();
        let db = Arc::clone(&self.db);

        tokio::task::spawn_blocking(move || {
            let agent_id_str = edge
                .agent_id
                .as_ref()
                .map_or_else(String::new, |a| a.as_str().to_owned());

            let params = to_params(&[
                ("id", DataValue::Str(edge.id.as_str().into())),
                ("source_id", DataValue::Str(edge.source_id.as_str().into())),
                ("target_id", DataValue::Str(edge.target_id.as_str().into())),
                ("relation", DataValue::Str(edge.relation.clone().into())),
                ("weight", DataValue::from(f64::from(edge.weight))),
                ("agent_id", DataValue::Str(agent_id_str.into())),
                ("created_at", DataValue::Str(edge.created_at.to_rfc3339().into())),
            ]);

            db.run_script(
                r"
                ?[id, source_id, target_id, relation, weight, agent_id, created_at] <-
                    [[$id, $source_id, $target_id, $relation, $weight, $agent_id, $created_at]]
                :put edges {id => source_id, target_id, relation, weight, agent_id, created_at}
                ",
                params,
                ScriptMutability::Mutable,
            )
            .map_err(cozo_err)?;

            Ok(edge.id)
        })
        .await
        .map_err(join_err)?
    }

    async fn get_node(&self, id: &NodeId) -> umms_core::error::Result<Option<KgNode>> {
        let id = id.clone();
        let db = Arc::clone(&self.db);

        tokio::task::spawn_blocking(move || {
            let params = to_params(&[("target_id", DataValue::Str(id.as_str().into()))]);

            let result = db
                .run_script(
                    r"
                    ?[id, agent_id, node_type, label, properties, importance, created_at, updated_at] :=
                        *nodes{id, agent_id, node_type, label, properties, importance, created_at, updated_at},
                        id = $target_id
                    ",
                    params,
                    ScriptMutability::Immutable,
                )
                .map_err(cozo_err)?;

            Ok(result.rows.first().and_then(|row| row_to_node(row)))
        })
        .await
        .map_err(join_err)?
    }

    async fn delete_node(&self, id: &NodeId) -> umms_core::error::Result<()> {
        let id = id.clone();
        let db = Arc::clone(&self.db);

        tokio::task::spawn_blocking(move || {
            let params = to_params(&[("node_id", DataValue::Str(id.as_str().into()))]);

            // Delete incident edges.
            db.run_script(
                r"
                ?[id] := *edges{id, source_id}, source_id = $node_id
                ?[id] := *edges{id, target_id}, target_id = $node_id
                :rm edges {id}
                ",
                params.clone(),
                ScriptMutability::Mutable,
            )
            .map_err(cozo_err)?;

            // Delete the node.
            db.run_script(
                r"
                ?[id] <- [[$node_id]]
                :rm nodes {id}
                ",
                params,
                ScriptMutability::Mutable,
            )
            .map_err(cozo_err)?;

            Ok(())
        })
        .await
        .map_err(join_err)?
    }

    async fn delete_edge(&self, id: &EdgeId) -> umms_core::error::Result<()> {
        let id = id.clone();
        let db = Arc::clone(&self.db);

        tokio::task::spawn_blocking(move || {
            let params = to_params(&[("id", DataValue::Str(id.as_str().into()))]);

            db.run_script(
                r"
                ?[id] <- [[$id]]
                :rm edges {id}
                ",
                params,
                ScriptMutability::Mutable,
            )
            .map_err(cozo_err)?;

            Ok(())
        })
        .await
        .map_err(join_err)?
    }

    async fn update_node(
        &self,
        id: &NodeId,
        label: Option<&str>,
        properties: Option<&serde_json::Value>,
        importance: Option<f32>,
    ) -> umms_core::error::Result<()> {
        if label.is_none() && properties.is_none() && importance.is_none() {
            return Ok(());
        }

        let id = id.clone();
        let label = label.map(String::from);
        let properties = properties.cloned();
        let db = Arc::clone(&self.db);

        tokio::task::spawn_blocking(move || {
            // Read existing node.
            let read_params = to_params(&[("target_id", DataValue::Str(id.as_str().into()))]);
            let existing = db
                .run_script(
                    r"
                    ?[id, agent_id, node_type, label, properties, importance, created_at, updated_at] :=
                        *nodes{id, agent_id, node_type, label, properties, importance, created_at, updated_at},
                        id = $target_id
                    ",
                    read_params,
                    ScriptMutability::Immutable,
                )
                .map_err(cozo_err)?;

            let row = existing
                .rows
                .first()
                .ok_or_else(|| cozo_err(format!("node {id} not found for update")))?;

            let new_label = label.unwrap_or_else(|| dv_to_string(&row[3]));
            let new_props = properties
                .map(|p| serde_json::to_string(&p).unwrap_or_else(|_| "{}".to_owned()))
                .unwrap_or_else(|| dv_to_string(&row[4]));
            let new_importance = importance.unwrap_or_else(|| dv_to_f32(&row[5]));
            let now = Utc::now().to_rfc3339();

            let params = to_params(&[
                ("id", DataValue::Str(id.as_str().into())),
                ("agent_id", row[1].clone()),
                ("node_type", row[2].clone()),
                ("label", DataValue::Str(new_label.into())),
                ("properties", DataValue::Str(new_props.into())),
                ("importance", DataValue::from(f64::from(new_importance))),
                ("created_at", row[6].clone()),
                ("updated_at", DataValue::Str(now.into())),
            ]);

            db.run_script(
                r"
                ?[id, agent_id, node_type, label, properties, importance, created_at, updated_at] <-
                    [[$id, $agent_id, $node_type, $label, $properties, $importance, $created_at, $updated_at]]
                :put nodes {id => agent_id, node_type, label, properties, importance, created_at, updated_at}
                ",
                params,
                ScriptMutability::Mutable,
            )
            .map_err(cozo_err)?;

            Ok(())
        })
        .await
        .map_err(join_err)?
    }

    async fn find_nodes(
        &self,
        query: &str,
        agent_id: Option<&AgentId>,
        limit: usize,
    ) -> umms_core::error::Result<Vec<KgNode>> {
        let query = query.to_owned();
        let agent_id = agent_id.cloned();
        let db = Arc::clone(&self.db);

        tokio::task::spawn_blocking(move || {
            let (script, params) = match &agent_id {
                Some(aid) => {
                    let p = to_params(&[
                        ("query", DataValue::Str(query.into())),
                        ("agent_id", DataValue::Str(aid.as_str().into())),
                        ("limit", DataValue::from(limit as i64)),
                    ]);
                    (
                        r#"
                        ?[id, agent_id, node_type, label, properties, importance, created_at, updated_at] :=
                            *nodes{id, agent_id, node_type, label, properties, importance, created_at, updated_at},
                            (agent_id = $agent_id or agent_id = ""),
                            str_includes(label, $query)
                        :limit $limit
                        "#,
                        p,
                    )
                }
                None => {
                    let p = to_params(&[
                        ("query", DataValue::Str(query.into())),
                        ("limit", DataValue::from(limit as i64)),
                    ]);
                    (
                        r#"
                        ?[id, agent_id, node_type, label, properties, importance, created_at, updated_at] :=
                            *nodes{id, agent_id, node_type, label, properties, importance, created_at, updated_at},
                            str_includes(label, $query)
                        :limit $limit
                        "#,
                        p,
                    )
                }
            };

            let result = db
                .run_script(script, params, ScriptMutability::Immutable)
                .map_err(cozo_err)?;

            Ok(result.rows.iter().filter_map(|row| row_to_node(row)).collect())
        })
        .await
        .map_err(join_err)?
    }

    async fn traverse(
        &self,
        start: &NodeId,
        max_hops: usize,
        agent_id: Option<&AgentId>,
    ) -> umms_core::error::Result<(Vec<KgNode>, Vec<KgEdge>)> {
        let start = start.clone();
        let agent_id = agent_id.cloned();
        let db = Arc::clone(&self.db);

        tokio::task::spawn_blocking(move || {
            let max_hops_i64 = max_hops as i64;

            // Gather reachable node IDs via recursive Datalog.
            let (node_script, node_params) = match &agent_id {
                Some(aid) => {
                    let p = to_params(&[
                        ("start_id", DataValue::Str(start.as_str().into())),
                        ("agent_id", DataValue::Str(aid.as_str().into())),
                    ]);
                    let script = match max_hops {
                        0 => r#"
                            ?[id, agent_id, node_type, label, properties, importance, created_at, updated_at] :=
                                *nodes{id, agent_id, node_type, label, properties, importance, created_at, updated_at},
                                id = $start_id,
                                (agent_id = $agent_id or agent_id = "")
                        "#,
                        1 => r#"
                            n1[to] := *edges{source_id: $start_id, target_id: to, agent_id: ea}, (ea = $agent_id or ea = "")
                            n1[to] := *edges{source_id: to, target_id: $start_id, agent_id: ea}, (ea = $agent_id or ea = "")
                            reachable[id] := id = $start_id
                            reachable[id] := n1[id]
                            ?[id, agent_id, node_type, label, properties, importance, created_at, updated_at] :=
                                reachable[id],
                                *nodes{id, agent_id, node_type, label, properties, importance, created_at, updated_at},
                                (agent_id = $agent_id or agent_id = "")
                        "#,
                        _ => r#"
                            n1[to] := *edges{source_id: $start_id, target_id: to, agent_id: ea}, (ea = $agent_id or ea = "")
                            n1[to] := *edges{source_id: to, target_id: $start_id, agent_id: ea}, (ea = $agent_id or ea = "")
                            n2[to] := n1[from], *edges{source_id: from, target_id: to, agent_id: ea}, (ea = $agent_id or ea = "")
                            n2[to] := n1[from], *edges{source_id: to, target_id: from, agent_id: ea}, (ea = $agent_id or ea = "")
                            reachable[id] := id = $start_id
                            reachable[id] := n1[id]
                            reachable[id] := n2[id]
                            ?[id, agent_id, node_type, label, properties, importance, created_at, updated_at] :=
                                reachable[id],
                                *nodes{id, agent_id, node_type, label, properties, importance, created_at, updated_at},
                                (agent_id = $agent_id or agent_id = "")
                        "#,
                    };
                    (script, p)
                }
                None => {
                    let p = to_params(&[
                        ("start_id", DataValue::Str(start.as_str().into())),
                    ]);
                    // Unrolled BFS — expand up to max_hops levels explicitly.
                    // CozoDB's Datalog has issues with arithmetic in recursive heads,
                    // so we unroll the recursion for common hop counts (1-3).
                    let script = match max_hops {
                        0 => r"
                            ?[id, agent_id, node_type, label, properties, importance, created_at, updated_at] :=
                                *nodes{id, agent_id, node_type, label, properties, importance, created_at, updated_at},
                                id = $start_id
                        ",
                        1 => r"
                            n1[to] := *edges{source_id: $start_id, target_id: to}
                            n1[to] := *edges{source_id: to, target_id: $start_id}
                            reachable[id] := id = $start_id
                            reachable[id] := n1[id]
                            ?[id, agent_id, node_type, label, properties, importance, created_at, updated_at] :=
                                reachable[id],
                                *nodes{id, agent_id, node_type, label, properties, importance, created_at, updated_at}
                        ",
                        _ => r"
                            n1[to] := *edges{source_id: $start_id, target_id: to}
                            n1[to] := *edges{source_id: to, target_id: $start_id}
                            n2[to] := n1[from], *edges{source_id: from, target_id: to}
                            n2[to] := n1[from], *edges{source_id: to, target_id: from}
                            reachable[id] := id = $start_id
                            reachable[id] := n1[id]
                            reachable[id] := n2[id]
                            ?[id, agent_id, node_type, label, properties, importance, created_at, updated_at] :=
                                reachable[id],
                                *nodes{id, agent_id, node_type, label, properties, importance, created_at, updated_at}
                        ",
                    };
                    (script, p)
                }
            };

            let node_result = db
                .run_script(node_script, node_params, ScriptMutability::Immutable)
                .map_err(cozo_err)?;

            let nodes: Vec<KgNode> = node_result.rows.iter().filter_map(|row| row_to_node(row)).collect();
            let node_ids: HashSet<String> = nodes.iter().map(|n| n.id.as_str().to_owned()).collect();

            // Find edges between reachable nodes using the IDs we already have.
            let node_id_list: Vec<DataValue> = node_ids
                .iter()
                .map(|id| DataValue::Str(id.as_str().into()))
                .collect();
            let edge_params = to_params(&[
                ("node_ids", DataValue::List(node_id_list)),
            ]);
            let edge_script = r"
                input[id] := id in $node_ids
                ?[id, source_id, target_id, relation, weight, agent_id, created_at] :=
                    *edges{id, source_id, target_id, relation, weight, agent_id, created_at},
                    input[source_id],
                    input[target_id]
            ";

            let edge_result = db
                .run_script(edge_script, edge_params, ScriptMutability::Immutable)
                .map_err(cozo_err)?;

            let edges: Vec<KgEdge> = edge_result
                .rows
                .iter()
                .filter_map(|row| {
                    let edge = row_to_edge(row)?;
                    if node_ids.contains(edge.source_id.as_str())
                        && node_ids.contains(edge.target_id.as_str())
                    {
                        Some(edge)
                    } else {
                        None
                    }
                })
                .collect();

            Ok((nodes, edges))
        })
        .await
        .map_err(join_err)?
    }

    async fn edges_of(&self, node_id: &NodeId) -> umms_core::error::Result<Vec<KgEdge>> {
        let node_id = node_id.clone();
        let db = Arc::clone(&self.db);

        tokio::task::spawn_blocking(move || {
            let params = to_params(&[("node_id", DataValue::Str(node_id.as_str().into()))]);

            let result = db
                .run_script(
                    r"
                    ?[id, source_id, target_id, relation, weight, agent_id, created_at] :=
                        *edges{id, source_id, target_id, relation, weight, agent_id, created_at},
                        source_id = $node_id
                    ?[id, source_id, target_id, relation, weight, agent_id, created_at] :=
                        *edges{id, source_id, target_id, relation, weight, agent_id, created_at},
                        target_id = $node_id
                    ",
                    params,
                    ScriptMutability::Immutable,
                )
                .map_err(cozo_err)?;

            Ok(result.rows.iter().filter_map(|row| row_to_edge(row)).collect())
        })
        .await
        .map_err(join_err)?
    }

    async fn nodes_for_agent(
        &self,
        agent_id: &AgentId,
        include_shared: bool,
    ) -> umms_core::error::Result<Vec<KgNode>> {
        let agent_id = agent_id.clone();
        let db = Arc::clone(&self.db);

        tokio::task::spawn_blocking(move || {
            let params = to_params(&[("agent_id", DataValue::Str(agent_id.as_str().into()))]);

            let script = if include_shared {
                r#"
                ?[id, agent_id, node_type, label, properties, importance, created_at, updated_at] :=
                    *nodes{id, agent_id, node_type, label, properties, importance, created_at, updated_at},
                    (agent_id = $agent_id or agent_id = "")
                "#
            } else {
                r"
                ?[id, agent_id, node_type, label, properties, importance, created_at, updated_at] :=
                    *nodes{id, agent_id, node_type, label, properties, importance, created_at, updated_at},
                    agent_id = $agent_id
                "
            };

            let result = db
                .run_script(script, params, ScriptMutability::Immutable)
                .map_err(cozo_err)?;

            Ok(result.rows.iter().filter_map(|row| row_to_node(row)).collect())
        })
        .await
        .map_err(join_err)?
    }

    async fn merge_nodes(
        &self,
        surviving: &NodeId,
        absorbed: &NodeId,
        merged_properties: serde_json::Value,
    ) -> umms_core::error::Result<Vec<EdgeId>> {
        let surviving = surviving.clone();
        let absorbed = absorbed.clone();
        let db = Arc::clone(&self.db);

        tokio::task::spawn_blocking(move || {
            let abs_id = absorbed.as_str().to_owned();
            let surv_id = surviving.as_str().to_owned();

            // 1. Collect edge IDs incident to absorbed.
            let params = to_params(&[("absorbed", DataValue::Str(abs_id.clone().into()))]);
            let edge_ids_result = db
                .run_script(
                    r"
                    ?[id] := *edges{id, source_id}, source_id = $absorbed
                    ?[id] := *edges{id, target_id}, target_id = $absorbed
                    ",
                    params,
                    ScriptMutability::Immutable,
                )
                .map_err(cozo_err)?;

            let redirected_ids: Vec<EdgeId> = edge_ids_result
                .rows
                .iter()
                .filter_map(|row| EdgeId::from_str(&dv_to_string(&row[0])).ok())
                .collect();

            // 2. Read full edge data for absorbed's edges.
            let params = to_params(&[("absorbed", DataValue::Str(abs_id.clone().into()))]);
            let edges_result = db
                .run_script(
                    r"
                    ?[id, source_id, target_id, relation, weight, agent_id, created_at] :=
                        *edges{id, source_id, target_id, relation, weight, agent_id, created_at},
                        source_id = $absorbed
                    ?[id, source_id, target_id, relation, weight, agent_id, created_at] :=
                        *edges{id, source_id, target_id, relation, weight, agent_id, created_at},
                        target_id = $absorbed
                    ",
                    params,
                    ScriptMutability::Immutable,
                )
                .map_err(cozo_err)?;

            // 3. Delete absorbed's edges.
            let del_params = to_params(&[("absorbed", DataValue::Str(abs_id.clone().into()))]);
            db.run_script(
                r"
                ?[id] := *edges{id, source_id}, source_id = $absorbed
                ?[id] := *edges{id, target_id}, target_id = $absorbed
                :rm edges {id}
                ",
                del_params,
                ScriptMutability::Mutable,
            )
            .map_err(cozo_err)?;

            // 4. Re-insert with redirected endpoints, deduplicating.
            let mut seen: HashSet<(String, String, String)> = HashSet::new();

            // Record existing surviving edges.
            let surv_params = to_params(&[("surv", DataValue::Str(surv_id.clone().into()))]);
            let surv_edges = db
                .run_script(
                    r"
                    ?[id, source_id, target_id, relation, weight, agent_id, created_at] :=
                        *edges{id, source_id, target_id, relation, weight, agent_id, created_at},
                        source_id = $surv
                    ?[id, source_id, target_id, relation, weight, agent_id, created_at] :=
                        *edges{id, source_id, target_id, relation, weight, agent_id, created_at},
                        target_id = $surv
                    ",
                    surv_params,
                    ScriptMutability::Immutable,
                )
                .map_err(cozo_err)?;

            for row in &surv_edges.rows {
                seen.insert((dv_to_string(&row[1]), dv_to_string(&row[2]), dv_to_string(&row[3])));
            }

            for row in &edges_result.rows {
                let edge_id = dv_to_string(&row[0]);
                let mut source = dv_to_string(&row[1]);
                let mut target = dv_to_string(&row[2]);
                let relation = dv_to_string(&row[3]);

                if source == abs_id {
                    source = surv_id.clone();
                }
                if target == abs_id {
                    target = surv_id.clone();
                }

                // Skip self-loops.
                if source == target {
                    continue;
                }

                let key = (source.clone(), target.clone(), relation.clone());
                if seen.contains(&key) {
                    continue;
                }
                seen.insert(key);

                let put_params = to_params(&[
                    ("id", DataValue::Str(edge_id.into())),
                    ("source_id", DataValue::Str(source.into())),
                    ("target_id", DataValue::Str(target.into())),
                    ("relation", DataValue::Str(relation.into())),
                    ("weight", row[4].clone()),
                    ("agent_id", row[5].clone()),
                    ("created_at", row[6].clone()),
                ]);

                db.run_script(
                    r"
                    ?[id, source_id, target_id, relation, weight, agent_id, created_at] <-
                        [[$id, $source_id, $target_id, $relation, $weight, $agent_id, $created_at]]
                    :put edges {id => source_id, target_id, relation, weight, agent_id, created_at}
                    ",
                    put_params,
                    ScriptMutability::Mutable,
                )
                .map_err(cozo_err)?;
            }

            // 5. Update surviving node properties.
            let props_str =
                serde_json::to_string(&merged_properties).unwrap_or_else(|_| "{}".to_owned());
            let now = Utc::now().to_rfc3339();

            let read_params = to_params(&[("surv", DataValue::Str(surv_id.clone().into()))]);
            let surv_node = db
                .run_script(
                    r"
                    ?[id, agent_id, node_type, label, properties, importance, created_at, updated_at] :=
                        *nodes{id, agent_id, node_type, label, properties, importance, created_at, updated_at},
                        id = $surv
                    ",
                    read_params,
                    ScriptMutability::Immutable,
                )
                .map_err(cozo_err)?;

            if let Some(row) = surv_node.rows.first() {
                let update_params = to_params(&[
                    ("id", DataValue::Str(surv_id.clone().into())),
                    ("agent_id", row[1].clone()),
                    ("node_type", row[2].clone()),
                    ("label", row[3].clone()),
                    ("properties", DataValue::Str(props_str.into())),
                    ("importance", row[5].clone()),
                    ("created_at", row[6].clone()),
                    ("updated_at", DataValue::Str(now.into())),
                ]);

                db.run_script(
                    r"
                    ?[id, agent_id, node_type, label, properties, importance, created_at, updated_at] <-
                        [[$id, $agent_id, $node_type, $label, $properties, $importance, $created_at, $updated_at]]
                    :put nodes {id => agent_id, node_type, label, properties, importance, created_at, updated_at}
                    ",
                    update_params,
                    ScriptMutability::Mutable,
                )
                .map_err(cozo_err)?;
            }

            // 6. Delete absorbed node.
            let del_params = to_params(&[("absorbed", DataValue::Str(abs_id.into()))]);
            db.run_script(
                r"
                ?[id] <- [[$absorbed]]
                :rm nodes {id}
                ",
                del_params,
                ScriptMutability::Mutable,
            )
            .map_err(cozo_err)?;

            Ok(redirected_ids)
        })
        .await
        .map_err(join_err)?
    }

    async fn batch_update_edge_weights(
        &self,
        updates: &[(EdgeId, f32)],
    ) -> umms_core::error::Result<()> {
        let updates: Vec<(EdgeId, f32)> = updates.to_vec();
        let db = Arc::clone(&self.db);

        tokio::task::spawn_blocking(move || {
            for (edge_id, weight) in &updates {
                let read_params =
                    to_params(&[("eid", DataValue::Str(edge_id.as_str().into()))]);

                let existing = db
                    .run_script(
                        r"
                        ?[id, source_id, target_id, relation, weight, agent_id, created_at] :=
                            *edges{id, source_id, target_id, relation, weight, agent_id, created_at},
                            id = $eid
                        ",
                        read_params,
                        ScriptMutability::Immutable,
                    )
                    .map_err(cozo_err)?;

                if let Some(row) = existing.rows.first() {
                    let update_params = to_params(&[
                        ("id", row[0].clone()),
                        ("source_id", row[1].clone()),
                        ("target_id", row[2].clone()),
                        ("relation", row[3].clone()),
                        ("weight", DataValue::from(f64::from(*weight))),
                        ("agent_id", row[5].clone()),
                        ("created_at", row[6].clone()),
                    ]);

                    db.run_script(
                        r"
                        ?[id, source_id, target_id, relation, weight, agent_id, created_at] <-
                            [[$id, $source_id, $target_id, $relation, $weight, $agent_id, $created_at]]
                        :put edges {id => source_id, target_id, relation, weight, agent_id, created_at}
                        ",
                        update_params,
                        ScriptMutability::Mutable,
                    )
                    .map_err(cozo_err)?;
                }
            }

            Ok(())
        })
        .await
        .map_err(join_err)?
    }

    async fn find_similar_node_pairs(
        &self,
        agent_id: Option<&AgentId>,
        min_similarity: f32,
        limit: usize,
    ) -> umms_core::error::Result<Vec<(KgNode, KgNode, f32)>> {
        let agent_id = agent_id.cloned();
        let db = Arc::clone(&self.db);

        tokio::task::spawn_blocking(move || {
            let (script, params) = match &agent_id {
                Some(aid) => {
                    let p = to_params(&[("agent_id", DataValue::Str(aid.as_str().into()))]);
                    (
                        r#"
                        ?[id, agent_id, node_type, label, properties, importance, created_at, updated_at] :=
                            *nodes{id, agent_id, node_type, label, properties, importance, created_at, updated_at},
                            (agent_id = $agent_id or agent_id = "")
                        "#,
                        p,
                    )
                }
                None => {
                    let p: BTreeMap<String, DataValue> = BTreeMap::new();
                    (
                        r"
                        ?[id, agent_id, node_type, label, properties, importance, created_at, updated_at] :=
                            *nodes{id, agent_id, node_type, label, properties, importance, created_at, updated_at}
                        ",
                        p,
                    )
                }
            };

            let result = db
                .run_script(script, params, ScriptMutability::Immutable)
                .map_err(cozo_err)?;

            let nodes: Vec<KgNode> = result.rows.iter().filter_map(|row| row_to_node(row)).collect();

            // Character bigram Jaccard similarity (same as SQLite store).
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

            pairs.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
            pairs.truncate(limit);

            Ok(pairs
                .into_iter()
                .map(|(i, j, sim)| (nodes[i].clone(), nodes[j].clone(), sim))
                .collect())
        })
        .await
        .map_err(join_err)?
    }

    async fn stats(&self, agent_id: Option<&AgentId>) -> umms_core::error::Result<GraphStats> {
        let agent_id = agent_id.cloned();
        let db = Arc::clone(&self.db);

        tokio::task::spawn_blocking(move || {
            let count_query = |script: &str, params: BTreeMap<String, DataValue>| -> std::result::Result<u64, UmmsError> {
                let result = db
                    .run_script(script, params, ScriptMutability::Immutable)
                    .map_err(cozo_err)?;
                Ok(result.rows.first().map_or(0, |r| dv_to_f32(&r[0]) as u64))
            };

            let empty = BTreeMap::new();

            let (node_count, shared_node_count) = match &agent_id {
                Some(aid) => {
                    let p = to_params(&[("agent_id", DataValue::Str(aid.as_str().into()))]);
                    let nc = count_query(
                        r#"?[count(id)] := *nodes{id, agent_id}, (agent_id = $agent_id or agent_id = "")"#,
                        p,
                    )?;
                    let snc = count_query(
                        r#"?[count(id)] := *nodes{id, agent_id}, agent_id = """#,
                        empty.clone(),
                    )?;
                    (nc, snc)
                }
                None => {
                    let nc = count_query(r"?[count(id)] := *nodes{id}", empty.clone())?;
                    let snc = count_query(
                        r#"?[count(id)] := *nodes{id, agent_id}, agent_id = """#,
                        empty.clone(),
                    )?;
                    (nc, snc)
                }
            };

            let (edge_count, shared_edge_count) = match &agent_id {
                Some(aid) => {
                    let p = to_params(&[("agent_id", DataValue::Str(aid.as_str().into()))]);
                    let ec = count_query(
                        r#"?[count(id)] := *edges{id, agent_id}, (agent_id = $agent_id or agent_id = "")"#,
                        p,
                    )?;
                    let sec = count_query(
                        r#"?[count(id)] := *edges{id, agent_id}, agent_id = """#,
                        empty,
                    )?;
                    (ec, sec)
                }
                None => {
                    let ec = count_query(r"?[count(id)] := *edges{id}", empty.clone())?;
                    let sec = count_query(
                        r#"?[count(id)] := *edges{id, agent_id}, agent_id = """#,
                        empty,
                    )?;
                    (ec, sec)
                }
            };

            Ok(GraphStats {
                node_count,
                edge_count,
                shared_node_count,
                shared_edge_count,
            })
        })
        .await
        .map_err(join_err)?
    }

    async fn clear(&self, agent_id: Option<&AgentId>) -> umms_core::error::Result<(u64, u64)> {
        let agent_id = agent_id.cloned();
        let db = Arc::clone(&self.db);

        tokio::task::spawn_blocking(move || {
            let empty = BTreeMap::new();
            let count_query = |script: &str, params: BTreeMap<String, DataValue>| -> std::result::Result<u64, UmmsError> {
                let result = db
                    .run_script(script, params, ScriptMutability::Immutable)
                    .map_err(cozo_err)?;
                Ok(result.rows.first().map_or(0, |r| dv_to_f32(&r[0]) as u64))
            };

            match &agent_id {
                Some(aid) => {
                    let p = to_params(&[("agent_id", DataValue::Str(aid.as_str().into()))]);

                    let edges_deleted = count_query(
                        r"?[count(id)] := *edges{id, agent_id}, agent_id = $agent_id",
                        p.clone(),
                    )?;
                    let nodes_deleted = count_query(
                        r"?[count(id)] := *nodes{id, agent_id}, agent_id = $agent_id",
                        p.clone(),
                    )?;

                    db.run_script(
                        r"
                        ?[id] := *edges{id, agent_id}, agent_id = $agent_id
                        :rm edges {id}
                        ",
                        p.clone(),
                        ScriptMutability::Mutable,
                    )
                    .map_err(cozo_err)?;

                    db.run_script(
                        r"
                        ?[id] := *nodes{id, agent_id}, agent_id = $agent_id
                        :rm nodes {id}
                        ",
                        p,
                        ScriptMutability::Mutable,
                    )
                    .map_err(cozo_err)?;

                    tracing::info!(
                        agent_id = aid.as_str(),
                        nodes_deleted,
                        edges_deleted,
                        "cozo graph cleared for agent"
                    );

                    Ok((nodes_deleted, edges_deleted))
                }
                None => {
                    let edges_deleted =
                        count_query(r"?[count(id)] := *edges{id}", empty.clone())?;
                    let nodes_deleted =
                        count_query(r"?[count(id)] := *nodes{id}", empty.clone())?;

                    db.run_script(
                        r"
                        ?[id] := *edges{id}
                        :rm edges {id}
                        ",
                        empty.clone(),
                        ScriptMutability::Mutable,
                    )
                    .map_err(cozo_err)?;

                    db.run_script(
                        r"
                        ?[id] := *nodes{id}
                        :rm nodes {id}
                        ",
                        empty,
                        ScriptMutability::Mutable,
                    )
                    .map_err(cozo_err)?;

                    tracing::info!(nodes_deleted, edges_deleted, "cozo graph cleared (all)");

                    Ok((nodes_deleted, edges_deleted))
                }
            }
        })
        .await
        .map_err(join_err)?
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::str::FromStr;

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

    fn make_edge(
        id: &str,
        src: &str,
        tgt: &str,
        relation: &str,
        agent_id: Option<&str>,
    ) -> KgEdge {
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

    fn in_memory_store() -> CozoGraphStore {
        CozoGraphStore::new("").expect("in-memory cozo store")
    }

    #[tokio::test]
    async fn add_node_get_node_roundtrip() {
        let store = in_memory_store();
        let node = make_node("node-1", "Rust Language", Some("agent-a"));

        let returned_id = store.add_node(&node).await.unwrap();
        assert_eq!(returned_id.as_str(), "node-1");

        let fetched = store
            .get_node(&NodeId::from_str("node-1").unwrap())
            .await
            .unwrap();
        assert!(fetched.is_some());
        let fetched = fetched.unwrap();
        assert_eq!(fetched.label, "Rust Language");
        assert_eq!(fetched.agent_id.as_ref().unwrap().as_str(), "agent-a");
        assert_eq!(fetched.node_type, KgNodeType::Entity);
    }

    #[tokio::test]
    async fn add_edge_and_edges_of() {
        let store = in_memory_store();
        store.add_node(&make_node("n1", "Node 1", None)).await.unwrap();
        store.add_node(&make_node("n2", "Node 2", None)).await.unwrap();

        let edge = make_edge("e1", "n1", "n2", "links_to", None);
        let eid = store.add_edge(&edge).await.unwrap();
        assert_eq!(eid.as_str(), "e1");

        let edges = store.edges_of(&NodeId::from_str("n1").unwrap()).await.unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].relation, "links_to");
    }

    #[tokio::test]
    async fn delete_node_cascades_edges() {
        let store = in_memory_store();
        store.add_node(&make_node("x", "X", None)).await.unwrap();
        store.add_node(&make_node("y", "Y", None)).await.unwrap();
        store.add_node(&make_node("z", "Z", None)).await.unwrap();
        store.add_edge(&make_edge("e-xy", "x", "y", "r", None)).await.unwrap();
        store.add_edge(&make_edge("e-yz", "y", "z", "r", None)).await.unwrap();

        store.delete_node(&NodeId::from_str("y").unwrap()).await.unwrap();

        let y = store.get_node(&NodeId::from_str("y").unwrap()).await.unwrap();
        assert!(y.is_none());

        let x_edges = store.edges_of(&NodeId::from_str("x").unwrap()).await.unwrap();
        assert!(x_edges.is_empty());

        let z = store.get_node(&NodeId::from_str("z").unwrap()).await.unwrap();
        assert!(z.is_some());
    }

    #[tokio::test]
    async fn find_nodes_scoped_by_agent() {
        let store = in_memory_store();
        store.add_node(&make_node("shared-1", "Shared Concept", None)).await.unwrap();
        store.add_node(&make_node("a-priv", "Agent A Secret", Some("agent-a"))).await.unwrap();
        store.add_node(&make_node("b-priv", "Agent B Secret", Some("agent-b"))).await.unwrap();

        let a_nodes = store
            .find_nodes("", Some(&AgentId::from_str("agent-a").unwrap()), 10)
            .await
            .unwrap();
        let a_labels: Vec<&str> = a_nodes.iter().map(|n| n.label.as_str()).collect();
        assert!(a_labels.contains(&"Shared Concept"));
        assert!(a_labels.contains(&"Agent A Secret"));
        assert!(!a_labels.contains(&"Agent B Secret"));
    }

    #[tokio::test]
    async fn traverse_two_hops() {
        let store = in_memory_store();
        store.add_node(&make_node("a", "A", None)).await.unwrap();
        store.add_node(&make_node("b", "B", None)).await.unwrap();
        store.add_node(&make_node("c", "C", None)).await.unwrap();
        store.add_node(&make_node("d", "D", None)).await.unwrap();
        store.add_edge(&make_edge("e-ab", "a", "b", "r", None)).await.unwrap();
        store.add_edge(&make_edge("e-bc", "b", "c", "r", None)).await.unwrap();
        store.add_edge(&make_edge("e-cd", "c", "d", "r", None)).await.unwrap();

        let (nodes, edges) = store
            .traverse(&NodeId::from_str("a").unwrap(), 2, None)
            .await
            .unwrap();

        let ids: HashSet<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
        assert!(ids.contains("a"));
        assert!(ids.contains("b"));
        assert!(ids.contains("c"));
        assert!(!ids.contains("d"));
        assert!(edges.iter().any(|e| e.id.as_str() == "e-ab"));
        assert!(edges.iter().any(|e| e.id.as_str() == "e-bc"));
    }

    #[tokio::test]
    async fn merge_nodes_redirects_edges() {
        let store = in_memory_store();
        store.add_node(&make_node("survive", "Survivor", None)).await.unwrap();
        store.add_node(&make_node("absorb", "Absorbed", None)).await.unwrap();
        store.add_node(&make_node("other", "Other", None)).await.unwrap();
        store.add_edge(&make_edge("e1", "other", "absorb", "rel", None)).await.unwrap();
        store.add_edge(&make_edge("e2", "absorb", "other", "rel2", None)).await.unwrap();

        let redirected = store
            .merge_nodes(
                &NodeId::from_str("survive").unwrap(),
                &NodeId::from_str("absorb").unwrap(),
                serde_json::json!({"merged": true}),
            )
            .await
            .unwrap();

        assert_eq!(redirected.len(), 2);

        let absorbed = store.get_node(&NodeId::from_str("absorb").unwrap()).await.unwrap();
        assert!(absorbed.is_none());

        let survivor = store
            .get_node(&NodeId::from_str("survive").unwrap())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(survivor.properties["merged"], true);

        let survivor_edges = store
            .edges_of(&NodeId::from_str("survive").unwrap())
            .await
            .unwrap();
        assert!(!survivor_edges.is_empty());
        for edge in &survivor_edges {
            assert!(edge.source_id.as_str() == "survive" || edge.target_id.as_str() == "survive");
            assert!(edge.source_id.as_str() != "absorb" && edge.target_id.as_str() != "absorb");
        }
    }

    #[tokio::test]
    async fn stats_returns_correct_counts() {
        let store = in_memory_store();
        store.add_node(&make_node("s1", "Shared1", None)).await.unwrap();
        store.add_node(&make_node("s2", "Shared2", None)).await.unwrap();
        store.add_node(&make_node("a1", "AgentA", Some("agent-a"))).await.unwrap();
        store.add_edge(&make_edge("es1", "s1", "s2", "r", None)).await.unwrap();
        store.add_edge(&make_edge("ea1", "s1", "a1", "r", Some("agent-a"))).await.unwrap();

        let s = store
            .stats(Some(&AgentId::from_str("agent-a").unwrap()))
            .await
            .unwrap();
        assert_eq!(s.node_count, 3);
        assert_eq!(s.shared_node_count, 2);
        assert_eq!(s.edge_count, 2);
        assert_eq!(s.shared_edge_count, 1);

        let s_all = store.stats(None).await.unwrap();
        assert_eq!(s_all.node_count, 3);
        assert_eq!(s_all.shared_node_count, 2);
        assert_eq!(s_all.edge_count, 2);
        assert_eq!(s_all.shared_edge_count, 1);
    }

    #[tokio::test]
    async fn agent_isolation() {
        let store = in_memory_store();
        store.add_node(&make_node("a1", "Agent A Node", Some("agent-a"))).await.unwrap();
        store.add_node(&make_node("b1", "Agent B Node", Some("agent-b"))).await.unwrap();

        let a_nodes = store
            .nodes_for_agent(&AgentId::from_str("agent-a").unwrap(), false)
            .await
            .unwrap();
        assert_eq!(a_nodes.len(), 1);
        assert_eq!(a_nodes[0].id.as_str(), "a1");

        let b_nodes = store
            .nodes_for_agent(&AgentId::from_str("agent-b").unwrap(), false)
            .await
            .unwrap();
        assert_eq!(b_nodes.len(), 1);
        assert_eq!(b_nodes[0].id.as_str(), "b1");
    }

    #[tokio::test]
    async fn batch_update_edge_weights_changes_weights() {
        let store = in_memory_store();
        store.add_node(&make_node("n1", "N1", None)).await.unwrap();
        store.add_node(&make_node("n2", "N2", None)).await.unwrap();
        store.add_node(&make_node("n3", "N3", None)).await.unwrap();
        store.add_edge(&make_edge("e1", "n1", "n2", "r", None)).await.unwrap();
        store.add_edge(&make_edge("e2", "n2", "n3", "r", None)).await.unwrap();

        store
            .batch_update_edge_weights(&[
                (EdgeId::from_str("e1").unwrap(), 2.5),
                (EdgeId::from_str("e2").unwrap(), 0.1),
            ])
            .await
            .unwrap();

        let edges = store.edges_of(&NodeId::from_str("n2").unwrap()).await.unwrap();
        let e1 = edges.iter().find(|e| e.id.as_str() == "e1").unwrap();
        let e2 = edges.iter().find(|e| e.id.as_str() == "e2").unwrap();
        assert!((e1.weight - 2.5).abs() < f32::EPSILON);
        assert!((e2.weight - 0.1).abs() < f32::EPSILON);
    }

    #[tokio::test]
    async fn find_similar_node_pairs_works() {
        let store = in_memory_store();
        store.add_node(&make_node("n1", "machine learning", Some("agent-a"))).await.unwrap();
        store
            .add_node(&make_node("n2", "machine learning algorithms", Some("agent-a")))
            .await
            .unwrap();
        store.add_node(&make_node("n3", "quantum physics", Some("agent-a"))).await.unwrap();

        let pairs = store
            .find_similar_node_pairs(Some(&AgentId::from_str("agent-a").unwrap()), 0.3, 10)
            .await
            .unwrap();

        assert!(!pairs.is_empty());
        let top_pair = &pairs[0];
        let labels = [top_pair.0.label.as_str(), top_pair.1.label.as_str()];
        assert!(
            labels.contains(&"machine learning")
                && labels.contains(&"machine learning algorithms")
        );
        assert!(top_pair.2 > 0.3);
    }
}
