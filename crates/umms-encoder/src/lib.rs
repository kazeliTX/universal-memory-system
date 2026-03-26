#![deny(clippy::all, clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]

//! # umms-encoder
//!
//! Encoding service for converting text into embedding vectors.
//!
//! Currently provides:
//! - [`GeminiEncoder`] — Google Gemini embedding API backend
//!
//! All encoders implement the [`umms_core::traits::Encoder`] trait.
//! Upper layers depend only on that trait, never on concrete implementations.

pub mod gemini;

pub use gemini::{GeminiConfig, GeminiEncoder, EncoderStatsSnapshot};
