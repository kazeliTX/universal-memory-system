//! Core task types for the unified scheduler.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A scheduled task definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTask {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Task type determines what action to execute.
    pub task_type: TaskType,
    /// Schedule expression controlling when the task runs.
    pub schedule: Schedule,
    pub enabled: bool,
    /// Parameters passed to the task (JSON).
    pub params: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_run_at: Option<DateTime<Utc>>,
    pub next_run_at: Option<DateTime<Utc>>,
}

/// What kind of work the task performs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskType {
    /// Run consolidation for an agent (decay, graph evolution, promotion).
    Consolidation,
    /// Custom task type for future extensibility.
    Custom(String),
}

/// When a task should run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Schedule {
    /// Run every N seconds.
    IntervalSeconds(u64),
    /// Manual-only — never auto-scheduled.
    Manual,
}

impl Schedule {
    /// Compute the next run time given the last run time (or now if never run).
    pub fn next_run_after(&self, last: DateTime<Utc>) -> Option<DateTime<Utc>> {
        match self {
            Self::IntervalSeconds(secs) => {
                Some(last + chrono::Duration::seconds(*secs as i64))
            }
            Self::Manual => None,
        }
    }
}

impl ScheduledTask {
    /// Returns true if the task is due for execution.
    pub fn is_due(&self, now: DateTime<Utc>) -> bool {
        if !self.enabled {
            return false;
        }
        match self.schedule {
            Schedule::Manual => false,
            Schedule::IntervalSeconds(_) => {
                if let Some(next) = self.next_run_at {
                    now >= next
                } else {
                    // Never run before — run immediately.
                    true
                }
            }
        }
    }

    /// Recompute `next_run_at` based on the current time and schedule.
    pub fn recompute_next_run(&mut self, now: DateTime<Utc>) {
        self.next_run_at = self.schedule.next_run_after(now);
    }
}

/// Record of a single task execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskExecution {
    pub id: String,
    pub task_id: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub status: ExecutionStatus,
    /// Result details (JSON).
    pub result: serde_json::Value,
}

/// Status of a task execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExecutionStatus {
    Running,
    Success,
    Failed,
}

impl std::fmt::Display for ExecutionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Running => write!(f, "running"),
            Self::Success => write!(f, "success"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

impl std::str::FromStr for ExecutionStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "running" => Ok(Self::Running),
            "success" => Ok(Self::Success),
            "failed" => Ok(Self::Failed),
            other => Err(format!("unknown execution status: {other}")),
        }
    }
}

impl TaskExecution {
    /// Create a new execution record in Running state.
    pub fn new_running(task_id: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            task_id: task_id.to_owned(),
            started_at: Utc::now(),
            finished_at: None,
            status: ExecutionStatus::Running,
            result: serde_json::Value::Null,
        }
    }

    /// Mark the execution as finished.
    pub fn finish(&mut self, success: bool, details: serde_json::Value) {
        self.finished_at = Some(Utc::now());
        self.status = if success {
            ExecutionStatus::Success
        } else {
            ExecutionStatus::Failed
        };
        self.result = details;
    }
}

impl std::fmt::Display for TaskType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Consolidation => write!(f, "consolidation"),
            Self::Custom(name) => write!(f, "custom:{name}"),
        }
    }
}

impl std::str::FromStr for TaskType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "consolidation" => Ok(Self::Consolidation),
            other if other.starts_with("custom:") => {
                Ok(Self::Custom(other.strip_prefix("custom:").unwrap().to_owned()))
            }
            other => Err(format!("unknown task type: {other}")),
        }
    }
}

impl std::fmt::Display for Schedule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IntervalSeconds(secs) => write!(f, "interval_seconds:{secs}"),
            Self::Manual => write!(f, "manual"),
        }
    }
}

impl std::str::FromStr for Schedule {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "manual" => Ok(Self::Manual),
            other if other.starts_with("interval_seconds:") => {
                let n = other
                    .strip_prefix("interval_seconds:")
                    .unwrap()
                    .parse::<u64>()
                    .map_err(|e| format!("invalid interval: {e}"))?;
                Ok(Self::IntervalSeconds(n))
            }
            other => Err(format!("unknown schedule: {other}")),
        }
    }
}
