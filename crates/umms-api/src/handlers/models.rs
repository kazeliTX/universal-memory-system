//! Model pool API handlers.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;

use crate::AppState;
use crate::response::{ModelStatsResponse, ModelsResponse};

/// GET /api/models — list all registered models and their status.
pub async fn list_models(State(state): State<Arc<AppState>>) -> Json<ModelsResponse> {
    match &state.model_pool {
        Some(pool) => {
            let models: Vec<crate::response::ModelInfoResponse> = pool
                .status()
                .into_iter()
                .map(|m| crate::response::ModelInfoResponse {
                    id: m.id,
                    provider: m.provider,
                    model_name: m.model_name,
                    tasks: m.tasks,
                    dimension: m.dimension,
                    max_tokens: None,
                    available: m.available,
                    stats: m.stats.map(|s| ModelStatsResponse {
                        total_requests: s.total_requests,
                        total_errors: s.total_errors,
                        avg_latency_ms: s.avg_latency_ms,
                    }),
                })
                .collect();

            Json(ModelsResponse {
                pool_available: true,
                models,
            })
        }
        None => Json(ModelsResponse {
            pool_available: false,
            models: Vec::new(),
        }),
    }
}
