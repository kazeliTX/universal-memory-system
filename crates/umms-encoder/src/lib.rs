
//! # umms-encoder
//!
//! Encoding service for UMMS.
//!
//! The model management layer ([`ModelPool`], [`GeminiProvider`]) has moved to
//! the `umms-model` crate. This crate re-exports them for backwards
//! compatibility and retains the legacy [`GeminiEncoder`] implementation.
//!
//! All encoders implement the [`umms_core::traits::Encoder`] trait.
//! The `ModelPool` also implements `Encoder`, so existing code that uses
//! `Arc<dyn Encoder>` continues to work without changes.

pub mod gemini;

// Re-export from umms-model for backwards compatibility.
pub use umms_model::{
    EncoderStats, EncoderStatsSnapshot, GeminiProvider, ModelPool, ModelStats, ModelStatus,
};
