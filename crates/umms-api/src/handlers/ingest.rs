//! Document ingestion API handlers.

use std::str::FromStr;
use std::sync::Arc;
use std::time::Instant;

use axum::extract::State;
use axum::Json;

use umms_core::config;
use umms_core::traits::{RawFileStore, VectorStore};
use umms_core::types::{AgentId, IsolationScope, MemoryEntryBuilder, MemoryLayer, Modality};
use umms_observe::{AuditEventBuilder, AuditEventType};
use umms_retriever::ingest::chunker::ChunkerConfig;
use umms_retriever::ingest::pipeline::IngestPipeline;
use umms_retriever::ingest::skeleton::DocSkeleton;
use umms_retriever::ingest::tag_extractor::TagExtractor;
use umms_retriever::tokenizer;

use crate::AppState;
use crate::response::{
    ChunkDetailResponse, IngestLatencyResponse, IngestResponse, MultimodalIngestResponse,
};

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

    // Wire up graph store for chunk-level graph node creation
    pipeline = pipeline.with_graph(Arc::clone(&state.graph));

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
            graph_ms: result.latency.graph_ms,
        },
        chunks,
        graph_nodes_created: result.graph_nodes_created,
        graph_edges_created: result.graph_edges_created,
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

// ---------------------------------------------------------------------------
// Multimodal ingest (image + audio)
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
pub struct MultimodalIngestRequest {
    /// Base64-encoded image or audio data.
    pub data: String,
    /// MIME type of the data (e.g., "image/png", "audio/wav").
    pub mime_type: String,
    /// Agent to own the ingested memory.
    pub agent_id: String,
    /// Description for image content (optional; used as content_text for images).
    pub description: Option<String>,
    /// Tags to attach to the memory entry.
    pub tags: Option<Vec<String>>,
    /// Scope: "private" (default) or "shared".
    pub scope: Option<String>,
}

/// POST /api/ingest/multimodal — ingest image or audio content.
///
/// Images are embedded directly using Gemini's multimodal embedding model.
/// Audio is transcribed to text first, then the text is embedded.
pub async fn ingest_multimodal(
    State(state): State<Arc<AppState>>,
    Json(body): Json<MultimodalIngestRequest>,
) -> Result<Json<MultimodalIngestResponse>, String> {
    let start = Instant::now();

    let pool = state
        .model_pool
        .as_ref()
        .ok_or_else(|| "Model pool not available (GEMINI_API_KEY not set)".to_owned())?;

    let agent_id = AgentId::from_str(&body.agent_id)
        .map_err(|e| format!("Invalid agent_id: {e}"))?;

    let scope = match body.scope.as_deref() {
        Some("shared") => IsolationScope::Shared,
        _ => IsolationScope::Private,
    };

    let tags = body.tags.unwrap_or_default();

    // Determine modality and process based on MIME type prefix.
    let media_type = body
        .mime_type
        .split('/')
        .next()
        .unwrap_or("");

    let (content_text, vector, modality) = match media_type {
        "image" => {
            let vec = pool
                .embed_image(&body.data, &body.mime_type)
                .await
                .map_err(|e| format!("Image embedding failed: {e}"))?;
            let desc = body
                .description
                .unwrap_or_else(|| "Image content".to_owned());
            (desc, vec, Modality::Image)
        }
        "audio" => {
            let (text, vec) = pool
                .embed_audio(&body.data, &body.mime_type)
                .await
                .map_err(|e| format!("Audio processing failed: {e}"))?;
            (text, vec, Modality::Audio)
        }
        _ => {
            return Err(format!("Unsupported mime type: {}", body.mime_type));
        }
    };

    let vector_dimension = vector.len();

    // Build memory entry and store in L2 (episodic memory).
    let entry = MemoryEntryBuilder::new(agent_id.clone(), modality)
        .layer(MemoryLayer::EpisodicMemory)
        .scope(scope)
        .content_text(&content_text)
        .vector(vector)
        .tags(tags)
        .metadata(serde_json::json!({
            "mime_type": body.mime_type,
            "source": "multimodal_ingest",
        }))
        .build();

    let memory_id = entry.id.to_string();

    // Insert into vector store.
    state
        .vector
        .insert(&entry)
        .await
        .map_err(|e| format!("Vector store insert failed: {e}"))?;

    // Store raw file in L4 (raw storage).
    let raw_bytes = base64_decode(&body.data)
        .map_err(|e| format!("Invalid base64 data: {e}"))?;
    let extension = mime_extension(&body.mime_type);
    let filename = format!("{}.{}", memory_id, extension);
    let _ = state
        .files
        .store(&agent_id, &filename, &raw_bytes)
        .await;

    let latency_ms = start.elapsed().as_millis() as u64;

    state.audit.record(
        AuditEventBuilder::new(AuditEventType::Ingest, agent_id.as_str().to_owned())
            .details(serde_json::json!({
                "action": "ingest_multimodal",
                "mime_type": body.mime_type,
                "modality": modality.display_name(),
                "memory_id": memory_id,
                "latency_ms": latency_ms,
            })),
    );

    Ok(Json(MultimodalIngestResponse {
        memory_id,
        modality: modality.display_name().to_owned(),
        content_text,
        vector_dimension,
        latency_ms,
    }))
}

/// Decode base64 string to bytes.
fn base64_decode(input: &str) -> std::result::Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(input)
        .map_err(|e| e.to_string())
}

/// Map MIME type to a file extension.
fn mime_extension(mime_type: &str) -> &str {
    match mime_type {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        "audio/wav" => "wav",
        "audio/mp3" | "audio/mpeg" => "mp3",
        "audio/ogg" => "ogg",
        "audio/flac" => "flac",
        _ => "bin",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_extension_mapping() {
        assert_eq!(mime_extension("image/png"), "png");
        assert_eq!(mime_extension("image/jpeg"), "jpg");
        assert_eq!(mime_extension("audio/wav"), "wav");
        assert_eq!(mime_extension("audio/mp3"), "mp3");
        assert_eq!(mime_extension("audio/mpeg"), "mp3");
        assert_eq!(mime_extension("audio/ogg"), "ogg");
        assert_eq!(mime_extension("audio/flac"), "flac");
        assert_eq!(mime_extension("application/pdf"), "bin");
    }

    #[test]
    fn modality_from_mime_type_prefix() {
        let cases = vec![
            ("image/png", "image"),
            ("audio/wav", "audio"),
            ("text/plain", "text"),
        ];
        for (mime, expected_prefix) in cases {
            let prefix = mime.split('/').next().unwrap_or("");
            assert_eq!(prefix, expected_prefix);
        }
    }
}
