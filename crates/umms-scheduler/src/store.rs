//! SQLite-backed storage for scheduled tasks and execution history.

use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use tokio::sync::Mutex;

use crate::task::{ExecutionStatus, Schedule, ScheduledTask, TaskExecution, TaskType};

/// Persistent task store backed by SQLite.
pub struct TaskStore {
    conn: Arc<Mutex<Connection>>,
}

impl TaskStore {
    /// Open (or create) the task store at the given path.
    pub fn new(path: impl AsRef<Path>) -> Result<Self, String> {
        let conn = Connection::open(path.as_ref())
            .map_err(|e| format!("failed to open scheduler db: {e}"))?;

        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS scheduled_tasks (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                task_type TEXT NOT NULL,
                schedule TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                params TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                last_run_at TEXT,
                next_run_at TEXT
            );

            CREATE TABLE IF NOT EXISTS task_executions (
                id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL,
                started_at TEXT NOT NULL,
                finished_at TEXT,
                status TEXT NOT NULL,
                result TEXT NOT NULL DEFAULT '{}'
            );

            CREATE INDEX IF NOT EXISTS idx_exec_task_id ON task_executions(task_id);
            CREATE INDEX IF NOT EXISTS idx_exec_started ON task_executions(started_at DESC);
            ",
        )
        .map_err(|e| format!("failed to initialise scheduler schema: {e}"))?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Save (insert or replace) a scheduled task.
    pub async fn save_task(&self, task: &ScheduledTask) -> Result<(), String> {
        let task = task.clone();
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            conn.execute(
                "INSERT OR REPLACE INTO scheduled_tasks
                 (id, name, description, task_type, schedule, enabled, params,
                  created_at, updated_at, last_run_at, next_run_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                rusqlite::params![
                    task.id,
                    task.name,
                    task.description,
                    task.task_type.to_string(),
                    task.schedule.to_string(),
                    i32::from(task.enabled),
                    task.params.to_string(),
                    task.created_at.to_rfc3339(),
                    task.updated_at.to_rfc3339(),
                    task.last_run_at.map(|t| t.to_rfc3339()),
                    task.next_run_at.map(|t| t.to_rfc3339()),
                ],
            )
            .map_err(|e| format!("save_task: {e}"))?;
            Ok(())
        })
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))?
    }

    /// Get a single task by ID.
    pub async fn get_task(&self, id: &str) -> Result<Option<ScheduledTask>, String> {
        let id = id.to_owned();
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            let mut stmt = conn
                .prepare(
                    "SELECT id, name, description, task_type, schedule, enabled, params,
                            created_at, updated_at, last_run_at, next_run_at
                     FROM scheduled_tasks WHERE id = ?1",
                )
                .map_err(|e| format!("get_task prepare: {e}"))?;

            let mut rows = stmt
                .query_map(rusqlite::params![id], row_to_task)
                .map_err(|e| format!("get_task query: {e}"))?;

            match rows.next() {
                Some(Ok(task)) => Ok(Some(task)),
                Some(Err(e)) => Err(format!("get_task row: {e}")),
                None => Ok(None),
            }
        })
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))?
    }

    /// List all scheduled tasks.
    pub async fn list_tasks(&self) -> Result<Vec<ScheduledTask>, String> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            let mut stmt = conn
                .prepare(
                    "SELECT id, name, description, task_type, schedule, enabled, params,
                            created_at, updated_at, last_run_at, next_run_at
                     FROM scheduled_tasks ORDER BY created_at ASC",
                )
                .map_err(|e| format!("list_tasks prepare: {e}"))?;

            let rows = stmt
                .query_map([], row_to_task)
                .map_err(|e| format!("list_tasks query: {e}"))?;

            let mut tasks = Vec::new();
            for row in rows {
                tasks.push(row.map_err(|e| format!("list_tasks row: {e}"))?);
            }
            Ok(tasks)
        })
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))?
    }

    /// Delete a task by ID.
    pub async fn delete_task(&self, id: &str) -> Result<(), String> {
        let id = id.to_owned();
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            conn.execute(
                "DELETE FROM scheduled_tasks WHERE id = ?1",
                rusqlite::params![id],
            )
            .map_err(|e| format!("delete_task: {e}"))?;
            // Also clean up execution history
            conn.execute(
                "DELETE FROM task_executions WHERE task_id = ?1",
                rusqlite::params![id],
            )
            .map_err(|e| format!("delete_task executions: {e}"))?;
            Ok(())
        })
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))?
    }

    /// Update last_run_at and recompute next_run_at for a task.
    pub async fn update_last_run(
        &self,
        id: &str,
        at: DateTime<Utc>,
        next: Option<DateTime<Utc>>,
    ) -> Result<(), String> {
        let id = id.to_owned();
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            conn.execute(
                "UPDATE scheduled_tasks SET last_run_at = ?1, next_run_at = ?2, updated_at = ?3
                 WHERE id = ?4",
                rusqlite::params![
                    at.to_rfc3339(),
                    next.map(|t| t.to_rfc3339()),
                    Utc::now().to_rfc3339(),
                    id,
                ],
            )
            .map_err(|e| format!("update_last_run: {e}"))?;
            Ok(())
        })
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))?
    }

    /// Record a new execution.
    pub async fn record_execution(&self, exec: &TaskExecution) -> Result<(), String> {
        let exec = exec.clone();
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            conn.execute(
                "INSERT INTO task_executions (id, task_id, started_at, finished_at, status, result)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    exec.id,
                    exec.task_id,
                    exec.started_at.to_rfc3339(),
                    exec.finished_at.map(|t| t.to_rfc3339()),
                    exec.status.to_string(),
                    exec.result.to_string(),
                ],
            )
            .map_err(|e| format!("record_execution: {e}"))?;
            Ok(())
        })
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))?
    }

    /// Update an existing execution record (e.g., when it finishes).
    pub async fn update_execution(&self, exec: &TaskExecution) -> Result<(), String> {
        let exec = exec.clone();
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            conn.execute(
                "UPDATE task_executions SET finished_at = ?1, status = ?2, result = ?3
                 WHERE id = ?4",
                rusqlite::params![
                    exec.finished_at.map(|t| t.to_rfc3339()),
                    exec.status.to_string(),
                    exec.result.to_string(),
                    exec.id,
                ],
            )
            .map_err(|e| format!("update_execution: {e}"))?;
            Ok(())
        })
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))?
    }

    /// Get execution history for a specific task.
    pub async fn executions_for_task(
        &self,
        task_id: &str,
        limit: usize,
    ) -> Result<Vec<TaskExecution>, String> {
        let task_id = task_id.to_owned();
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            let mut stmt = conn
                .prepare(
                    "SELECT id, task_id, started_at, finished_at, status, result
                     FROM task_executions
                     WHERE task_id = ?1
                     ORDER BY started_at DESC
                     LIMIT ?2",
                )
                .map_err(|e| format!("executions_for_task prepare: {e}"))?;

            let rows = stmt
                .query_map(rusqlite::params![task_id, limit as i64], row_to_execution)
                .map_err(|e| format!("executions_for_task query: {e}"))?;

            let mut execs = Vec::new();
            for row in rows {
                execs.push(row.map_err(|e| format!("executions_for_task row: {e}"))?);
            }
            Ok(execs)
        })
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))?
    }

    /// Get recent execution history across all tasks.
    pub async fn recent_executions(&self, limit: usize) -> Result<Vec<TaskExecution>, String> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            let mut stmt = conn
                .prepare(
                    "SELECT id, task_id, started_at, finished_at, status, result
                     FROM task_executions
                     ORDER BY started_at DESC
                     LIMIT ?1",
                )
                .map_err(|e| format!("recent_executions prepare: {e}"))?;

            let rows = stmt
                .query_map(rusqlite::params![limit as i64], row_to_execution)
                .map_err(|e| format!("recent_executions query: {e}"))?;

            let mut execs = Vec::new();
            for row in rows {
                execs.push(row.map_err(|e| format!("recent_executions row: {e}"))?);
            }
            Ok(execs)
        })
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))?
    }

    /// Check if any tasks exist (for first-run seeding).
    pub async fn is_empty(&self) -> Result<bool, String> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.blocking_lock();
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM scheduled_tasks", [], |row| row.get(0))
                .map_err(|e| format!("is_empty: {e}"))?;
            Ok(count == 0)
        })
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))?
    }
}

