//! Task executor trait — defines how tasks are run.
//!
//! The trait is defined here in umms-scheduler so it can be referenced by
//! the engine. The concrete implementation lives in umms-api where it has
//! access to AppState.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::task::ScheduledTask;

/// Result of executing a scheduled task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub success: bool,
    pub details: serde_json::Value,
}

/// Trait for executing scheduled tasks.
///
/// Implementors receive the full task definition (including params) and
/// return a result indicating success/failure with details.
#[async_trait]
pub trait TaskExecutor: Send + Sync {
    async fn execute(&self, task: &ScheduledTask) -> TaskResult;
}
