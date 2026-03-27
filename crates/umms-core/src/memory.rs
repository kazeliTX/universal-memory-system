//! Memory entry types: the fundamental unit of storage, plus query/result types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::enums::*;
use crate::ids::*;

// ---------------------------------------------------------------------------
// MemoryEntry — the fundamental unit of storage
// ---------------------------------------------------------------------------

/// A single memory entry. Constructed via [`MemoryEntryBuilder`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: MemoryId,
    pub agent_id: AgentId,
    pub layer: MemoryLayer,
    pub scope: IsolationScope,
    pub modality: Modality,
    pub content_text: Option<String>,
    pub vector: Option<Vec<f32>>,
    pub importance: f32,
    pub decay_category: DecayCategory,
    pub tags: Vec<String>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub accessed_at: DateTime<Utc>,
    pub access_count: u64,
    /// User feedback rating for importance scoring (-1.0 = not useful, 1.0 = very useful).
    #[serde(default)]
    pub user_rating: Option<f32>,
}

/// Builder for [`MemoryEntry`]. Required fields are constructor params;
/// optional fields have sensible defaults.
pub struct MemoryEntryBuilder {
    agent_id: AgentId,
    modality: Modality,
    id: Option<MemoryId>,
    layer: MemoryLayer,
    scope: IsolationScope,
    content_text: Option<String>,
    vector: Option<Vec<f32>>,
    importance: f32,
    decay_category: DecayCategory,
    tags: Vec<String>,
    metadata: serde_json::Value,
}

impl MemoryEntryBuilder {
    /// Create a builder. `agent_id` and `modality` are always required —
    /// you cannot create a memory entry without knowing who owns it and what it is.
    #[must_use]
    pub fn new(agent_id: AgentId, modality: Modality) -> Self {
        Self {
            agent_id,
            modality,
            id: None,
            layer: MemoryLayer::SensoryBuffer,
            scope: IsolationScope::Private,
            content_text: None,
            vector: None,
            importance: 0.5,
            decay_category: DecayCategory::SessionTopic,
            tags: Vec::new(),
            metadata: serde_json::Value::Null,
        }
    }

    #[must_use]
    pub fn id(mut self, id: MemoryId) -> Self {
        self.id = Some(id);
        self
    }

    #[must_use]
    pub fn layer(mut self, layer: MemoryLayer) -> Self {
        self.layer = layer;
        self
    }

    #[must_use]
    pub fn scope(mut self, scope: IsolationScope) -> Self {
        self.scope = scope;
        self
    }

    #[must_use]
    pub fn content_text(mut self, text: impl Into<String>) -> Self {
        self.content_text = Some(text.into());
        self
    }

    #[must_use]
    pub fn vector(mut self, v: Vec<f32>) -> Self {
        self.vector = Some(v);
        self
    }

    #[must_use]
    pub fn importance(mut self, val: f32) -> Self {
        self.importance = val.clamp(0.0, 1.0);
        self
    }

    #[must_use]
    pub fn decay_category(mut self, cat: DecayCategory) -> Self {
        self.decay_category = cat;
        self
    }

    #[must_use]
    pub fn tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    #[must_use]
    pub fn metadata(mut self, meta: serde_json::Value) -> Self {
        self.metadata = meta;
        self
    }

