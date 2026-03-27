//! Scheduler API handlers — CRUD for scheduled tasks and execution history.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use tracing::{error, info};
use umms_scheduler::{Schedule, ScheduledTask, TaskExecution, TaskType};

use crate::response::{
    ExecutionListResponse, ScheduledTaskResponse, TaskExecutionResponse, TaskListResponse,
    TriggerTaskResponse,
};
use crate::state::AppState;
use super::memory::ApiError;

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateTaskRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub task_type: String,
    #[serde(default = "default_schedule")]
    pub schedule: String,
    #[serde(default)]
    pub params: serde_json::Value,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_schedule() -> String {
    "manual".to_owned()
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
pub struct UpdateTaskRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub schedule: Option<String>,
    pub enabled: Option<bool>,
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct PaginationParams {
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    50
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `GET /api/scheduler/tasks` — list all scheduled tasks.
pub async fn list_tasks(
    State(state): State<Arc<AppState>>,
) -> Result<Json<TaskListResponse>, ApiError> {
    let store = state.task_store.as_ref().ok_or_else(|| {
        ApiError::Internal("scheduler not initialised".into())
    })?;

    let tasks = store.list_tasks().await.map_err(|e| {
        error!(error = %e, "failed to list tasks");
        ApiError::Internal(format!("failed to list tasks: {e}"))
    })?;

    Ok(Json(TaskListResponse {
        tasks: tasks.into_iter().map(ScheduledTaskResponse::from).collect(),
    }))
}

/// `POST /api/scheduler/tasks` — create a new scheduled task.
pub async fn create_task(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateTaskRequest>,
) -> Result<Json<ScheduledTaskResponse>, ApiError> {
    let store = state.task_store.as_ref().ok_or_else(|| {
        ApiError::Internal("scheduler not initialised".into())
    })?;

    let task_type: TaskType = req
        .task_type
        .parse()
        .map_err(|e: String| ApiError::BadRequest(e))?;

    let schedule: Schedule = req
        .schedule
        .parse()
        .map_err(|e: String| ApiError::BadRequest(e))?;

    let now = Utc::now();
    let mut task = ScheduledTask {
        id: uuid::Uuid::new_v4().to_string(),
        name: req.name,
        description: req.description,
        task_type,
        schedule,
        enabled: req.enabled,
        params: req.params,
        created_at: now,
        updated_at: now,
        last_run_at: None,
        next_run_at: None,
    };

    // Compute initial next_run_at
    task.recompute_next_run(now);

    store.save_task(&task).await.map_err(|e| {
        error!(error = %e, "failed to save task");
        ApiError::Internal(format!("failed to save task: {e}"))
    })?;

    // Wake the scheduler so it picks up the new task immediately.
    if let Some(ref engine) = state.scheduler {
        engine.wake();
    }

    info!(task_id = %task.id, task_name = %task.name, "created scheduled task");

    Ok(Json(ScheduledTaskResponse::from(task)))
}

/// `GET /api/scheduler/tasks/:id` — get a single task.
pub async fn get_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ScheduledTaskResponse>, ApiError> {
    let store = state.task_store.as_ref().ok_or_else(|| {
        ApiError::Internal("scheduler not initialised".into())
    })?;

    let task = store
        .get_task(&id)
        .await
        .map_err(|e| ApiError::Internal(format!("failed to get task: {e}")))?
        .ok_or_else(|| ApiError::NotFound(format!("task not found: {id}")))?;

    Ok(Json(ScheduledTaskResponse::from(task)))
}

/// `PUT /api/scheduler/tasks/:id` — update a task.
pub async fn update_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateTaskRequest>,
) -> Result<Json<ScheduledTaskResponse>, ApiError> {
    let store = state.task_store.as_ref().ok_or_else(|| {
        ApiError::Internal("scheduler not initialised".into())
    })?;

    let mut task = store
        .get_task(&id)
        .await
        .map_err(|e| ApiError::Internal(format!("failed to get task: {e}")))?
        .ok_or_else(|| ApiError::NotFound(format!("task not found: {id}")))?;

    if let Some(name) = req.name {
        task.name = name;
    }
    if let Some(description) = req.description {
        task.description = description;
    }
    if let Some(schedule_str) = req.schedule {
        task.schedule = schedule_str
            .parse()
            .map_err(|e: String| ApiError::BadRequest(e))?;
        task.recompute_next_run(Utc::now());
    }
    if let Some(enabled) = req.enabled {
        task.enabled = enabled;
    }
    if let Some(params) = req.params {
        task.params = params;
    }

    task.updated_at = Utc::now();

    store.save_task(&task).await.map_err(|e| {
        error!(error = %e, "failed to update task");
        ApiError::Internal(format!("failed to update task: {e}"))
    })?;

    if let Some(ref engine) = state.scheduler {
        engine.wake();
    }

    info!(task_id = %task.id, "updated scheduled task");

    Ok(Json(ScheduledTaskResponse::from(task)))
}

/// `DELETE /api/scheduler/tasks/:id` — delete a task.
pub async fn delete_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let store = state.task_store.as_ref().ok_or_else(|| {
        ApiError::Internal("scheduler not initialised".into())
    })?;

    store
        .delete_task(&id)
        .await
        .map_err(|e| ApiError::Internal(format!("failed to delete task: {e}")))?;

    info!(task_id = %id, "deleted scheduled task");

    Ok(Json(serde_json::json!({ "deleted": true, "id": id })))
}

