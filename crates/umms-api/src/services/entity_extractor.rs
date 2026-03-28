//! LLM-powered entity extraction via ModelPool.
//!
//! Bridges [`ModelPool::generate()`] and the [`GenerativeLlm`] trait defined in
//! `umms-consolidation`. This lives in `umms-api` rather than `umms-consolidation`
//! to avoid adding the `umms-encoder` dependency to the consolidation crate.

use std::sync::Arc;

use async_trait::async_trait;
use tracing::warn;

use umms_consolidation::llm::{ExtractedEntity, ExtractedRelation, GenerativeLlm};
use umms_core::error::Result;
use umms_model::ModelPool;

/// Prompt template for entity extraction.
const ENTITY_EXTRACTION_PROMPT: &str = r#"Extract entities and their relationships from this text.
Return ONLY a valid JSON array (no markdown, no explanation):
[{{"name": "entity", "entity_type": "concept|person|technology|method|tool|dataset|organization", "relationships": [{{"target": "other entity", "relation": "uses|created_by|depends_on|relates_to|part_of", "weight": 0.8}}]}}]

Text:
---
{text}
---"#;

/// Prompt template for multi-entry summarization.
const SUMMARIZE_PROMPT: &str = r"Summarize the following memory entries into a single coherent paragraph.
Preserve key technical details, names, and relationships.
Return ONLY the summary text, no explanation or formatting.

Entries:
---
{entries}
---";

/// Prompt template for entity resolution.
const ENTITY_RESOLUTION_PROMPT: &str = r#"Do these two entity names refer to the same real-world concept?
Entity A: "{a}"
Entity B: "{b}"

Return ONLY a JSON object: {{"same": true/false, "confidence": 0.0-1.0}}
No explanation."#;

/// Implementation of [`GenerativeLlm`] backed by [`ModelPool`].
///
/// Provides entity extraction, summarization, and entity resolution
/// for the consolidation system.
pub struct ModelPoolLlm {
    pool: Arc<ModelPool>,
}

impl ModelPoolLlm {
    /// Create a new LLM bridge from a model pool.
    pub fn new(pool: Arc<ModelPool>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl GenerativeLlm for ModelPoolLlm {
    async fn extract_entities(&self, text: &str) -> Result<Vec<ExtractedEntity>> {
        let prompt = ENTITY_EXTRACTION_PROMPT.replace("{text}", text);

        let response = self.pool.generate(&prompt).await?;
        parse_entities(&response)
    }

    async fn summarize(&self, entries: &[&str]) -> Result<String> {
        let joined = entries.join("\n\n---\n\n");
        let prompt = SUMMARIZE_PROMPT.replace("{entries}", &joined);

        self.pool.generate(&prompt).await
    }

    async fn are_same_entity(&self, a: &str, b: &str) -> Result<f32> {
        let prompt = ENTITY_RESOLUTION_PROMPT.replace("{a}", a).replace("{b}", b);

        let response = self.pool.generate(&prompt).await?;
        parse_entity_resolution(&response)
    }
}

/// Parse an LLM entity extraction response into structured entities.
#[allow(clippy::unnecessary_wraps)]
fn parse_entities(response: &str) -> Result<Vec<ExtractedEntity>> {
    let json_str = response
        .trim()
        .strip_prefix("```json")
        .or_else(|| response.trim().strip_prefix("```"))
        .unwrap_or(response.trim());
    let json_str = json_str.strip_suffix("```").unwrap_or(json_str).trim();

    match serde_json::from_str::<serde_json::Value>(json_str) {
        Ok(serde_json::Value::Array(arr)) => {
            let entities = arr
                .iter()
                .filter_map(|e| {
                    let name = e.get("name")?.as_str()?.to_owned();
                    let entity_type = e
                        .get("entity_type")
                        .and_then(|t| t.as_str())
                        .unwrap_or("concept")
                        .to_owned();

                    let relationships = e
                        .get("relationships")
                        .and_then(|r| r.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|r| {
                                    Some(ExtractedRelation {
                                        target: r.get("target")?.as_str()?.to_owned(),
                                        relation: r
                                            .get("relation")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("relates_to")
                                            .to_owned(),
                                        weight: r
                                            .get("weight")
                                            .and_then(serde_json::Value::as_f64)
                                            .unwrap_or(0.5)
                                            as f32,
                                    })
                                })
                                .collect()
                        })
                        .unwrap_or_default();

                    Some(ExtractedEntity {
                        name,
                        entity_type,
                        relationships,
                    })
                })
                .collect();

            Ok(entities)
        }
        Ok(_) => {
            warn!("LLM entity response was not a JSON array, returning empty");
            Ok(Vec::new())
        }
        Err(e) => {
            warn!("LLM entity JSON parsing failed: {e}, returning empty");
            Ok(Vec::new())
        }
    }
}

/// Parse an entity resolution response into a confidence score.
#[allow(clippy::unnecessary_wraps)]
fn parse_entity_resolution(response: &str) -> Result<f32> {
    let json_str = response
        .trim()
        .strip_prefix("```json")
        .or_else(|| response.trim().strip_prefix("```"))
        .unwrap_or(response.trim());
    let json_str = json_str.strip_suffix("```").unwrap_or(json_str).trim();

    match serde_json::from_str::<serde_json::Value>(json_str) {
        Ok(v) => {
            let confidence = v
                .get("confidence")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0) as f32;
            Ok(confidence)
        }
        Err(e) => {
            warn!("Entity resolution parsing failed: {e}, returning 0.0");
            Ok(0.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_entities_valid() {
        let json = r#"[
            {"name": "Rust", "entity_type": "technology", "relationships": [
                {"target": "Mozilla", "relation": "created_by", "weight": 0.95}
            ]},
            {"name": "tokio", "entity_type": "library", "relationships": []}
        ]"#;

        let entities = parse_entities(json).unwrap();
        assert_eq!(entities.len(), 2);
        assert_eq!(entities[0].name, "Rust");
        assert_eq!(entities[0].relationships.len(), 1);
        assert_eq!(entities[0].relationships[0].target, "Mozilla");
        assert_eq!(entities[1].name, "tokio");
    }

    #[test]
    fn parse_entities_with_fences() {
        let json = "```json\n[{\"name\": \"X\", \"entity_type\": \"concept\", \"relationships\": []}]\n```";
        let entities = parse_entities(json).unwrap();
        assert_eq!(entities.len(), 1);
    }

    #[test]
    fn parse_entities_malformed_returns_empty() {
        let bad = "not json";
        let entities = parse_entities(bad).unwrap();
        assert!(entities.is_empty());
    }

    #[test]
    fn parse_entity_resolution_valid() {
        let json = r#"{"same": true, "confidence": 0.92}"#;
        let score = parse_entity_resolution(json).unwrap();
        assert!((score - 0.92).abs() < 0.01);
    }

    #[test]
    fn parse_entity_resolution_malformed() {
        let bad = "I think they are the same";
        let score = parse_entity_resolution(bad).unwrap();
        assert!((score - 0.0).abs() < f32::EPSILON);
    }
}
