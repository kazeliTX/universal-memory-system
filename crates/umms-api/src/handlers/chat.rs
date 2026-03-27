//! Chat API handler — conversational interface backed by retrieval + generation.

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use chrono::Utc;
use serde::Deserialize;

use umms_core::config::load_config;
use umms_core::types::AgentId;
use umms_observe::{AuditEventBuilder, AuditEventType};

use crate::prompt::diary_generator::DiaryGenerator;
use crate::response::{ChatResponse, ChatSource};
use crate::session::{self, ChatMessage as SessionChatMessage, ChatSession, ChatSourceRecord};
use crate::AppState;

/// Chat message in conversation history (backwards-compatible input format).
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
    /// If provided, loads/continues an existing session.
    pub session_id: Option<String>,
    /// Kept for backwards compatibility; ignored if session_id is provided.
    #[serde(default)]
    pub history: Vec<ChatMessage>,
}

/// POST /api/chat — send a message to an agent and get a response.
///
/// Flow:
/// 1. Load or create session
/// 2. Load agent persona for system prompt
/// 3. Load diary entries for personalisation
/// 4. Retrieve relevant memories via retrieval pipeline
/// 5. Build full prompt via PromptEngine (with context-truncated history)
/// 6. Generate response via model pool
/// 7. Append messages to session and save
/// 8. Spawn background diary generation (fire-and-forget)
/// 9. Return response with session_id
pub async fn chat(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, String> {
    let chat_config = load_config().chat;

    // 1. Validate agent_id
    let agent_id =
        AgentId::from_str(&body.agent_id).map_err(|e| format!("Invalid agent_id: {e}"))?;

    // 2. Load or create session
    let mut current_session = if let Some(ref sid) = body.session_id {
        // Continue existing session
        state
            .session_store
            .get_session(sid)
            .await
            .map_err(|e| format!("Failed to load session: {e}"))?
            .ok_or_else(|| format!("Session not found: {sid}"))?
    } else {
        // Create new session
        let now = Utc::now();
        let session_id = uuid::Uuid::new_v4().to_string();

        // Use first message as title (auto-title), truncated
        let title = if chat_config.auto_title {
            let t: String = body.message.chars().take(50).collect();
            if body.message.chars().count() > 50 {
                format!("{t}...")
            } else {
                t
            }
        } else {
            "新对话".to_owned()
        };

        let mut session = ChatSession {
            id: session_id,
            agent_id: body.agent_id.clone(),
            title,
            messages: vec![],
            created_at: now,
            updated_at: now,
            metadata: serde_json::json!({}),
        };

        // If backwards-compat history is provided, populate session messages
        for msg in &body.history {
            session.messages.push(SessionChatMessage {
                role: msg.role.clone(),
                content: msg.content.clone(),
                timestamp: now,
                sources: vec![],
                latency_ms: None,
            });
        }

        session
    };

    // 3. Load persona
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

    // 4. Load diary entries for this agent
    let diary_entries = state
        .diary_store
        .get_entries(&body.agent_id, chat_config.diary_entries_in_prompt)
        .await
        .unwrap_or_default();

    let diary_content = if diary_entries.is_empty() {
        String::new()
    } else {
        diary_entries
            .iter()
            .map(|e| format!("- [{}] {}", e.category, e.content))
            .collect::<Vec<_>>()
            .join("\n")
    };

    // 5. Retrieve relevant memories
    let mut sources = Vec::new();
    if let Some(ref retriever) = state.retriever {
        if let Ok(pr) = retriever.retrieve_with_sources(&body.message, &agent_id).await {
            sources = pr
                .retrieval
                .entries
                .iter()
                .take(chat_config.max_sources)
                .map(|sm| ChatSource {
                    content: sm.entry.content_text.clone().unwrap_or_default(),
                    score: sm.score,
                    memory_id: sm.entry.id.to_string(),
                })
                .collect();
        }
    }

    // 6. Build memory content for prompt
    let memory_content = if sources.is_empty() {
        String::new()
    } else {
        sources
            .iter()
            .enumerate()
            .map(|(i, s)| {
                format!(
                    "[\u{8BB0}\u{5FC6} {}] (\u{76F8}\u{5173}\u{5EA6}: {:.0}%)\n{}",
                    i + 1,
                    s.score * 100.0,
                    s.content
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    };

    // 7. Build history content — truncate to fit context window
    let truncated = session::truncate_history(
        &current_session.messages,
        chat_config.max_history_messages,
        chat_config.max_history_tokens,
    );

    let history_content = if truncated.is_empty() {
        String::new()
    } else {
        truncated
            .iter()
            .map(|m| {
                format!(
                    "{}: {}",
                    if m.role == "user" {
                        "\u{7528}\u{6237}"
                    } else {
                        "\u{52A9}\u{624B}"
                    },
                    m.content
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    // 8. Build prompt via PromptEngine
    let mut vars = HashMap::new();
    vars.insert("system_prompt".to_owned(), system_prompt);
    vars.insert("memory_content".to_owned(), memory_content);
    vars.insert("diary_content".to_owned(), diary_content);
    vars.insert("history_content".to_owned(), history_content);
    vars.insert("user_message".to_owned(), body.message.clone());

    let full_prompt = state
        .prompt_engine
        .build("chat", &vars)
        .map_err(|e| format!("Prompt build failed: {e}"))?;

    // 9. Generate response
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

    // 10. Append messages to session and save
    let now = Utc::now();

    // Append user message
    current_session.messages.push(SessionChatMessage {
        role: "user".to_owned(),
        content: body.message.clone(),
        timestamp: now,
        sources: vec![],
        latency_ms: None,
    });

    // Append assistant response
    let source_records: Vec<ChatSourceRecord> = sources
        .iter()
        .map(|s| ChatSourceRecord {
            memory_id: s.memory_id.clone(),
            score: s.score,
            content_preview: s.content.chars().take(200).collect(),
        })
        .collect();

    current_session.messages.push(SessionChatMessage {
        role: "assistant".to_owned(),
        content: response_text.clone(),
        timestamp: now,
        sources: source_records,
        latency_ms: Some(latency_ms),
    });

    current_session.updated_at = now;

    let session_id = current_session.id.clone();

    // Save session (fire-and-forget on a clone to not block response)
    {
        let session_store = Arc::clone(&state.session_store);
        let session_to_save = current_session.clone();
        tokio::spawn(async move {
            if let Err(e) = session_store.save_session(&session_to_save).await {
                tracing::warn!(error = %e, "failed to save chat session");
            }
        });
    }

    // 11. Record audit
    state.audit.record(
        AuditEventBuilder::new(AuditEventType::Encode, body.agent_id.clone()).details(
            serde_json::json!({
                "action": "chat",
                "session_id": session_id,
                "message_preview": &body.message[..body.message.len().min(100)],
                "sources": sources.len(),
                "diary_entries": diary_entries.len(),
                "latency_ms": latency_ms,
            }),
        ),
    );

    // 12. Spawn background diary generation (fire-and-forget, no latency impact)
    {
        let pool = Arc::clone(pool);
        let diary_store = Arc::clone(&state.diary_store);
        let agent_id_str = body.agent_id.clone();
        let user_msg = body.message.clone();
        let assistant_resp = response_text.clone();

        tokio::spawn(async move {
            let new_entries = DiaryGenerator::analyze_turn(
                &pool,
                &agent_id_str,
                &user_msg,
                &assistant_resp,
                &diary_entries,
            )
            .await;

            for entry in &new_entries {
                if let Err(e) = diary_store.add_entry(entry).await {
                    tracing::warn!(error = %e, "failed to save diary entry");
                }
            }

            if !new_entries.is_empty() {
                tracing::debug!(
                    agent_id = agent_id_str,
                    count = new_entries.len(),
                    "diary entries generated"
                );
            }
        });
    }

    Ok(Json(ChatResponse {
        message: response_text,
        agent_id: body.agent_id,
        session_id,
        sources,
        latency_ms,
    }))
}