/// Map a SQLite row to a `ScheduledTask`.
fn row_to_task(row: &rusqlite::Row<'_>) -> rusqlite::Result<ScheduledTask> {
    let task_type_str: String = row.get(3)?;
    let schedule_str: String = row.get(4)?;
    let enabled_int: i32 = row.get(5)?;
    let params_str: String = row.get(6)?;
    let created_str: String = row.get(7)?;
    let updated_str: String = row.get(8)?;
    let last_run_str: Option<String> = row.get(9)?;
    let next_run_str: Option<String> = row.get(10)?;

    Ok(ScheduledTask {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        task_type: task_type_str
            .parse()
            .unwrap_or(TaskType::Custom(task_type_str)),
        schedule: schedule_str.parse().unwrap_or(Schedule::Manual),
        enabled: enabled_int != 0,
        params: serde_json::from_str(&params_str).unwrap_or_default(),
        created_at: DateTime::parse_from_rfc3339(&created_str)
            .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc)),
        updated_at: DateTime::parse_from_rfc3339(&updated_str)
            .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc)),
        last_run_at: last_run_str.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .map(|dt| dt.with_timezone(&Utc))
                .ok()
        }),
        next_run_at: next_run_str.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .map(|dt| dt.with_timezone(&Utc))
                .ok()
        }),
    })
}

