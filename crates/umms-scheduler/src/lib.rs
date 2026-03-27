//! # umms-scheduler
//!
//! Unified scheduler / timer module for UMMS.
//!
//! Provides:
//! - [`ScheduledTask`] — task definitions with schedule, type, and params
//! - [`TaskStore`] — SQLite-backed persistence for tasks and execution history
//! - [`SchedulerEngine`] — background loop that checks for and runs due tasks
//! - [`TaskExecutor`] — trait for pluggable task execution (implemented in umms-api)
//!
//! The scheduler is generic: `TaskType::Custom` allows adding new task types
//! without modifying this crate.

pub mod engine;
pub mod executor;
pub mod store;
pub mod task;

pub use engine::SchedulerEngine;
pub use executor::{TaskExecutor, TaskResult};
pub use store::TaskStore;
pub use task::*;
