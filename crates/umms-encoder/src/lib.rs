
//! # umms-encoder
//!
//! Encoding and model management service for UMMS.
//!
//! Provides:
//! - [`GeminiEncoder`] — Google Gemini embedding API backend (legacy, standalone)
//! - [`GeminiProvider`] — Gemini model provider supporting embedding + generation
//! - [`ModelPool`] — Centralized registry routing requests to the right model
//!
//! All encoders implement the [`umms_core::traits::Encoder`] trait.
//! The `ModelPool` also implements `Encoder`, so existing code that uses
//! `Arc<dyn Encoder>` continues to work without changes.

pub mod gemini;
pub mod gemini_provider;
pub mod pool;

pub use gemini::{GeminiConfig, GeminiEncoder, EncoderStatsSnapshot};
pub use gemini_provider::GeminiProvider;
pub use pool::ModelPool;
