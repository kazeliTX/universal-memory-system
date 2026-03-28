//! In-memory audit log for tracking memory operations.
//!
//! Ring buffer with configurable capacity. Diagnostic-only — no persistence,
//! data is lost on restart. This is intentional: the audit log is a dev/debug
//! tool, not a compliance system.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use umms_core::types::AgentId;

/// Default ring buffer capacity.
const DEFAULT_CAPACITY: usize = 10_000;

// ---------------------------------------------------------------------------
// Event types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventType {
    CachePut,
    CacheGet,
    CacheEvict,
    VectorInsert,
    VectorSearch,
    VectorDelete,
    GraphAddNode,
    GraphAddEdge,
    GraphDeleteNode,
    GraphTraverse,
    Promote,
    Demote,
    AgentSwitch,
    FileStore,
    FileRead,
    Encode,
    Ingest,
}

impl std::fmt::Display for AuditEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CachePut => write!(f, "cache_put"),
            Self::CacheGet => write!(f, "cache_get"),
            Self::CacheEvict => write!(f, "cache_evict"),
            Self::VectorInsert => write!(f, "vector_insert"),
            Self::VectorSearch => write!(f, "vector_search"),
            Self::VectorDelete => write!(f, "vector_delete"),
            Self::GraphAddNode => write!(f, "graph_add_node"),
            Self::GraphAddEdge => write!(f, "graph_add_edge"),
            Self::GraphDeleteNode => write!(f, "graph_delete_node"),
            Self::GraphTraverse => write!(f, "graph_traverse"),
            Self::Promote => write!(f, "promote"),
            Self::Demote => write!(f, "demote"),
            Self::AgentSwitch => write!(f, "agent_switch"),
            Self::FileStore => write!(f, "file_store"),
            Self::FileRead => write!(f, "file_read"),
            Self::Encode => write!(f, "encode"),
            Self::Ingest => write!(f, "ingest"),
        }
    }
}

// ---------------------------------------------------------------------------
// Event
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: u64,
    pub timestamp: DateTime<Utc>,
    pub event_type: AuditEventType,
    pub agent_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
    pub details: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Builder for ergonomic event creation
// ---------------------------------------------------------------------------

pub struct AuditEventBuilder {
    event_type: AuditEventType,
    agent_id: String,
    memory_id: Option<String>,
    node_id: Option<String>,
    layer: Option<String>,
    details: serde_json::Value,
}

impl AuditEventBuilder {
    /// Create a builder for an agent-scoped audit event.
    ///
    /// Use [`new_system`](Self::new_system) for system-level events that don't
    /// belong to a specific agent (e.g. encoder, shared-layer operations).
    pub fn new(event_type: AuditEventType, agent_id: &AgentId) -> Self {
        Self {
            event_type,
            agent_id: agent_id.as_str().to_owned(),
            memory_id: None,
            node_id: None,
            layer: None,
            details: serde_json::Value::Null,
        }
    }

    /// Create a builder for a system-level audit event.
    ///
    /// The `label` identifies the subsystem (e.g. `"_encoder"`, `"_shared"`).
    /// For agent-scoped events, prefer [`new`](Self::new) which requires a
    /// typed `AgentId`.
    pub fn new_system(event_type: AuditEventType, label: &str) -> Self {
        Self {
            event_type,
            agent_id: label.to_owned(),
            memory_id: None,
            node_id: None,
            layer: None,
            details: serde_json::Value::Null,
        }
    }

    #[must_use]
    pub fn memory_id(mut self, id: impl Into<String>) -> Self {
        self.memory_id = Some(id.into());
        self
    }

    #[must_use]
    pub fn node_id(mut self, id: impl Into<String>) -> Self {
        self.node_id = Some(id.into());
        self
    }

    #[must_use]
    pub fn layer(mut self, layer: impl Into<String>) -> Self {
        self.layer = Some(layer.into());
        self
    }

    #[must_use]
    pub fn details(mut self, details: serde_json::Value) -> Self {
        self.details = details;
        self
    }
}

// ---------------------------------------------------------------------------
// Filter
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct AuditFilter {
    pub agent_id: Option<String>,
    pub event_type: Option<AuditEventType>,
    pub limit: usize,
    pub offset: usize,
}

impl AuditFilter {
    #[must_use]
    pub fn new() -> Self {
        Self {
            limit: 50,
            ..Default::default()
        }
    }

    /// Filter by a typed `AgentId`.
    #[must_use]
    pub fn agent(mut self, agent_id: &AgentId) -> Self {
        self.agent_id = Some(agent_id.as_str().to_owned());
        self
    }

    /// Filter by a raw label string (for system-level audit entries).
    #[must_use]
    pub fn agent_label(mut self, label: &str) -> Self {
        self.agent_id = Some(label.to_owned());
        self
    }

    #[must_use]
    pub fn event_type(mut self, t: AuditEventType) -> Self {
        self.event_type = Some(t);
        self
    }

    #[must_use]
    pub fn limit(mut self, n: usize) -> Self {
        self.limit = n;
        self
    }

    #[must_use]
    pub fn offset(mut self, n: usize) -> Self {
        self.offset = n;
        self
    }
}

