//! # umms-model
//!
//! Unified LLM model management layer. Provides:
//! - [`ModelPool`]: centralized registry with task-based routing
//! - [`GeminiProvider`]: Google Gemini API (embedding + generation)
//! - Model activation status and statistics

pub mod gemini_provider;
pub mod pool;
pub mod stats;
pub mod trace;

pub use gemini_provider::GeminiProvider;
pub use pool::{ModelPool, ModelStats, ModelStatus};
pub use stats::{EncoderStats, EncoderStatsSnapshot};
pub use trace::{ModelTrace, TraceStore, TraceSummary};
