use std::sync::Arc;

use tauri::State;

use umms_api::response::{EncoderStatusResponse, SearchHit, SemanticSearchResponse};
use umms_api::AppState;
use umms_core::traits::{Encoder, VectorStore};

#[tauri::command]
pub async fn encoder_status(
    state: State<'_, Arc<AppState>>,
) -> Result<EncoderStatusResponse, String> {
    match &state.encoder {
        Some(enc) => {
            let stats = enc.stats.snapshot();
            Ok(EncoderStatusResponse {
                available: true,
                model: Some(enc.model_name().to_owned()),
                dimension: Some(enc.dimension()),
                total_requests: stats.total_requests,
                total_texts_encoded: stats.total_texts_encoded,
                total_errors: stats.total_errors,
                total_retries: stats.total_retries,
                avg_latency_ms: stats.avg_latency_ms,
            })
        }
        None => Ok(EncoderStatusResponse {
            available: false,
            model: None,
            dimension: None,
            total_requests: 0,
            total_texts_encoded: 0,
            total_errors: 0,
            total_retries: 0,
            avg_latency_ms: 0.0,
        }),
    }
}

#[tauri::command]
pub async fn encode_text(
    state: State<'_, Arc<AppState>>,
    text: String,
) -> Result<Vec<f32>, String> {
    let encoder = state
        .encoder
        .as_ref()
        .ok_or_else(|| "Encoder not available (GEMINI_API_KEY not set)".to_owned())?;

    encoder
        .encode_text(&text)
        .await
        .map_err(|e| format!("{e}"))
}

#[tauri::command]
pub async fn semantic_search(
    state: State<'_, Arc<AppState>>,
    query: String,
    agent_id: Option<String>,
    top_k: Option<usize>,
    include_shared: Option<bool>,
) -> Result<SemanticSearchResponse, String> {
    let start = std::time::Instant::now();

    let encoder = state
        .encoder
        .as_ref()
        .ok_or_else(|| "Encoder not available".to_owned())?;

    let query_vec = encoder
        .encode_text(&query)
        .await
        .map_err(|e| format!("Encoding failed: {e}"))?;

    let aid = umms_core::types::AgentId::from_str(
        agent_id.as_deref().unwrap_or("coder"),
    )
    .map_err(|e| format!("Invalid agent_id: {e}"))?;

    let results = state
        .vector
        .search(&aid, &query_vec, top_k.unwrap_or(5).min(20), include_shared.unwrap_or(true))
        .await
        .map_err(|e| format!("Search failed: {e}"))?;

    Ok(SemanticSearchResponse {
        query,
        results: results.into_iter().map(|sm| SearchHit { entry: sm.entry, score: sm.score }).collect(),
        latency_ms: start.elapsed().as_millis() as u64,
    })
}
