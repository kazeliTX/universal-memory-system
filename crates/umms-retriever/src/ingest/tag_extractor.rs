//! Automatic tag extraction during document ingestion.
//!
//! Extracts candidate tag labels from document skeleton metadata and chunk
//! content, deduplicates by canonical form, encodes new tags via the Encoder,
//! and upserts them into the TagStore. Records co-occurrences for all tags
//! sharing the same chunk.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::Utc;
use tracing::{debug, instrument};

use umms_core::error::Result;
use umms_core::tag::Tag;
use umms_core::traits::{Encoder, TagStore};
use umms_core::types::{AgentId, TagId};

use super::chunker::Chunk;
use super::skeleton::DocSkeleton;

/// English stopwords to filter from title words.
const STOPWORDS: &[&str] = &[
    "a", "an", "the", "and", "or", "but", "in", "on", "at", "to", "for",
    "of", "with", "by", "from", "as", "is", "was", "are", "were", "be",
    "been", "being", "have", "has", "had", "do", "does", "did", "will",
    "would", "could", "should", "may", "might", "shall", "can", "need",
    "it", "its", "this", "that", "these", "those", "he", "she", "they",
    "we", "you", "me", "him", "her", "us", "them", "my", "our", "your",
    "his", "their", "not", "no", "nor", "so", "if", "then", "than",
    "too", "very", "just", "about", "up", "out", "all", "also", "into",
];

/// Extracts semantic tags from document content during ingestion.
pub struct TagExtractor {
    tag_store: Arc<dyn TagStore>,
    encoder: Arc<dyn Encoder>,
}

impl TagExtractor {
    pub fn new(tag_store: Arc<dyn TagStore>, encoder: Arc<dyn Encoder>) -> Self {
        Self { tag_store, encoder }
    }

    /// Extract tags from a document's skeleton and chunks.
    ///
    /// Returns `Vec<Vec<TagId>>` — one inner vec per chunk, containing the
    /// tag IDs assigned to that chunk.
    #[instrument(skip(self, skeleton, chunks), fields(agent = %agent_id, num_chunks = chunks.len()))]
    pub async fn extract(
        &self,
        skeleton: &DocSkeleton,
        chunks: &[Chunk],
        agent_id: &AgentId,
    ) -> Result<Vec<Vec<TagId>>> {
        // Step 1: Extract candidate labels per chunk
        let per_chunk_labels = self.extract_candidates(skeleton, chunks);

        // Step 2: Collect all unique canonical forms
        let mut all_canonicals: HashMap<String, String> = HashMap::new(); // canonical -> best label
        for labels in &per_chunk_labels {
            for label in labels {
                let canonical = Tag::canonicalize(label);
                all_canonicals
                    .entry(canonical)
                    .or_insert_with(|| label.clone());
            }
        }

        if all_canonicals.is_empty() {
            return Ok(vec![Vec::new(); chunks.len()]);
        }

        debug!(unique_labels = all_canonicals.len(), "Extracted candidate tag labels");

        // Step 3: Check which tags already exist
        let mut canonical_to_id: HashMap<String, TagId> = HashMap::new();
        for canonical in all_canonicals.keys() {
            let existing = self
                .tag_store
                .find_by_label(canonical, Some(agent_id), 1)
                .await?;
            if let Some(tag) = existing.into_iter().next() {
                if tag.canonical == *canonical {
                    canonical_to_id.insert(canonical.clone(), tag.id);
                }
            }
        }

        // Step 4: Encode new labels that don't have existing tags
        let new_canonicals: Vec<String> = all_canonicals
            .keys()
            .filter(|c| !canonical_to_id.contains_key(*c))
            .cloned()
            .collect();

        if !new_canonicals.is_empty() {
            let texts_to_encode: Vec<String> = new_canonicals
                .iter()
                .map(|c| all_canonicals[c].clone())
                .collect();

            let vectors = self.encoder.encode_batch(&texts_to_encode).await?;

            let now = Utc::now();
            let mut new_tags = Vec::with_capacity(new_canonicals.len());
            for (canonical, vector) in new_canonicals.iter().zip(vectors.into_iter()) {
                let label = &all_canonicals[canonical];
                new_tags.push(Tag {
                    id: TagId::new(),
                    label: label.clone(),
                    canonical: canonical.clone(),
                    agent_id: Some(agent_id.clone()),
                    vector,
                    frequency: 1,
                    importance: 0.5,
                    created_at: now,
                    updated_at: now,
                });
            }

            let ids = self.tag_store.upsert_batch(&new_tags).await?;
            for (canonical, id) in new_canonicals.into_iter().zip(ids.into_iter()) {
                canonical_to_id.insert(canonical, id);
            }
        }

        // Also upsert existing tags to bump frequency
        for (canonical, id) in &canonical_to_id {
            if let Some(existing) = self.tag_store.get(id).await? {
                // Just touch it — upsert will increment frequency
                let _ = self.tag_store.upsert(&existing).await?;
            }
            let _ = canonical; // suppress unused warning
        }

        // Step 5: Build per-chunk tag ID lists and record co-occurrences
        let mut result = Vec::with_capacity(chunks.len());
        for labels in &per_chunk_labels {
            let mut chunk_tag_ids: Vec<TagId> = Vec::new();
            let mut seen = HashSet::new();
            for label in labels {
                let canonical = Tag::canonicalize(label);
                if let Some(id) = canonical_to_id.get(&canonical) {
                    if seen.insert(canonical) {
                        chunk_tag_ids.push(id.clone());
                    }
                }
            }

            // Record co-occurrences for all tags on this chunk
            if chunk_tag_ids.len() > 1 {
                self.tag_store.record_cooccurrence(&chunk_tag_ids).await?;
            }

            result.push(chunk_tag_ids);
        }

        debug!(chunks = result.len(), "Tag extraction complete");
        Ok(result)
    }

