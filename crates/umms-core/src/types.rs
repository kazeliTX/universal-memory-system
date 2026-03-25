//! Core data types shared across all UMMS modules.
//!
//! Design principle: make invalid states unrepresentable.
//! - Newtype wrappers for IDs prevent parameter mixups at compile time.
//! - Builder pattern for MemoryEntry enforces required fields.
//! - Enums encode all valid states; no stringly-typed fields.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Newtype IDs — compile-time safety against parameter mixups
// ---------------------------------------------------------------------------

macro_rules! define_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub struct $name(String);

        impl $name {
            /// Generate a new random ID.
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::new_v4().to_string())
            }

            /// Wrap an existing string as this ID type.
            /// Validates: non-empty, only `[a-zA-Z0-9_-]`, max 128 chars.
            pub fn from_str(s: &str) -> std::result::Result<Self, &'static str> {
                if s.is_empty() {
                    return Err("ID must not be empty");
                }
                if s.len() > 128 {
                    return Err("ID must not exceed 128 characters");
                }
                if !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
                    return Err("ID must only contain [a-zA-Z0-9_-]");
                }
                Ok(Self(s.to_owned()))
            }

            /// Access the inner string. Use sparingly — prefer passing the typed ID.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

define_id!(
    /// Unique identifier for an Agent.
    /// This is the isolation key — every storage operation must carry one.
    AgentId
);

define_id!(
    /// Unique identifier for a memory entry.
    MemoryId
);

define_id!(
    /// Unique identifier for a user session.
    SessionId
);

define_id!(
    /// Unique identifier for a knowledge graph node.
    NodeId
);

define_id!(
    /// Unique identifier for a knowledge graph edge.
    EdgeId
);

// ---------------------------------------------------------------------------
// Enums — all valid states, no stringly-typed fields
// ---------------------------------------------------------------------------

/// Input content modality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Modality {
    Text,
    Image,
    Audio,
    Code,
    File,
}

/// Memory layer classification.
/// Promotion is one-way: L0 → L1 → L2 → L3. Never backwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryLayer {
    /// L0: Sensory buffer (moka, TTL ~30s)
    SensoryBuffer = 0,
    /// L1: Working memory (moka, capacity ~9, TTI ~300s)
    WorkingMemory = 1,
    /// L2: Episodic memory (LanceDB vectors)
    EpisodicMemory = 2,
    /// L3: Semantic memory (SQLite + petgraph knowledge graph)
    SemanticMemory = 3,
    /// L4: Raw storage (local filesystem)
    RawStorage = 4,
}

/// Memory isolation scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IsolationScope {
    /// Only visible to the owning agent.
    Private,
    /// Visible to all agents. Write only through consolidation or promote API.
    Shared,
    /// External knowledge base, access controlled by permissions.
    External,
}

/// Decay rate category for the four-tier forgetting function.
/// Based on Ebbinghaus forgetting curve with empirically derived λ values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecayCategory {
    /// Task context: λ=0.5, half-life ~1.4 days
    TaskContext,
    /// Session topic: λ=0.1, half-life ~7 days
    SessionTopic,
    /// User preference: λ=0.01, half-life ~69 days
    UserPreference,
    /// Domain knowledge: λ=0.001, half-life ~693 days
    DomainKnowledge,
}

impl DecayCategory {
    /// Returns the λ (decay rate) for use in exponential decay: score = importance × e^(-λ × hours)
    #[must_use]
    pub fn lambda(self) -> f64 {
        match self {
            Self::TaskContext => 0.5,
            Self::SessionTopic => 0.1,
            Self::UserPreference => 0.01,
            Self::DomainKnowledge => 0.001,
        }
    }
}

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
}

/// Builder for [`MemoryEntry`]. Required fields are constructor params;
/// optional fields have sensible defaults.
pub struct MemoryEntryBuilder {
    agent_id: AgentId,
    modality: Modality,
    // Optional with defaults
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
    /// Create a builder. `agent_id` and `modality` are always required.
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
    /// Construct a query scoped to a specific agent.
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

/// Where the score came from in the retrieval pipeline.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoreSource {
    Bm25,
    Vector,
    Hybrid,
    Rerank,
    Diffusion,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KgNodeType {
    Entity,
    Concept,
    Relation,
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_id_generates_unique_ids() {
        let id1 = MemoryId::new();
        let id2 = MemoryId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn agent_id_rejects_invalid_input() {
        assert!(AgentId::from_str("").is_err());
        assert!(AgentId::from_str("has spaces").is_err());
        assert!(AgentId::from_str("has/slash").is_err());
        assert!(AgentId::from_str(&"x".repeat(129)).is_err());
    }

    #[test]
    fn agent_id_accepts_valid_input() {
        assert!(AgentId::from_str("coding_assistant").is_ok());
        assert!(AgentId::from_str("agent-01").is_ok());
        assert!(AgentId::from_str("A").is_ok());
    }

    #[test]
    fn builder_enforces_required_fields_and_defaults() {
        let agent = AgentId::from_str("test-agent").unwrap();
        let entry = MemoryEntryBuilder::new(agent.clone(), Modality::Text)
            .content_text("hello world")
            .importance(0.8)
            .build();

        assert_eq!(entry.agent_id, agent);
        assert_eq!(entry.modality, Modality::Text);
        assert_eq!(entry.layer, MemoryLayer::SensoryBuffer); // default
        assert_eq!(entry.scope, IsolationScope::Private);     // default
        assert!(entry.access_count == 0);
    }

    #[test]
    fn importance_is_clamped() {
        let agent = AgentId::from_str("test").unwrap();
        let entry = MemoryEntryBuilder::new(agent, Modality::Text)
            .importance(1.5) // above 1.0
            .build();
        assert!((entry.importance - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn memory_layer_ordering() {
        assert!(MemoryLayer::SensoryBuffer < MemoryLayer::WorkingMemory);
        assert!(MemoryLayer::WorkingMemory < MemoryLayer::EpisodicMemory);
        assert!(MemoryLayer::EpisodicMemory < MemoryLayer::SemanticMemory);
    }

    #[test]
    fn decay_category_lambda_values() {
        assert!((DecayCategory::TaskContext.lambda() - 0.5).abs() < f64::EPSILON);
        assert!((DecayCategory::DomainKnowledge.lambda() - 0.001).abs() < f64::EPSILON);
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
