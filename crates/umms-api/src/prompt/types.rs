//! Data model for the VCP-inspired three-mode prompt system.
//!
//! Three editing modes:
//! - **Original**: raw text prompt (simple textarea)
//! - **Modular**: block-based prompt with ordered sections, each block has variants
//! - **Preset**: load prompt from template files (.md/.txt)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Prompt editing mode.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PromptMode {
    Original,
    Modular,
    Preset,
}

impl Default for PromptMode {
    fn default() -> Self {
        Self::Modular
    }
}

/// A single prompt block in modular mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptBlock {
    /// Unique block id: "block_{timestamp}_{random}"
    pub id: String,
    /// Human-readable label.
    pub name: String,
    /// Semantic type of this block.
    pub block_type: BlockType,
    /// Active content (convenience — always equals `variants[selected_variant]`).
    pub content: String,
    /// All content versions (including active).
    pub variants: Vec<String>,
    /// Index into `variants` for the active version.
    pub selected_variant: usize,
    /// Whether this block is included in the final prompt.
    pub enabled: bool,
    /// Position in the prompt (lower = earlier).
    pub order: usize,
}

/// Semantic type of a prompt block.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BlockType {
    /// Agent persona / identity.
    System,
    /// Memory injection rules.
    Memory,
    /// User behavior diary.
    Diary,
    /// Conversation history.
    History,
    /// User message.
    User,
    /// Response instructions.
    Instruction,
    /// User-defined custom block.
    Custom,
    /// Visual separator (newline).
    Separator,
}

/// Complete prompt configuration for an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPromptConfig {
    pub agent_id: String,
    pub mode: PromptMode,
    /// Original mode: raw prompt text.
    pub original_prompt: String,
    /// Modular mode: ordered blocks.
    pub blocks: Vec<PromptBlock>,
    /// Preset mode: path to template file.
    pub preset_path: Option<String>,
    /// Preset mode: resolved template content.
    pub preset_content: Option<String>,
    pub updated_at: DateTime<Utc>,
}

/// A warehouse is a named collection of reusable prompt blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptWarehouse {
    pub name: String,
    pub blocks: Vec<PromptBlock>,
    /// true = shared across all agents.
    pub is_global: bool,
}

/// Variable definition for template substitution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptVariable {
    /// Variable name, e.g. "AgentName".
    pub name: String,
    /// Tooltip / description text.
    pub description: String,
    /// Resolver type: "static", "runtime", "config".
    pub resolver: String,
}

/// Generate a unique block ID using timestamp + uuid fragment.
pub fn new_block_id() -> String {
    let ts = Utc::now().timestamp_millis();
    let rand_part = uuid::Uuid::new_v4().to_string();
    let short = rand_part.split('-').next().unwrap_or("0");
    format!("block_{ts}_{short}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_mode_default_is_modular() {
        assert_eq!(PromptMode::default(), PromptMode::Modular);
    }

    #[test]
    fn block_id_is_unique() {
        let a = new_block_id();
        // Small sleep not needed — uuid ensures uniqueness
        let b = new_block_id();
        assert_ne!(a, b);
    }

    #[test]
    fn serde_roundtrip_prompt_config() {
        let config = AgentPromptConfig {
            agent_id: "test".into(),
            mode: PromptMode::Modular,
            original_prompt: String::new(),
            blocks: vec![PromptBlock {
                id: "block_1".into(),
                name: "test".into(),
                block_type: BlockType::System,
                content: "hello".into(),
                variants: vec!["hello".into()],
                selected_variant: 0,
                enabled: true,
                order: 0,
            }],
            preset_path: None,
            preset_content: None,
            updated_at: Utc::now(),
        };
        let json = serde_json::to_string(&config).unwrap();
        let back: AgentPromptConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.agent_id, "test");
        assert_eq!(back.blocks.len(), 1);
        assert_eq!(back.blocks[0].block_type, BlockType::System);
    }

    #[test]
    fn serde_roundtrip_warehouse() {
        let wh = PromptWarehouse {
            name: "global".into(),
            blocks: vec![],
            is_global: true,
        };
        let json = serde_json::to_string(&wh).unwrap();
        let back: PromptWarehouse = serde_json::from_str(&json).unwrap();
        assert!(back.is_global);
        assert_eq!(back.name, "global");
    }
}
