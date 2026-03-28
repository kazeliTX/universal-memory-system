//! Full-stack integration test for the UMMS HTTP API.
//!
//! Starts the complete Axum server (without a TCP listener) and exercises
//! every major API endpoint via `tower::ServiceExt::oneshot`. Each test
//! creates its own isolated temp directory and `AppState` so tests are
//! fully independent.
//!
//! These tests pass WITHOUT a `GEMINI_API_KEY` — the encoder will be `None`
//! and encoder-dependent endpoints return graceful errors.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use umms_api::{AppConfig, AppState};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a fully initialised Axum router backed by a fresh temp directory.
///
/// Returns `(Router, TempDir)` — the `TempDir` must be kept alive for the
/// duration of the test so the temp directory is not deleted prematurely.
async fn build_test_app() -> (axum::Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("failed to create temp dir");

    let config = AppConfig {
        data_dir: dir.path().to_path_buf(),
        vector_dim: 8, // small dim for fast tests
        audit_capacity: 1_000,
    };

    let state = AppState::new(config).await.expect("AppState::new failed");
    let state = Arc::new(state);

    let router = umms_api::build_router(state);
    (router, dir)
}

/// Extract the response body as a `serde_json::Value`.
async fn body_json(response: axum::http::Response<Body>) -> serde_json::Value {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("failed to read response body");
    serde_json::from_slice(&body).expect("response body is not valid JSON")
}

// ---------------------------------------------------------------------------
// 1. Health endpoint
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_health_endpoint() {
    let (app, _dir) = build_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let json = body_json(response).await;
    assert_eq!(json["status"], "healthy");
    assert!(json["uptime_secs"].is_number());
    assert_eq!(json["storage"]["cache"], "ok");
    assert_eq!(json["storage"]["vector"], "ok");
    assert_eq!(json["storage"]["graph"], "ok");
    assert_eq!(json["storage"]["files"], "ok");
}

// ---------------------------------------------------------------------------
// 2. Stats endpoint
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_stats_endpoint() {
    let (app, _dir) = build_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/stats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let json = body_json(response).await;
    assert!(json["cache"].is_object());
    assert!(json["vector"].is_object());
    assert!(json["graph"].is_object());
    assert!(json["agents"].is_array());
}

// ---------------------------------------------------------------------------
// 3. Seed and clear cycle
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_seed_and_clear() {
    let (app, _dir) = build_test_app().await;

    // Seed (GET /api/demo/seed)
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/demo/seed")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let seed_json = body_json(response).await;
    assert_eq!(seed_json["seeded"], true);
    assert!(
        seed_json["memories"].as_u64().unwrap() > 0,
        "seed should create memories"
    );
    assert!(
        seed_json["nodes"].as_u64().unwrap() > 0,
        "seed should create graph nodes"
    );
    assert!(
        seed_json["edges"].as_u64().unwrap() > 0,
        "seed should create graph edges"
    );

    // Stats should now show data
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/stats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let stats_json = body_json(response).await;
    assert!(
        stats_json["vector"]["total_entries"].as_u64().unwrap() > 0,
        "stats should show vector entries after seed"
    );

    // Clear (GET /api/demo/clear)
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/demo/clear")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let clear_json = body_json(response).await;
    assert_eq!(clear_json["cleared"], true);
    assert!(clear_json["vectors_deleted"].as_u64().unwrap() > 0);

    // Stats should be empty after clear
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/stats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let stats_json = body_json(response).await;
    assert_eq!(
        stats_json["vector"]["total_entries"].as_u64().unwrap(),
        0,
        "stats should show 0 entries after clear"
    );
}

// ---------------------------------------------------------------------------
// 4. Agent persona CRUD
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_agent_persona_crud() {
    let (app, _dir) = build_test_app().await;

    // List agents — should have default personas (coder, researcher, writer)
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/agents")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let list_json = body_json(response).await;
    let agents = list_json["agents"].as_array().unwrap();
    assert!(agents.len() >= 3, "should have at least 3 default personas");

    // Create a new agent
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/agents")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "agent_id": "test-agent",
                        "name": "Test Agent",
                        "role": "tester",
                        "description": "An agent for integration tests"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let create_json = body_json(response).await;
    assert_eq!(create_json["agent_id"], "test-agent");
    assert_eq!(create_json["name"], "Test Agent");
    assert_eq!(create_json["role"], "tester");

    // Get the agent detail
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/agents/test-agent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let detail_json = body_json(response).await;
    assert_eq!(detail_json["agent_id"], "test-agent");
    assert_eq!(detail_json["role"], "tester");

    // Update the agent role
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/agents/test-agent")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "role": "senior-tester"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let update_json = body_json(response).await;
    assert_eq!(update_json["role"], "senior-tester");

    // Delete the agent
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/agents/test-agent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let delete_json = body_json(response).await;
    assert_eq!(delete_json["deleted"], true);
    assert_eq!(delete_json["agent_id"], "test-agent");

    // Verify the agent is gone — detail should still return 200 with empty fields
    // (the handler returns defaults for unknown agents, not 404)
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/agents/test-agent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let gone_json = body_json(response).await;
    assert_eq!(gone_json["agent_id"], "test-agent");
    // Name falls back to agent_id when persona is missing
    assert_eq!(gone_json["name"], "test-agent");
}

