//! Weighted Knowledge Distillation (WKD) — compression of redundant memories.
//!
//! When a user stores many memories about the same topic, they accumulate
//! redundant information. WKD compresses these by:
//!
//! 1. Finding clusters of semantically similar memories (greedy nearest-neighbor).
//! 2. Merging each cluster into a single **distilled** memory.
//! 3. The distilled memory preserves key information from all originals.
//! 4. Original memories are archived (importance set to near-zero, tagged).
//!
//! The engine is intentionally simple: no external dependencies beyond the
//! `VectorStore` trait. LLM summarization is opt-in via a callback.

use std::future::Future;
use std::pin::Pin;
use std::time::Instant;

use serde::Serialize;
use tracing::{info, warn};

use umms_core::config::WkdConfig;
use umms_core::error::Result;
use umms_core::traits::VectorStore;
use umms_core::types::{AgentId, MemoryEntry, MemoryEntryBuilder};

// ---------------------------------------------------------------------------
// Result type
// ---------------------------------------------------------------------------

/// Result of a WKD compression run.
#[derive(Debug, Clone, Serialize)]
pub struct WkdResult {
    /// Total memories scanned for this agent.
    pub memories_scanned: usize,
    /// Number of clusters that met `min_cluster_size`.
    pub clusters_found: usize,
    /// Total original memories that were merged (sum of cluster sizes).
    pub memories_merged: usize,
    /// Total original memories that were archived.
    pub memories_archived: usize,
    /// Number of new distilled memories created.
    pub distilled_created: usize,
    /// Wall-clock time in milliseconds.
    pub elapsed_ms: u64,
}

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

/// WKD compression engine.
pub struct WkdEngine {
    config: WkdConfig,
}

impl WkdEngine {
    /// Create a new WKD engine with the given configuration.
    pub fn new(config: WkdConfig) -> Self {
        Self { config }
    }

    /// Access the configuration.
    pub fn config(&self) -> &WkdConfig {
        &self.config
    }

