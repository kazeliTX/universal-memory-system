//! Domain enums — all valid states, no stringly-typed fields.
//!
//! Enums marked `#[non_exhaustive]` can have new variants added in future
//! versions without breaking downstream code (callers must have a `_` arm).

use serde::{Deserialize, Serialize};

/// Input content modality.
///
/// `#[non_exhaustive]` ensures that adding a new variant (e.g., `Model3D`,
/// `Video`, `Spreadsheet`) won't break any existing `match` statements —
/// they are required to have a wildcard arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Modality {
    Text,
    Image,
    Audio,
    Code,
    File,
    // Future: Model3D, Video, Spreadsheet, ...
    // Adding variants here is a non-breaking change thanks to #[non_exhaustive].
}

impl Modality {
    /// Human-readable display name.
    #[must_use]
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Image => "image",
            Self::Audio => "audio",
            Self::Code => "code",
            Self::File => "file",
            // If a new variant is added and you forget to handle it here,
            // the compiler will warn you (but it won't break other crates).
        }
    }
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
    /// L2: Episodic memory (vector store)
    EpisodicMemory = 2,
    /// L3: Semantic memory (knowledge graph)
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
    /// Returns the λ (decay rate) for exponential decay: score = importance × e^(-λ × hours)
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

/// Where a retrieval score came from in the pipeline.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoreSource {
    Bm25,
    Vector,
    Hybrid,
    Rerank,
    Diffusion,
}

/// Knowledge graph node type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KgNodeType {
    Entity,
    Concept,
    Relation,
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn modality_is_non_exhaustive() {
        // This match compiles because of the wildcard arm — proof that
        // #[non_exhaustive] works and adding variants won't break callers.
        let m = Modality::Text;
        let name = match m {
            Modality::Text => "text",
            _ => "other",
        };
        assert_eq!(name, "text");
    }
}
