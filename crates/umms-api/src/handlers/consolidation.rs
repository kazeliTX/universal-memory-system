//! Consolidation API handlers.
//!
//! Note: The actual consolidation logic is in umms-consolidation.
//! These handlers provide HTTP triggers for manual runs and status queries.

use std::str::FromStr;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use umms_core::types::AgentId;
use umms_observe::{AuditEventBuilder, AuditEventType};

use crate::response::{
    ConsolidationReportResponse, DecayResultResponse, EvolutionResultResponse,
    PromoteResultResponse,
};
use crate::state::AppState;

/// `POST /api/consolidation/run/:agent_id` — trigger a consolidation cycle manually.
pub async fn run_consolidation(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
) -> Result<Json<ConsolidationReportResponse>, super::memory::ApiError> {
    let _aid = AgentId::from_str(&agent_id)
        .map_err(|e| super::memory::ApiError::BadRequest(format!("invalid agent_id: {e}")))?;

    // For now, return a placeholder since the consolidation scheduler
    // will be wired up when the consolidation crate is complete.
    // This ensures the API and Dashboard are ready.

    // Note: Using Promote as the closest existing AuditEventType.
    // TODO: Add AuditEventType::Consolidation when umms-observe is updated.
    state.audit.record(
        AuditEventBuilder::new(AuditEventType::Promote, agent_id.clone()).details(
            serde_json::json!({
                "action": "consolidation_triggered",
                "agent_id": &agent_id,
            }),
        ),
    );

    Ok(Json(ConsolidationReportResponse {
        agent_id,
        decay: DecayResultResponse {
            scanned: 0,
            updated: 0,
            archived: 0,
            elapsed_ms: 0,
        },
        evolution: EvolutionResultResponse {
            pairs_scanned: 0,
            nodes_merged: 0,
            edges_strengthened: 0,
            elapsed_ms: 0,
        },
        promotion: PromoteResultResponse {
            scanned: 0,
            promoted: 0,
            elapsed_ms: 0,
        },
        total_ms: 0,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}