    /// Run WKD compression for an agent.
    ///
    /// Steps:
    /// 1. Load all memories for the agent from the `VectorStore`.
    /// 2. Find clusters of similar memories (greedy nearest-neighbor).
    /// 3. For each cluster with `>= min_cluster_size` members:
    ///    a. Create a distilled memory (merge content + max importance).
    ///    b. Insert distilled memory into the store.
    ///    c. Archive original memories (importance → near-zero, tagged).
    ///
    /// `generate_fn` is an optional LLM callback for summarization. When
    /// `use_llm_summary` is true and the callback is provided, the engine
    /// asks the LLM to produce a concise merged summary. Otherwise it
    /// concatenates the original content.
    pub async fn compress(
        &self,
        store: &dyn VectorStore,
        agent_id: &AgentId,
        generate_fn: Option<
            &(dyn Fn(&str) -> Pin<Box<dyn Future<Output = Result<String>> + Send + '_>> + Send + Sync),
        >,
    ) -> Result<WkdResult> {
        let start = Instant::now();

        if !self.config.enabled {
            return Ok(WkdResult {
                memories_scanned: 0,
                clusters_found: 0,
                memories_merged: 0,
                memories_archived: 0,
                distilled_created: 0,
                elapsed_ms: 0,
            });
        }

        // 1. Load all memories via paginated list (same pattern as DecayEngine).
        let mut all_memories: Vec<MemoryEntry> = Vec::new();
        let page_size: u64 = 500;
        let mut offset: u64 = 0;

        loop {
            let page = store.list(agent_id, offset, page_size, false).await?;
            let page_len = page.len();
            all_memories.extend(page);
            if (page_len as u64) < page_size {
                break;
            }
            offset += page_size;
        }

        let total = all_memories.len();
        info!(total_entries = total, "WKD: loaded memories for clustering");

        // 2. Find clusters
        let clusters = Self::find_clusters(
            &all_memories,
            self.config.similarity_threshold,
            self.config.max_cluster_size,
            self.config.min_cluster_size,
        );

        let mergeable: Vec<_> = clusters
            .into_iter()
            .take(self.config.max_merges_per_run)
            .collect();

        let clusters_found = mergeable.len();
        let mut merged = 0usize;
        let mut archived = 0usize;
        let mut distilled = 0usize;

        // 3. Merge each cluster
        for cluster_indices in &mergeable {
            let cluster_entries: Vec<&MemoryEntry> =
                cluster_indices.iter().map(|&i| &all_memories[i]).collect();
            match self
                .merge_cluster_entries(&cluster_entries, store, agent_id, generate_fn)
                .await
            {
                Ok((m, a)) => {
                    merged += m;
                    archived += a;
                    distilled += 1;
                }
                Err(e) => {
                    warn!(error = %e, "WKD: cluster merge failed, skipping");
                }
            }
        }

        let elapsed_ms = start.elapsed().as_millis() as u64;

        info!(
            memories_scanned = total,
            clusters_found,
            memories_merged = merged,
            distilled_created = distilled,
            elapsed_ms,
            "WKD compression complete"
        );

        Ok(WkdResult {
            memories_scanned: total,
            clusters_found,
            memories_merged: merged,
            memories_archived: archived,
            distilled_created: distilled,
            elapsed_ms,
        })
    }

    /// Greedy nearest-neighbor clustering.
    ///
    /// For each unvisited memory with a vector, scan remaining memories
    /// and pull in those within `similarity_threshold`. Cap cluster size
    /// at `max_cluster_size`. Only return clusters with `>= min_cluster_size`.
    fn find_clusters(
        memories: &[MemoryEntry],
        similarity_threshold: f32,
        max_cluster_size: usize,
        min_cluster_size: usize,
    ) -> Vec<Vec<usize>> {
        let mut visited = vec![false; memories.len()];
        let mut clusters: Vec<Vec<usize>> = Vec::new();

        for i in 0..memories.len() {
            if visited[i] {
                continue;
            }
            let Some(ref vec_i) = memories[i].vector else {
                continue;
            };
            if vec_i.is_empty() {
                continue;
            }

            let mut cluster = vec![i];
            visited[i] = true;

            for j in (i + 1)..memories.len() {
                if visited[j] {
                    continue;
                }
                let Some(ref vec_j) = memories[j].vector else {
                    continue;
                };
                if vec_j.is_empty() {
                    continue;
                }
                if cluster.len() >= max_cluster_size {
                    break;
                }

                let sim = cosine_similarity(vec_i, vec_j);
                if sim >= similarity_threshold {
                    cluster.push(j);
                    visited[j] = true;
                }
            }

            if cluster.len() >= min_cluster_size {
                clusters.push(cluster);
            }
        }

        clusters
    }

    /// Merge a cluster of memory entries into a single distilled memory.
    async fn merge_cluster_entries(
        &self,
        cluster: &[&MemoryEntry],
        store: &dyn VectorStore,
        agent_id: &AgentId,
        generate_fn: Option<
            &(dyn Fn(&str) -> Pin<Box<dyn Future<Output = Result<String>> + Send + '_>> + Send + Sync),
        >,
    ) -> Result<(usize, usize)> {
        // Build merged content
        let contents: Vec<&str> = cluster
            .iter()
            .filter_map(|e| e.content_text.as_deref())
            .collect();

        let merged_content = if self.config.use_llm_summary {
            if let Some(generate) = generate_fn {
                let prompt = format!(
                    "The following are multiple memories about the same topic. \
                     Please merge them into a concise summary preserving all key information.\n\n{}",
                    contents
                        .iter()
                        .enumerate()
                        .map(|(i, c)| format!("[{}] {}", i + 1, c))
                        .collect::<Vec<_>>()
                        .join("\n\n")
                );
                generate(&prompt)
                    .await
                    .unwrap_or_else(|_| contents.join("\n\n"))
            } else {
                contents.join("\n\n")
            }
        } else {
            contents.join("\n\n")
        };

        // Max importance from cluster
        let max_importance = cluster
            .iter()
            .map(|e| e.importance)
            .fold(0.0f32, f32::max);

        // Average vector from cluster
        let avg_vector = average_vectors(cluster);

        // Merge tags, deduplicate, add WKD marker
        let mut all_tags: Vec<String> = cluster
            .iter()
            .flat_map(|e| e.tags.iter().cloned())
            .collect();
        all_tags.sort();
        all_tags.dedup();
        all_tags.push("wkd:distilled".to_owned());

        // Use the first entry's layer/scope/modality as representative
        let first = cluster[0];

        // Create distilled entry
        let mut builder =
            MemoryEntryBuilder::new(agent_id.clone(), first.modality.clone())
                .layer(first.layer.clone())
                .scope(first.scope.clone())
                .importance(max_importance)
                .tags(all_tags);

        if !merged_content.is_empty() {
            builder = builder.content_text(merged_content);
        }

        if !avg_vector.is_empty() {
            builder = builder.vector(avg_vector);
        }

        let distilled = builder.build();

        store.insert(&distilled).await?;

        // Archive originals: set importance to near-zero and tag as archived
        let mut archived_count = 0;
        for entry in cluster {
            // Add the wkd:archived tag to existing tags
            let mut new_tags = entry.tags.clone();
            if !new_tags.contains(&"wkd:archived".to_owned()) {
                new_tags.push("wkd:archived".to_owned());
            }

            store
                .update_metadata(
                    &entry.id,
                    Some(0.01), // near-zero importance = effectively archived
                    Some(new_tags),
                    None,
                    None,
                )
                .await?;
            archived_count += 1;
        }

        info!(
            distilled_id = %distilled.id,
            sources = cluster.len(),
            "WKD: created distilled memory"
        );

        Ok((cluster.len(), archived_count))
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Cosine similarity between two vectors.
///
/// Returns 0.0 if either vector is empty or has zero magnitude.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let mut dot = 0.0f64;
    let mut mag_a = 0.0f64;
    let mut mag_b = 0.0f64;

    for (x, y) in a.iter().zip(b.iter()) {
        let x = *x as f64;
        let y = *y as f64;
        dot += x * y;
        mag_a += x * x;
        mag_b += y * y;
    }

    let denom = mag_a.sqrt() * mag_b.sqrt();
    if denom < f64::EPSILON {
        return 0.0;
    }

    (dot / denom) as f32
}

/// Average the vectors of a set of memory entries.
///
/// Entries without a vector are skipped. Returns an empty vec if no entries
/// have vectors.
pub fn average_vectors(entries: &[&MemoryEntry]) -> Vec<f32> {
    let vectors: Vec<&Vec<f32>> = entries
        .iter()
        .filter_map(|e| e.vector.as_ref())
        .filter(|v| !v.is_empty())
        .collect();

    if vectors.is_empty() {
        return Vec::new();
    }

    let dim = vectors[0].len();
    let count = vectors.len() as f32;
    let mut avg = vec![0.0f32; dim];

    for v in &vectors {
        if v.len() != dim {
            continue; // skip mismatched dimensions
        }
        for (i, val) in v.iter().enumerate() {
            avg[i] += val;
        }
    }

    for val in &mut avg {
        *val /= count;
    }

    avg
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    use umms_core::types::{AgentId, IsolationScope, MemoryEntryBuilder, MemoryLayer, Modality};

    fn make_entry(agent: &AgentId, text: &str, vector: Vec<f32>, importance: f32) -> MemoryEntry {
        MemoryEntryBuilder::new(agent.clone(), Modality::Text)
            .layer(MemoryLayer::EpisodicMemory)
            .scope(IsolationScope::Private)
            .content_text(text)
            .vector(vector)
            .importance(importance)
            .build()
    }

    // --- cosine_similarity tests ---

    #[test]
    fn cosine_similarity_identical_vectors() {
        let v = vec![1.0, 0.0, 0.0];
        let sim = cosine_similarity(&v, &v);
        assert!((sim - 1.0).abs() < 1e-5);
    }

    #[test]
    fn cosine_similarity_orthogonal_vectors() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-5);
    }

    #[test]
    fn cosine_similarity_opposite_vectors() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - (-1.0)).abs() < 1e-5);
    }

    #[test]
    fn cosine_similarity_empty_vectors() {
        let sim = cosine_similarity(&[], &[]);
        assert!(sim.abs() < 1e-5);
    }

    #[test]
    fn cosine_similarity_mismatched_length() {
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-5);
    }

    #[test]
    fn cosine_similarity_zero_magnitude() {
        let a = vec![0.0, 0.0];
        let b = vec![1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-5);
    }

    // --- average_vectors tests ---

    #[test]
    fn average_vectors_basic() {
        let agent = AgentId::from_str("test").unwrap();
        let e1 = make_entry(&agent, "a", vec![1.0, 0.0], 0.5);
        let e2 = make_entry(&agent, "b", vec![0.0, 1.0], 0.5);
        let refs: Vec<&MemoryEntry> = vec![&e1, &e2];
        let avg = average_vectors(&refs);
        assert_eq!(avg.len(), 2);
        assert!((avg[0] - 0.5).abs() < 1e-5);
        assert!((avg[1] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn average_vectors_no_vectors() {
        let agent = AgentId::from_str("test").unwrap();
        let e = MemoryEntryBuilder::new(agent, Modality::Text)
            .content_text("no vector")
            .build();
        let refs: Vec<&MemoryEntry> = vec![&e];
        let avg = average_vectors(&refs);
        assert!(avg.is_empty());
    }

    // --- find_clusters tests ---

    #[test]
    fn find_clusters_two_separate_clusters() {
        // Two pairs of similar vectors, well separated
        let agent = AgentId::from_str("test").unwrap();
        let entries = vec![
            make_entry(&agent, "a1", vec![1.0, 0.0, 0.0], 0.5),
            make_entry(&agent, "a2", vec![0.99, 0.1, 0.0], 0.5),
            make_entry(&agent, "b1", vec![0.0, 1.0, 0.0], 0.5),
            make_entry(&agent, "b2", vec![0.0, 0.99, 0.1], 0.5),
        ];

        let clusters = WkdEngine::find_clusters(&entries, 0.9, 5, 2);
        assert_eq!(clusters.len(), 2);
        // First cluster should contain indices 0,1
        assert!(clusters[0].contains(&0));
        assert!(clusters[0].contains(&1));
        // Second cluster should contain indices 2,3
        assert!(clusters[1].contains(&2));
        assert!(clusters[1].contains(&3));
    }

    #[test]
    fn find_clusters_all_similar() {
        // All vectors very similar → one cluster
        let agent = AgentId::from_str("test").unwrap();
        let entries = vec![
            make_entry(&agent, "a", vec![1.0, 0.0], 0.5),
            make_entry(&agent, "b", vec![0.99, 0.01], 0.5),
            make_entry(&agent, "c", vec![0.98, 0.02], 0.5),
        ];

        let clusters = WkdEngine::find_clusters(&entries, 0.9, 5, 2);
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].len(), 3);
    }

    #[test]
    fn find_clusters_empty_input() {
        let entries: Vec<MemoryEntry> = vec![];
        let clusters = WkdEngine::find_clusters(&entries, 0.85, 5, 2);
        assert!(clusters.is_empty());
    }

    #[test]
    fn find_clusters_singletons_excluded() {
        // All vectors orthogonal → no clusters with min_cluster_size=2
        let agent = AgentId::from_str("test").unwrap();
        let entries = vec![
            make_entry(&agent, "a", vec![1.0, 0.0, 0.0], 0.5),
            make_entry(&agent, "b", vec![0.0, 1.0, 0.0], 0.5),
            make_entry(&agent, "c", vec![0.0, 0.0, 1.0], 0.5),
        ];

        let clusters = WkdEngine::find_clusters(&entries, 0.9, 5, 2);
        assert!(clusters.is_empty());
    }

    #[test]
    fn find_clusters_respects_max_cluster_size() {
        // 5 similar vectors, max_cluster_size = 3
        let agent = AgentId::from_str("test").unwrap();
        let entries = vec![
            make_entry(&agent, "a", vec![1.0, 0.0], 0.5),
            make_entry(&agent, "b", vec![0.99, 0.01], 0.5),
            make_entry(&agent, "c", vec![0.98, 0.02], 0.5),
            make_entry(&agent, "d", vec![0.97, 0.03], 0.5),
            make_entry(&agent, "e", vec![0.96, 0.04], 0.5),
        ];

        let clusters = WkdEngine::find_clusters(&entries, 0.9, 3, 2);
        // First cluster gets 3 (max), remaining 2 may or may not form a second cluster
        assert!(!clusters.is_empty());
        assert!(clusters[0].len() <= 3);
    }

    // --- WkdConfig defaults ---

    #[test]
    fn wkd_config_default_values() {
        let cfg = WkdConfig::default();
        assert!(!cfg.enabled);
        assert!((cfg.similarity_threshold - 0.85).abs() < f32::EPSILON);
        assert_eq!(cfg.max_cluster_size, 5);
        assert_eq!(cfg.min_cluster_size, 2);
        assert_eq!(cfg.max_merges_per_run, 10);
        assert!(!cfg.use_llm_summary);
    }

    #[test]
    fn wkd_config_custom_values() {
        let cfg = WkdConfig {
            enabled: true,
            similarity_threshold: 0.9,
            max_cluster_size: 10,
            min_cluster_size: 3,
            max_merges_per_run: 20,
            use_llm_summary: true,
        };
        assert!(cfg.enabled);
        assert!((cfg.similarity_threshold - 0.9).abs() < f32::EPSILON);
        assert_eq!(cfg.max_cluster_size, 10);
        assert_eq!(cfg.min_cluster_size, 3);
    }
}
