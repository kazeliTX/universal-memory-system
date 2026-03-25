#![deny(clippy::all, clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]

//! # umms-core
//!
//! Core types, traits, and error definitions for the UMMS memory system.
//! This crate contains no business logic — only shared data structures
//! and trait interfaces that other crates depend on.

pub mod error;
pub mod traits;
pub mod types;

pub use error::{Result, StorageError, UmmsError};
pub use traits::*;
pub use types::*;
