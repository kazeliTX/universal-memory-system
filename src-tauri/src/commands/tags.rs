use std::str::FromStr;
use std::sync::Arc;

use tauri::State;

use umms_api::response::{
    ActivatedTagResponse, CoocEntry, CooccurrenceResponse, EpaAnalyzeResponse, TagListResponse,
    TagMatchResponse, TagResponse, TagSearchResponse,
};
use umms_api::AppState;
use umms_core::tag::Tag;
use umms_core::types::{AgentId, TagId};
use umms_retriever::epa::EpaAnalyzer;

#[tauri::command]
pub async fn list_tags(
    state: State<'_, Arc<AppState>>,
    agent_id: String,
) -> Result<TagListResponse, String> {
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

    Ok(TagListResponse {
        agent_id,
        tags: tag_responses,
        total,
    })
}

#[tauri::command]
pub async fn search_tags(
    state: State<'_, Arc<AppState>>,
    query: String,
    agent_id: Option<String>,
    top_k: Option<usize>,
) -> Result<TagSearchResponse, String> {
    let tag_store = state
        .tag_store
        .as_ref()
        .ok_or_else(|| "Tag system not enabled".to_owned())?;

    let encoder = state
        .encoder
        .as_ref()
        .ok_or_else(|| "Encoder not available".to_owned())?;

    let aid = agent_id
        .as_deref()
        .map(AgentId::from_str)
        .transpose()
        .map_err(|e| format!("Invalid agent_id: {e}"))?;

    let query_vec = encoder
        .encode_text(&query)
        .await
        .map_err(|e| format!("Encoding failed: {e}"))?;

    let matches = tag_store
        .search_by_vector(&query_vec, aid.as_ref(), top_k.unwrap_or(10))
        .await
        .map_err(|e| format!("Search failed: {e}"))?;

    let results: Vec<TagMatchResponse> = matches
        .into_iter()
        .map(|m| TagMatchResponse {
            tag: tag_to_response(m.tag),
            similarity: m.similarity,
        })
        .collect();

    Ok(TagSearchResponse { results })
}

#[tauri::command]
pub async fn tag_cooccurrences(
    state: State<'_, Arc<AppState>>,
    tag_id: String,
) -> Result<CooccurrenceResponse, String> {
    let tag_store = state
        .tag_store
        .as_ref()
        .ok_or_else(|| "Tag system not enabled".to_owned())?;

    let tid = TagId::from_str(&tag_id)
        .map_err(|e| format!("Invalid tag_id: {e}"))?;

    let coocs = tag_store
        .cooccurrences(&tid, 50)
        .await
        .map_err(|e| format!("Failed to fetch cooccurrences: {e}"))?;

    let mut entries = Vec::with_capacity(coocs.len());
    for cooc in coocs {
        let partner_id = if cooc.tag_a == tid {
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

    Ok(CooccurrenceResponse {
        tag_id,
        cooccurrences: entries,
    })
}

#[tauri::command]
pub async fn epa_analyze(
    state: State<'_, Arc<AppState>>,
    query: String,
    agent_id: Option<String>,
) -> Result<EpaAnalyzeResponse, String> {
    let tag_store = state
        .tag_store
        .as_ref()
        .ok_or_else(|| "Tag system not enabled".to_owned())?;

    let encoder = state
        .encoder
        .as_ref()
        .ok_or_else(|| "Encoder not available".to_owned())?;

    let aid = AgentId::from_str(agent_id.as_deref().unwrap_or("coder"))
        .map_err(|e| format!("Invalid agent_id: {e}"))?;

    let query_vec = encoder
        .encode_text(&query)
        .await
        .map_err(|e| format!("Encoding failed: {e}"))?;

    let umms_config = umms_core::config::load_config();
    let analyzer = EpaAnalyzer::new(Arc::clone(tag_store), umms_config.epa);

    let result = analyzer
        .analyze(&query_vec, &aid)
        .await
        .map_err(|e| format!("EPA analysis failed: {e}"))?;

    let activated_tags: Vec<ActivatedTagResponse> = result
        .activated_tags
        .iter()
        .map(|at| ActivatedTagResponse {
            tag_id: at.tag_id.as_str().to_owned(),
            label: at.label.clone(),
            similarity: at.similarity,
        })
        .collect();

    Ok(EpaAnalyzeResponse {
        logic_depth: result.logic_depth,
        cross_domain_resonance: result.cross_domain_resonance,
        activated_tags,
        alpha: result.alpha,
        num_semantic_axes: result.semantic_axes.len(),
    })
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
