//! Agent identity and persona handlers.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::Json;
use chrono::Utc;
use serde::Deserialize;

use umms_core::traits::{MemoryCache, VectorStore};
use umms_core::types::*;
use umms_persona::{AgentPersona, AgentRetrievalConfig};

use crate::handlers::memory::ApiError;
use crate::response::*;
use crate::state::AppState;

/// `GET /api/agents/:agent_id` — per-agent stats aggregated across all layers.
pub async fn agent_detail(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
) -> Result<Json<AgentPersonaResponse>, ApiError> {
    let aid = AgentId::from_str(&agent_id)
        .map_err(|e| ApiError::BadRequest(format!("invalid agent_id: {e}")))?;

    let persona = state
        .persona_store
        .get(&aid)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let (name, role, description, expertise, retrieval_config, created_at, updated_at) =
        match persona {
            Some(p) => (
                p.name,
                p.role,
                p.description,
                p.expertise,
                p.retrieval_config,
                p.created_at.to_rfc3339(),
                p.updated_at.to_rfc3339(),
            ),
            None => (
                agent_id.clone(),
                String::new(),
                String::new(),
                Vec::new(),
                AgentRetrievalConfig::default(),
                String::new(),
                String::new(),
            ),
        };

    // Cache breakdown
    let cache_entries = state.cache.entries_for_agent(&aid).await;
    let mut l0 = 0usize;
    let mut l1 = 0usize;
    for entry in &cache_entries {
        match entry.layer {
            MemoryLayer::SensoryBuffer => l0 += 1,
            MemoryLayer::WorkingMemory => l1 += 1,
            _ => {}
        }
    }

    // Vector count
    let vector_count = state
        .vector
        .count(&aid, false)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(AgentPersonaResponse {
        agent_id,
        name,
        role,
        description,
        expertise,
        retrieval_config: AgentRetrievalConfigResponse {
            bm25_weight: retrieval_config.bm25_weight,
            min_score: retrieval_config.min_score,
            top_k_final: retrieval_config.top_k_final,
            lif_hops: retrieval_config.lif_hops,
        },
        created_at,
        updated_at,
        cache_l0: l0,
        cache_l1: l1,
        vector_count,
    }))
}

/// `GET /api/agents` — list all agents with their personas and stats.
pub async fn list_agents(
    State(state): State<Arc<AppState>>,
) -> Result<Json<AgentListResponse>, ApiError> {
    let personas = state
        .persona_store
        .list()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let mut agents = Vec::with_capacity(personas.len());

    for persona in personas {
        let aid = &persona.agent_id;

        // Cache breakdown
        let cache_entries = state.cache.entries_for_agent(aid).await;
        let mut l0 = 0usize;
        let mut l1 = 0usize;
        for entry in &cache_entries {
            match entry.layer {
                MemoryLayer::SensoryBuffer => l0 += 1,
                MemoryLayer::WorkingMemory => l1 += 1,
                _ => {}
            }
        }

        let vector_count = state
            .vector
            .count(aid, false)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;

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

    Ok(Json(AgentListResponse { agents }))
}

/// Request body for creating a new agent.
#[derive(Debug, Deserialize)]
pub struct CreateAgentRequest {
    pub agent_id: String,
    pub name: String,
    pub role: Option<String>,
    pub description: Option<String>,
    pub expertise: Option<Vec<String>>,
}

/// `POST /api/agents` — create a new agent with persona.
pub async fn create_agent(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateAgentRequest>,
) -> Result<Json<AgentPersonaResponse>, ApiError> {
    let aid = AgentId::from_str(&req.agent_id)
        .map_err(|e| ApiError::BadRequest(format!("invalid agent_id: {e}")))?;

    // Check if persona already exists
    let existing = state
        .persona_store
        .get(&aid)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    if existing.is_some() {
        return Err(ApiError::BadRequest(format!(
            "agent '{}' already exists",
            req.agent_id
        )));
    }

    let now = Utc::now();
    let persona = AgentPersona {
        agent_id: aid,
        name: req.name,
        role: req.role.unwrap_or_default(),
        description: req.description.unwrap_or_default(),
        expertise: req.expertise.unwrap_or_default(),
        system_prompt: String::new(),
        retrieval_config: AgentRetrievalConfig::default(),
        created_at: now,
        updated_at: now,
    };

    state
        .persona_store
        .save(&persona)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(AgentPersonaResponse {
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
    }))
}

/// Request body for updating an agent persona.
#[derive(Debug, Deserialize)]
pub struct UpdateAgentRequest {
    pub name: Option<String>,
    pub role: Option<String>,
    pub description: Option<String>,
    pub expertise: Option<Vec<String>>,
    pub system_prompt: Option<String>,
    pub retrieval_config: Option<UpdateRetrievalConfig>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRetrievalConfig {
    pub bm25_weight: Option<f32>,
    pub min_score: Option<f32>,
    pub top_k_final: Option<usize>,
    pub lif_hops: Option<usize>,
}

/// `PUT /api/agents/:agent_id` — update an agent persona.
pub async fn update_agent(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
    Json(req): Json<UpdateAgentRequest>,
) -> Result<Json<AgentPersonaResponse>, ApiError> {
    let aid = AgentId::from_str(&agent_id)
        .map_err(|e| ApiError::BadRequest(format!("invalid agent_id: {e}")))?;

    let mut persona = state
        .persona_store
        .get(&aid)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("agent '{agent_id}' not found")))?;

    // Apply partial updates
    if let Some(name) = req.name {
        persona.name = name;
    }
    if let Some(role) = req.role {
        persona.role = role;
    }
    if let Some(description) = req.description {
        persona.description = description;
    }
    if let Some(expertise) = req.expertise {
        persona.expertise = expertise;
    }
    if let Some(system_prompt) = req.system_prompt {
        persona.system_prompt = system_prompt;
    }
    if let Some(ret_cfg) = req.retrieval_config {
        persona.retrieval_config = AgentRetrievalConfig {
            bm25_weight: ret_cfg.bm25_weight,
            min_score: ret_cfg.min_score,
            top_k_final: ret_cfg.top_k_final,
            lif_hops: ret_cfg.lif_hops,
        };
    }
    persona.updated_at = Utc::now();

    state
        .persona_store
        .save(&persona)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Gather stats
    let cache_entries = state.cache.entries_for_agent(&persona.agent_id).await;
    let mut l0 = 0usize;
    let mut l1 = 0usize;
    for entry in &cache_entries {
        match entry.layer {
            MemoryLayer::SensoryBuffer => l0 += 1,
            MemoryLayer::WorkingMemory => l1 += 1,
            _ => {}
        }
    }
    let vector_count = state
        .vector
        .count(&persona.agent_id, false)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(AgentPersonaResponse {
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
    }))
}

/// `DELETE /api/agents/:agent_id` — delete an agent.
pub async fn delete_agent(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
) -> Result<Json<DeleteAgentResponse>, ApiError> {
    let aid = AgentId::from_str(&agent_id)
        .map_err(|e| ApiError::BadRequest(format!("invalid agent_id: {e}")))?;

    // Check if the agent has memories (warn but allow deletion)
    let vector_count = state
        .vector
        .count(&aid, false)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let had_memories = vector_count > 0;

    if had_memories {
        tracing::warn!(
            agent_id = agent_id.as_str(),
            vector_count,
            "deleting agent that still has memories"
        );
    }

    state
        .persona_store
        .delete(&aid)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(DeleteAgentResponse {
        deleted: true,
        agent_id,
        had_memories,
    }))
}
