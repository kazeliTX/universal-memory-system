//! Document skeleton extraction and context injection.
//!
//! DocSkeleton captures the high-level structure of a document in a single
//! LLM call. Each chunk then gets context injected by pure string operations
//! (0 additional API calls).

use serde::{Deserialize, Serialize};
use tracing::warn;
use umms_core::error::Result;
use umms_model::ModelPool;

/// The structural skeleton of a document, extracted by a single LLM call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocSkeleton {
    /// Document title (extracted or inferred).
    pub title: String,
    /// 2-3 sentence summary of the entire document.
    pub summary: String,
    /// Key entities mentioned in the document.
    pub key_entities: Vec<EntityMention>,
    /// Section-level metadata.
    pub sections: Vec<SectionMeta>,
}

/// An entity mentioned in the document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityMention {
    pub name: String,
    pub entity_type: String,
}

/// Metadata for a document section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionMeta {
    pub title: String,
    /// Which chunk indices belong to this section.
    pub chunk_range: (usize, usize),
    /// One-sentence summary of this section.
    pub summary: String,
}

impl DocSkeleton {
    /// Find which section a chunk belongs to.
    pub fn section_for(&self, chunk_index: usize) -> Option<&SectionMeta> {
        self.sections
            .iter()
            .find(|s| chunk_index >= s.chunk_range.0 && chunk_index <= s.chunk_range.1)
    }

    /// Build a context prefix for a specific chunk.
    ///
    /// This is the core of the "one LLM call per document" strategy:
    /// context is assembled from pre-extracted skeleton data, not generated
    /// per-chunk.
    pub fn contextualize(&self, chunk_index: usize, chunk_text: &str) -> String {
        let section_title = self
            .section_for(chunk_index)
            .map(|s| s.title.as_str())
            .unwrap_or("General");

        format!(
            "[Document: {} | Section: {} | Summary: {}]\n{}",
            self.title, section_title, self.summary, chunk_text,
        )
    }

    /// Build the LLM prompt for skeleton extraction.
    ///
    /// The caller sends this to the LLM and parses the JSON response
    /// into a `DocSkeleton`.
    pub fn extraction_prompt(document_text: &str, num_chunks: usize) -> String {
        format!(
            r#"Analyze the following document and extract its structural skeleton as JSON.

The document has been split into {num_chunks} chunks (0-indexed).

Return ONLY valid JSON with this exact structure:
{{
  "title": "document title",
  "summary": "2-3 sentence summary of the entire document",
  "key_entities": [
    {{"name": "entity name", "entity_type": "person|concept|method|tool|dataset|organization"}}
  ],
  "sections": [
    {{"title": "section title", "chunk_range": [start_chunk, end_chunk], "summary": "one sentence"}}
  ]
}}

Document:
---
{document_text}
---"#
        )
    }

    /// Create a minimal skeleton when LLM is unavailable.
    ///
    /// Uses the first line as title and first 200 chars as summary.
    /// Better than nothing — at least the chunk gets document-level context.
    pub fn fallback(text: &str, num_chunks: usize) -> Self {
        let title = text
            .lines()
            .next()
            .unwrap_or("Untitled")
            .chars()
            .take(100)
            .collect::<String>();

        let summary = text.chars().take(200).collect::<String>();

        Self {
            title,
            summary,
            key_entities: Vec::new(),
            sections: vec![SectionMeta {
                title: "Full Document".to_owned(),
                chunk_range: (0, num_chunks.saturating_sub(1)),
                summary: String::new(),
            }],
        }
    }
}

// ---------------------------------------------------------------------------
// LLM-powered skeleton extraction
// ---------------------------------------------------------------------------

/// Maximum characters of document text to include in the LLM prompt.
/// Keeps token usage reasonable while giving the model enough context.
const SKELETON_PREVIEW_CHARS: usize = 4000;

/// Prompt template for LLM skeleton extraction.
///
/// The `{preview}` and `{num_chunks}` placeholders are filled at call time.
const SKELETON_EXTRACTION_PROMPT: &str = r#"Analyze the following document and extract its structural skeleton as JSON.

The document has been split into {num_chunks} chunks (0-indexed).

Return ONLY valid JSON with this exact structure (no markdown fences, no explanation):
{{
  "title": "document title",
  "summary": "2-3 sentence summary of the entire document",
  "key_entities": [
    {{"name": "entity name", "entity_type": "person|concept|method|tool|dataset|organization"}}
  ],
  "sections": [
    {{"title": "section title", "chunk_range": [start_chunk, end_chunk], "summary": "one sentence"}}
  ]
}}

