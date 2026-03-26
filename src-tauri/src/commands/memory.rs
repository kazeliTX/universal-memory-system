use std::sync::Arc;

use tauri::State;

use umms_api::response::*;
use umms_api::AppState;
use umms_core::traits::{MemoryCache, VectorStore};
use umms_core::types::*;

#[tauri::command]
pub async fn get_cache_entries(
    state: State<'_, Arc<AppState>>,
    agent_id: String,
) -> Result<CacheEntriesResponse, String> {
    let aid = AgentId::from_str(&agent_id).map_err(|e| e.to_string())?;
    let entries = state.cache.entries_for_agent(&aid).await;

    let (mut l0, mut l1) = (Vec::new(), Vec::new());
    for entry in entries {
        match entry.layer {
            MemoryLayer::SensoryBuffer => l0.push(entry),
            MemoryLayer::WorkingMemory => l1.push(entry),
            _ => {}
        }
    }

    Ok(CacheEntriesResponse {
        agent_id,
        l0,
        l1,
    })
}

#[tauri::command]
pub async fn list_vector_entries(
    state: State<'_, Arc<AppState>>,
    agent_id: String,
    offset: Option<u64>,
    limit: Option<u64>,
    include_shared: Option<bool>,
) -> Result<VectorEntriesResponse, String> {
    let aid = AgentId::from_str(&agent_id).map_err(|e| e.to_string())?;
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(20);
    let include_shared = include_shared.unwrap_or(true);

    let total = state
        .vector
        .count(&aid, include_shared)
        .await
        .map_err(|e| e.to_string())?;

    let entries = state
        .vector
        .list(&aid, offset, limit, include_shared)
        .await
        .map_err(|e| e.to_string())?;

    Ok(VectorEntriesResponse {
        agent_id,
        entries,
        total,
        offset,
        limit,
    })
}

#[tauri::command]
pub async fn get_memory_detail(
    state: State<'_, Arc<AppState>>,
    memory_id: String,
) -> Result<MemoryDetailResponse, String> {
    let mid = MemoryId::from_str(&memory_id).map_err(|e| e.to_string())?;

    let entry = state
        .vector
        .get(&mid)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("memory {memory_id} not found"))?;

    Ok(MemoryDetailResponse { entry })
}
