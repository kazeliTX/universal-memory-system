use std::str::FromStr;
use std::sync::Arc;

use tauri::State;
use tracing::{error, info};

use umms_api::AppState;
use umms_api::response::{
    ConsolidationReportResponse, DecayResultResponse, EvolutionResultResponse,
    PromoteResultResponse, WkdResultResponse,
};
use umms_consolidation::ConsolidationScheduler;
use umms_core::config;
use umms_core::types::AgentId;

#[tauri::command]
pub async fn run_consolidation(
    state: State<'_, Arc<AppState>>,
    agent_id: String,
) -> Result<ConsolidationReportResponse, String> {
    let aid = AgentId::from_str(&agent_id).map_err(|e| format!("invalid agent_id: {e}"))?;

    let cfg = config::load_config();

    let scheduler = ConsolidationScheduler::from_config(
        cfg.decay.clone(),
        cfg.graph_evolution.clone(),
        cfg.wkd.clone(),
        cfg.promotion.clone(),
    );

    info!(agent_id = %aid, "Running consolidation cycle via Tauri command");

    let report = scheduler
        .run_cycle(&*state.vector, &*state.graph, &aid)
        .await
        .map_err(|e| {
            error!(agent_id = %aid, error = %e, "Consolidation cycle failed");
            format!("Consolidation failed: {e}")
        })?;

    Ok(ConsolidationReportResponse {
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
    })
}
