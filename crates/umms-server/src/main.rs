//! UMMS Core Service — standalone HTTP server.
//!
//! Run: `cargo run -p umms-server`
//! Or:  `cargo run -p umms-server` with `UMMS_HOST` / `UMMS_PORT` env overrides.

use std::net::SocketAddr;

use umms_api::{AppConfig, AppState, build_router};
use umms_core::config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load .env file (walk up directories to find it)
    let _ = dotenvy::dotenv();

    // Load configuration (umms.toml + env vars)
    let umms_config = config::load_config();

    // Initialize tracing early
    umms_observe::init_tracing(
        &umms_config.observe.log_level,
        umms_config.observe.log_format == "json",
    );

    tracing::info!("UMMS Core Service starting...");

    // Build application state
    let app_config = AppConfig::dev();
    tracing::info!(data_dir = ?app_config.data_dir, "initialising storage backends");

    let state = AppState::shared(app_config).await
        .map_err(|e| format!("Failed to initialise: {e}"))?;

    // Log available model information
    if let Some(ref pool) = state.model_pool {
        for model in pool.models() {
            tracing::info!(
                id = %model.id,
                provider = %model.provider,
                model_name = %model.model_name,
                "model available"
            );
        }
    } else {
        tracing::warn!("No models available — generation and chat APIs will be disabled");
    }

    if state.encoder.is_some() {
        tracing::info!("Encoder ready — embedding API enabled");
    } else {
        tracing::warn!("Encoder not available — set GEMINI_API_KEY to enable embedding");
    }

    // Build router
    let app = build_router(state);

    // Parse host/port: env vars override config
    let host = std::env::var("UMMS_HOST")
        .unwrap_or_else(|_| umms_config.http.host.clone());
    let port: u16 = std::env::var("UMMS_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(umms_config.http.port);

    let addr: SocketAddr = format!("{host}:{port}").parse()?;

    tracing::info!(%addr, "UMMS Core Service listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app.into_make_service()).await?;

    Ok(())
}
