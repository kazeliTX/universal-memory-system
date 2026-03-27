//! Tauri Commands — thin wrappers that delegate to `umms-api` logic.
//!
//! Each command receives `State<Arc<AppState>>` via Tauri's managed state
//! and returns serialisable response types from `umms_api::response`.
//!
//! Design rule: **zero business logic here**. Commands are glue code that:
//! 1. Extract parameters from Tauri's invoke payload
//! 2. Call the appropriate storage/service method
//! 3. Return the response type
//!
//! If you find yourself writing more than ~10 lines in a command, the logic
//! belongs in `umms-api` or `umms-storage`.

pub mod memory;
pub mod graph;
pub mod agent;
pub mod chat;
pub mod consolidation;
pub mod audit;
pub mod diary;
pub mod encoder;
pub mod ingest;
pub mod persona;
pub mod system;
pub mod files;
pub mod sessions;
pub mod prompts;
pub mod tags;
pub mod traces;
