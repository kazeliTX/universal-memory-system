//! Prompt Engine — centralised prompt construction from composable sections.
//!
//! Replaces inline string formatting in chat handlers with a template-based
//! approach. Each prompt is built from named sections (system, memory, diary,
//! history, user, instruction) that can be independently configured and
//! truncated.

pub mod diary_generator;
pub mod engine;

pub use engine::{PromptEngine, PromptSection, PromptTemplate};