    /// Consume the builder and produce a [`MemoryEntry`].
    #[must_use]
    pub fn build(self) -> MemoryEntry {
        let now = Utc::now();
        MemoryEntry {
            id: self.id.unwrap_or_default(),
            agent_id: self.agent_id,
            layer: self.layer,
            scope: self.scope,
            modality: self.modality,
            content_text: self.content_text,
            vector: self.vector,
            importance: self.importance,
            decay_category: self.decay_category,
            tags: self.tags,
            metadata: self.metadata,
            created_at: now,
            accessed_at: now,
            access_count: 0,
            user_rating: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Query types
// ---------------------------------------------------------------------------

/// Query parameters for memory retrieval.
/// `agent_id` is a required constructor param — you cannot query without it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryQuery {
    pub agent_id: AgentId,
    pub include_shared: bool,
    pub query_text: Option<String>,
    pub query_vector: Option<Vec<f32>>,
    pub top_k: usize,
    pub filters: Option<MetadataFilter>,
}

impl MemoryQuery {
    #[must_use]
    pub fn new(agent_id: AgentId, top_k: usize) -> Self {
        Self {
            agent_id,
            include_shared: true,
            query_text: None,
            query_vector: None,
            top_k,
            filters: None,
        }
    }

    #[must_use]
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.query_text = Some(text.into());
        self
    }

    #[must_use]
    pub fn with_vector(mut self, v: Vec<f32>) -> Self {
        self.query_vector = Some(v);
        self
    }

    #[must_use]
    pub fn exclude_shared(mut self) -> Self {
        self.include_shared = false;
        self
    }

    #[must_use]
    pub fn with_filters(mut self, f: MetadataFilter) -> Self {
        self.filters = Some(f);
        self
    }
}

/// Metadata-based filter for memory queries.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetadataFilter {
    pub modality: Option<Modality>,
    pub layer: Option<MemoryLayer>,
    pub min_importance: Option<f32>,
    pub tags: Option<Vec<String>>,
    pub created_after: Option<DateTime<Utc>>,
    pub created_before: Option<DateTime<Utc>>,
}

/// Scored memory entry returned from retrieval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredMemory {
    pub entry: MemoryEntry,
    pub score: f32,
    pub source: ScoreSource,
}

// ---------------------------------------------------------------------------
// Knowledge graph types
// ---------------------------------------------------------------------------

/// A node in the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KgNode {
    pub id: NodeId,
    /// `None` means this is a shared node visible to all agents.
    pub agent_id: Option<AgentId>,
    pub node_type: KgNodeType,
    pub label: String,
    pub properties: serde_json::Value,
    pub importance: f32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// An edge in the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KgEdge {
    pub id: EdgeId,
    pub source_id: NodeId,
    pub target_id: NodeId,
    pub relation: String,
    pub weight: f32,
    /// `None` means this is a shared edge.
    pub agent_id: Option<AgentId>,
    pub created_at: DateTime<Utc>,
}

/// Summary statistics for a knowledge graph scope.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GraphStats {
    pub node_count: u64,
    pub edge_count: u64,
    pub shared_node_count: u64,
    pub shared_edge_count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn builder_enforces_required_fields_and_defaults() {
        let agent = AgentId::from_str("test-agent").unwrap();
        let entry = MemoryEntryBuilder::new(agent.clone(), Modality::Text)
            .content_text("hello world")
            .importance(0.8)
            .build();

        assert_eq!(entry.agent_id, agent);
        assert_eq!(entry.modality, Modality::Text);
        assert_eq!(entry.layer, MemoryLayer::SensoryBuffer);
        assert_eq!(entry.scope, IsolationScope::Private);
        assert!(entry.access_count == 0);
    }

    #[test]
    fn importance_is_clamped() {
        let agent = AgentId::from_str("test").unwrap();
        let entry = MemoryEntryBuilder::new(agent, Modality::Text)
            .importance(1.5)
            .build();
        assert!((entry.importance - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn query_builder_scopes_to_agent() {
        let agent = AgentId::from_str("my-agent").unwrap();
        let q = MemoryQuery::new(agent.clone(), 10)
            .with_text("search term")
            .exclude_shared();

        assert_eq!(q.agent_id, agent);
        assert!(!q.include_shared);
        assert_eq!(q.top_k, 10);
    }

    #[test]
    fn serialization_roundtrip() {
        let agent = AgentId::from_str("test-agent").unwrap();
        let entry = MemoryEntryBuilder::new(agent, Modality::Text)
            .content_text("hello")
            .build();

        let json = serde_json::to_string(&entry).unwrap();
        let de: MemoryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(de.agent_id.as_str(), "test-agent");
        assert_eq!(de.modality, Modality::Text);
    }
}
