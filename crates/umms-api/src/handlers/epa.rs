//! EPA (Embedding Projection Analysis) API handlers.

use std::str::FromStr;
use std::sync::Arc;

use axum::Json;
use axum::extract::State;

use umms_analyzer::epa::EpaAnalyzer;
use umms_core::types::AgentId;

use crate::AppState;
use crate::error::ApiError;
use crate::response::{ActivatedTagResponse, EpaAnalyzeResponse};

/// POST /api/epa/analyze — run EPA on a query.
pub async fn epa_analyze(
    State(state): State<Arc<AppState>>,
    Json(body): Json<EpaAnalyzeRequest>,
) -> Result<Json<EpaAnalyzeResponse>, ApiError> {
    let tag_store = state
        .tag_store
        .as_ref()
        .ok_or_else(|| ApiError::Internal("Tag system not enabled".into()))?;

    let encoder = state
        .encoder
        .as_ref()
        .ok_or_else(|| ApiError::Internal("Encoder not available".into()))?;

    let agent_id_str = body
        .agent_id
        .as_deref()
        .ok_or_else(|| ApiError::BadRequest("agent_id is required".into()))?;
    let agent_id = AgentId::from_str(agent_id_str)
        .map_err(|e| ApiError::BadRequest(format!("Invalid agent_id: {e}")))?;

    let query_vec = encoder
        .encode_text(&body.query)
        .await
        .map_err(|e| ApiError::Internal(format!("Encoding failed: {e}")))?;

    let umms_config = umms_core::config::load_config();
    let analyzer = EpaAnalyzer::new(Arc::clone(tag_store), umms_config.epa);

    let result = analyzer
        .analyze(&query_vec, &agent_id)
        .await
        .map_err(|e| ApiError::Internal(format!("EPA analysis failed: {e}")))?;

    let activated_tags: Vec<ActivatedTagResponse> = result
        .activated_tags
        .iter()
        .map(|at| ActivatedTagResponse {
            tag_id: at.tag_id.as_str().to_owned(),
            label: at.label.clone(),
            similarity: at.similarity,
        })
        .collect();

    Ok(Json(EpaAnalyzeResponse {
        logic_depth: result.logic_depth,
        cross_domain_resonance: result.cross_domain_resonance,
        activated_tags,
        alpha: result.alpha,
        num_semantic_axes: result.semantic_axes.len(),
    }))
}

#[derive(serde::Deserialize)]
pub struct EpaAnalyzeRequest {
    pub query: String,
    pub agent_id: Option<String>,
}
