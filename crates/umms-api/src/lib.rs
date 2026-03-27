
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
//! - [`ApiError`](handlers::memory::ApiError) converts to proper HTTP status codes.

pub mod handlers;
pub mod response;
pub mod services;
pub mod state;

use std::sync::Arc;

use axum::routing::{delete, get, post, put};
use axum::Router;
use tower_http::cors::CorsLayer;

pub use state::{AppConfig, AppState};

/// Build the complete Axum router with all API endpoints.
///
/// The returned router is ready to be served with `axum::serve()`.
/// It does not include static file serving — that is the caller's responsibility
/// (Tauri serves the Vue build, tests don't need it).
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
        // Chat
        .route("/api/chat", post(handlers::chat::chat))
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
