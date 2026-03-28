//! Chat session management — persistent conversation storage.
//!
//! A `ChatSession` captures the full message history of a conversation
//! between a user and an agent. Sessions are stored in SQLite with messages
//! serialised as a JSON blob (adequate for personal-use scale).

mod store;

pub use store::SessionStore;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A chat session between a user and an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    /// Unique session identifier (UUID).
    pub id: String,
    /// Agent this session belongs to.
    pub agent_id: String,
    /// Session title (auto-generated from first message or user-set).
    pub title: String,
    /// Full message history.
    pub messages: Vec<ChatMessage>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Extensible metadata (model used, token counts, etc.).
    pub metadata: serde_json::Value,
}

/// A single message in a chat session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// "user" or "assistant".
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    /// Memory sources referenced in this response.
    pub sources: Vec<ChatSourceRecord>,
    /// Response generation latency (assistant messages only).
    pub latency_ms: Option<u64>,
}

/// Record of a memory source used during response generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSourceRecord {
    pub memory_id: String,
    pub score: f32,
    pub content_preview: String,
}

/// Lightweight session summary for listing (no full messages).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSessionSummary {
    pub id: String,
    pub agent_id: String,
    pub title: String,
    pub message_count: usize,
    pub last_message_preview: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Truncate history messages to fit within context window limits.
///
/// Returns the most recent messages that fit within both `max_messages`
/// and `max_tokens` constraints. Token estimation uses chars/4 as a
/// rough heuristic (works reasonably for mixed CJK/English text).
pub fn truncate_history(
    messages: &[ChatMessage],
    max_messages: usize,
    max_tokens: usize,
) -> Vec<&ChatMessage> {
    if messages.is_empty() {
        return Vec::new();
    }

    // Start from the most recent messages
    let candidates: Vec<&ChatMessage> = messages.iter().rev().take(max_messages).collect();

    let mut result = Vec::new();
    let mut token_budget = max_tokens;

    for msg in &candidates {
        // Rough token estimate: chars / 4 (conservative for mixed content)
        let estimated_tokens = estimate_tokens(&msg.content);
        if estimated_tokens > token_budget {
            break;
        }
        token_budget -= estimated_tokens;
        result.push(*msg);
    }

    // Reverse back to chronological order
    result.reverse();
    result
}

/// Rough token estimation. CJK characters count as ~1 token each,
/// ASCII characters count as ~0.25 tokens each.
fn estimate_tokens(text: &str) -> usize {
    let mut tokens = 0usize;
    for ch in text.chars() {
        if ch.is_ascii() {
            // ~4 ASCII chars per token
            tokens += 1;
        } else {
            // CJK and other non-ASCII: ~1 char per token, but we count
            // each as 4 to match the /4 division below
            tokens += 4;
        }
    }
    // Divide by 4 to get approximate token count
    tokens.div_ceil(4)
}
