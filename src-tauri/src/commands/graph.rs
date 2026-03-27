use std::sync::Arc;

use tauri::State;

use umms_api::response::*;
use umms_api::AppState;
use umms_core::types::*;

#[tauri::command]
pub async fn list_graph_nodes(
    state: State<'_, Arc<AppState>>,
    agent_id: String,
    limit: Option<usize>,
) -> Result<GraphNodesResponse, String> {
    let aid = AgentId::from_str(&agent_id).map_err(|e| e.to_string())?;

    let mut nodes = state
        .graph
        .nodes_for_agent(&aid, true)
        .await
        .map_err(|e| e.to_string())?;

    let total = nodes.len();
    nodes.truncate(limit.unwrap_or(50));

    Ok(GraphNodesResponse {
        agent_id,
        nodes,
        total,
    })
}

#[tauri::command]
pub async fn get_node_detail(
    state: State<'_, Arc<AppState>>,
    node_id: String,
) -> Result<NodeDetailResponse, String> {
    let nid = NodeId::from_str(&node_id).map_err(|e| e.to_string())?;

    let node = state
        .graph
        .get_node(&nid)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("node {node_id} not found"))?;

    let edges = state.graph.edges_of(&nid).await.map_err(|e| e.to_string())?;

    Ok(NodeDetailResponse { node, edges })
}

#[tauri::command]
pub async fn traverse_graph(
    state: State<'_, Arc<AppState>>,
    node_id: String,
    hops: Option<usize>,
    agent_id: Option<String>,
) -> Result<TraverseResponse, String> {
    let nid = NodeId::from_str(&node_id).map_err(|e| e.to_string())?;

    let aid = agent_id
        .as_deref()
        .map(AgentId::from_str)
        .transpose()
        .map_err(|e| e.to_string())?;

    let (nodes, edges) = state
        .graph
        .traverse(&nid, hops.unwrap_or(2), aid.as_ref())
        .await
        .map_err(|e| e.to_string())?;

    Ok(TraverseResponse { nodes, edges })
}

#[tauri::command]
pub async fn search_graph(
    state: State<'_, Arc<AppState>>,
    query: String,
    agent_id: Option<String>,
    limit: Option<usize>,
) -> Result<GraphSearchResponse, String> {
    let aid = agent_id
        .as_deref()
        .map(AgentId::from_str)
        .transpose()
        .map_err(|e| e.to_string())?;

    let nodes = state
        .graph
        .find_nodes(&query, aid.as_ref(), limit.unwrap_or(10))
        .await
        .map_err(|e| e.to_string())?;

    Ok(GraphSearchResponse { nodes, query })
}
