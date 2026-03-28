//! Consolidation API handlers.
//!
//! Wires the HTTP trigger to the real ConsolidationScheduler, which
//! orchestrates decay, graph evolution, and auto-promotion.

use std::str::FromStr;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use tracing::{error, info};
use umms_consolidation::ConsolidationScheduler;
use umms_core::config;
use umms_core::types::AgentId;
use umms_observe::{AuditEventBuilder, AuditEventType};

use crate::error::ApiError;
use crate::response::{
    ConsolidationReportResponse, DecayResultResponse, EvolutionResultResponse,
    PromoteResultResponse, WkdResultResponse,
};
use crate::state::AppState;

/// `POST /api/consolidation/run/:agent_id` — trigger a consolidation cycle manually.
pub async fn run_consolidation(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
) -> Result<Json<ConsolidationReportResponse>, ApiError> {
    let aid = AgentId::from_str(&agent_id)
        .map_err(|e| ApiError::BadRequest(format!("invalid agent_id: {e}")))?;

    let cfg = config::load_config();

    let scheduler = ConsolidationScheduler::from_config(
        cfg.decay.clone(),
        cfg.graph_evolution.clone(),
        cfg.wkd.clone(),
        cfg.promotion.clone(),
    );

    info!(agent_id = %aid, "Running consolidation cycle via API");

    let report = scheduler
        .run_cycle(&*state.vector, &*state.graph, &aid)
        .await
        .map_err(|e| {
            error!(agent_id = %aid, error = %e, "Consolidation cycle failed");
            ApiError::Internal(format!("Consolidation failed: {e}"))
        })?;

    // Record audit event
    state.audit.record(
        AuditEventBuilder::new(AuditEventType::Promote, &aid).details(serde_json::json!({
            "action": "consolidation_completed",
            "agent_id": &agent_id,
            "decay_updated": report.decay.updated,
            "decay_archived": report.decay.archived,
            "wkd_clusters": report.wkd.clusters_found,
            "wkd_distilled": report.wkd.distilled_created,
            "nodes_merged": report.evolution.nodes_merged,
            "edges_strengthened": report.evolution.edges_strengthened,
            "promoted": report.promotion.promoted,
            "total_ms": report.total_ms,
        })),
    );

    Ok(Json(ConsolidationReportResponse {
        agent_id,
        decay: DecayResultResponse {
            scanned: report.decay.scanned,
            updated: report.decay.updated,
            archived: report.decay.archived,
            elapsed_ms: report.decay.elapsed_ms,
        },
        wkd: WkdResultResponse {
            memories_scanned: report.wkd.memories_scanned,
            clusters_found: report.wkd.clusters_found,
            memories_merged: report.wkd.memories_merged,
            memories_archived: report.wkd.memories_archived,
            distilled_created: report.wkd.distilled_created,
            elapsed_ms: report.wkd.elapsed_ms,
        },
        evolution: EvolutionResultResponse {
            pairs_scanned: report.evolution.pairs_scanned,
            nodes_merged: report.evolution.nodes_merged,
            edges_strengthened: report.evolution.edges_strengthened,
            elapsed_ms: report.evolution.elapsed_ms,
        },
        promotion: PromoteResultResponse {
            scanned: report.promotion.scanned,
            promoted: report.promotion.promoted,
            elapsed_ms: report.promotion.elapsed_ms,
        },
        total_ms: report.total_ms,
        timestamp: report.timestamp.to_rfc3339(),
    }))
}
