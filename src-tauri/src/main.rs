//! UMMS Desktop Application — Tauri v2 entry point.
//!
//! This binary:
//! 1. Initialises all storage backends via `AppState`
//! 2. Spawns an embedded Axum HTTP server for external Agent access
//! 3. Launches the Tauri GUI window with Vue frontend
//!
//! Both the GUI (via Tauri Commands / IPC) and external agents (via HTTP)
//! share the same `Arc<AppState>`, ensuring zero data synchronisation overhead.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

use std::sync::Arc;

use umms_api::{AppConfig, AppState};

#[tokio::main]
async fn main() {
    // Load .env file (look in current dir, then project root)
    if let Err(e) = dotenvy::dotenv() {
        eprintln!("Note: .env not loaded ({e}), using system env vars only");
    }

    // Tracing
    umms_observe::init_tracing("info", false);

    // Initialise shared state
    let config = AppConfig::dev();
    tracing::info!(data_dir = ?config.data_dir, "starting UMMS desktop");

    let state = Arc::new(
        AppState::new(config)
            .await
            .expect("failed to initialise storage backends"),
    );

    // Spawn embedded Axum HTTP server (for external Agent access)
    let http_state = state.clone();
    tokio::spawn(async move {
        let router = umms_api::build_router(http_state);
        let addr = "127.0.0.1:8720";
        tracing::info!(%addr, "embedded HTTP server starting");

        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .expect("failed to bind HTTP port 8720");
        axum::serve(listener, router.into_make_service())
            .await
            .expect("HTTP server error");
    });

    // Launch Tauri GUI
    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            // System
            commands::system::get_health,
            commands::system::get_stats,
            commands::system::get_metrics,
            // Memory
            commands::memory::get_cache_entries,
            commands::memory::list_vector_entries,
            commands::memory::get_memory_detail,
            // Graph
            commands::graph::list_graph_nodes,
            commands::graph::get_node_detail,
            commands::graph::traverse_graph,
            commands::graph::search_graph,
            // Agent
            commands::agent::get_agent_detail,
            // Audit
            commands::audit::query_audit_events,
            // Encoder
            commands::encoder::encoder_status,
            commands::encoder::encode_text,
            commands::encoder::semantic_search,
            // Files
            commands::files::list_files,
            // Ingest
            commands::ingest::ingest_document,
            // Tags & EPA
            commands::tags::list_tags,
            commands::tags::search_tags,
            commands::tags::tag_cooccurrences,
            commands::tags::epa_analyze,
        ])
        .run(tauri::generate_context!())
        .expect("error running UMMS desktop");
}
