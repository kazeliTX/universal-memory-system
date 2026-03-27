//! Document ingestion API handlers.

use std::str::FromStr;
use std::sync::Arc;

use axum::extract::State;
use axum::Json;

use umms_core::config;
use umms_core::types::{AgentId, IsolationScope};
use umms_observe::{AuditEventBuilder, AuditEventType};
use umms_retriever::ingest::chunker::ChunkerConfig;
use umms_retriever::ingest::pipeline::IngestPipeline;
use umms_retriever::ingest::skeleton::DocSkeleton;
use umms_retriever::ingest::tag_extractor::TagExtractor;
use umms_retriever::tokenizer;

use crate::AppState;
use crate::response::{ChunkDetailResponse, IngestLatencyResponse, IngestResponse};

/// POST /api/ingest — ingest a document into the memory system.
pub async fn ingest_document(
    State(state): State<Arc<AppState>>,
    Json(body): Json<IngestRequest>,
) -> Result<Json<IngestResponse>, String> {
    let encoder = state
        .encoder
        .as_ref()
        .ok_or_else(|| "Encoder not available (GEMINI_API_KEY not set)".to_owned())?;

    let agent_id = AgentId::from_str(body.agent_id.as_deref().unwrap_or("coder"))
        .map_err(|e| format!("Invalid agent_id: {e}"))?;

    let scope = match body.scope.as_deref() {
        Some("shared") => IsolationScope::Shared,
        _ => IsolationScope::Private,
    };

    let tags = body.tags.unwrap_or_default();

    let chunker_config = ChunkerConfig {
        target_size: body.chunk_size.unwrap_or(1500),
        overlap: body.chunk_overlap.unwrap_or(200),
    };

    let enc_arc: Arc<dyn umms_core::traits::Encoder> = Arc::clone(encoder) as _;
    let vec_arc: Arc<dyn umms_core::traits::VectorStore> = Arc::clone(&state.vector) as _;

    let umms_config = config::load_config();
    let tok = tokenizer::build_tokenizer(
        &umms_config.tag.tokenizer,
        Some(Arc::clone(encoder) as Arc<dyn umms_core::traits::Encoder>),
    );

    let mut pipeline = IngestPipeline::new(
        enc_arc,
        vec_arc,
        Arc::clone(&state.bm25),
        chunker_config,
    );

    // Wire up LLM-powered skeleton extraction if model pool is available
    if let Some(ref pool) = state.model_pool {
        pipeline = pipeline.with_model_pool(Arc::clone(pool));
    }

    // Wire up tag extraction if tag system is enabled
    if umms_config.tag.enabled && umms_config.tag.auto_extract {
        if let Some(ref tag_store) = state.tag_store {
            let extractor = Arc::new(TagExtractor::new(
                Arc::clone(tag_store),
                Arc::clone(encoder) as Arc<dyn umms_core::traits::Encoder>,
                tok,
            ));
            pipeline = pipeline.with_tag_extractor(extractor);
        }
    }

    // Use fallback skeleton (no LLM call for structure extraction yet)
    let skeleton = body.skeleton.map(|s| {
        serde_json::from_value::<DocSkeleton>(s).ok()
    }).flatten();

    let result = pipeline
        .ingest(&body.text, &agent_id, scope, tags, skeleton)
        .await
        .map_err(|e| format!("Ingestion failed: {e}"))?;

    state.audit.record(
        AuditEventBuilder::new(AuditEventType::Ingest, agent_id.as_str().to_owned())
            .details(serde_json::json!({
                "action": "ingest_document",
                "chunks_created": result.chunks_created,
                "chunks_stored": result.chunks_stored,
                "total_ms": result.total_ms,
            })),
    );

    let chunks: Vec<ChunkDetailResponse> = result
        .chunk_details
        .iter()
        .map(|cd| ChunkDetailResponse {
            index: cd.index,
            original_text: cd.original_text.clone(),
            context_prefix: cd.context_prefix.clone(),
            section: cd.section.clone(),
            tags: cd.tags.clone(),
            memory_id: cd.memory_id.clone(),
            char_count: cd.char_count,
        })
        .collect();

    Ok(Json(IngestResponse {
        chunks_created: result.chunks_created,
        chunks_stored: result.chunks_stored,
        title: result.skeleton.title.clone(),
        summary: result.skeleton.summary.clone(),
        total_ms: result.total_ms,
        latency: IngestLatencyResponse {
            chunk_ms: result.latency.chunk_ms,
            skeleton_ms: result.latency.skeleton_ms,
            encode_ms: result.latency.encode_ms,
            store_ms: result.latency.store_ms,
        },
        chunks,
    }))
}

#[derive(serde::Deserialize)]
pub struct IngestRequest {
    /// The full document text to ingest.
    pub text: String,
    /// Agent to own the ingested memories. Default: "coder".
    pub agent_id: Option<String>,
    /// Scope: "private" (default) or "shared".
    pub scope: Option<String>,
    /// Tags to attach to all chunks.
    pub tags: Option<Vec<String>>,
    /// Target chunk size in characters. Default: 1500.
    pub chunk_size: Option<usize>,
    /// Overlap between chunks in characters. Default: 200.
    pub chunk_overlap: Option<usize>,
    /// Pre-extracted document skeleton (JSON). If not provided, fallback is used.
    pub skeleton: Option<serde_json::Value>,
}
