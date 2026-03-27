//! Chat API handler — conversational interface backed by retrieval + generation.

use std::str::FromStr;
use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde::Deserialize;

use umms_core::types::AgentId;
use umms_observe::{AuditEventBuilder, AuditEventType};

use crate::response::{ChatResponse, ChatSource};
use crate::AppState;

/// Chat message in conversation history.
#[derive(Debug, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// POST /api/chat request body.
#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub agent_id: String,
    pub message: String,
    #[serde(default)]
    pub history: Vec<ChatMessage>,
}

/// POST /api/chat — send a message to an agent and get a response.
///
/// Flow:
/// 1. Load agent persona for system prompt
/// 2. Retrieve relevant memories via retrieval pipeline
/// 3. Build full prompt with context + history
/// 4. Generate response via model pool
/// 5. Return response with source citations
pub async fn chat(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, String> {
    // 1. Validate agent_id
    let agent_id =
        AgentId::from_str(&body.agent_id).map_err(|e| format!("Invalid agent_id: {e}"))?;

    // 2. Load persona
    let persona = state
        .persona_store
        .get(&agent_id)
        .await
        .map_err(|e| format!("Persona lookup failed: {e}"))?;

    let system_prompt = persona
        .as_ref()
        .map(|p| p.system_prompt.clone())
        .unwrap_or_else(|| {
            format!(
                "You are a helpful AI assistant named {}.",
                body.agent_id
            )
        });

    // 3. Retrieve relevant memories
    let mut sources = Vec::new();
    if let Some(ref retriever) = state.retriever {
        if let Ok(pr) = retriever.retrieve_with_sources(&body.message, &agent_id).await {
            sources = pr
                .retrieval
                .entries
                .iter()
                .take(5)
                .map(|sm| ChatSource {
                    content: sm.entry.content_text.clone().unwrap_or_default(),
                    score: sm.score,
                    memory_id: sm.entry.id.to_string(),
                })
                .collect();
        }
    }

    // 4. Build prompt
    let context = if sources.is_empty() {
        String::new()
    } else {
        let ctx: Vec<String> = sources
            .iter()
            .enumerate()
            .map(|(i, s)| format!("[记忆 {}] (相关度: {:.2})\n{}", i + 1, s.score, s.content))
            .collect();
        format!("\n\n相关记忆:\n{}\n", ctx.join("\n\n"))
    };

    let history = body
        .history
        .iter()
        .map(|m| {
            format!(
                "{}: {}",
                if m.role == "user" { "用户" } else { "助手" },
                m.content
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let full_prompt = format!(
        "{system_prompt}\n\n{context}\n对话历史:\n{history}\n\n用户: {}\n\n请用中文回答。基于上述记忆上下文回答用户问题。如果记忆中没有相关信息，请如实说明。",
        body.message
    );

    // 5. Generate response
    let pool = state
        .model_pool
        .as_ref()
        .ok_or("Model pool not available")?;

    let start = std::time::Instant::now();
    let response_text = pool
        .chat(&full_prompt)
        .await
        .map_err(|e| format!("Generation failed: {e}"))?;
    let latency_ms = start.elapsed().as_millis() as u64;

    // 6. Record audit
    state.audit.record(
        AuditEventBuilder::new(AuditEventType::Encode, body.agent_id.clone()).details(
            serde_json::json!({
                "action": "chat",
                "message_preview": &body.message[..body.message.len().min(100)],
                "sources": sources.len(),
                "latency_ms": latency_ms,
            }),
        ),
    );

    Ok(Json(ChatResponse {
        message: response_text,
        agent_id: body.agent_id,
        sources,
        latency_ms,
    }))
}
