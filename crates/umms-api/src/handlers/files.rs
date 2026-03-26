//! File storage handlers (L4).

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::Json;

use umms_core::traits::RawFileStore;
use umms_core::types::{AgentId, FromStr};

use crate::handlers::memory::ApiError;
use crate::response::FileListResponse;
use crate::state::AppState;

/// `GET /api/memories/files/:agent_id` — list all files for an agent.
pub async fn file_list(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
) -> Result<Json<FileListResponse>, ApiError> {
    let aid = AgentId::from_str(&agent_id)
        .map_err(|e| ApiError::BadRequest(format!("invalid agent_id: {e}")))?;

    let files = state
        .files
        .list(&aid)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(FileListResponse { agent_id, files }))
}
