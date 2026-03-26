#![deny(clippy::all, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::missing_errors_doc)]

//! # umms-observe
//!
//! Tracing, metrics, and observability infrastructure for UMMS.

pub mod audit;
pub mod metrics;
pub mod tracing_setup;

pub use audit::{AuditEvent, AuditEventBuilder, AuditEventType, AuditFilter, AuditLog};
pub use metrics::{encode_metrics, init_metrics, metrics, UmmsMetrics};
pub use tracing_setup::init_tracing;
