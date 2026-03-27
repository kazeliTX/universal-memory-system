//! Automatic tag extraction during document ingestion.
//!
//! Extracts candidate tag labels from document skeleton metadata and chunk
//! content, deduplicates by canonical form, encodes new tags via the Encoder,
//! and upserts them into the TagStore. Records co-occurrences for all tags
//! sharing the same chunk.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::Utc;
use jieba_rs::Jieba;
use tracing::{debug, instrument};

use umms_core::error::Result;
use umms_core::tag::Tag;
use umms_core::traits::{Encoder, TagStore};
use umms_core::types::{AgentId, TagId};

use super::chunker::Chunk;
use super::skeleton::DocSkeleton;

/// English stopwords to filter from title words.
const EN_STOPWORDS: &[&str] = &[
    "a", "an", "the", "and", "or", "but", "in", "on", "at", "to", "for",
    "of", "with", "by", "from", "as", "is", "was", "are", "were", "be",
    "been", "being", "have", "has", "had", "do", "does", "did", "will",
    "would", "could", "should", "may", "might", "shall", "can", "need",
    "it", "its", "this", "that", "these", "those", "he", "she", "they",
    "we", "you", "me", "him", "her", "us", "them", "my", "our", "your",
    "his", "their", "not", "no", "nor", "so", "if", "then", "than",
    "too", "very", "just", "about", "up", "out", "all", "also", "into",
];

/// Chinese stopwords (common function words, particles, pronouns).
const ZH_STOPWORDS: &[&str] = &[
    "的", "了", "在", "是", "我", "有", "和", "就", "不", "人", "都", "一",
    "一个", "上", "也", "很", "到", "说", "要", "去", "你", "会", "着",
    "没有", "看", "好", "自己", "这", "他", "她", "它", "们", "那", "被",
    "从", "把", "对", "与", "为", "中", "等", "能", "以", "及", "其",
    "而", "之", "所", "或", "但", "如", "这个", "那个", "什么", "怎么",
    "可以", "已经", "因为", "所以", "如果", "虽然", "只是", "可能",
    "通过", "进行", "使用", "以及", "之间", "关于", "这些", "那些",
];

/// Returns true if text contains CJK characters.
fn has_cjk(text: &str) -> bool {
    text.chars().any(|c| {
        ('\u{4E00}'..='\u{9FFF}').contains(&c)
            || ('\u{3400}'..='\u{4DBF}').contains(&c)
            || ('\u{F900}'..='\u{FAFF}').contains(&c)
    })
}

/// Extracts semantic tags from document content during ingestion.
///
/// Uses jieba-rs for Chinese text segmentation and whitespace splitting
/// for English. Filters stopwords in both languages.
pub struct TagExtractor {
    tag_store: Arc<dyn TagStore>,
    encoder: Arc<dyn Encoder>,
    jieba: Jieba,
}

impl TagExtractor {
    pub fn new(tag_store: Arc<dyn TagStore>, encoder: Arc<dyn Encoder>) -> Self {
        Self {
            tag_store,
            encoder,
            jieba: Jieba::new(),
        }
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

    /// Segment text into meaningful tokens using jieba for Chinese,
    /// whitespace for English, and hybrid handling for mixed text.
    fn segment_text(&self, text: &str) -> Vec<String> {
        let en_stops: HashSet<&str> = EN_STOPWORDS.iter().copied().collect();
        let zh_stops: HashSet<&str> = ZH_STOPWORDS.iter().copied().collect();
        let mut results = Vec::new();

        if has_cjk(text) {
            // Use jieba for text containing Chinese characters.
            // cut_for_search produces fine-grained segments good for indexing.
            for word in self.jieba.cut_for_search(text, true) {
                let trimmed = word.trim();
                if trimmed.is_empty() {
                    continue;
                }
                // Skip single CJK characters (too generic: 的, 了, 在...)
                // and single ASCII characters
                let char_count = trimmed.chars().count();
                if char_count < 2 {
                    continue;
                }
                // Skip stopwords in both languages
                if zh_stops.contains(trimmed) || en_stops.contains(&trimmed.to_lowercase().as_str()) {
                    continue;
                }
                // Skip pure punctuation / numbers
                if trimmed.chars().all(|c| c.is_ascii_punctuation() || c.is_ascii_digit()) {
                    continue;
                }
                results.push(trimmed.to_owned());
            }
        } else {
            // Pure English/ASCII: whitespace split
            for word in text.split_whitespace() {
                let clean = word
                    .trim_matches(|c: char| !c.is_alphanumeric())
                    .to_owned();
                if clean.len() >= 2 && !en_stops.contains(clean.to_lowercase().as_str()) {
                    results.push(clean);
                }
            }
        }

        results
    }

    /// Extract candidate label strings from skeleton + chunks.
    /// Returns one label set per chunk.
    fn extract_candidates(
        &self,
        skeleton: &DocSkeleton,
        chunks: &[Chunk],
    ) -> Vec<Vec<String>> {
        // Global labels from skeleton (applied to all chunks)
        let mut global_labels: Vec<String> = Vec::new();

        // (a) Title — full segmentation
        global_labels.extend(self.segment_text(&skeleton.title));

        // (b) Key entities — keep as-is (already meaningful phrases)
        for entity in &skeleton.key_entities {
            if !entity.name.is_empty() {
                global_labels.push(entity.name.clone());
            }
        }

        // (c) Section titles
        for section in &skeleton.sections {
            global_labels.extend(self.segment_text(&section.title));
        }

        // Per-chunk labels
        chunks
            .iter()
            .map(|chunk| {
                let mut labels = global_labels.clone();

                // (d) First sentence of chunk — extract key terms
                let first_sentence = chunk
                    .text
                    .split_once('。')          // Chinese period
                    .or_else(|| chunk.text.split_once(". "))  // English period
                    .map(|(s, _)| s)
                    .unwrap_or(&chunk.text);

                // Limit first sentence analysis to 200 chars
                let truncated = if first_sentence.len() > 200 {
                    &first_sentence[..first_sentence.floor_char_boundary(200)]
                } else {
                    first_sentence
                };

                labels.extend(self.segment_text(truncated));
                labels
            })
            .collect()
    }
}
