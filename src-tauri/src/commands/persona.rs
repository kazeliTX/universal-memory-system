use std::str::FromStr;
use std::sync::Arc;

use chrono::Utc;
use tauri::State;

use umms_api::response::*;
use umms_api::AppState;
use umms_core::traits::{MemoryCache, VectorStore};
use umms_core::types::*;
use umms_persona::{AgentPersona, AgentRetrievalConfig};

#[tauri::command]
pub async fn list_agents(
    state: State<'_, Arc<AppState>>,
) -> Result<AgentListResponse, String> {
    let personas = state.persona_store.list().await.map_err(|e| e.to_string())?;

    let mut agents = Vec::with_capacity(personas.len());
    for persona in personas {
        let aid = &persona.agent_id;

        let cache_entries = state.cache.entries_for_agent(aid).await;
        let (mut l0, mut l1) = (0usize, 0usize);
        for entry in &cache_entries {
            match entry.layer {
                MemoryLayer::SensoryBuffer => l0 += 1,
                MemoryLayer::WorkingMemory => l1 += 1,
                _ => {}
            }
        }

        let vector_count = state.vector.count(aid, false).await.map_err(|e| e.to_string())?;

        agents.push(AgentPersonaResponse {
            agent_id: persona.agent_id.as_str().to_owned(),
            name: persona.name,
            role: persona.role,
            description: persona.description,
            expertise: persona.expertise,
            retrieval_config: AgentRetrievalConfigResponse {
                bm25_weight: persona.retrieval_config.bm25_weight,
                min_score: persona.retrieval_config.min_score,
                top_k_final: persona.retrieval_config.top_k_final,
                lif_hops: persona.retrieval_config.lif_hops,
            },
            created_at: persona.created_at.to_rfc3339(),
            updated_at: persona.updated_at.to_rfc3339(),
            cache_l0: l0,
            cache_l1: l1,
            vector_count,
        });
    }

    Ok(AgentListResponse { agents })
}

#[tauri::command]
pub async fn get_agent(
    state: State<'_, Arc<AppState>>,
    agent_id: String,
) -> Result<AgentPersonaResponse, String> {
    let aid = AgentId::from_str(&agent_id).map_err(|e| e.to_string())?;

    let persona = state
        .persona_store
        .get(&aid)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("agent '{agent_id}' not found"))?;

    let cache_entries = state.cache.entries_for_agent(&aid).await;
    let (mut l0, mut l1) = (0usize, 0usize);
    for entry in &cache_entries {
        match entry.layer {
            MemoryLayer::SensoryBuffer => l0 += 1,
            MemoryLayer::WorkingMemory => l1 += 1,
            _ => {}
        }
    }
    let vector_count = state.vector.count(&aid, false).await.map_err(|e| e.to_string())?;

    Ok(AgentPersonaResponse {
        agent_id: persona.agent_id.as_str().to_owned(),
        name: persona.name,
        role: persona.role,
        description: persona.description,
        expertise: persona.expertise,
        retrieval_config: AgentRetrievalConfigResponse {
            bm25_weight: persona.retrieval_config.bm25_weight,
            min_score: persona.retrieval_config.min_score,
            top_k_final: persona.retrieval_config.top_k_final,
            lif_hops: persona.retrieval_config.lif_hops,
        },
        created_at: persona.created_at.to_rfc3339(),
        updated_at: persona.updated_at.to_rfc3339(),
        cache_l0: l0,
        cache_l1: l1,
        vector_count,
    })
}

