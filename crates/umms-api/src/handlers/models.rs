//! Model pool API handlers.

use std::sync::Arc;

use axum::extract::State;
use axum::Json;

use crate::AppState;
use crate::response::ModelsResponse;

/// GET /api/models — list all registered models and their status.
pub async fn list_models(
    State(state): State<Arc<AppState>>,
) -> Json<ModelsResponse> {
    match &state.model_pool {
        Some(pool) => {
            let models: Vec<crate::response::ModelInfoResponse> = pool
                .models()
                .into_iter()
                .map(|m| crate::response::ModelInfoResponse {
                    id: m.id,
                    provider: m.provider,
                    model_name: m.model_name,
                    tasks: m.tasks.iter().map(|t| t.to_string()).collect(),
                    dimension: m.dimension,
                    max_tokens: m.max_tokens,
                    available: m.available,
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
