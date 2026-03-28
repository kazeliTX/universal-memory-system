//! Knowledge graph handlers: node listing, detail, traversal, search.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::Json;
use serde::Deserialize;

use umms_core::types::*;

use crate::error::ApiError;
use crate::response::*;
use crate::state::AppState;

/// `GET /api/memories/graph/:agent_id?limit=50`
pub async fn graph_nodes(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
    Query(params): Query<GraphListParams>,
) -> Result<Json<GraphNodesResponse>, ApiError> {
    let aid = AgentId::from_str(&agent_id)
        .map_err(|e| ApiError::BadRequest(format!("invalid agent_id: {e}")))?;

    let mut nodes = state
        .graph
        .nodes_for_agent(&aid, true)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let total = nodes.len();
    nodes.truncate(params.limit.unwrap_or(50));

    Ok(Json(GraphNodesResponse {
        agent_id,
        nodes,
        total,
    }))
}

#[derive(Debug, Deserialize)]
pub struct GraphListParams {
    pub limit: Option<usize>,
}

/// `GET /api/memories/graph/node/:node_id`
pub async fn node_detail(
    State(state): State<Arc<AppState>>,
    Path(node_id): Path<String>,
) -> Result<Json<NodeDetailResponse>, ApiError> {
    let nid = NodeId::from_str(&node_id)
        .map_err(|e| ApiError::BadRequest(format!("invalid node_id: {e}")))?;

    let node = state
        .graph
        .get_node(&nid)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("node {node_id} not found")))?;

    let edges = state
        .graph
        .edges_of(&nid)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(NodeDetailResponse { node, edges }))
}

/// `GET /api/memories/graph/traverse/:node_id?hops=2&agent_id=coder`
pub async fn traverse(
    State(state): State<Arc<AppState>>,
    Path(node_id): Path<String>,
    Query(params): Query<TraverseParams>,
) -> Result<Json<TraverseResponse>, ApiError> {
    let nid = NodeId::from_str(&node_id)
        .map_err(|e| ApiError::BadRequest(format!("invalid node_id: {e}")))?;

    let agent_id = params
        .agent_id
        .as_deref()
        .map(AgentId::from_str)
        .transpose()
        .map_err(|e| ApiError::BadRequest(format!("invalid agent_id: {e}")))?;

    let (nodes, edges) = state
        .graph
        .traverse(&nid, params.hops.unwrap_or(2), agent_id.as_ref())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(TraverseResponse { nodes, edges }))
}

#[derive(Debug, Deserialize)]
pub struct TraverseParams {
    pub hops: Option<usize>,
    pub agent_id: Option<String>,
}

/// `GET /api/memories/graph/search?q=...&agent_id=...&limit=10`
pub async fn graph_search(
    State(state): State<Arc<AppState>>,
    Query(params): Query<GraphSearchParams>,
) -> Result<Json<GraphSearchResponse>, ApiError> {
    let q = params
        .q
        .as_deref()
        .ok_or_else(|| ApiError::BadRequest("missing 'q' parameter".into()))?;

    let agent_id = params
        .agent_id
        .as_deref()
        .map(AgentId::from_str)
        .transpose()
        .map_err(|e| ApiError::BadRequest(format!("invalid agent_id: {e}")))?;

    let nodes = state
        .graph
        .find_nodes(q, agent_id.as_ref(), params.limit.unwrap_or(10))
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(GraphSearchResponse {
        nodes,
        query: q.to_owned(),
    }))
}

#[derive(Debug, Deserialize)]
pub struct GraphSearchParams {
    pub q: Option<String>,
    pub agent_id: Option<String>,
    pub limit: Option<usize>,
}
