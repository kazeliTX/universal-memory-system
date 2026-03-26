use std::sync::Arc;

use tauri::State;

use umms_api::response::AuditResponse;
use umms_api::AppState;
use umms_observe::AuditFilter;

#[tauri::command]
pub async fn query_audit_events(
    state: State<'_, Arc<AppState>>,
    agent_id: Option<String>,
    event_type: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<AuditResponse, String> {
    let mut filter = AuditFilter::new()
        .limit(limit.unwrap_or(50))
        .offset(offset.unwrap_or(0));

    if let Some(ref aid) = agent_id {
        filter = filter.agent(aid.as_str());
    }

    // Reuse the same parsing logic as the HTTP handler
    if let Some(ref et) = event_type {
        if let Some(parsed) = parse_event_type(et) {
            filter = filter.event_type(parsed);
        }
    }

    let total = state.audit.len();
    let events = state.audit.query(&filter);

    Ok(AuditResponse { events, total })
}

/// Parse event type string — mirrors `handlers::audit::parse_event_type`.
fn parse_event_type(s: &str) -> Option<umms_observe::AuditEventType> {
    use umms_observe::AuditEventType;
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
