//! Diary API handlers — CRUD for agent diary entries.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use chrono::Utc;
use serde::Deserialize;

use umms_persona::{DiaryCategory, DiaryEntry};

use crate::AppState;
use crate::response::{DiaryEntryResponse, DiaryListResponse};

/// GET /api/diary/:agent_id — list diary entries for an agent.
pub async fn list_diary(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
) -> Result<Json<DiaryListResponse>, String> {
    let entries = state
        .diary_store
        .get_entries(&agent_id, 100)
        .await
        .map_err(|e| format!("Failed to list diary: {e}"))?;

    let total = entries.len();
    let response_entries: Vec<DiaryEntryResponse> =
        entries.into_iter().map(DiaryEntryResponse::from).collect();

    Ok(Json(DiaryListResponse {
        agent_id,
        entries: response_entries,
        total,
    }))
}

/// POST /api/diary/:agent_id — manually add a diary entry.
#[derive(Debug, Deserialize)]
pub struct AddDiaryRequest {
    pub category: String,
    pub content: String,
    #[serde(default = "default_confidence")]
    pub confidence: f32,
}

fn default_confidence() -> f32 {
    0.8
}

pub async fn add_diary(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
    Json(body): Json<AddDiaryRequest>,
) -> Result<Json<DiaryEntryResponse>, String> {
    let category: DiaryCategory = body
        .category
        .parse()
        .map_err(|e: String| format!("Invalid category: {e}"))?;

    let now = Utc::now();
    let entry = DiaryEntry {
        id: uuid::Uuid::new_v4().to_string(),
        agent_id,
        category,
        content: body.content,
        confidence: body.confidence.clamp(0.0, 1.0),
        source_session_id: None,
        created_at: now,
        updated_at: now,
    };

    state
        .diary_store
        .add_entry(&entry)
        .await
        .map_err(|e| format!("Failed to add diary entry: {e}"))?;

    Ok(Json(DiaryEntryResponse::from(entry)))
}

/// PUT /api/diary/:agent_id/:entry_id — update a diary entry.
#[derive(Debug, Deserialize)]
pub struct UpdateDiaryRequest {
    pub content: String,
    #[serde(default = "default_confidence")]
    pub confidence: f32,
}

pub async fn update_diary(
    State(state): State<Arc<AppState>>,
    Path((_agent_id, entry_id)): Path<(String, String)>,
    Json(body): Json<UpdateDiaryRequest>,
) -> Result<Json<serde_json::Value>, String> {
    state
        .diary_store
        .update_entry(&entry_id, &body.content, body.confidence)
        .await
        .map_err(|e| format!("Failed to update diary entry: {e}"))?;

    Ok(Json(serde_json::json!({
        "updated": true,
        "entry_id": entry_id,
    })))
}

/// DELETE /api/diary/:agent_id/:entry_id — delete a diary entry.
pub async fn delete_diary(
    State(state): State<Arc<AppState>>,
    Path((_agent_id, entry_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, String> {
    state
        .diary_store
        .delete_entry(&entry_id)
        .await
        .map_err(|e| format!("Failed to delete diary entry: {e}"))?;

    Ok(Json(serde_json::json!({
        "deleted": true,
        "entry_id": entry_id,
    })))
}
