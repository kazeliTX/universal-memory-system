use std::sync::Arc;

use tauri::State;

use umms_api::response::{
    EncoderStatusResponse, PipelineLatency, PipelineStats, SearchHit, SemanticSearchResponse,
};
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
    _top_k: Option<usize>,
    _include_shared: Option<bool>,
) -> Result<SemanticSearchResponse, String> {
    let aid = umms_core::types::AgentId::from_str(
        agent_id.as_deref().unwrap_or("coder"),
    )
    .map_err(|e| format!("Invalid agent_id: {e}"))?;

    // Use hybrid pipeline when available
    if let Some(ref retriever) = state.retriever {
        let pr = retriever
            .retrieve_with_sources(&query, &aid)
            .await
            .map_err(|e| format!("Retrieval failed: {e}"))?;

        let results: Vec<SearchHit> = pr
            .retrieval
            .entries
            .into_iter()
            .zip(pr.hit_sources.into_iter())
            .map(|(sm, src)| {
                let source = match (src.bm25_rank, src.vector_rank) {
                    (Some(_), Some(_)) => "both",
                    (Some(_), None) => "bm25_only",
                    (None, Some(_)) => "vector_only",
                    _ => "unknown",
                };
                SearchHit {
                    entry: sm.entry,
                    score: sm.score,
                    source: source.to_owned(),
                    bm25_rank: src.bm25_rank,
                    vector_rank: src.vector_rank,
                    bm25_contribution: src.bm25_contribution,
                    vector_contribution: src.vector_contribution,
                }
            })
            .collect();

        return Ok(SemanticSearchResponse {
            query,
            latency: PipelineLatency {
                encode_ms: pr.retrieval.latency.encode_ms,
                recall_ms: pr.retrieval.latency.recall_ms,
                rerank_ms: pr.retrieval.latency.rerank_ms,
                diffusion_ms: pr.retrieval.latency.diffusion_ms,
                total_ms: pr.retrieval.latency.total_ms,
            },
            pipeline: PipelineStats {
                recall_count: pr.recall_count,
                rerank_count: pr.rerank_count,
                diffusion_count: pr.diffusion_count,
                final_count: results.len(),
                bm25_only: pr.bm25_only,
                vector_only: pr.vector_only,
                both: pr.both,
            },
            results,
        });
    }

    // Fallback: pure vector search
    let start = std::time::Instant::now();
    let encoder = state
        .encoder
        .as_ref()
        .ok_or_else(|| "Encoder not available".to_owned())?;

    let query_vec = encoder
        .encode_text(&query)
        .await
        .map_err(|e| format!("Encoding failed: {e}"))?;

    let scored = state
        .vector
        .search(&aid, &query_vec, 10, true)
        .await
        .map_err(|e| format!("Search failed: {e}"))?;

    let results: Vec<SearchHit> = scored
        .into_iter()
        .map(|sm| SearchHit {
            entry: sm.entry,
            score: sm.score,
            source: "vector_only".to_owned(),
            bm25_rank: None,
            vector_rank: None,
            bm25_contribution: 0.0,
            vector_contribution: 0.0,
        })
        .collect();

    Ok(SemanticSearchResponse {
        query,
        latency: PipelineLatency {
            total_ms: start.elapsed().as_millis() as u64,
            ..PipelineLatency::default()
        },
        pipeline: PipelineStats {
            final_count: results.len(),
            vector_only: results.len(),
            ..PipelineStats::default()
        },
        results,
    })
}
