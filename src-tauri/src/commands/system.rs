use std::sync::Arc;

use tauri::State;

use umms_api::AppState;
use umms_api::response::*;
use umms_core::traits::{MemoryCache, VectorStore};
use umms_core::types::*;

#[tauri::command]
pub async fn get_health(state: State<'_, Arc<AppState>>) -> Result<HealthResponse, String> {
    Ok(HealthResponse {
        status: "healthy",
        uptime_secs: state.started_at.elapsed().as_secs(),
        storage: StorageHealth {
            cache: "ok",
            vector: "ok",
            graph: "ok",
            files: "ok",
        },
    })
}

#[tauri::command]
pub async fn get_stats(state: State<'_, Arc<AppState>>) -> Result<StatsResponse, String> {
    let known = ["coder", "researcher", "writer"];

    let (mut l0, mut l1) = (0usize, 0usize);
    for name in &known {
        if let Ok(aid) = AgentId::from_str(name) {
            for entry in state.cache.entries_for_agent(&aid).await {
                match entry.layer {
                    MemoryLayer::SensoryBuffer => l0 += 1,
                    MemoryLayer::WorkingMemory => l1 += 1,
                    _ => {}
                }
            }
        }
    }

    // Total = each agent's private entries + shared entries
    let mut vector_total = 0u64;
    for name in &known {
        if let Ok(aid) = AgentId::from_str(name) {
            vector_total += state.vector.count(&aid, false).await.unwrap_or(0);
        }
    }
    if let Ok(aid) = AgentId::from_str(known[0]) {
        let with_shared = state.vector.count(&aid, true).await.unwrap_or(0);
        let without = state.vector.count(&aid, false).await.unwrap_or(0);
        vector_total += with_shared.saturating_sub(without);
    }

    let gs = state.graph.stats(None).await.unwrap_or_default();

    Ok(StatsResponse {
        cache: CacheStats {
            l0_entries: l0,
            l1_entries: l1,
        },
        vector: VectorStats {
            total_entries: vector_total,
        },
        graph: GraphStatsDto::from(gs),
        agents: known.iter().map(std::string::ToString::to_string).collect(),
    })
}

#[tauri::command]
pub async fn get_metrics(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    Ok(umms_observe::encode_metrics(&state.metrics_registry))
}
