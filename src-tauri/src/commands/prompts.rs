//! Tauri Commands for the VCP-inspired three-mode prompt system.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use serde::Deserialize;
use tauri::State;

use umms_api::prompt::engine::PromptEngine;
use umms_api::prompt::types::*;
use umms_api::response::*;
use umms_api::AppState;

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
// Prompt config commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_prompt_config(
    state: State<'_, Arc<AppState>>,
    agent_id: String,
) -> Result<AgentPromptConfigResponse, String> {
    let config = state
        .prompt_store
        .get_prompt_config(&agent_id)
        .await
        .map_err(|e| format!("Failed to get prompt config: {e}"))?;

    let config = match config {
        Some(c) => c,
        None => {
            let blocks = PromptEngine::default_blocks(&agent_id);
            AgentPromptConfig {
                agent_id: agent_id.clone(),
                mode: PromptMode::default(),
                original_prompt: String::new(),
                blocks,
                preset_path: None,
                preset_content: None,
                updated_at: Utc::now(),
            }
        }
    };

    Ok(config_to_response(&config))
}

#[derive(Debug, Deserialize)]
pub struct SavePromptConfigArgs {
    pub mode: Option<String>,
    pub original_prompt: Option<String>,
    pub blocks: Option<Vec<BlockInput>>,
    pub preset_path: Option<String>,
    pub preset_content: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BlockInput {
    pub id: Option<String>,
    pub name: String,
    pub block_type: String,
    pub content: String,
    pub variants: Vec<String>,
    pub selected_variant: usize,
    pub enabled: bool,
    pub order: usize,
}

#[tauri::command]
pub async fn save_prompt_config(
    state: State<'_, Arc<AppState>>,
    agent_id: String,
    config_data: SavePromptConfigArgs,
) -> Result<AgentPromptConfigResponse, String> {
    let mode = config_data
        .mode
        .as_deref()
        .map(|m| serde_json::from_value::<PromptMode>(serde_json::Value::String(m.to_owned())))
        .transpose()
        .map_err(|_| "Invalid mode")?
        .unwrap_or_default();

    let blocks: Vec<PromptBlock> = config_data
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
        original_prompt: config_data.original_prompt.unwrap_or_default(),
        blocks,
        preset_path: config_data.preset_path,
        preset_content: config_data.preset_content,
        updated_at: Utc::now(),
    };

    state
        .prompt_store
        .save_prompt_config(&config)
        .await
        .map_err(|e| format!("Failed to save: {e}"))?;

    Ok(config_to_response(&config))
}

#[tauri::command]
pub async fn switch_prompt_mode(
    state: State<'_, Arc<AppState>>,
    agent_id: String,
    mode: String,
) -> Result<AgentPromptConfigResponse, String> {
    let new_mode: PromptMode =
        serde_json::from_value(serde_json::Value::String(mode.clone()))
            .map_err(|_| format!("Invalid mode: {mode}"))?;

    let mut config = state
        .prompt_store
        .get_prompt_config(&agent_id)
        .await
        .map_err(|e| format!("Failed to get prompt config: {e}"))?
        .ok_or_else(|| format!("No prompt config for agent: {agent_id}"))?;

    config.mode = new_mode;
    config.updated_at = Utc::now();

    state
        .prompt_store
        .save_prompt_config(&config)
        .await
        .map_err(|e| format!("Failed to save: {e}"))?;

    Ok(config_to_response(&config))
}

// ---------------------------------------------------------------------------
// Block commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn add_block(
    state: State<'_, Arc<AppState>>,
    agent_id: String,
    block: BlockInput,
) -> Result<AgentPromptConfigResponse, String> {
    let mut config = state
        .prompt_store
        .get_prompt_config(&agent_id)
        .await
        .map_err(|e| format!("Failed to get: {e}"))?
        .ok_or_else(|| format!("No prompt config for agent: {agent_id}"))?;

    let bt: BlockType =
        serde_json::from_value(serde_json::Value::String(block.block_type.clone()))
            .unwrap_or(BlockType::Custom);

    config.blocks.push(PromptBlock {
        id: block.id.unwrap_or_else(new_block_id),
        name: block.name,
        block_type: bt,
        content: block.content,
        variants: block.variants,
        selected_variant: block.selected_variant,
        enabled: block.enabled,
        order: block.order,
    });
    config.updated_at = Utc::now();

    state
        .prompt_store
        .save_prompt_config(&config)
        .await
        .map_err(|e| format!("Failed to save: {e}"))?;

    Ok(config_to_response(&config))
}