// ---------------------------------------------------------------------------
// Ring buffer log
// ---------------------------------------------------------------------------

pub struct AuditLog {
    buffer: Mutex<VecDeque<AuditEvent>>,
    capacity: usize,
    next_id: AtomicU64,
}

impl AuditLog {
    /// Create a new audit log with default capacity (10,000 events).
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    /// Create a new audit log with custom capacity.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer: Mutex::new(VecDeque::with_capacity(capacity)),
            capacity,
            next_id: AtomicU64::new(1),
        }
    }

    /// Record an event from a builder.
    pub fn record(&self, builder: AuditEventBuilder) {
        let event = AuditEvent {
            id: self.next_id.fetch_add(1, Ordering::Relaxed),
            timestamp: Utc::now(),
            event_type: builder.event_type,
            agent_id: builder.agent_id,
            memory_id: builder.memory_id,
            node_id: builder.node_id,
            layer: builder.layer,
            details: builder.details,
        };

        let mut buf = self.buffer.lock().expect("audit log lock poisoned");
        if buf.len() >= self.capacity {
            buf.pop_front();
        }
        buf.push_back(event);
    }

    /// Query events matching the filter. Returns newest-first.
    pub fn query(&self, filter: &AuditFilter) -> Vec<AuditEvent> {
        let buf = self.buffer.lock().expect("audit log lock poisoned");

        buf.iter()
            .rev() // newest first
            .filter(|e| {
                if let Some(ref aid) = filter.agent_id {
                    if e.agent_id != *aid {
                        return false;
                    }
                }
                if let Some(ref et) = filter.event_type {
                    if e.event_type != *et {
                        return false;
                    }
                }
                true
            })
            .skip(filter.offset)
            .take(if filter.limit == 0 { usize::MAX } else { filter.limit })
            .cloned()
            .collect()
    }

    /// Total number of events currently in the buffer.
    pub fn len(&self) -> usize {
        self.buffer.lock().expect("audit log lock poisoned").len()
    }

    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for AuditLog {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    fn test_agent(name: &str) -> AgentId {
        AgentId::from_str(name).expect("valid test agent id")
    }

    #[test]
    fn record_and_query() {
        let log = AuditLog::new();
        let agent_a = test_agent("agent-a");
        let agent_b = test_agent("agent-b");

        log.record(
            AuditEventBuilder::new(AuditEventType::VectorInsert, &agent_a)
                .memory_id("mem-1")
                .layer("L2"),
        );
        log.record(
            AuditEventBuilder::new(AuditEventType::CachePut, &agent_b)
                .memory_id("mem-2")
                .layer("L0"),
        );

        assert_eq!(log.len(), 2);

        // Query all
        let all = log.query(&AuditFilter::new().limit(100));
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].agent_id, "agent-b"); // newest first

        // Filter by agent
        let filtered = log.query(&AuditFilter::new().agent(&agent_a));
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].event_type, AuditEventType::VectorInsert);

        // Filter by event type
        let filtered = log.query(&AuditFilter::new().event_type(AuditEventType::CachePut));
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].agent_id, "agent-b");
    }

    #[test]
    fn record_and_query_system_events() {
        let log = AuditLog::new();

        log.record(
            AuditEventBuilder::new_system(AuditEventType::Encode, "_encoder")
                .details(serde_json::json!({"action": "encode_text"})),
        );

        let filtered = log.query(&AuditFilter::new().agent_label("_encoder"));
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].agent_id, "_encoder");
    }

    #[test]
    fn ring_buffer_evicts_oldest() {
        let log = AuditLog::with_capacity(3);

        for i in 0..5 {
            let agent = test_agent(&format!("agent-{i}"));
            log.record(AuditEventBuilder::new(AuditEventType::CachePut, &agent));
        }

        assert_eq!(log.len(), 3);
        let events = log.query(&AuditFilter::new().limit(100));
        // Only agent-2, agent-3, agent-4 should remain
        assert_eq!(events[0].agent_id, "agent-4");
        assert_eq!(events[1].agent_id, "agent-3");
        assert_eq!(events[2].agent_id, "agent-2");
    }

    #[test]
    fn pagination_with_offset() {
        let log = AuditLog::new();
        for i in 0..10 {
            let agent = test_agent(&format!("agent-{i}"));
            log.record(AuditEventBuilder::new(AuditEventType::VectorInsert, &agent));
        }

        let page = log.query(&AuditFilter::new().offset(3).limit(2));
        assert_eq!(page.len(), 2);
        // newest first: 9,8,7,6,5,... → offset 3 = agent-6, agent-5
        assert_eq!(page[0].agent_id, "agent-6");
        assert_eq!(page[1].agent_id, "agent-5");
    }

    #[test]
    fn monotonic_ids() {
        let log = AuditLog::new();
        let a = test_agent("a");
        let b = test_agent("b");
        log.record(AuditEventBuilder::new(AuditEventType::CachePut, &a));
        log.record(AuditEventBuilder::new(AuditEventType::CachePut, &b));

        let events = log.query(&AuditFilter::new().limit(100));
        assert!(events[0].id > events[1].id); // newest has higher id
    }
}
