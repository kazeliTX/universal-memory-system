//! Agent identity handlers.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::Json;

use umms_core::traits::{KnowledgeGraphStore, MemoryCache, RawFileStore, VectorStore};
use umms_core::types::*;

use crate::handlers::memory::ApiError;
use crate::response::*;
use crate::state::AppState;

/// `GET /api/agents/:agent_id` — per-agent stats aggregated across all layers.
pub async fn agent_detail(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
) -> Result<Json<AgentDetailResponse>, ApiError> {
    let aid = AgentId::from_str(&agent_id)
        .map_err(|e| ApiError::BadRequest(format!("invalid agent_id: {e}")))?;

    // Cache breakdown
    let cache_entries = state.cache.entries_for_agent(&aid).await;
    let mut l0 = 0usize;
    let mut l1 = 0usize;
    for entry in &cache_entries {
        match entry.layer {
            MemoryLayer::SensoryBuffer => l0 += 1,
            MemoryLayer::WorkingMemory => l1 += 1,
            _ => {}
        }
    }

    // Vector count
    let vector_count = state
        .vector
        .count(&aid, false)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Graph stats
    let gs = state
        .graph
        .stats(Some(&aid))
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // File count
    let files = state
        .files
        .list(&aid)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(AgentDetailResponse {
        agent_id,
        cache_l0: l0,
        cache_l1: l1,
        vector_count,
        graph: GraphStatsDto::from(gs),
        file_count: files.len(),
    }))
}
