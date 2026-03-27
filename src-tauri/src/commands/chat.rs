use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use chrono::Utc;
use serde::Deserialize;
use tauri::State;

use umms_api::prompt::diary_generator::DiaryGenerator;
use umms_api::prompt::engine::PromptEngine;
use umms_api::response::*;
use umms_api::session::{self, ChatMessage as SessionChatMessage, ChatSession, ChatSourceRecord};
use umms_api::AppState;
use umms_core::config::load_config;
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
    session_id: Option<String>,
    #[allow(unused_variables)] history: Vec<ChatMessageArg>,
) -> Result<ChatResponse, String> {
    let chat_config = load_config().chat;
    let aid = AgentId::from_str(&agent_id).map_err(|e| format!("Invalid agent_id: {e}"))?;

    // Load or create session
    let mut current_session = if let Some(ref sid) = session_id {
        state
            .session_store
            .get_session(sid)
            .await
            .map_err(|e| format!("Failed to load session: {e}"))?
            .ok_or_else(|| format!("Session not found: {sid}"))?
    } else {
        let now = Utc::now();
        let sid = uuid::Uuid::new_v4().to_string();
        let title = if chat_config.auto_title {
            let t: String = message.chars().take(50).collect();
            if message.chars().count() > 50 {
                format!("{t}...")
            } else {
                t
            }
        } else {
            "\u{65B0}\u{5BF9}\u{8BDD}".to_owned()
        };

        let mut session = ChatSession {
            id: sid,
            agent_id: agent_id.clone(),
            title,
            messages: vec![],
            created_at: now,
            updated_at: now,
            metadata: serde_json::json!({}),
        };

        // Backwards compat: populate from history arg
        for msg in &history {
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

    // Load diary entries
    let diary_entries = state
        .diary_store
        .get_entries(&agent_id, chat_config.diary_entries_in_prompt)
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

    // Retrieve relevant memories
    let mut sources = Vec::new();
    if let Some(ref retriever) = state.retriever {
        if let Ok(pr) = retriever.retrieve_with_sources(&message, &aid).await {
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

    // Build memory content
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

    // Build history content with context truncation
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

    // Build prompt — try new three-mode system first, fall back to legacy
    let mut vars = HashMap::new();
    vars.insert("system_prompt".to_owned(), system_prompt.clone());
    vars.insert("memory_content".to_owned(), memory_content);
    vars.insert("diary_content".to_owned(), diary_content);
    vars.insert("history_content".to_owned(), history_content);
    vars.insert("user_message".to_owned(), message.clone());

    // Runtime variables for the new prompt system
    let agent_name_val = persona.as_ref().map(|p| p.name.clone()).unwrap_or_else(|| agent_id.clone());
    let agent_role_val = persona.as_ref().map(|p| p.role.clone()).unwrap_or_default();
    vars.insert("AgentName".to_owned(), agent_name_val);
    vars.insert("AgentRole".to_owned(), agent_role_val);
    vars.insert("DateTime".to_owned(), Utc::now().format("%Y-%m-%d %H:%M").to_string());
    vars.insert("SessionTitle".to_owned(), current_session.title.clone());

    let full_prompt = match state.prompt_store.get_prompt_config(&agent_id).await {
        Ok(Some(prompt_config)) => {
            PromptEngine::build_prompt(&prompt_config, &vars)
                .map_err(|e| format!("Prompt build failed: {e}"))?
        }
        _ => {
            // Fallback to legacy template engine
            state
                .prompt_engine
                .build("chat", &vars)
                .map_err(|e| format!("Prompt build failed: {e}"))?
        }
    };

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

    // Append messages to session and save
    let now = Utc::now();

    current_session.messages.push(SessionChatMessage {
        role: "user".to_owned(),
        content: message.clone(),
        timestamp: now,
        sources: vec![],
        latency_ms: None,
    });

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
    let final_session_id = current_session.id.clone();

    // Save session in background
    {
        let session_store = Arc::clone(&state.session_store);
        let session_to_save = current_session.clone();
        tokio::spawn(async move {
            if let Err(e) = session_store.save_session(&session_to_save).await {
                tracing::warn!(error = %e, "failed to save chat session");
            }
        });
    }

    // Audit
    state.audit.record(
        AuditEventBuilder::new(AuditEventType::Encode, agent_id.clone()).details(
            serde_json::json!({
                "action": "chat",
                "session_id": final_session_id,
                "message_preview": &message[..message.len().min(100)],
                "sources": sources.len(),
                "diary_entries": diary_entries.len(),
                "latency_ms": latency_ms,
            }),
        ),
    );

    // Spawn background diary generation
    {
        let pool = Arc::clone(pool);
        let diary_store = Arc::clone(&state.diary_store);
        let agent_id_clone = agent_id.clone();
        let user_msg = message.clone();
        let assistant_resp = response_text.clone();

        tokio::spawn(async move {
            let new_entries = DiaryGenerator::analyze_turn(
                &pool,
                &agent_id_clone,
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
                    agent_id = agent_id_clone,
                    count = new_entries.len(),
                    "diary entries generated"
                );
            }
        });
    }

    Ok(ChatResponse {
        message: response_text,
        agent_id,
        session_id: final_session_id,
        sources,
        latency_ms,
    })
}