Document:
---
{preview}
---"#;

/// Extract document skeleton using a generative LLM via the [`ModelPool`].
///
/// Falls back to [`DocSkeleton::fallback`] if the LLM call fails or returns
/// unparseable output.
pub async fn extract_skeleton_llm(
    text: &str,
    num_chunks: usize,
    pool: &ModelPool,
) -> Result<DocSkeleton> {
    // Truncate to save tokens — first N chars is enough for structure extraction.
    let preview = if text.len() > SKELETON_PREVIEW_CHARS {
        &text[..text.floor_char_boundary(SKELETON_PREVIEW_CHARS)]
    } else {
        text
    };

    let prompt = SKELETON_EXTRACTION_PROMPT
        .replace("{preview}", preview)
        .replace("{num_chunks}", &num_chunks.to_string());

    match pool.generate(&prompt).await {
        Ok(response) => parse_skeleton_response(&response, text, num_chunks),
        Err(e) => {
            warn!("LLM skeleton extraction failed: {e}, using fallback");
            Ok(DocSkeleton::fallback(text, num_chunks))
        }
    }
}

/// Parse an LLM JSON response into a [`DocSkeleton`].
///
/// Handles common LLM quirks: markdown code fences, extra whitespace,
/// missing fields. Falls back to heuristic extraction on parse failure.
fn parse_skeleton_response(
    response: &str,
    original_text: &str,
    num_chunks: usize,
) -> Result<DocSkeleton> {
    // Strip markdown code fences that LLMs love to add.
    let json_str = response
        .trim()
        .strip_prefix("```json")
        .or_else(|| response.trim().strip_prefix("```"))
        .unwrap_or(response.trim());
    let json_str = json_str
        .strip_suffix("```")
        .unwrap_or(json_str)
        .trim();

    match serde_json::from_str::<serde_json::Value>(json_str) {
        Ok(v) => {
            let title = v
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or("Untitled")
                .to_owned();

            let summary = v
                .get("summary")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_owned();

            let key_entities = v
                .get("key_entities")
                .and_then(|arr| arr.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|e| {
                            Some(EntityMention {
                                name: e.get("name")?.as_str()?.to_owned(),
                                entity_type: e
                                    .get("entity_type")
                                    .and_then(|t| t.as_str())
                                    .unwrap_or("concept")
                                    .to_owned(),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();

            let sections: Vec<SectionMeta> = v
                .get("sections")
                .and_then(|arr| arr.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|s| {
                            let title = s.get("title")?.as_str()?.to_owned();
                            let summary = s
                                .get("summary")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_owned();
                            let range = s.get("chunk_range").and_then(|r| r.as_array())?;
                            let start = range.first()?.as_u64()? as usize;
                            let end = range.get(1)?.as_u64()? as usize;
                            Some(SectionMeta {
                                title,
                                chunk_range: (start, end),
                                summary,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();

            // If we got no sections at all, add a default one
            let sections = if sections.is_empty() {
                vec![SectionMeta {
                    title: "Full Document".to_owned(),
                    chunk_range: (0, num_chunks.saturating_sub(1)),
                    summary: String::new(),
                }]
            } else {
                sections
            };

            Ok(DocSkeleton {
                title,
                summary,
                key_entities,
                sections,
            })
        }
        Err(e) => {
            warn!("LLM skeleton JSON parsing failed: {e}, using fallback");
            Ok(DocSkeleton::fallback(original_text, num_chunks))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_skeleton() -> DocSkeleton {
        DocSkeleton {
            title: "XYZ Algorithm".to_owned(),
            summary: "Proposes XYZ for image classification, achieves 95% on ImageNet.".to_owned(),
            key_entities: vec![
                EntityMention {
                    name: "XYZ".to_owned(),
                    entity_type: "method".to_owned(),
                },
            ],
            sections: vec![
                SectionMeta {
                    title: "Introduction".to_owned(),
                    chunk_range: (0, 2),
                    summary: "Background and motivation.".to_owned(),
                },
                SectionMeta {
                    title: "Experiments".to_owned(),
                    chunk_range: (3, 5),
                    summary: "Training setup and results.".to_owned(),
                },
            ],
        }
    }

    #[test]
    fn section_for_finds_correct_section() {
        let sk = sample_skeleton();
        assert_eq!(sk.section_for(0).unwrap().title, "Introduction");
        assert_eq!(sk.section_for(2).unwrap().title, "Introduction");
        assert_eq!(sk.section_for(4).unwrap().title, "Experiments");
        assert!(sk.section_for(10).is_none());
    }

    #[test]
    fn contextualize_injects_prefix() {
        let sk = sample_skeleton();
        let result = sk.contextualize(4, "Trained on 4x A100 for 72 hours.");
        assert!(result.starts_with("[Document: XYZ Algorithm | Section: Experiments"));
        assert!(result.contains("Trained on 4x A100"));
    }

    #[test]
    fn fallback_produces_valid_skeleton() {
        let sk = DocSkeleton::fallback("My Document Title\n\nSome content here.", 5);
        assert_eq!(sk.title, "My Document Title");
        assert_eq!(sk.sections.len(), 1);
        assert_eq!(sk.sections[0].chunk_range, (0, 4));
    }

    // --- LLM skeleton parsing tests ---

    #[test]
    fn parse_skeleton_valid_json() {
        let json = r#"{
            "title": "My Paper",
            "summary": "A paper about things.",
            "key_entities": [
                {"name": "Rust", "entity_type": "technology"},
                {"name": "Alice", "entity_type": "person"}
            ],
            "sections": [
                {"title": "Intro", "chunk_range": [0, 2], "summary": "Introduction."},
                {"title": "Methods", "chunk_range": [3, 5], "summary": "Methodology."}
            ]
        }"#;

        let sk = parse_skeleton_response(json, "text", 6).unwrap();
        assert_eq!(sk.title, "My Paper");
        assert_eq!(sk.summary, "A paper about things.");
        assert_eq!(sk.key_entities.len(), 2);
        assert_eq!(sk.key_entities[0].name, "Rust");
        assert_eq!(sk.sections.len(), 2);
        assert_eq!(sk.sections[0].chunk_range, (0, 2));
        assert_eq!(sk.sections[1].title, "Methods");
    }

    #[test]
    fn parse_skeleton_with_markdown_fences() {
        let json = "```json\n{\"title\": \"Fenced\", \"summary\": \"S\", \"key_entities\": [], \"sections\": []}\n```";
        let sk = parse_skeleton_response(json, "text", 1).unwrap();
        assert_eq!(sk.title, "Fenced");
        // Empty sections should get a default
        assert_eq!(sk.sections.len(), 1);
        assert_eq!(sk.sections[0].title, "Full Document");
    }

    #[test]
    fn parse_skeleton_with_plain_fences() {
        let json = "```\n{\"title\": \"Plain\", \"summary\": \"S\", \"key_entities\": [], \"sections\": [{\"title\": \"A\", \"chunk_range\": [0, 0], \"summary\": \"a\"}]}\n```";
        let sk = parse_skeleton_response(json, "text", 1).unwrap();
        assert_eq!(sk.title, "Plain");
    }

    #[test]
    fn parse_skeleton_malformed_json_falls_back() {
        let bad = "this is not json at all";
        let sk = parse_skeleton_response(bad, "Fallback Title\nBody", 3).unwrap();
        // Should use fallback: first line as title
        assert_eq!(sk.title, "Fallback Title");
    }

    #[test]
    fn parse_skeleton_missing_fields_graceful() {
        // Only title present
        let json = r#"{"title": "Partial"}"#;
        let sk = parse_skeleton_response(json, "text", 2).unwrap();
        assert_eq!(sk.title, "Partial");
        assert_eq!(sk.summary, "");
        assert!(sk.key_entities.is_empty());
        // Default section inserted
        assert_eq!(sk.sections.len(), 1);
    }

    #[test]
    fn parse_skeleton_missing_entity_type_defaults() {
        let json = r#"{"title": "T", "summary": "S", "key_entities": [{"name": "X"}], "sections": []}"#;
        let sk = parse_skeleton_response(json, "text", 1).unwrap();
        // Entity with missing entity_type should be skipped (name requires Some)
        // Actually, entity_type defaults to "concept" but name is required via filter_map
        // The entity with name "X" but no entity_type should get "concept"
        assert_eq!(sk.key_entities.len(), 1);
        assert_eq!(sk.key_entities[0].entity_type, "concept");
    }

    #[test]
    fn extraction_prompt_includes_chunks_and_text() {
        let prompt = DocSkeleton::extraction_prompt("Hello world", 5);
        assert!(prompt.contains("5 chunks"));
        assert!(prompt.contains("Hello world"));
    }

    #[test]
    fn skeleton_preview_chars_constant() {
        // Sanity: the constant should be reasonable
        assert!(SKELETON_PREVIEW_CHARS >= 1000);
        assert!(SKELETON_PREVIEW_CHARS <= 10000);
    }

    // Note: `extract_skeleton_llm` integration tests require a live ModelPool
    // and are covered by the integration test suite with API keys.
}
