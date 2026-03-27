use std::str::FromStr;
use std::sync::Arc;

use tauri::State;

use umms_api::response::{
    ConsolidationReportResponse, DecayResultResponse, EvolutionResultResponse,
    PromoteResultResponse,
};
use umms_api::AppState;
use umms_core::types::AgentId;

#[tauri::command]
pub async fn run_consolidation(
    _state: State<'_, Arc<AppState>>,
    agent_id: String,
) -> Result<ConsolidationReportResponse, String> {
    // Validate agent_id
    let _aid = AgentId::from_str(&agent_id).map_err(|e| format!("invalid agent_id: {e}"))?;

    // Placeholder — will be wired to ConsolidationScheduler
    // when the consolidation crate is complete.
    Ok(ConsolidationReportResponse {
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
    })
}
