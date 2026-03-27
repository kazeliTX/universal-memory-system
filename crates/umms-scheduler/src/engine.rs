//! Scheduler engine — background loop that checks for and runs due tasks.

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tokio::sync::Notify;
use tracing::{debug, error, info, warn};

use crate::executor::TaskExecutor;
use crate::store::TaskStore;
use crate::task::TaskExecution;

/// The scheduler engine periodically checks for due tasks and executes them.
///
/// Runs as a background tokio task. Can be woken early via [`wake`] when
/// tasks are created or updated.
pub struct SchedulerEngine {
    store: Arc<TaskStore>,
    notify: Arc<Notify>,
    check_interval_secs: u64,
}

impl SchedulerEngine {
    /// Create a new scheduler engine.
    ///
    /// `check_interval_secs` controls how often the engine polls for due tasks.
    /// The engine can also be woken early via [`wake`].
    pub fn new(store: Arc<TaskStore>, check_interval_secs: u64) -> Self {
        Self {
            store,
            notify: Arc::new(Notify::new()),
            check_interval_secs,
        }
    }

    /// Start the scheduler loop as a background tokio task.
    ///
    /// The loop runs until the `JoinHandle` is dropped/aborted.
    pub fn start(
        self: Arc<Self>,
        executor: Arc<dyn TaskExecutor>,
    ) -> tokio::task::JoinHandle<()> {
        let engine = Arc::clone(&self);
        tokio::spawn(async move {
            info!(
                interval_secs = engine.check_interval_secs,
                "scheduler engine started"
            );
            engine.run_loop(executor).await;
        })
    }

    /// Notify the engine to re-check tasks immediately.
    ///
    /// Call this after creating, updating, or deleting a task so the engine
    /// picks up changes without waiting for the next poll cycle.
    pub fn wake(&self) {
        self.notify.notify_one();
    }

    /// The main scheduler loop.
    async fn run_loop(&self, executor: Arc<dyn TaskExecutor>) {
        loop {
            tokio::select! {
                () = tokio::time::sleep(Duration::from_secs(self.check_interval_secs)) => {
                    debug!("scheduler tick (interval)");
                }
                () = self.notify.notified() => {
                    debug!("scheduler tick (wake)");
                }
            }

            self.check_and_run(&executor).await;
        }
    }

    /// Check all enabled tasks and run any that are due.
    async fn check_and_run(&self, executor: &Arc<dyn TaskExecutor>) {
        let tasks = match self.store.list_tasks().await {
            Ok(t) => t,
            Err(e) => {
                error!(error = %e, "failed to list tasks");
                return;
            }
        };

        let now = Utc::now();

        for task in &tasks {
            if !task.is_due(now) {
                continue;
            }

            info!(task_id = %task.id, task_name = %task.name, "executing scheduled task");

            // Record execution start
            let mut exec = TaskExecution::new_running(&task.id);
            if let Err(e) = self.store.record_execution(&exec).await {
                warn!(task_id = %task.id, error = %e, "failed to record execution start");
            }

            // Execute the task
            let result = executor.execute(task).await;

            // Record execution result
            exec.finish(result.success, result.details.clone());
            if let Err(e) = self.store.update_execution(&exec).await {
                warn!(task_id = %task.id, error = %e, "failed to update execution record");
            }

            // Update last_run_at and recompute next_run_at
            let mut updated_task = task.clone();
            updated_task.last_run_at = Some(now);
            updated_task.recompute_next_run(now);

            if let Err(e) = self
                .store
                .update_last_run(&task.id, now, updated_task.next_run_at)
                .await
            {
                warn!(task_id = %task.id, error = %e, "failed to update last_run");
            }

            if result.success {
                info!(task_id = %task.id, "task completed successfully");
            } else {
                warn!(task_id = %task.id, "task failed: {:?}", result.details);
            }
        }
    }
}