// ---------------------------------------------------------------------------
// 5. Memory browsing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_memory_browsing() {
    let (app, _dir) = build_test_app().await;

    // Seed data first
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/demo/seed")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Cache entries for coder (cache is empty since seed writes to vector, not cache)
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/memories/cache/coder")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let cache_json = body_json(response).await;
    assert_eq!(cache_json["agent_id"], "coder");
    assert!(cache_json["l0"].is_array());
    assert!(cache_json["l1"].is_array());

    // Vector entries for coder
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/memories/vector/coder")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let vector_json = body_json(response).await;
    assert_eq!(vector_json["agent_id"], "coder");
    assert!(
        vector_json["total"].as_u64().unwrap() > 0,
        "coder should have vector entries after seed"
    );
    assert!(vector_json["entries"].is_array());
    assert!(
        !vector_json["entries"].as_array().unwrap().is_empty(),
        "coder should have vector entries listed"
    );

    // Vector entries for researcher
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/memories/vector/researcher")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let vector_json = body_json(response).await;
    assert_eq!(vector_json["agent_id"], "researcher");
    assert!(
        vector_json["total"].as_u64().unwrap() > 0,
        "researcher should have vector entries after seed"
    );
}

// ---------------------------------------------------------------------------
// 6. Graph endpoints
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_graph_endpoints() {
    let (app, _dir) = build_test_app().await;

    // Seed creates shared graph nodes
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/demo/seed")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Graph nodes for coder (includes shared nodes)
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/memories/graph/coder")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let graph_json = body_json(response).await;
    assert_eq!(graph_json["agent_id"], "coder");
    assert!(graph_json["nodes"].is_array());
    assert!(
        graph_json["total"].as_u64().unwrap() > 0,
        "should have graph nodes after seed (shared nodes visible to all agents)"
    );

    // Graph search
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/memories/graph/search?q=Rust&limit=10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let search_json = body_json(response).await;
    assert_eq!(search_json["query"], "Rust");
    assert!(search_json["nodes"].is_array());
    assert!(
        !search_json["nodes"].as_array().unwrap().is_empty(),
        "graph search for 'Rust' should find the seeded node"
    );
}

// ---------------------------------------------------------------------------
// 7. Audit trail
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_audit_trail() {
    let (app, _dir) = build_test_app().await;

    // Seed generates audit events
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/demo/seed")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Query audit events
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/audit")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let audit_json = body_json(response).await;
    assert!(audit_json["events"].is_array());
    assert!(
        audit_json["total"].as_u64().unwrap() > 0,
        "should have audit events after seed"
    );
    assert!(
        !audit_json["events"].as_array().unwrap().is_empty(),
        "audit events array should not be empty"
    );
}

// ---------------------------------------------------------------------------
// 8. Encoder status
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_encoder_status() {
    let (app, _dir) = build_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/encoder/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let json = body_json(response).await;
    // Without GEMINI_API_KEY, encoder should be unavailable
    assert!(json["available"].is_boolean());
    // Stats fields should be present regardless
    assert!(json["total_requests"].is_number());
    assert!(json["total_texts_encoded"].is_number());
    assert!(json["total_errors"].is_number());
    assert!(json["avg_latency_ms"].is_number());
}

// ---------------------------------------------------------------------------
// 9. Models endpoint
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_models_endpoint() {
    let (app, _dir) = build_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/models")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let json = body_json(response).await;
    assert!(json["pool_available"].is_boolean());
    assert!(json["models"].is_array());
}

// ---------------------------------------------------------------------------
// 10. Consolidation run
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_consolidation_run() {
    let (app, _dir) = build_test_app().await;

    // Seed first so there is data to consolidate
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/demo/seed")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Trigger consolidation for coder
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/consolidation/run/coder")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let json = body_json(response).await;
    assert_eq!(json["agent_id"], "coder");
    assert!(json["decay"].is_object());
    assert!(json["evolution"].is_object());
    assert!(json["promotion"].is_object());
    assert!(json["total_ms"].is_number());
    assert!(json["timestamp"].is_string());
}
