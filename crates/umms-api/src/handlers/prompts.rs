//! Prompt configuration API handlers.
//!
//! Manages the VCP-inspired three-mode prompt system: Original, Modular, Preset.

use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use chrono::Utc;
use serde::Deserialize;
use tracing::debug;

use crate::AppState;
use crate::prompt::engine::PromptEngine;
use crate::prompt::types::*;
use crate::response::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn block_to_response(b: &PromptBlock) -> PromptBlockResponse {
    PromptBlockResponse {
        id: b.id.clone(),
        name: b.name.clone(),
        block_type: serde_json::to_value(&b.block_type)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| "custom".into()),
        content: b.content.clone(),
        variants: b.variants.clone(),
        selected_variant: b.selected_variant,
        enabled: b.enabled,
        order: b.order,
    }
}

fn config_to_response(c: &AgentPromptConfig) -> AgentPromptConfigResponse {
    AgentPromptConfigResponse {
        agent_id: c.agent_id.clone(),
        mode: serde_json::to_value(&c.mode)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| "modular".into()),
        original_prompt: c.original_prompt.clone(),
        blocks: c.blocks.iter().map(block_to_response).collect(),
        preset_path: c.preset_path.clone(),
        preset_content: c.preset_content.clone(),
        updated_at: c.updated_at.to_rfc3339(),
    }
}

fn warehouse_to_response(w: &PromptWarehouse) -> PromptWarehouseResponse {
    PromptWarehouseResponse {
        name: w.name.clone(),
        blocks: w.blocks.iter().map(block_to_response).collect(),
        is_global: w.is_global,
    }
}

// ---------------------------------------------------------------------------
// GET /api/prompts/:agent_id
// ---------------------------------------------------------------------------

pub async fn get_prompt_config(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
) -> Result<Json<AgentPromptConfigResponse>, String> {
    let config = state
        .prompt_store
        .get_prompt_config(&agent_id)
        .await
        .map_err(|e| format!("Failed to get prompt config: {e}"))?;

    let config = if let Some(c) = config {
        debug!(agent_id = %agent_id, "loaded prompt config from store");
        c
    } else {
        debug!(agent_id = %agent_id, "no stored prompt config, generating default");
        // Generate default config from persona
        let persona = state
            .persona_store
            .get(
                &agent_id
                    .parse()
                    .map_err(|e| format!("Invalid agent_id: {e}"))?,
            )
            .await
            .map_err(|e| format!("Persona lookup failed: {e}"))?;

        let agent_name = persona
            .as_ref()
            .map_or(agent_id.as_str(), |p| p.name.as_str());
        let blocks = PromptEngine::default_blocks(agent_name);

        AgentPromptConfig {
            agent_id: agent_id.clone(),
            mode: PromptMode::default(),
            original_prompt: persona
                .as_ref()
                .map(|p| p.system_prompt.clone())
                .unwrap_or_default(),
            blocks,
            preset_path: None,
            preset_content: None,
            updated_at: Utc::now(),
        }
    };

    Ok(Json(config_to_response(&config)))
}

