//! Session CRUD API handlers.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;

use crate::response::{
    ChatSource, CreateSessionResponse, SessionDetailResponse, SessionListResponse,
    SessionMessageResponse, SessionSummaryResponse,
};
use crate::session::ChatSession;
use crate::AppState;

/// Query parameters for listing sessions.
#[derive(Debug, Deserialize)]
pub struct ListSessionsQuery {
    pub agent_id: Option<String>,
}

/// Request body for creating a new session.
#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    pub agent_id: String,
    pub title: Option<String>,
}

/// Request body for renaming a session.
#[derive(Debug, Deserialize)]
pub struct RenameSessionRequest {
    pub title: String,
}

/// GET /api/sessions — list all sessions (optionally filtered by agent_id).
pub async fn list_sessions(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListSessionsQuery>,
) -> Result<Json<SessionListResponse>, String> {
    let summaries = state
        .session_store
        .list_sessions(query.agent_id.as_deref())
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

    Ok(Json(SessionListResponse { sessions }))
}

/// GET /api/sessions/:id — get full session with messages.
pub async fn get_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<SessionDetailResponse>, String> {
    let session = state
        .session_store
        .get_session(&id)
        .await
        .map_err(|e| format!("Failed to get session: {e}"))?
        .ok_or_else(|| format!("Session not found: {id}"))?;

    Ok(Json(session_to_detail(session)))
}

/// POST /api/sessions — create a new empty session.
pub async fn create_session(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateSessionRequest>,
) -> Result<Json<CreateSessionResponse>, String> {
    let now = chrono::Utc::now();
    let session_id = uuid::Uuid::new_v4().to_string();
    let title = body.title.unwrap_or_else(|| "新对话".to_owned());

    let session = ChatSession {
        id: session_id.clone(),
        agent_id: body.agent_id.clone(),
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

    Ok(Json(CreateSessionResponse {
        id: session_id,
        agent_id: body.agent_id,
        title,
        created_at: now.to_rfc3339(),
    }))
}

/// PUT /api/sessions/:id/title — rename a session.
pub async fn rename_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<RenameSessionRequest>,
) -> Result<Json<serde_json::Value>, String> {
    state
        .session_store
        .update_title(&id, &body.title)
        .await
        .map_err(|e| format!("Failed to rename session: {e}"))?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// DELETE /api/sessions/:id — delete a session.
pub async fn delete_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, String> {
    state
        .session_store
        .delete_session(&id)
        .await
        .map_err(|e| format!("Failed to delete session: {e}"))?;

    Ok(Json(serde_json::json!({ "ok": true, "id": id })))
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
