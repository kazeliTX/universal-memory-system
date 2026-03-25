//! Core data types shared across all UMMS modules.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a memory entry.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MemoryId(pub String);

impl MemoryId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

impl Default for MemoryId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for MemoryId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Input content modality.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Modality {
    Text,
    Image,
    Audio,
    Code,
    File,
}

/// Memory layer classification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryLayer {
    /// L0: Sensory buffer (moka, TTL ~30s)
    SensoryBuffer,
    /// L1: Working memory (moka, capacity ~9, TTI ~300s)
    WorkingMemory,
    /// L2: Episodic memory (LanceDB vectors)
    EpisodicMemory,
    /// L3: Semantic memory (SQLite + petgraph knowledge graph)
    SemanticMemory,
    /// L4: Raw storage (local filesystem)
    RawStorage,
}

/// Memory isolation scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IsolationScope {
    /// Only visible to the owning agent.
    Private,
    /// Visible to all agents.
    Shared,
    /// External knowledge base, access controlled by permissions.
    External,
}

/// Decay rate category for the four-tier forgetting function.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// A single memory entry — the fundamental unit of storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: MemoryId,
    pub agent_id: String,
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

/// Query parameters for memory retrieval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryQuery {
    pub agent_id: String,
    pub include_shared: bool,
    pub query_text: Option<String>,
    pub query_vector: Option<Vec<f32>>,
    pub top_k: usize,
    pub filters: Option<MetadataFilter>,
}

/// Metadata-based filter for memory queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoreSource {
    Bm25,
    Vector,
    Hybrid,
    Rerank,
    Diffusion,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_id_generates_unique_ids() {
        let id1 = MemoryId::new();
        let id2 = MemoryId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_memory_entry_serialization_roundtrip() {
        let entry = MemoryEntry {
            id: MemoryId::new(),
            agent_id: "test_agent".to_string(),
            layer: MemoryLayer::EpisodicMemory,
            scope: IsolationScope::Private,
            modality: Modality::Text,
            content_text: Some("hello world".to_string()),
            vector: None,
            importance: 0.8,
            decay_category: DecayCategory::SessionTopic,
            tags: vec!["test".to_string()],
            metadata: serde_json::json!({}),
            created_at: Utc::now(),
            accessed_at: Utc::now(),
            access_count: 0,
        };

        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: MemoryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.agent_id, "test_agent");
        assert_eq!(deserialized.modality, Modality::Text);
    }
}
