//! Tag API handlers.

use std::str::FromStr;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;

use umms_core::tag::Tag;
use umms_core::traits::Encoder;
use umms_core::types::{AgentId, TagId};

use crate::AppState;
use crate::response::{
    CoocEntry, CooccurrenceResponse, TagListResponse, TagMatchResponse, TagResponse,
    TagSearchResponse,
};

/// GET /api/tags/:agent_id — list all tags for an agent.
pub async fn list_tags(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
) -> Result<Json<TagListResponse>, String> {
    let tag_store = state
        .tag_store
        .as_ref()
        .ok_or_else(|| "Tag system not enabled".to_owned())?;

    let aid = AgentId::from_str(&agent_id)
        .map_err(|e| format!("Invalid agent_id: {e}"))?;

    let tags = tag_store
        .all_tags(Some(&aid))
        .await
        .map_err(|e| format!("Failed to fetch tags: {e}"))?;

    let total = tags.len();
    let tag_responses: Vec<TagResponse> = tags.into_iter().map(tag_to_response).collect();

    Ok(Json(TagListResponse {
        agent_id,
        tags: tag_responses,
        total,
    }))
}

/// POST /api/tags/search — search tags by vector similarity.
pub async fn search_tags(
    State(state): State<Arc<AppState>>,
    Json(body): Json<TagSearchRequest>,
) -> Result<Json<TagSearchResponse>, String> {
    let tag_store = state
        .tag_store
        .as_ref()
        .ok_or_else(|| "Tag system not enabled".to_owned())?;

    let encoder = state
        .encoder
        .as_ref()
        .ok_or_else(|| "Encoder not available".to_owned())?;

    let agent_id = body
        .agent_id
        .as_deref()
        .map(AgentId::from_str)
        .transpose()
        .map_err(|e| format!("Invalid agent_id: {e}"))?;

    let query_vec = encoder
        .encode_text(&body.query)
        .await
        .map_err(|e| format!("Encoding failed: {e}"))?;

    let top_k = body.top_k.unwrap_or(10);
    let matches = tag_store
        .search_by_vector(&query_vec, agent_id.as_ref(), top_k)
        .await
        .map_err(|e| format!("Search failed: {e}"))?;

    let results: Vec<TagMatchResponse> = matches
        .into_iter()
        .map(|m| TagMatchResponse {
            tag: tag_to_response(m.tag),
            similarity: m.similarity,
        })
        .collect();

    Ok(Json(TagSearchResponse { results }))
}

/// GET /api/tags/cooccurrences/:tag_id — get co-occurring tags.
pub async fn tag_cooccurrences(
    State(state): State<Arc<AppState>>,
    Path(tag_id_str): Path<String>,
) -> Result<Json<CooccurrenceResponse>, String> {
    let tag_store = state
        .tag_store
        .as_ref()
        .ok_or_else(|| "Tag system not enabled".to_owned())?;

    let tag_id = TagId::from_str(&tag_id_str)
        .map_err(|e| format!("Invalid tag_id: {e}"))?;

    let coocs = tag_store
        .cooccurrences(&tag_id, 50)
        .await
        .map_err(|e| format!("Failed to fetch cooccurrences: {e}"))?;

    let mut entries = Vec::with_capacity(coocs.len());
    for cooc in coocs {
        // Determine the partner tag ID
        let partner_id = if cooc.tag_a == tag_id {
            &cooc.tag_b
        } else {
            &cooc.tag_a
        };

        if let Ok(Some(partner)) = tag_store.get(partner_id).await {
            entries.push(CoocEntry {
                partner_tag: tag_to_response(partner),
                count: cooc.count,
                pmi: cooc.pmi,
            });
        }
    }

    Ok(Json(CooccurrenceResponse {
        tag_id: tag_id_str,
        cooccurrences: entries,
    }))
}

fn tag_to_response(tag: Tag) -> TagResponse {
    TagResponse {
        id: tag.id.as_str().to_owned(),
        label: tag.label,
        canonical: tag.canonical,
        frequency: tag.frequency,
        importance: tag.importance,
    }
}

#[derive(serde::Deserialize)]
pub struct TagSearchRequest {
    pub query: String,
    pub agent_id: Option<String>,
    pub top_k: Option<usize>,
}
