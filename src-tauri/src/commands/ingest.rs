use std::str::FromStr;
use std::sync::Arc;

use tauri::State;

use umms_api::response::{ChunkDetailResponse, IngestLatencyResponse, IngestResponse};
use umms_api::AppState;
use umms_core::types::{AgentId, IsolationScope};
use umms_retriever::ingest::chunker::ChunkerConfig;
use umms_retriever::ingest::pipeline::IngestPipeline;

#[tauri::command]
pub async fn ingest_document(
    state: State<'_, Arc<AppState>>,
    text: String,
    agent_id: Option<String>,
    scope: Option<String>,
    tags: Option<Vec<String>>,
) -> Result<IngestResponse, String> {
    let encoder = state
        .encoder
        .as_ref()
        .ok_or_else(|| "Encoder not available".to_owned())?;

    let aid = AgentId::from_str(agent_id.as_deref().unwrap_or("coder"))
        .map_err(|e| format!("Invalid agent_id: {e}"))?;

    let iso_scope = match scope.as_deref() {
        Some("shared") => IsolationScope::Shared,
        _ => IsolationScope::Private,
    };

    let enc_arc: Arc<dyn umms_core::traits::Encoder> = Arc::clone(encoder) as _;
    let vec_arc: Arc<dyn umms_core::traits::VectorStore> = Arc::clone(&state.vector) as _;

    let pipeline = IngestPipeline::new(
        enc_arc,
        vec_arc,
        Arc::clone(&state.bm25),
        ChunkerConfig::default(),
    );

    let result = pipeline
        .ingest(&text, &aid, iso_scope, tags.unwrap_or_default(), None)
        .await
        .map_err(|e| format!("Ingestion failed: {e}"))?;

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

    Ok(IngestResponse {
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
    })
}