/// Map a SQLite row to a `TaskExecution`.
fn row_to_execution(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskExecution> {
    let started_str: String = row.get(2)?;
    let finished_str: Option<String> = row.get(3)?;
    let status_str: String = row.get(4)?;
    let result_str: String = row.get(5)?;

    Ok(TaskExecution {
        id: row.get(0)?,
        task_id: row.get(1)?,
        started_at: DateTime::parse_from_rfc3339(&started_str)
            .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc)),
        finished_at: finished_str.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .map(|dt| dt.with_timezone(&Utc))
                .ok()
        }),
        status: status_str.parse().unwrap_or(ExecutionStatus::Failed),
        result: serde_json::from_str(&result_str).unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{Schedule, TaskType};

    fn temp_store() -> TaskStore {
        TaskStore::new(":memory:").expect("in-memory store")
    }

    #[tokio::test]
    async fn crud_task() {
        let store = temp_store();
        let now = Utc::now();
        let task = ScheduledTask {
            id: "test-1".into(),
            name: "Test Task".into(),
            description: "A test".into(),
            task_type: TaskType::Consolidation,
            schedule: Schedule::IntervalSeconds(3600),
            enabled: true,
            params: serde_json::json!({"agent_id": "coder"}),
            created_at: now,
            updated_at: now,
            last_run_at: None,
            next_run_at: Some(now),
        };

        store.save_task(&task).await.unwrap();

        let loaded = store.get_task("test-1").await.unwrap().unwrap();
        assert_eq!(loaded.id, "test-1");
        assert_eq!(loaded.name, "Test Task");
        assert_eq!(loaded.task_type, TaskType::Consolidation);

        let all = store.list_tasks().await.unwrap();
        assert_eq!(all.len(), 1);

        store.delete_task("test-1").await.unwrap();
        assert!(store.get_task("test-1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn execution_recording() {
        let store = temp_store();

        let mut exec = TaskExecution::new_running("task-1");
        store.record_execution(&exec).await.unwrap();

        exec.finish(true, serde_json::json!({"ok": true}));
        store.update_execution(&exec).await.unwrap();

        let history = store.executions_for_task("task-1", 10).await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].status, ExecutionStatus::Success);
    }

    #[tokio::test]
    async fn is_due_logic() {
        let now = Utc::now();
        let past = now - chrono::Duration::seconds(100);

        let mut task = ScheduledTask {
            id: "t".into(),
            name: "T".into(),
            description: String::new(),
            task_type: TaskType::Consolidation,
            schedule: Schedule::IntervalSeconds(60),
            enabled: true,
            params: serde_json::Value::Null,
            created_at: now,
            updated_at: now,
            last_run_at: None,
            next_run_at: Some(past),
        };

        assert!(task.is_due(now));

        task.next_run_at = Some(now + chrono::Duration::seconds(3600));
        assert!(!task.is_due(now));

        task.schedule = Schedule::Manual;
        task.next_run_at = Some(past);
        assert!(!task.is_due(now));

        task.schedule = Schedule::IntervalSeconds(60);
        task.enabled = false;
        task.next_run_at = Some(past);
        assert!(!task.is_due(now));
    }

    #[test]
    fn schedule_roundtrip() {
        let s = Schedule::IntervalSeconds(3600);
        let serialised = s.to_string();
        assert_eq!(serialised, "interval_seconds:3600");
        let parsed: Schedule = serialised.parse().unwrap();
        assert_eq!(parsed, s);

        let m = Schedule::Manual;
        assert_eq!(m.to_string().parse::<Schedule>().unwrap(), m);
    }

    #[test]
    fn task_type_roundtrip() {
        let c = TaskType::Consolidation;
        assert_eq!(c.to_string().parse::<TaskType>().unwrap(), c);

        let custom = TaskType::Custom("my-task".into());
        assert_eq!(custom.to_string().parse::<TaskType>().unwrap(), custom);
    }
}