#[tauri::command]
pub async fn update_block(
    state: State<'_, Arc<AppState>>,
    agent_id: String,
    block_id: String,
    block: BlockInput,
) -> Result<AgentPromptConfigResponse, String> {
    let mut config = state
        .prompt_store
        .get_prompt_config(&agent_id)
        .await
        .map_err(|e| format!("Failed to get: {e}"))?
        .ok_or_else(|| format!("No prompt config for agent: {agent_id}"))?;

    let existing = config
        .blocks
        .iter_mut()
        .find(|b| b.id == block_id)
        .ok_or_else(|| format!("Block not found: {block_id}"))?;

    let bt: BlockType =
        serde_json::from_value(serde_json::Value::String(block.block_type.clone()))
            .unwrap_or(BlockType::Custom);

    existing.name = block.name;
    existing.block_type = bt;
    existing.content = block.content;
    existing.variants = block.variants;
    existing.selected_variant = block.selected_variant;
    existing.enabled = block.enabled;
    existing.order = block.order;
    config.updated_at = Utc::now();

    state
        .prompt_store
        .save_prompt_config(&config)
        .await
        .map_err(|e| format!("Failed to save: {e}"))?;

    Ok(config_to_response(&config))
}

#[tauri::command]
pub async fn delete_block(
    state: State<'_, Arc<AppState>>,
    agent_id: String,
    block_id: String,
) -> Result<AgentPromptConfigResponse, String> {
    let mut config = state
        .prompt_store
        .get_prompt_config(&agent_id)
        .await
        .map_err(|e| format!("Failed to get: {e}"))?
        .ok_or_else(|| format!("No prompt config for agent: {agent_id}"))?;

    config.blocks.retain(|b| b.id != block_id);
    config.updated_at = Utc::now();

    state
        .prompt_store
        .save_prompt_config(&config)
        .await
        .map_err(|e| format!("Failed to save: {e}"))?;

    Ok(config_to_response(&config))
}

#[tauri::command]
pub async fn reorder_blocks(
    state: State<'_, Arc<AppState>>,
    agent_id: String,
    block_ids: Vec<String>,
) -> Result<AgentPromptConfigResponse, String> {
    let mut config = state
        .prompt_store
        .get_prompt_config(&agent_id)
        .await
        .map_err(|e| format!("Failed to get: {e}"))?
        .ok_or_else(|| format!("No prompt config for agent: {agent_id}"))?;

    for (order, bid) in block_ids.iter().enumerate() {
        if let Some(block) = config.blocks.iter_mut().find(|b| &b.id == bid) {
            block.order = order;
        }
    }
    config.updated_at = Utc::now();

    state
        .prompt_store
        .save_prompt_config(&config)
        .await
        .map_err(|e| format!("Failed to save: {e}"))?;

    Ok(config_to_response(&config))
}

// ---------------------------------------------------------------------------
// Variant commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn add_variant(
    state: State<'_, Arc<AppState>>,
    agent_id: String,
    block_id: String,
    content: String,
) -> Result<AgentPromptConfigResponse, String> {
    let mut config = state
        .prompt_store
        .get_prompt_config(&agent_id)
        .await
        .map_err(|e| format!("Failed to get: {e}"))?
        .ok_or_else(|| format!("No prompt config for agent: {agent_id}"))?;

    let block = config
        .blocks
        .iter_mut()
        .find(|b| b.id == block_id)
        .ok_or_else(|| format!("Block not found: {block_id}"))?;

    block.variants.push(content);
    config.updated_at = Utc::now();

    state
        .prompt_store
        .save_prompt_config(&config)
        .await
        .map_err(|e| format!("Failed to save: {e}"))?;

    Ok(config_to_response(&config))
}