/// `POST /api/scheduler/tasks/:id/run` — trigger a task manually.
pub async fn trigger_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<TriggerTaskResponse>, ApiError> {
    let store = state.task_store.as_ref().ok_or_else(|| {
        ApiError::Internal("scheduler not initialised".into())
    })?;

    let task = store
        .get_task(&id)
        .await
        .map_err(|e| ApiError::Internal(format!("failed to get task: {e}")))?
        .ok_or_else(|| ApiError::NotFound(format!("task not found: {id}")))?;

    let executor = state.task_executor.as_ref().ok_or_else(|| {
        ApiError::Internal("task executor not available".into())
    })?;

    // Record execution start
    let mut exec = TaskExecution::new_running(&task.id);
    let _ = store.record_execution(&exec).await;

    let exec_id = exec.id.clone();

    // Execute in background so the API responds immediately
    let executor = Arc::clone(executor);
    let store = Arc::clone(store);
    let task_clone = task.clone();
    tokio::spawn(async move {
        let result = executor.execute(&task_clone).await;
        exec.finish(result.success, result.details);
        let _ = store.update_execution(&exec).await;

        let now = Utc::now();
        let mut updated = task_clone;
        updated.last_run_at = Some(now);
        updated.recompute_next_run(now);
        let _ = store
            .update_last_run(&updated.id, now, updated.next_run_at)
            .await;
    });

    Ok(Json(TriggerTaskResponse {
        execution_id: exec_id,
        message: format!("task '{}' triggered", task.name),
    }))
}

/// `GET /api/scheduler/executions` — recent execution history (all tasks).
pub async fn recent_executions(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<ExecutionListResponse>, ApiError> {
    let store = state.task_store.as_ref().ok_or_else(|| {
        ApiError::Internal("scheduler not initialised".into())
    })?;

    let execs = store
        .recent_executions(params.limit)
        .await
        .map_err(|e| ApiError::Internal(format!("failed to list executions: {e}")))?;

    Ok(Json(ExecutionListResponse {
        executions: execs.into_iter().map(TaskExecutionResponse::from).collect(),
    }))
}

/// `GET /api/scheduler/tasks/:id/executions` — execution history for a task.
pub async fn task_executions(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<ExecutionListResponse>, ApiError> {
    let store = state.task_store.as_ref().ok_or_else(|| {
        ApiError::Internal("scheduler not initialised".into())
    })?;

    let execs = store
        .executions_for_task(&id, params.limit)
        .await
        .map_err(|e| ApiError::Internal(format!("failed to list executions: {e}")))?;

    Ok(Json(ExecutionListResponse {
        executions: execs.into_iter().map(TaskExecutionResponse::from).collect(),
    }))
}