// ---------------------------------------------------------------------------
// PUT /api/prompts/:agent_id
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SavePromptConfigRequest {
    pub mode: Option<String>,
    pub original_prompt: Option<String>,
    pub blocks: Option<Vec<PromptBlockInput>>,
    pub preset_path: Option<String>,
    pub preset_content: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PromptBlockInput {
    pub id: Option<String>,
    pub name: String,
    pub block_type: String,
    pub content: String,
    pub variants: Vec<String>,
    pub selected_variant: usize,
    pub enabled: bool,
    pub order: usize,
}

pub async fn save_prompt_config(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
    Json(body): Json<SavePromptConfigRequest>,
) -> Result<Json<AgentPromptConfigResponse>, String> {
    let mode = body
        .mode
        .as_deref()
        .map(|m| serde_json::from_value::<PromptMode>(serde_json::Value::String(m.to_owned())))
        .transpose()
        .map_err(|_| "Invalid mode")?
        .unwrap_or_default();

    let blocks: Vec<PromptBlock> = body
        .blocks
        .unwrap_or_default()
        .into_iter()
        .map(|b| {
            let bt: BlockType =
                serde_json::from_value(serde_json::Value::String(b.block_type.clone()))
                    .unwrap_or(BlockType::Custom);
            PromptBlock {
                id: b.id.unwrap_or_else(new_block_id),
                name: b.name,
                block_type: bt,
                content: b.content,
                variants: b.variants,
                selected_variant: b.selected_variant,
                enabled: b.enabled,
                order: b.order,
            }
        })
        .collect();

    let config = AgentPromptConfig {
        agent_id: agent_id.clone(),
        mode,
        original_prompt: body.original_prompt.unwrap_or_default(),
        blocks,
        preset_path: body.preset_path,
        preset_content: body.preset_content,
        updated_at: Utc::now(),
    };

    state
        .prompt_store
        .save_prompt_config(&config)
        .await
        .map_err(|e| format!("Failed to save prompt config: {e}"))?;

    Ok(Json(config_to_response(&config)))
}

// ---------------------------------------------------------------------------
// PUT /api/prompts/:agent_id/mode
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SwitchModeRequest {
    pub mode: String,
}

pub async fn switch_mode(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
    Json(body): Json<SwitchModeRequest>,
) -> Result<Json<AgentPromptConfigResponse>, String> {
    let new_mode: PromptMode = serde_json::from_value(serde_json::Value::String(body.mode.clone()))
        .map_err(|_| format!("Invalid mode: {}", body.mode))?;

    let mut config = state
        .prompt_store
        .get_prompt_config(&agent_id)
        .await
        .map_err(|e| format!("Failed to get prompt config: {e}"))?
        .ok_or_else(|| format!("No prompt config found for agent: {agent_id}"))?;

    config.mode = new_mode;
    config.updated_at = Utc::now();

    state
        .prompt_store
        .save_prompt_config(&config)
        .await
        .map_err(|e| format!("Failed to save prompt config: {e}"))?;

    Ok(Json(config_to_response(&config)))
}

// ---------------------------------------------------------------------------
// POST /api/prompts/:agent_id/blocks
// ---------------------------------------------------------------------------

pub async fn add_block(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
    Json(body): Json<PromptBlockInput>,
) -> Result<Json<AgentPromptConfigResponse>, String> {
    let mut config = state
        .prompt_store
        .get_prompt_config(&agent_id)
        .await
        .map_err(|e| format!("Failed to get prompt config: {e}"))?
        .ok_or_else(|| format!("No prompt config found for agent: {agent_id}"))?;

    let bt: BlockType = serde_json::from_value(serde_json::Value::String(body.block_type.clone()))
        .unwrap_or(BlockType::Custom);

    let block = PromptBlock {
        id: body.id.unwrap_or_else(new_block_id),
        name: body.name,
        block_type: bt,
        content: body.content,
        variants: body.variants,
        selected_variant: body.selected_variant,
        enabled: body.enabled,
        order: body.order,
    };

    config.blocks.push(block);
    config.updated_at = Utc::now();

    state
        .prompt_store
        .save_prompt_config(&config)
        .await
        .map_err(|e| format!("Failed to save: {e}"))?;

    Ok(Json(config_to_response(&config)))
}

// ---------------------------------------------------------------------------
// PUT /api/prompts/:agent_id/blocks/:block_id
// ---------------------------------------------------------------------------

pub async fn update_block(
    State(state): State<Arc<AppState>>,
    Path((agent_id, block_id)): Path<(String, String)>,
    Json(body): Json<PromptBlockInput>,
) -> Result<Json<AgentPromptConfigResponse>, String> {
    let mut config = state
        .prompt_store
        .get_prompt_config(&agent_id)
        .await
        .map_err(|e| format!("Failed to get prompt config: {e}"))?
        .ok_or_else(|| format!("No prompt config found for agent: {agent_id}"))?;

    let block = config
        .blocks
        .iter_mut()
        .find(|b| b.id == block_id)
        .ok_or_else(|| format!("Block not found: {block_id}"))?;

    let bt: BlockType = serde_json::from_value(serde_json::Value::String(body.block_type.clone()))
        .unwrap_or(BlockType::Custom);

    block.name = body.name;
    block.block_type = bt;
    block.content = body.content;
    block.variants = body.variants;
    block.selected_variant = body.selected_variant;
    block.enabled = body.enabled;
    block.order = body.order;
    config.updated_at = Utc::now();

    state
        .prompt_store
        .save_prompt_config(&config)
        .await
        .map_err(|e| format!("Failed to save: {e}"))?;

    Ok(Json(config_to_response(&config)))
}

// ---------------------------------------------------------------------------
// DELETE /api/prompts/:agent_id/blocks/:block_id
// ---------------------------------------------------------------------------

pub async fn delete_block(
    State(state): State<Arc<AppState>>,
    Path((agent_id, block_id)): Path<(String, String)>,
) -> Result<Json<AgentPromptConfigResponse>, String> {
    let mut config = state
        .prompt_store
        .get_prompt_config(&agent_id)
        .await
        .map_err(|e| format!("Failed to get prompt config: {e}"))?
        .ok_or_else(|| format!("No prompt config found for agent: {agent_id}"))?;

    config.blocks.retain(|b| b.id != block_id);
    config.updated_at = Utc::now();

    state
        .prompt_store
        .save_prompt_config(&config)
        .await
        .map_err(|e| format!("Failed to save: {e}"))?;

    Ok(Json(config_to_response(&config)))
}

// ---------------------------------------------------------------------------
// PUT /api/prompts/:agent_id/blocks/reorder
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ReorderBlocksRequest {
    pub block_ids: Vec<String>,
}

pub async fn reorder_blocks(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
    Json(body): Json<ReorderBlocksRequest>,
) -> Result<Json<AgentPromptConfigResponse>, String> {
    let mut config = state
        .prompt_store
        .get_prompt_config(&agent_id)
        .await
        .map_err(|e| format!("Failed to get prompt config: {e}"))?
        .ok_or_else(|| format!("No prompt config found for agent: {agent_id}"))?;

    for (order, block_id) in body.block_ids.iter().enumerate() {
        if let Some(block) = config.blocks.iter_mut().find(|b| &b.id == block_id) {
            block.order = order;
        }
    }
    config.updated_at = Utc::now();

    state
        .prompt_store
        .save_prompt_config(&config)
        .await
        .map_err(|e| format!("Failed to save: {e}"))?;

    Ok(Json(config_to_response(&config)))
}

// ---------------------------------------------------------------------------
// POST /api/prompts/:agent_id/blocks/:block_id/variants
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct AddVariantRequest {
    pub content: String,
}

pub async fn add_variant(
    State(state): State<Arc<AppState>>,
    Path((agent_id, block_id)): Path<(String, String)>,
    Json(body): Json<AddVariantRequest>,
) -> Result<Json<AgentPromptConfigResponse>, String> {
    let mut config = state
        .prompt_store
        .get_prompt_config(&agent_id)
        .await
        .map_err(|e| format!("Failed to get prompt config: {e}"))?
        .ok_or_else(|| format!("No prompt config found for agent: {agent_id}"))?;

    let block = config
        .blocks
        .iter_mut()
        .find(|b| b.id == block_id)
        .ok_or_else(|| format!("Block not found: {block_id}"))?;

    block.variants.push(body.content);
    config.updated_at = Utc::now();

    state
        .prompt_store
        .save_prompt_config(&config)
        .await
        .map_err(|e| format!("Failed to save: {e}"))?;

    Ok(Json(config_to_response(&config)))
}

// ---------------------------------------------------------------------------
// PUT /api/prompts/:agent_id/blocks/:block_id/variant/:idx
// ---------------------------------------------------------------------------

pub async fn select_variant(
    State(state): State<Arc<AppState>>,
    Path((agent_id, block_id, idx)): Path<(String, String, usize)>,
) -> Result<Json<AgentPromptConfigResponse>, String> {
    let mut config = state
        .prompt_store
        .get_prompt_config(&agent_id)
        .await
        .map_err(|e| format!("Failed to get prompt config: {e}"))?
        .ok_or_else(|| format!("No prompt config found for agent: {agent_id}"))?;

    let block = config
        .blocks
        .iter_mut()
        .find(|b| b.id == block_id)
        .ok_or_else(|| format!("Block not found: {block_id}"))?;

    if idx >= block.variants.len() {
        return Err(format!(
            "Variant index {idx} out of range (max {})",
            block.variants.len() - 1
        ));
    }

    block.selected_variant = idx;
    block.content = block.variants[idx].clone();
    config.updated_at = Utc::now();

    state
        .prompt_store
        .save_prompt_config(&config)
        .await
        .map_err(|e| format!("Failed to save: {e}"))?;

    Ok(Json(config_to_response(&config)))
}

// ---------------------------------------------------------------------------
// Warehouse endpoints
// ---------------------------------------------------------------------------

pub async fn list_warehouses(
    State(state): State<Arc<AppState>>,
) -> Result<Json<PromptWarehouseListResponse>, String> {
    let warehouses = state
        .prompt_store
        .list_warehouses()
        .await
        .map_err(|e| format!("Failed to list warehouses: {e}"))?;

    Ok(Json(PromptWarehouseListResponse {
        warehouses: warehouses.iter().map(warehouse_to_response).collect(),
    }))
}

pub async fn get_warehouse(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<PromptWarehouseResponse>, String> {
    let wh = state
        .prompt_store
        .get_warehouse(&name)
        .await
        .map_err(|e| format!("Failed to get warehouse: {e}"))?
        .ok_or_else(|| format!("Warehouse not found: {name}"))?;

    Ok(Json(warehouse_to_response(&wh)))
}

#[derive(Debug, Deserialize)]
pub struct CreateWarehouseRequest {
    pub name: String,
    pub blocks: Vec<PromptBlockInput>,
    pub is_global: Option<bool>,
}

pub async fn create_warehouse(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateWarehouseRequest>,
) -> Result<Json<PromptWarehouseResponse>, String> {
    let blocks: Vec<PromptBlock> = body
        .blocks
        .into_iter()
        .map(|b| {
            let bt: BlockType =
                serde_json::from_value(serde_json::Value::String(b.block_type.clone()))
                    .unwrap_or(BlockType::Custom);
            PromptBlock {
                id: b.id.unwrap_or_else(new_block_id),
                name: b.name,
                block_type: bt,
                content: b.content,
                variants: b.variants,
                selected_variant: b.selected_variant,
                enabled: b.enabled,
                order: b.order,
            }
        })
        .collect();

    let wh = PromptWarehouse {
        name: body.name,
        blocks,
        is_global: body.is_global.unwrap_or(false),
    };

    state
        .prompt_store
        .save_warehouse(&wh)
        .await
        .map_err(|e| format!("Failed to save warehouse: {e}"))?;

    Ok(Json(warehouse_to_response(&wh)))
}

pub async fn update_warehouse(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<CreateWarehouseRequest>,
) -> Result<Json<PromptWarehouseResponse>, String> {
    let blocks: Vec<PromptBlock> = body
        .blocks
        .into_iter()
        .map(|b| {
            let bt: BlockType =
                serde_json::from_value(serde_json::Value::String(b.block_type.clone()))
                    .unwrap_or(BlockType::Custom);
            PromptBlock {
                id: b.id.unwrap_or_else(new_block_id),
                name: b.name,
                block_type: bt,
                content: b.content,
                variants: b.variants,
                selected_variant: b.selected_variant,
                enabled: b.enabled,
                order: b.order,
            }
        })
        .collect();

    let wh = PromptWarehouse {
        name,
        blocks,
        is_global: body.is_global.unwrap_or(false),
    };

    state
        .prompt_store
        .save_warehouse(&wh)
        .await
        .map_err(|e| format!("Failed to save warehouse: {e}"))?;

    Ok(Json(warehouse_to_response(&wh)))
}

pub async fn delete_warehouse(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, String> {
    state
        .prompt_store
        .delete_warehouse(&name)
        .await
        .map_err(|e| format!("Failed to delete warehouse: {e}"))?;

    Ok(Json(serde_json::json!({ "deleted": true, "name": name })))
}

// ---------------------------------------------------------------------------
// Variables & Preview
// ---------------------------------------------------------------------------

pub async fn list_variables() -> Json<PromptVariableListResponse> {
    let variables = PromptEngine::available_variables()
        .into_iter()
        .map(|v| PromptVariableResponse {
            name: v.name,
            description: v.description,
            resolver: v.resolver,
        })
        .collect();

    Json(PromptVariableListResponse { variables })
}

#[derive(Debug, Deserialize)]
pub struct PreviewRequest {
    pub agent_id: String,
    pub test_vars: Option<HashMap<String, String>>,
}

pub async fn preview_prompt(
    State(state): State<Arc<AppState>>,
    Json(body): Json<PreviewRequest>,
) -> Result<Json<PromptPreviewResponse>, String> {
    let config = state
        .prompt_store
        .get_prompt_config(&body.agent_id)
        .await
        .map_err(|e| format!("Failed to get prompt config: {e}"))?
        .ok_or_else(|| format!("No prompt config found for agent: {}", body.agent_id))?;

    let vars = body.test_vars.unwrap_or_default();

    let resolved =
        PromptEngine::build_prompt(&config, &vars).map_err(|e| format!("Build failed: {e}"))?;

    let mode_str = serde_json::to_value(&config.mode)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| "modular".into());

    Ok(Json(PromptPreviewResponse {
        resolved_prompt: resolved,
        mode: mode_str,
        block_count: config.blocks.len(),
    }))
}

// ---------------------------------------------------------------------------
// Presets
// ---------------------------------------------------------------------------

pub async fn list_presets(
    State(state): State<Arc<AppState>>,
) -> Result<Json<PresetListResponse>, String> {
    let presets_dir = state.config.data_dir.join("presets");

    if !presets_dir.exists() {
        return Ok(Json(PresetListResponse { presets: vec![] }));
    }

    let mut presets = Vec::new();
    let entries =
        std::fs::read_dir(&presets_dir).map_err(|e| format!("Failed to read presets dir: {e}"))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read entry: {e}"))?;
        let path = entry.path();
        if let Some(ext) = path.extension() {
            if ext == "md" || ext == "txt" {
                let filename = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let content = std::fs::read_to_string(&path)
                    .map_err(|e| format!("Failed to read preset: {e}"))?;
                presets.push(PresetFileResponse { filename, content });
            }
        }
    }

    Ok(Json(PresetListResponse { presets }))
}

pub async fn get_preset(
    State(state): State<Arc<AppState>>,
    Path(filename): Path<String>,
) -> Result<Json<PresetFileResponse>, String> {
    let presets_dir = state.config.data_dir.join("presets");
    let path = presets_dir.join(&filename);

    if !path.exists() {
        return Err(format!("Preset not found: {filename}"));
    }

    let content =
        std::fs::read_to_string(&path).map_err(|e| format!("Failed to read preset: {e}"))?;

    Ok(Json(PresetFileResponse { filename, content }))
}
