use std::sync::Arc;

use tauri::State;

use umms_api::response::FileListResponse;
use umms_api::AppState;
use umms_core::traits::RawFileStore;
use umms_core::types::AgentId;

#[tauri::command]
pub async fn list_files(
    state: State<'_, Arc<AppState>>,
    agent_id: String,
) -> Result<FileListResponse, String> {
    let aid = AgentId::from_str(&agent_id).map_err(|e| e.to_string())?;

    let files = state.files.list(&aid).await.map_err(|e| e.to_string())?;

    Ok(FileListResponse { agent_id, files })
}
