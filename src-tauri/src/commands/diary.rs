use std::sync::Arc;

use chrono::Utc;
use tauri::State;

use umms_api::response::*;
use umms_api::AppState;
use umms_persona::{DiaryCategory, DiaryEntry};

#[tauri::command]
pub async fn list_diary(
    state: State<'_, Arc<AppState>>,
    agent_id: String,
) -> Result<DiaryListResponse, String> {
    let entries = state
        .diary_store
        .get_entries(&agent_id, 100)
        .await
        .map_err(|e| format!("Failed to list diary: {e}"))?;

    let total = entries.len();
    let response_entries: Vec<DiaryEntryResponse> =
        entries.into_iter().map(DiaryEntryResponse::from).collect();

    Ok(DiaryListResponse {
        agent_id,
        entries: response_entries,
        total,
    })
}

#[tauri::command]
pub async fn add_diary_entry(
    state: State<'_, Arc<AppState>>,
    agent_id: String,
    category: String,
    content: String,
) -> Result<DiaryEntryResponse, String> {
    let cat: DiaryCategory = category
        .parse()
        .map_err(|e: String| format!("Invalid category: {e}"))?;

    let now = Utc::now();
    let entry = DiaryEntry {
        id: uuid::Uuid::new_v4().to_string(),
        agent_id,
        category: cat,
        content,
        confidence: 0.8,
        source_session_id: None,
        created_at: now,
        updated_at: now,
    };

    state
        .diary_store
        .add_entry(&entry)
        .await
        .map_err(|e| format!("Failed to add diary entry: {e}"))?;

    Ok(DiaryEntryResponse::from(entry))
}

#[tauri::command]
pub async fn update_diary_entry(
    state: State<'_, Arc<AppState>>,
    entry_id: String,
    content: String,
    confidence: f32,
) -> Result<serde_json::Value, String> {
    state
        .diary_store
        .update_entry(&entry_id, &content, confidence)
        .await
        .map_err(|e| format!("Failed to update diary entry: {e}"))?;

    Ok(serde_json::json!({
        "updated": true,
        "entry_id": entry_id,
    }))
}

#[tauri::command]
pub async fn delete_diary_entry(
    state: State<'_, Arc<AppState>>,
    entry_id: String,
) -> Result<serde_json::Value, String> {
    state
        .diary_store
        .delete_entry(&entry_id)
        .await
        .map_err(|e| format!("Failed to delete diary entry: {e}"))?;

    Ok(serde_json::json!({
        "deleted": true,
        "entry_id": entry_id,
    }))
}