    /// Extract candidate label strings from skeleton + chunks.
    /// Returns one label set per chunk.
    fn extract_candidates(
        &self,
        skeleton: &DocSkeleton,
        chunks: &[Chunk],
    ) -> Vec<Vec<String>> {
        let stopwords: HashSet<&str> = STOPWORDS.iter().copied().collect();

        // Global labels from skeleton (applied to all chunks)
        let mut global_labels: Vec<String> = Vec::new();

        // (a) Title words
        for word in skeleton.title.split_whitespace() {
            let clean = word
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_owned();
            if clean.len() >= 2 && !stopwords.contains(clean.to_lowercase().as_str()) {
                global_labels.push(clean);
            }
        }

        // (b) Key entities
        for entity in &skeleton.key_entities {
            if !entity.name.is_empty() {
                global_labels.push(entity.name.clone());
            }
        }

        // (c) Section titles
        for section in &skeleton.sections {
            for word in section.title.split_whitespace() {
                let clean = word
                    .trim_matches(|c: char| !c.is_alphanumeric())
                    .to_owned();
                if clean.len() >= 2 && !stopwords.contains(clean.to_lowercase().as_str()) {
                    global_labels.push(clean);
                }
            }
        }

        // Per-chunk labels
        chunks
            .iter()
            .map(|chunk| {
                let mut labels = global_labels.clone();

                // (d) First sentence noun-like words
                let first_sentence = chunk
                    .text
                    .split_once(". ")
                    .map(|(s, _)| s)
                    .unwrap_or(&chunk.text);

                for word in first_sentence.split_whitespace() {
                    let clean = word
                        .trim_matches(|c: char| !c.is_alphanumeric())
                        .to_owned();
                    if clean.is_empty() {
                        continue;
                    }
                    // Heuristic: capitalized words or words > 4 chars
                    let is_capitalized = clean.chars().next().map(|c| c.is_uppercase()).unwrap_or(false);
                    let is_long = clean.len() > 4;
                    if (is_capitalized || is_long)
                        && !stopwords.contains(clean.to_lowercase().as_str())
                    {
                        labels.push(clean);
                    }
                }

                labels
            })
            .collect()
    }
}
