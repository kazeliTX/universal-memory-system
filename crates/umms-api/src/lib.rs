//! # umms-api
//!
//! HTTP API layer for the Universal Multimodal Memory System.
//!
//! This crate provides:
//! - [`AppState`] — shared application state (storage backends + audit + metrics)
//! - [`AppConfig`] — configuration for state initialisation
//! - [`build_router`] — Axum router factory with all API endpoints
//!
//! The router is consumed by:
//! - **Tauri**: embedded as a background Axum server for external Agent access
//! - **Tests**: constructed in integration tests without a real server
//!
//! ## Design decisions
//!
//! - Handlers are stateless functions that receive `Arc<AppState>` via Axum extractors.
//! - Response types in [`response`] are shared with Tauri Commands (same JSON shape).
//! - [`ApiError`](error::ApiError) converts to proper HTTP status codes.

pub mod error;
pub mod handlers;
pub mod prompt;
pub mod response;
pub mod services;
pub mod session;
pub mod state;

use std::sync::Arc;

use axum::Router;
use axum::routing::{delete, get, post, put};
use tower_http::cors::CorsLayer;

pub use state::{AppConfig, AppState};

/// Build the complete Axum router with all API endpoints.
///
/// The returned router is ready to be served with `axum::serve()`.
/// It does not include static file serving — that is the caller's responsibility
/// (Tauri serves the Vue build, tests don't need it).
#[allow(clippy::too_many_lines)]
pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        // System
        .route("/api/health", get(handlers::system::health))
        .route("/api/stats", get(handlers::system::stats))
        .route("/api/metrics", get(handlers::system::metrics))
        .route("/api/demo/seed", get(handlers::system::seed))
        .route("/api/demo/clear", get(handlers::system::clear))
        // Memory browsing
        .route(
            "/api/memories/cache/{agent_id}",
            get(handlers::memory::cache_entries),
        )
        .route(
            "/api/memories/vector/{agent_id}",
            get(handlers::memory::vector_entries),
        )
        .route(
            "/api/memories/vector/entry/{id}",
            get(handlers::memory::vector_detail),
        )
        .route(
            "/api/memories/{memory_id}/rate",
            post(handlers::memory::rate_memory),
        )
        // Knowledge graph
        .route(
            "/api/memories/graph/{agent_id}",
            get(handlers::graph::graph_nodes),
        )
        .route(
            "/api/memories/graph/node/{node_id}",
            get(handlers::graph::node_detail),
        )
        .route(
            "/api/memories/graph/traverse/{node_id}",
            get(handlers::graph::traverse),
        )
        .route(
            "/api/memories/graph/search",
            get(handlers::graph::graph_search),
        )
        // Files
        .route(
            "/api/memories/files/{agent_id}",
            get(handlers::files::file_list),
        )
        // Agent persona (M7)
        .route("/api/agents", get(handlers::agent::list_agents))
        .route("/api/agents", post(handlers::agent::create_agent))
        .route(
            "/api/agents/{agent_id}",
            get(handlers::agent::agent_detail),
        )
        .route(
            "/api/agents/{agent_id}",
            put(handlers::agent::update_agent),
        )
        .route(
            "/api/agents/{agent_id}",
            delete(handlers::agent::delete_agent),
        )
        // Prompts (three-mode system)
        .route(
            "/api/prompts/{agent_id}",
            get(handlers::prompts::get_prompt_config),
        )
        .route(
            "/api/prompts/{agent_id}",
            put(handlers::prompts::save_prompt_config),
        )
        .route(
            "/api/prompts/{agent_id}/mode",
            put(handlers::prompts::switch_mode),
        )
        .route(
            "/api/prompts/{agent_id}/blocks",
            post(handlers::prompts::add_block),
        )
        .route(
            "/api/prompts/{agent_id}/blocks/{block_id}",
            put(handlers::prompts::update_block),
        )
        .route(
            "/api/prompts/{agent_id}/blocks/{block_id}",
            delete(handlers::prompts::delete_block),
        )
        .route(
            "/api/prompts/{agent_id}/blocks/reorder",
            put(handlers::prompts::reorder_blocks),
        )
        .route(
            "/api/prompts/{agent_id}/blocks/{block_id}/variants",
            post(handlers::prompts::add_variant),
        )
        .route(
            "/api/prompts/{agent_id}/blocks/{block_id}/variant/{idx}",
            put(handlers::prompts::select_variant),
        )
        .route(
            "/api/prompts/warehouses",
            get(handlers::prompts::list_warehouses),
        )
        .route(
            "/api/prompts/warehouses",
            post(handlers::prompts::create_warehouse),
        )
        .route(
            "/api/prompts/warehouses/{name}",
            get(handlers::prompts::get_warehouse),
        )
        .route(
            "/api/prompts/warehouses/{name}",
            put(handlers::prompts::update_warehouse),
        )
        .route(
            "/api/prompts/warehouses/{name}",
            delete(handlers::prompts::delete_warehouse),
        )
        .route(
            "/api/prompts/variables",
            get(handlers::prompts::list_variables),
        )
        .route(
            "/api/prompts/preview",
            post(handlers::prompts::preview_prompt),
        )
        .route(
            "/api/prompts/presets",
            get(handlers::prompts::list_presets),
        )
        .route(
            "/api/prompts/presets/{filename}",
            get(handlers::prompts::get_preset),
        )
        // Chat
        .route("/api/chat", post(handlers::chat::chat))
        // Sessions
        .route("/api/sessions", get(handlers::sessions::list_sessions))
        .route("/api/sessions", post(handlers::sessions::create_session))
        .route(
            "/api/sessions/{id}",
            get(handlers::sessions::get_session),
        )
        .route(
            "/api/sessions/{id}/title",
            put(handlers::sessions::rename_session),
        )
        .route(
            "/api/sessions/{id}",
            delete(handlers::sessions::delete_session),
        )
        // Diary
        .route(
            "/api/diary/{agent_id}",
            get(handlers::diary::list_diary),
        )
        .route(
            "/api/diary/{agent_id}",
            post(handlers::diary::add_diary),
        )
        .route(
            "/api/diary/{agent_id}/{entry_id}",
            put(handlers::diary::update_diary),
        )
        .route(
            "/api/diary/{agent_id}/{entry_id}",
            delete(handlers::diary::delete_diary),
        )
        // Audit
        .route("/api/audit", get(handlers::audit::audit_events))
        // Encoder
        .route("/api/encode", post(handlers::encoder::encode_text))
        .route(
            "/api/encoder/status",
            get(handlers::encoder::encoder_status),
        )
        // Semantic search
        .route("/api/search", post(handlers::encoder::semantic_search))
        // Document ingestion
        .route("/api/ingest", post(handlers::ingest::ingest_document))
        .route(
            "/api/ingest/multimodal",
            post(handlers::ingest::ingest_multimodal),
        )
        // Benchmarks
        .route(
            "/api/benchmarks",
            get(handlers::benchmarks::benchmarks),
        )
        // Tags
        .route(
            "/api/tags/{agent_id}",
            get(handlers::tags::list_tags),
        )
        .route("/api/tags/search", post(handlers::tags::search_tags))
        .route(
            "/api/tags/cooccurrences/{tag_id}",
            get(handlers::tags::tag_cooccurrences),
        )
        // EPA
        .route("/api/epa/analyze", post(handlers::epa::epa_analyze))
        // LGSRR Analysis
        .route("/api/analyze", post(handlers::analyze::analyze_query))
        // Models (M5)
        .route("/api/models", get(handlers::models::list_models))
        // Model Traces
        .route("/api/traces", get(handlers::traces::list_traces))
        .route("/api/traces/summary", get(handlers::traces::trace_summary))
        .route("/api/traces", delete(handlers::traces::clear_traces))
        // Consolidation
        .route(
            "/api/consolidation/run/{agent_id}",
            post(handlers::consolidation::run_consolidation),
        )
        // WebSocket
        .route("/ws/events", get(handlers::ws::events_ws))
        // Middleware
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// Alias for [`build_router`] — convenience for the standalone server crate.
pub fn create_router(state: Arc<AppState>) -> Router {
    build_router(state)
}
