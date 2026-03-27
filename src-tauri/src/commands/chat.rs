use std::str::FromStr;
use std::sync::Arc;

use serde::Deserialize;
use tauri::State;

use umms_api::response::*;
use umms_api::AppState;
use umms_core::types::AgentId;
use umms_observe::{AuditEventBuilder, AuditEventType};

#[derive(Debug, Deserialize)]
pub struct ChatMessageArg {
    pub role: String,
    pub content: String,
}

#[tauri::command]
pub async fn chat(
    state: State<'_, Arc<AppState>>,
    agent_id: String,
    message: String,
    history: Vec<ChatMessageArg>,
) -> Result<ChatResponse, String> {
    let aid = AgentId::from_str(&agent_id).map_err(|e| format!("Invalid agent_id: {e}"))?;

    // Load persona
    let persona = state
        .persona_store
        .get(&aid)
        .await
        .map_err(|e| format!("Persona lookup failed: {e}"))?;

    let system_prompt = persona
        .as_ref()
        .map(|p| p.system_prompt.clone())
        .unwrap_or_else(|| format!("You are a helpful AI assistant named {}.", agent_id));

    // Retrieve relevant memories
    let mut sources = Vec::new();
    if let Some(ref retriever) = state.retriever {
        if let Ok(pr) = retriever.retrieve_with_sources(&message, &aid).await {
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

    // Build prompt
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

    let hist = history
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
        "{system_prompt}\n\n{context}\n对话历史:\n{hist}\n\n用户: {message}\n\n请用中文回答。基于上述记忆上下文回答用户问题。如果记忆中没有相关信息，请如实说明。"
    );

    // Generate
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

    // Audit
    state.audit.record(
        AuditEventBuilder::new(AuditEventType::Encode, agent_id.clone()).details(
            serde_json::json!({
                "action": "chat",
                "message_preview": &message[..message.len().min(100)],
                "sources": sources.len(),
                "latency_ms": latency_ms,
            }),
        ),
    );

    Ok(ChatResponse {
        message: response_text,
        agent_id,
        sources,
        latency_ms,
    })
}
