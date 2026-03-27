use std::sync::Arc;

use tauri::State;

use umms_api::response::*;
use umms_api::AppState;
use umms_core::traits::{MemoryCache, RawFileStore, VectorStore};
use umms_core::types::*;

#[tauri::command]
pub async fn get_agent_detail(
    state: State<'_, Arc<AppState>>,
    agent_id: String,
) -> Result<AgentDetailResponse, String> {
    let aid = AgentId::from_str(&agent_id).map_err(|e| e.to_string())?;

    let cache_entries = state.cache.entries_for_agent(&aid).await;
    let (mut l0, mut l1) = (0usize, 0usize);
    for entry in &cache_entries {
        match entry.layer {
            MemoryLayer::SensoryBuffer => l0 += 1,
            MemoryLayer::WorkingMemory => l1 += 1,
            _ => {}
        }
    }

    let vector_count = state.vector.count(&aid, false).await.map_err(|e| e.to_string())?;
    let gs = state.graph.stats(Some(&aid)).await.map_err(|e| e.to_string())?;
    let files = state.files.list(&aid).await.map_err(|e| e.to_string())?;

    Ok(AgentDetailResponse {
        agent_id,
        cache_l0: l0,
        cache_l1: l1,
        vector_count,
        graph: GraphStatsDto::from(gs),
        file_count: files.len(),
    })
}