#[tauri::command]
pub async fn create_agent(
    state: State<'_, Arc<AppState>>,
    agent_id: String,
    name: String,
    role: Option<String>,
    description: Option<String>,
    expertise: Option<Vec<String>>,
) -> Result<AgentPersonaResponse, String> {
    let aid = AgentId::from_str(&agent_id).map_err(|e| e.to_string())?;

    // Check for duplicates
    let existing = state.persona_store.get(&aid).await.map_err(|e| e.to_string())?;
    if existing.is_some() {
        return Err(format!("agent '{agent_id}' already exists"));
    }

    let now = Utc::now();
    let persona = AgentPersona {
        agent_id: aid,
        name,
        role: role.unwrap_or_default(),
        description: description.unwrap_or_default(),
        expertise: expertise.unwrap_or_default(),
        system_prompt: String::new(),
        retrieval_config: AgentRetrievalConfig::default(),
        created_at: now,
        updated_at: now,
    };

    state.persona_store.save(&persona).await.map_err(|e| e.to_string())?;

    Ok(AgentPersonaResponse {
        agent_id: persona.agent_id.as_str().to_owned(),
        name: persona.name,
        role: persona.role,
        description: persona.description,
        expertise: persona.expertise,
        retrieval_config: AgentRetrievalConfigResponse {
            bm25_weight: None,
            min_score: None,
            top_k_final: None,
            lif_hops: None,
        },
        created_at: persona.created_at.to_rfc3339(),
        updated_at: persona.updated_at.to_rfc3339(),
        cache_l0: 0,
        cache_l1: 0,
        vector_count: 0,
    })
}

#[tauri::command]
pub async fn update_agent(
    state: State<'_, Arc<AppState>>,
    agent_id: String,
    name: Option<String>,
    role: Option<String>,
    description: Option<String>,
    expertise: Option<Vec<String>>,
    system_prompt: Option<String>,
) -> Result<AgentPersonaResponse, String> {
    let aid = AgentId::from_str(&agent_id).map_err(|e| e.to_string())?;

    let mut persona = state
        .persona_store
        .get(&aid)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("agent '{agent_id}' not found"))?;

    if let Some(n) = name {
        persona.name = n;
    }
    if let Some(r) = role {
        persona.role = r;
    }
    if let Some(d) = description {
        persona.description = d;
    }
    if let Some(e) = expertise {
        persona.expertise = e;
    }
    if let Some(sp) = system_prompt {
        persona.system_prompt = sp;
    }
    persona.updated_at = Utc::now();

    state.persona_store.save(&persona).await.map_err(|e| e.to_string())?;

    let cache_entries = state.cache.entries_for_agent(&persona.agent_id).await;
    let (mut l0, mut l1) = (0usize, 0usize);
    for entry in &cache_entries {
        match entry.layer {
            MemoryLayer::SensoryBuffer => l0 += 1,
            MemoryLayer::WorkingMemory => l1 += 1,
            _ => {}
        }
    }
    let vector_count = state.vector.count(&persona.agent_id, false).await.map_err(|e| e.to_string())?;

    Ok(AgentPersonaResponse {
        agent_id: persona.agent_id.as_str().to_owned(),
        name: persona.name,
        role: persona.role,
        description: persona.description,
        expertise: persona.expertise,
        retrieval_config: AgentRetrievalConfigResponse {
            bm25_weight: persona.retrieval_config.bm25_weight,
            min_score: persona.retrieval_config.min_score,
            top_k_final: persona.retrieval_config.top_k_final,
            lif_hops: persona.retrieval_config.lif_hops,
        },
        created_at: persona.created_at.to_rfc3339(),
        updated_at: persona.updated_at.to_rfc3339(),
        cache_l0: l0,
        cache_l1: l1,
        vector_count,
    })
}

#[tauri::command]
pub async fn delete_agent(
    state: State<'_, Arc<AppState>>,
    agent_id: String,
) -> Result<DeleteAgentResponse, String> {
    let aid = AgentId::from_str(&agent_id).map_err(|e| e.to_string())?;

    let vector_count = state.vector.count(&aid, false).await.map_err(|e| e.to_string())?;
    let had_memories = vector_count > 0;

    state.persona_store.delete(&aid).await.map_err(|e| e.to_string())?;

    Ok(DeleteAgentResponse {
        deleted: true,
        agent_id,
        had_memories,
    })
}
