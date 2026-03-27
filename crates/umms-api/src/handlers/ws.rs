//! WebSocket handler for real-time event streaming.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::response::Response;
use umms_observe::AuditFilter;

use crate::AppState;

/// GET /ws/events — stream real-time audit events over WebSocket.
///
/// Polls the audit log every 2 seconds and sends any new events as JSON
/// text frames. A future iteration will use `tokio::sync::broadcast` for
/// true push semantics.
pub async fn events_ws(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> Response {
    ws.on_upgrade(move |socket| handle_events(socket, state))
}

async fn handle_events(mut socket: WebSocket, state: Arc<AppState>) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
    let mut last_count = state.audit.len();

    loop {
        interval.tick().await;

        let current_count = state.audit.len();
        if current_count > last_count {
            // Fetch only the newest events we haven't sent yet.
            let new_count = current_count - last_count;
            let filter = AuditFilter {
                limit: new_count,
                ..AuditFilter::default()
            };
            let events = state.audit.query(&filter);

            for event in events.iter().rev() {
                let json = serde_json::to_string(event).unwrap_or_default();
                if socket.send(Message::Text(json.into())).await.is_err() {
                    return; // client disconnected
                }
            }
            last_count = current_count;
        }
    }
}
