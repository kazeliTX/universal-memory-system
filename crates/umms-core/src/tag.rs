//! Semantic tag types — first-class entities with embeddings and co-occurrence.
//!
//! Tags bridge the gap between flat string labels and structured knowledge:
//! - Each tag has a 3072-dim embedding (same space as memories)
//! - Co-occurrence statistics enable EPA projection analysis
//! - Tags serve as "gravity sources" for query vector reshaping

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::types::{AgentId, TagId};

// ---------------------------------------------------------------------------
// Core tag entity
// ---------------------------------------------------------------------------

/// A semantic tag with its own embedding vector.
///
/// Tags are not just labels — they are semantic anchors in the embedding space.
/// EPA uses them to analyze query structure, and query reshaping uses them
/// to warp the search vector toward relevant semantic regions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: TagId,
    /// Human-readable label (e.g., "Rust ownership", "neural network").
    pub label: String,
    /// Normalized form for deduplication: lowercase, trimmed, collapsed whitespace.
    pub canonical: String,
    /// `None` = shared tag visible to all agents.
    pub agent_id: Option<AgentId>,
    /// Embedding vector (3072 dims, same space as memory vectors).
    pub vector: Vec<f32>,
    /// How many memory entries carry this tag.
    pub frequency: u64,
    /// Computed importance (frequency-weighted, decayed over time).
    pub importance: f32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Tag {
    /// Create a canonical form from a label.
    pub fn canonicalize(label: &str) -> String {
        label
            .trim()
            .to_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }
}

// ---------------------------------------------------------------------------
// Co-occurrence
// ---------------------------------------------------------------------------

/// Records how often two tags appear on the same memory entry.
///
/// Used by EPA to build the co-occurrence network for LIF spike propagation
/// and by query reshaping for Level 2 pyramid expansion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagCooccurrence {
    pub tag_a: TagId,
    pub tag_b: TagId,
    /// Raw co-occurrence count.
    pub count: u64,
    /// Pointwise Mutual Information: log2(P(a,b) / (P(a) * P(b))).
    /// High PMI = tags appear together more than expected by chance.
    pub pmi: f32,
}

// ---------------------------------------------------------------------------
// Search result
// ---------------------------------------------------------------------------

/// A tag matched by vector similarity search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagMatch {
    pub tag: Tag,
    /// Cosine similarity between query vector and tag embedding.
    pub similarity: f32,
}

// ---------------------------------------------------------------------------
// EPA output types
// ---------------------------------------------------------------------------

/// Result of Embedding Projection Analysis.
///
/// EPA analyzes a query vector's position in the tag embedding space,
/// producing metrics that drive dynamic parameter adjustment and
/// query vector reshaping.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpaResult {
    /// Focus level (0.0 = diffuse across many topics, 1.0 = tightly focused).
    /// Computed from weight concentration in the dominant K-Means cluster.
    pub logic_depth: f32,

    /// Cross-domain resonance (0.0 = single topic, 1.0 = spans all clusters).
    /// High resonance = query touches multiple distinct semantic regions.
    pub cross_domain_resonance: f32,

    /// Principal semantic axes extracted from activated tag embeddings.
    pub semantic_axes: Vec<SemanticAxis>,

    /// Tags activated by this query, with their similarity weights.
    pub activated_tags: Vec<ActivatedTag>,

    /// Dynamically computed blending factor for query reshaping.
    /// Higher alpha = more tag influence on the search vector.
    pub alpha: f32,
}

impl EpaResult {
    /// Default passthrough result when EPA is disabled or has no tags.
    pub fn passthrough() -> Self {
        Self {
            logic_depth: 0.0,
            cross_domain_resonance: 0.0,
            semantic_axes: Vec::new(),
            activated_tags: Vec::new(),
            alpha: 0.0,
        }
    }
}

/// An activated tag with its similarity weight.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivatedTag {
    pub tag_id: TagId,
    pub label: String,
    pub similarity: f32,
}

/// A principal semantic axis from PCA decomposition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticAxis {
    /// Unit vector in embedding space (3072 dims).
    pub direction: Vec<f32>,
    /// Fraction of total variance explained by this axis.
    pub explained_variance: f32,
}

// ---------------------------------------------------------------------------
// Dynamic parameters
// ---------------------------------------------------------------------------

/// Retriever parameters dynamically computed by EPA for each query.
///
/// These override the static `RetrieverConfig` values when EPA is active.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynamicRetrieverParams {
    pub bm25_weight: f32,
    pub top_k_recall: usize,
    pub top_k_rerank: usize,
    pub min_score: f32,
    pub lif_hops: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalize_normalizes() {
        assert_eq!(Tag::canonicalize("  Rust  Ownership  "), "rust ownership");
        assert_eq!(Tag::canonicalize("Neural Network"), "neural network");
        assert_eq!(Tag::canonicalize("AI"), "ai");
    }

    #[test]
    fn epa_passthrough_has_zero_alpha() {
        let epa = EpaResult::passthrough();
        assert_eq!(epa.alpha, 0.0);
        assert!(epa.activated_tags.is_empty());
    }
}
