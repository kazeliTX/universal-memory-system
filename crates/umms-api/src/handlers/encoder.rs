//! Encoder API handlers.

use std::sync::Arc;

use axum::extract::State;
use axum::Json;

use std::str::FromStr;

use umms_core::traits::{Encoder, VectorStore};
use umms_observe::{AuditEventBuilder, AuditEventType};

use crate::AppState;
use crate::response::{
    EncoderStatusResponse, PipelineLatency, PipelineStats, SearchHit, SemanticSearchResponse,
};

/// POST /api/encode — encode a single text into a vector.
pub async fn encode_text(
    State(state): State<Arc<AppState>>,
    Json(body): Json<EncodeTextRequest>,
) -> Result<Json<EncodeTextResponse>, String> {
    let encoder = state
        .encoder
        .as_ref()
        .ok_or_else(|| "Encoder not available (GEMINI_API_KEY not set)".to_owned())?;

    let vector = encoder
        .encode_text(&body.text)
        .await
        .map_err(|e| format!("Encoding failed: {e}"))?;

    state.audit.record(
        AuditEventBuilder::new(AuditEventType::Encode, "_encoder")
            .details(serde_json::json!({
                "action": "encode_text",
                "chars": body.text.len(),
                "dims": vector.len(),
            })),
    );

    Ok(Json(EncodeTextResponse {
        vector,
        model: encoder.model_name().to_owned(),
    }))
}

/// GET /api/encoder/status — encoder backend status and stats.
pub async fn encoder_status(
    State(state): State<Arc<AppState>>,
) -> Json<EncoderStatusResponse> {
    match &state.model_pool {
        Some(pool) => {
            if let Some(stats) = pool.embedding_stats() {
                let snap = stats.snapshot();
                Json(EncoderStatusResponse {
                    available: true,
                    model: Some(pool.model_name().to_owned()),
                    dimension: Some(pool.dimension()),
                    total_requests: snap.total_requests,
                    total_texts_encoded: snap.total_texts_encoded,
                    total_errors: snap.total_errors,
                    total_retries: snap.total_retries,
                    avg_latency_ms: snap.avg_latency_ms,
                })
            } else {
                Json(EncoderStatusResponse {
                    available: true,
                    model: Some(pool.model_name().to_owned()),
                    dimension: Some(pool.dimension()),
                    total_requests: 0,
                    total_texts_encoded: 0,
                    total_errors: 0,
                    total_retries: 0,
                    avg_latency_ms: 0.0,
                })
            }
        }
        None => Json(EncoderStatusResponse {
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

/// POST /api/search — hybrid retrieval pipeline (BM25+Vector+Rerank+Diffusion).
///
/// Falls back to pure vector search if the retrieval pipeline is unavailable.
pub async fn semantic_search(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SemanticSearchRequest>,
) -> Result<Json<SemanticSearchResponse>, String> {
    let agent_id = umms_core::types::AgentId::from_str(
        body.agent_id.as_deref().unwrap_or("coder"),
    )
    .map_err(|e| format!("Invalid agent_id: {e}"))?;

    // Try hybrid pipeline first, fall back to pure vector search
    let (results, latency, pipeline) = if let Some(ref retriever) = state.retriever {
        let pr = retriever
            .retrieve_with_sources(&body.query, &agent_id)
            .await
            .map_err(|e| format!("Retrieval failed: {e}"))?;

        let hits: Vec<SearchHit> = pr
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

        let lat = PipelineLatency {
            encode_ms: pr.retrieval.latency.encode_ms,
            recall_ms: pr.retrieval.latency.recall_ms,
            rerank_ms: pr.retrieval.latency.rerank_ms,
            diffusion_ms: pr.retrieval.latency.diffusion_ms,
            total_ms: pr.retrieval.latency.total_ms,
        };

        let stats = PipelineStats {
            recall_count: pr.recall_count,
            rerank_count: pr.rerank_count,
            diffusion_count: pr.diffusion_count,
            final_count: hits.len(),
            bm25_only: pr.bm25_only,
            vector_only: pr.vector_only,
            both: pr.both,
        };

        (hits, lat, stats)
    } else {
        // Fallback: pure vector search
        let start = std::time::Instant::now();
        let encoder = state
            .encoder
            .as_ref()
            .ok_or_else(|| "Encoder not available (GEMINI_API_KEY not set)".to_owned())?;

        let query_vec = encoder
            .encode_text(&body.query)
            .await
            .map_err(|e| format!("Encoding failed: {e}"))?;

        let top_k = body.top_k.unwrap_or(5).min(20);
        let include_shared = body.include_shared.unwrap_or(true);

        let scored = state
            .vector
            .search(&agent_id, &query_vec, top_k, include_shared)
            .await
            .map_err(|e| format!("Search failed: {e}"))?;

        let hits: Vec<SearchHit> = scored
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

        let lat = PipelineLatency {
            total_ms: start.elapsed().as_millis() as u64,
            ..PipelineLatency::default()
        };

        let stats = PipelineStats {
            final_count: hits.len(),
            vector_only: hits.len(),
            ..PipelineStats::default()
        };

        (hits, lat, stats)
    };

    state.audit.record(
        AuditEventBuilder::new(AuditEventType::Encode, agent_id.as_str().to_owned())
            .details(serde_json::json!({
                "action": "semantic_search",
                "query": &body.query,
                "results": results.len(),
                "latency_ms": latency.total_ms,
            })),
    );

    Ok(Json(SemanticSearchResponse {
        query: body.query,
        results,
        latency,
        pipeline,
    }))
}

#[derive(serde::Deserialize)]
pub struct EncodeTextRequest {
    pub text: String,
}

#[derive(serde::Serialize)]
pub struct EncodeTextResponse {
    pub vector: Vec<f32>,
    pub model: String,
}

#[derive(serde::Deserialize)]
pub struct SemanticSearchRequest {
    pub query: String,
    pub agent_id: Option<String>,
    pub top_k: Option<usize>,
    pub include_shared: Option<bool>,
}
