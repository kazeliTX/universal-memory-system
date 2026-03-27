//! Tauri commands for chat session management.

use std::sync::Arc;

use tauri::State;

use umms_api::response::*;
use umms_api::session::ChatSession;
use umms_api::AppState;

#[tauri::command]
pub async fn list_sessions(
    state: State<'_, Arc<AppState>>,
    agent_id: Option<String>,
) -> Result<SessionListResponse, String> {
    let summaries = state
        .session_store
        .list_sessions(agent_id.as_deref())
        .await
        .map_err(|e| format!("Failed to list sessions: {e}"))?;

    let sessions = summaries
        .into_iter()
        .map(|s| SessionSummaryResponse {
            id: s.id,
            agent_id: s.agent_id,
            title: s.title,
            message_count: s.message_count,
            last_message_preview: s.last_message_preview,
            created_at: s.created_at.to_rfc3339(),
            updated_at: s.updated_at.to_rfc3339(),
        })
        .collect();

    Ok(SessionListResponse { sessions })
}

#[tauri::command]
pub async fn get_session(
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> Result<SessionDetailResponse, String> {
    let session = state
        .session_store
        .get_session(&session_id)
        .await
        .map_err(|e| format!("Failed to get session: {e}"))?
        .ok_or_else(|| format!("Session not found: {session_id}"))?;

    Ok(session_to_detail(session))
}

#[tauri::command]
pub async fn create_session(
    state: State<'_, Arc<AppState>>,
    agent_id: String,
    title: Option<String>,
) -> Result<CreateSessionResponse, String> {
    let now = chrono::Utc::now();
    let session_id = uuid::Uuid::new_v4().to_string();
    let title = title.unwrap_or_else(|| "\u{65B0}\u{5BF9}\u{8BDD}".to_owned());

    let session = ChatSession {
        id: session_id.clone(),
        agent_id: agent_id.clone(),
        title: title.clone(),
        messages: vec![],
        created_at: now,
        updated_at: now,
        metadata: serde_json::json!({}),
    };

    state
        .session_store
        .save_session(&session)
        .await
        .map_err(|e| format!("Failed to create session: {e}"))?;

    Ok(CreateSessionResponse {
        id: session_id,
        agent_id,
        title,
        created_at: now.to_rfc3339(),
    })
}

#[tauri::command]
pub async fn rename_session(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    title: String,
) -> Result<serde_json::Value, String> {
    state
        .session_store
        .update_title(&session_id, &title)
        .await
        .map_err(|e| format!("Failed to rename session: {e}"))?;

    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
pub async fn delete_session(
    state: State<'_, Arc<AppState>>,
    session_id: String,
) -> Result<serde_json::Value, String> {
    state
        .session_store
        .delete_session(&session_id)
        .await
        .map_err(|e| format!("Failed to delete session: {e}"))?;

    Ok(serde_json::json!({ "ok": true, "id": session_id }))
}

/// Convert a `ChatSession` to the API response type.
fn session_to_detail(session: ChatSession) -> SessionDetailResponse {
    let messages = session
        .messages
        .into_iter()
        .map(|m| SessionMessageResponse {
            role: m.role,
            content: m.content,
            timestamp: m.timestamp.to_rfc3339(),
            sources: m
                .sources
                .into_iter()
                .map(|s| ChatSource {
                    content: s.content_preview,
                    score: s.score,
                    memory_id: s.memory_id,
                })
                .collect(),
            latency_ms: m.latency_ms,
        })
        .collect();

    SessionDetailResponse {
        id: session.id,
        agent_id: session.agent_id,
        title: session.title,
        messages,
        created_at: session.created_at.to_rfc3339(),
        updated_at: session.updated_at.to_rfc3339(),
        metadata: session.metadata,
    }
}