#[tauri::command]
pub async fn select_variant(
    state: State<'_, Arc<AppState>>,
    agent_id: String,
    block_id: String,
    variant_index: usize,
) -> Result<AgentPromptConfigResponse, String> {
    let mut config = state
        .prompt_store
        .get_prompt_config(&agent_id)
        .await
        .map_err(|e| format!("Failed to get: {e}"))?
        .ok_or_else(|| format!("No prompt config for agent: {agent_id}"))?;

    let block = config
        .blocks
        .iter_mut()
        .find(|b| b.id == block_id)
        .ok_or_else(|| format!("Block not found: {block_id}"))?;

    if variant_index >= block.variants.len() {
        return Err(format!("Variant index out of range"));
    }

    block.selected_variant = variant_index;
    block.content = block.variants[variant_index].clone();
    config.updated_at = Utc::now();

    state
        .prompt_store
        .save_prompt_config(&config)
        .await
        .map_err(|e| format!("Failed to save: {e}"))?;

    Ok(config_to_response(&config))
}

// ---------------------------------------------------------------------------
// Warehouse commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_warehouses(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<PromptWarehouseResponse>, String> {
    let warehouses = state
        .prompt_store
        .list_warehouses()
        .await
        .map_err(|e| format!("Failed: {e}"))?;

    Ok(warehouses.iter().map(warehouse_to_response).collect())
}

#[tauri::command]
pub async fn get_warehouse(
    state: State<'_, Arc<AppState>>,
    name: String,
) -> Result<PromptWarehouseResponse, String> {
    let wh = state
        .prompt_store
        .get_warehouse(&name)
        .await
        .map_err(|e| format!("Failed: {e}"))?
        .ok_or_else(|| format!("Warehouse not found: {name}"))?;

    Ok(warehouse_to_response(&wh))
}

#[tauri::command]
pub async fn save_warehouse(
    state: State<'_, Arc<AppState>>,
    name: String,
    blocks: Vec<BlockInput>,
    is_global: bool,
) -> Result<PromptWarehouseResponse, String> {
    let blocks: Vec<PromptBlock> = blocks
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
        is_global,
    };

    state
        .prompt_store
        .save_warehouse(&wh)
        .await
        .map_err(|e| format!("Failed: {e}"))?;

    Ok(warehouse_to_response(&wh))
}

#[tauri::command]
pub async fn delete_warehouse(
    state: State<'_, Arc<AppState>>,
    name: String,
) -> Result<bool, String> {
    state
        .prompt_store
        .delete_warehouse(&name)
        .await
        .map_err(|e| format!("Failed: {e}"))?;
    Ok(true)
}

// ---------------------------------------------------------------------------
// Variables & Preview
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_variables() -> Vec<PromptVariableResponse> {
    PromptEngine::available_variables()
        .into_iter()
        .map(|v| PromptVariableResponse {
            name: v.name,
            description: v.description,
            resolver: v.resolver,
        })
        .collect()
}

#[tauri::command]
pub async fn preview_prompt(
    state: State<'_, Arc<AppState>>,
    agent_id: String,
    test_vars: Option<HashMap<String, String>>,
) -> Result<PromptPreviewResponse, String> {
    let config = state
        .prompt_store
        .get_prompt_config(&agent_id)
        .await
        .map_err(|e| format!("Failed: {e}"))?
        .ok_or_else(|| format!("No prompt config for agent: {agent_id}"))?;

    let vars = test_vars.unwrap_or_default();

    let resolved = PromptEngine::build_prompt(&config, &vars)
        .map_err(|e| format!("Build failed: {e}"))?;

    let mode_str = serde_json::to_value(&config.mode)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| "modular".into());

    Ok(PromptPreviewResponse {
        resolved_prompt: resolved,
        mode: mode_str,
        block_count: config.blocks.len(),
    })
}
