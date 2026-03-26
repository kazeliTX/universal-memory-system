//! Audit trail handlers.

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::response::Json;
use serde::Deserialize;

use umms_observe::{AuditEventType, AuditFilter};

use crate::response::AuditResponse;
use crate::state::AppState;

/// `GET /api/audit?agent_id=...&event_type=...&limit=50&offset=0`
pub async fn audit_events(
    State(state): State<Arc<AppState>>,
    Query(params): Query<AuditQueryParams>,
) -> Json<AuditResponse> {
    let mut filter = AuditFilter::new()
        .limit(params.limit.unwrap_or(50))
        .offset(params.offset.unwrap_or(0));

    if let Some(ref aid) = params.agent_id {
        filter = filter.agent(aid.as_str());
    }
    if let Some(ref et) = params.event_type {
        if let Some(parsed) = parse_event_type(et) {
            filter = filter.event_type(parsed);
        }
    }

    let total = state.audit.len();
    let events = state.audit.query(&filter);

    Json(AuditResponse { events, total })
}

#[derive(Debug, Deserialize)]
pub struct AuditQueryParams {
    pub agent_id: Option<String>,
    pub event_type: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Parse event type from query string. Returns None for unknown types
/// (lenient parsing — don't fail the request over a typo).
fn parse_event_type(s: &str) -> Option<AuditEventType> {
    match s {
        "cache_put" => Some(AuditEventType::CachePut),
        "cache_get" => Some(AuditEventType::CacheGet),
        "cache_evict" => Some(AuditEventType::CacheEvict),
        "vector_insert" => Some(AuditEventType::VectorInsert),
        "vector_search" => Some(AuditEventType::VectorSearch),
        "vector_delete" => Some(AuditEventType::VectorDelete),
        "graph_add_node" => Some(AuditEventType::GraphAddNode),
        "graph_add_edge" => Some(AuditEventType::GraphAddEdge),
        "graph_delete_node" => Some(AuditEventType::GraphDeleteNode),
        "graph_traverse" => Some(AuditEventType::GraphTraverse),
        "promote" => Some(AuditEventType::Promote),
        "demote" => Some(AuditEventType::Demote),
        "agent_switch" => Some(AuditEventType::AgentSwitch),
        "file_store" => Some(AuditEventType::FileStore),
        "file_read" => Some(AuditEventType::FileRead),
        _ => None,
    }
}
