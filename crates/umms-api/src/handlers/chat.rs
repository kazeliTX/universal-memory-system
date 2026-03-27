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
    // min_score in the retrieval pipeline handles filtering irrelevant results —
    // no need for a greeting blocklist. Low-relevance queries simply return empty sources.
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

    // 4. Build prompt — LLM-driven retrieval decision (inspired by VCP)
    //
    // The LLM decides whether to use memories, not the application layer.
    // We always provide whatever memories were found (with scores), and instruct
    // the LLM to ignore irrelevant ones. This avoids maintaining heuristic
    // blocklists and respects the model's contextual understanding.
    let memory_section = if sources.is_empty() {
        "【记忆系统】当前未检索到相关记忆。请直接回答用户。".to_owned()
    } else {
        let ctx: Vec<String> = sources
            .iter()
            .enumerate()
            .map(|(i, s)| {
                format!(
                    "[记忆 {}] (相关度: {:.0}%)\n{}",
                    i + 1,
                    s.score * 100.0,
                    s.content
                )
            })
            .collect();
        format!(
            "【记忆系统】以下是从你的记忆库中检索到的内容（按相关度排序）。\n\
             规则：\n\
             - 如果记忆与用户当前问题高度相关（相关度 > 60%），请自然地融入回答中\n\
             - 如果记忆与当前问题无关，请完全忽略它们，直接回答用户\n\
             - 不要提及记忆系统或相关度等系统术语\n\
             - 引用记忆中的知识时，像是你本来就知道的一样自然表达\n\n{}",
            ctx.join("\n\n")
        )
    };

    let history_section = if body.history.is_empty() {
        String::new()
    } else {
        let h: Vec<String> = body
            .history
            .iter()
            .map(|m| {
                format!(
                    "{}: {}",
                    if m.role == "user" { "用户" } else { "助手" },
                    m.content
                )
            })
            .collect();
        format!("\n对话历史:\n{}\n", h.join("\n"))
    };

    let full_prompt = format!(
        "{system_prompt}\n\n{memory_section}\n{history_section}\n用户: {}\n\n请用中文回答。",
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
