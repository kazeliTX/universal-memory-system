//! Document skeleton extraction and context injection.
//!
//! DocSkeleton captures the high-level structure of a document in a single
//! LLM call. Each chunk then gets context injected by pure string operations
//! (0 additional API calls).

use serde::{Deserialize, Serialize};

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
}
