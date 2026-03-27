//! Generative LLM trait — interface for AI-powered consolidation.
//!
//! This trait defines the LLM capabilities that consolidation needs:
//! entity extraction, summarization, and entity resolution.
//!
//! The trait is **interface only** for now. Concrete implementations will
//! be provided when M5 (model pool) is built. This allows the consolidation
//! scheduler to be designed around LLM capabilities without blocking on
//! the model pool implementation.

use async_trait::async_trait;
use serde::Serialize;

use umms_core::error::Result;

/// An entity extracted from text by the LLM.
#[derive(Debug, Clone, Serialize)]
pub struct ExtractedEntity {
    /// Name of the entity (e.g., "Rust", "tokio", "Alice").
    pub name: String,
    /// Type classification (e.g., "programming_language", "library", "person").
    pub entity_type: String,
    /// Relationships to other entities found in the same text.
    pub relationships: Vec<ExtractedRelation>,
}

/// A relationship between two entities, extracted from text.
#[derive(Debug, Clone, Serialize)]
pub struct ExtractedRelation {
    /// Name of the target entity.
    pub target: String,
    /// Type of relationship (e.g., "uses", "created_by", "depends_on").
    pub relation: String,
    /// Confidence weight (0.0..=1.0).
    pub weight: f32,
}

/// Trait for generative LLM capabilities needed by consolidation.
///
/// This will be implemented when M5 adds the model pool. For now,
/// consolidation operates without LLM assistance (using simpler
/// heuristics for node similarity and no automatic summarization).
///
/// # Future capabilities
///
/// - **Entity extraction**: Parse unstructured text into structured knowledge
///   graph nodes and edges, enabling automatic L2 -> L3 promotion.
/// - **Summarization**: Compress multiple related memories into a single
///   summary entry, reducing storage while preserving key information.
/// - **Entity resolution**: Determine whether two entity references
///   (e.g., "Rust" and "the Rust programming language") refer to the same
///   concept, improving graph evolution merge decisions.
#[async_trait]
pub trait GenerativeLlm: Send + Sync {
    /// Extract entities and relationships from text.
    ///
    /// Given a piece of text (typically a memory entry's `content_text`),
    /// returns structured entities with their types and inter-relationships.
    async fn extract_entities(&self, text: &str) -> Result<Vec<ExtractedEntity>>;

    /// Generate a summary of multiple memory entries.
    ///
    /// Takes a slice of text contents and produces a single coherent summary
    /// that captures the key information from all inputs.
    async fn summarize(&self, entries: &[&str]) -> Result<String>;

    /// Determine if two entity names refer to the same thing.
    ///
    /// Returns a confidence score (0.0 = definitely different, 1.0 = definitely same).
    /// Used by graph evolution to improve merge decisions beyond simple string similarity.
    async fn are_same_entity(&self, a: &str, b: &str) -> Result<f32>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracted_entity_serializes() {
        let entity = ExtractedEntity {
            name: "Rust".to_string(),
            entity_type: "programming_language".to_string(),
            relationships: vec![ExtractedRelation {
                target: "Mozilla".to_string(),
                relation: "created_by".to_string(),
                weight: 0.95,
            }],
        };

        let json = serde_json::to_string(&entity).unwrap();
        assert!(json.contains("Rust"));
        assert!(json.contains("programming_language"));
        assert!(json.contains("Mozilla"));
    }

    #[test]
    fn extracted_relation_fields() {
        let rel = ExtractedRelation {
            target: "tokio".to_string(),
            relation: "depends_on".to_string(),
            weight: 0.8,
        };

        assert_eq!(rel.target, "tokio");
        assert_eq!(rel.relation, "depends_on");
        assert!((rel.weight - 0.8).abs() < f32::EPSILON);
    }
}
