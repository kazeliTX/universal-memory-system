//! Memory browsing handlers: cache (L0/L1), vector (L2).

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::Json;
use serde::Deserialize;

use umms_core::traits::{MemoryCache, VectorStore};
use umms_core::types::*;
use umms_observe::{AuditEventBuilder, AuditEventType};

use crate::response::*;
use crate::state::AppState;

/// `GET /api/memories/cache/:agent_id`
pub async fn cache_entries(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
) -> Result<Json<CacheEntriesResponse>, ApiError> {
    let aid = parse_agent_id(&agent_id)?;

    let entries = state.cache.entries_for_agent(&aid).await;
    let (mut l0, mut l1) = (Vec::new(), Vec::new());
    for entry in entries {
        match entry.layer {
            MemoryLayer::SensoryBuffer => l0.push(entry),
            MemoryLayer::WorkingMemory => l1.push(entry),
            _ => {} // shouldn't happen in cache, but don't panic
        }
    }

    state.audit.record(
        AuditEventBuilder::new(AuditEventType::CacheGet, &aid)
            .details(serde_json::json!({"l0": l0.len(), "l1": l1.len()})),
    );

    Ok(Json(CacheEntriesResponse { agent_id, l0, l1 }))
}

/// Pagination params for vector listing.
#[derive(Debug, Deserialize)]
pub struct VectorListParams {
    #[serde(default)]
    pub offset: u64,
    #[serde(default = "default_limit")]
    pub limit: u64,
    #[serde(default = "default_true")]
    pub include_shared: bool,
}

fn default_limit() -> u64 {
    20
}
fn default_true() -> bool {
    true
}

/// `GET /api/memories/vector/:agent_id?offset=0&limit=20`
pub async fn vector_entries(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
    Query(params): Query<VectorListParams>,
) -> Result<Json<VectorEntriesResponse>, ApiError> {
    let aid = parse_agent_id(&agent_id)?;

    let total = state
        .vector
        .count(&aid, params.include_shared)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let entries = state
        .vector
        .list(&aid, params.offset, params.limit, params.include_shared)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(VectorEntriesResponse {
        agent_id,
        entries,
        total,
        offset: params.offset,
        limit: params.limit,
    }))
}

/// `GET /api/memories/vector/entry/:id`
pub async fn vector_detail(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<MemoryDetailResponse>, ApiError> {
    let memory_id = MemoryId::from_str(&id)
        .map_err(|e| ApiError::BadRequest(format!("invalid memory id: {e}")))?;

    let entry = state
        .vector
        .get(&memory_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("memory {id} not found")))?;

    Ok(Json(MemoryDetailResponse { entry }))
}

// ---------------------------------------------------------------------------
// User feedback rating (ADR-013)
// ---------------------------------------------------------------------------

/// Request body for `POST /api/memories/:memory_id/rate`.
#[derive(Debug, Deserialize)]
pub struct RateMemoryRequest {
    /// User feedback rating in `[-1.0, 1.0]`.
    pub rating: f32,
}

/// Response for the rate endpoint.
#[derive(Debug, serde::Serialize)]
pub struct RateMemoryResponse {
    pub memory_id: String,
    pub user_rating: f32,
}

/// `POST /api/memories/:memory_id/rate`
///
/// Set the user feedback rating on a memory entry for importance scoring.
pub async fn rate_memory(
    State(state): State<Arc<AppState>>,
    Path(memory_id): Path<String>,
    Json(body): Json<RateMemoryRequest>,
) -> Result<Json<RateMemoryResponse>, ApiError> {
    let mid = MemoryId::from_str(&memory_id)
        .map_err(|e| ApiError::BadRequest(format!("invalid memory id: {e}")))?;

    // Validate rating range.
    if !(-1.0..=1.0).contains(&body.rating) {
        return Err(ApiError::BadRequest(
            "rating must be between -1.0 and 1.0".to_owned(),
        ));
    }

    // Verify memory exists.
    state
        .vector
        .get(&mid)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("memory {memory_id} not found")))?;

    // Update user_rating.
    state
        .vector
        .update_user_rating(&mid, Some(body.rating))
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(RateMemoryResponse {
        memory_id,
        user_rating: body.rating,
    }))
}

// ---------------------------------------------------------------------------
// Error handling
// ---------------------------------------------------------------------------

// Re-export ApiError from the shared error module for backward compatibility.
pub use crate::error::ApiError;

/// Parse and validate an agent ID from a path parameter.
fn parse_agent_id(raw: &str) -> Result<AgentId, ApiError> {
    AgentId::from_str(raw).map_err(|e| ApiError::BadRequest(format!("invalid agent_id: {e}")))
}
