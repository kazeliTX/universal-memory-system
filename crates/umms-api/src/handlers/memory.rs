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
        AuditEventBuilder::new(AuditEventType::CacheGet, &agent_id)
            .details(serde_json::json!({"l0": l0.len(), "l1": l1.len()})),
    );

    Ok(Json(CacheEntriesResponse {
        agent_id,
        l0,
        l1,
    }))
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

    let total = state.vector.count(&aid, params.include_shared).await
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
// Error handling
// ---------------------------------------------------------------------------

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// Lightweight API error type.
///
/// Converts to appropriate HTTP status codes. Does NOT expose internal details
/// in production — the `Internal` variant logs the real error and returns a
/// generic message to the caller.
#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    NotFound(String),
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            Self::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            Self::Internal(msg) => {
                tracing::error!(error = %msg, "internal API error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error".to_owned(),
                )
            }
        };

        let body = serde_json::json!({ "error": message });
        (status, Json(body)).into_response()
    }
}

/// Parse and validate an agent ID from a path parameter.
fn parse_agent_id(raw: &str) -> Result<AgentId, ApiError> {
    AgentId::from_str(raw).map_err(|e| ApiError::BadRequest(format!("invalid agent_id: {e}")))
}
