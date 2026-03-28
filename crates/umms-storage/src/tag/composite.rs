//! Composite tag store combining LanceDB vectors with SQLite co-occurrence.
//!
//! Implements the [`TagStore`] trait by delegating vector operations to
//! [`LanceTagStore`] and co-occurrence tracking to [`SqliteCoocStore`].

use async_trait::async_trait;
use chrono::Utc;
use tracing::instrument;

use umms_core::error::Result;
use umms_core::tag::{Tag, TagCooccurrence, TagMatch};
use umms_core::traits::TagStore;
use umms_core::types::*;

use super::lance_tag::LanceTagStore;
use super::sqlite_cooc::SqliteCoocStore;

// ---------------------------------------------------------------------------
// CompositeTagStore
// ---------------------------------------------------------------------------

/// Combined tag storage: LanceDB for vectors, SQLite for co-occurrence.
///
/// This is the primary implementation of [`TagStore`] that upper layers use.
/// It coordinates between the two backends and handles upsert semantics
/// (dedup by canonical label + agent scope, running-average vectors).
pub struct CompositeTagStore {
    lance: LanceTagStore,
    cooc: SqliteCoocStore,
}

impl CompositeTagStore {
    /// Create a new composite store.
    pub fn new(lance: LanceTagStore, cooc: SqliteCoocStore) -> Self {
        Self { lance, cooc }
    }

    /// Convenience constructor that initialises both backends.
    ///
    /// `lance_path` — directory for the LanceDB database.
    /// `sqlite_path` — file path for the SQLite co-occurrence database.
    /// `vector_dim` — embedding dimension (typically 3072).
    pub async fn open(
        lance_path: &str,
        sqlite_path: &str,
        vector_dim: usize,
    ) -> std::result::Result<Self, umms_core::error::UmmsError> {
        let lance = LanceTagStore::new(lance_path, vector_dim).await?;
        let cooc = SqliteCoocStore::new(sqlite_path)?;
        Ok(Self { lance, cooc })
    }

    /// Merge an incoming tag with an existing one (running-average vector,
    /// incremented frequency, updated importance).
    fn merge_tags(existing: &Tag, incoming: &Tag) -> Tag {
        let new_freq = existing.frequency + 1;

        // Running average of vectors: new_vec = (old_vec * old_freq + incoming_vec) / new_freq
        let vector = if existing.vector.len() == incoming.vector.len() {
            existing
                .vector
                .iter()
                .zip(incoming.vector.iter())
                .map(|(old, new)| (old * existing.frequency as f32 + new) / new_freq as f32)
                .collect()
        } else {
            incoming.vector.clone()
        };

        Tag {
            id: existing.id.clone(),
            label: incoming.label.clone(),
            canonical: existing.canonical.clone(),
            agent_id: existing.agent_id.clone(),
            vector,
            frequency: new_freq,
            importance: incoming.importance.max(existing.importance),
            created_at: existing.created_at,
            updated_at: Utc::now(),
        }
    }
}

#[async_trait]
impl TagStore for CompositeTagStore {
    #[instrument(skip(self, tag), fields(label = %tag.label))]
    async fn upsert(&self, tag: &Tag) -> Result<TagId> {
        // Check for existing tag with same canonical + agent scope.
        let existing = self
            .lance
            .find_by_canonical(&tag.canonical, tag.agent_id.as_ref())
            .await?;

        let final_tag = match existing {
            Some(ex) => Self::merge_tags(&ex, tag),
            None => tag.clone(),
        };

        self.lance.upsert_tag(&final_tag).await
    }

    #[instrument(skip(self, tags), fields(count = tags.len()))]
    async fn upsert_batch(&self, tags: &[Tag]) -> Result<Vec<TagId>> {
        let mut ids = Vec::with_capacity(tags.len());
        for tag in tags {
            let id = self.upsert(tag).await?;
            ids.push(id);
        }
        Ok(ids)
    }

    #[instrument(skip(self, vector), fields(top_k))]
    async fn search_by_vector(
        &self,
        vector: &[f32],
        agent_id: Option<&AgentId>,
        top_k: usize,
    ) -> Result<Vec<TagMatch>> {
        self.lance.search_by_vector(vector, agent_id, top_k).await
    }

    #[instrument(skip(self), fields(query, limit))]
    async fn find_by_label(
        &self,
        query: &str,
        agent_id: Option<&AgentId>,
        limit: usize,
    ) -> Result<Vec<Tag>> {
        self.lance.find_by_label(query, agent_id, limit).await
    }

    #[instrument(skip(self, tag_ids), fields(count = tag_ids.len()))]
    async fn record_cooccurrence(&self, tag_ids: &[TagId]) -> Result<()> {
        self.cooc.record_cooccurrence(tag_ids).await
    }

    #[instrument(skip(self), fields(tag_id = %tag_id, limit))]
    async fn cooccurrences(&self, tag_id: &TagId, limit: usize) -> Result<Vec<TagCooccurrence>> {
        self.cooc.cooccurrences(tag_id, limit).await
    }

    async fn all_tags(&self, agent_id: Option<&AgentId>) -> Result<Vec<Tag>> {
        self.lance.all_tags(agent_id).await
    }

    #[instrument(skip(self), fields(id = %id))]
    async fn get(&self, id: &TagId) -> Result<Option<Tag>> {
        self.lance.get_tag(id).await
    }

    async fn get_batch(&self, ids: &[TagId]) -> Result<Vec<Tag>> {
        self.lance.get_batch_tags(ids).await
    }

    async fn count(&self, agent_id: Option<&AgentId>) -> Result<u64> {
        self.lance.count(agent_id).await
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::str::FromStr;

    fn temp_lance_path(suffix: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "umms_composite_tag_test_{suffix}_{}",
            uuid::Uuid::new_v4()
        ));
        p
    }

    fn make_tag(label: &str, agent: Option<&str>, dim: usize, seed: u8) -> Tag {
        Tag {
            id: TagId::new(),
            label: label.to_owned(),
            canonical: Tag::canonicalize(label),
            agent_id: agent.map(|a| AgentId::from_str(a).unwrap()),
            vector: random_vector(dim, seed),
            frequency: 1,
            importance: 0.5,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn random_vector(dim: usize, seed: u8) -> Vec<f32> {
        (0..dim)
            .map(|i| ((i as f32 + f32::from(seed)) * 0.1).sin())
            .collect()
    }

    async fn make_store(suffix: &str, dim: usize) -> CompositeTagStore {
        let lance_path = temp_lance_path(suffix);
        let lance = LanceTagStore::new(lance_path.to_str().unwrap(), dim)
            .await
            .unwrap();
        let cooc = SqliteCoocStore::new(":memory:").unwrap();
        CompositeTagStore::new(lance, cooc)
    }

    #[tokio::test]
    async fn upsert_deduplicates_by_canonical() {
        let store = make_store("dedup", 8).await;

        let tag1 = make_tag("Rust Ownership", Some("agent-a"), 8, 1);
        let id1 = store.upsert(&tag1).await.unwrap();

        // Same canonical ("rust ownership") should merge, not create new.
        let tag2 = make_tag("rust ownership", Some("agent-a"), 8, 2);
        let id2 = store.upsert(&tag2).await.unwrap();

        assert_eq!(id1, id2);

        let count = store.count(None).await.unwrap();
        assert_eq!(count, 1);

        // Frequency should be incremented.
        let fetched = store.get(&id1).await.unwrap().unwrap();
        assert_eq!(fetched.frequency, 2);
    }

    #[tokio::test]
    async fn upsert_batch_works() {
        let store = make_store("batch", 8).await;

        let tags = vec![
            make_tag("Neural network", Some("agent-a"), 8, 1),
            make_tag("Deep learning", Some("agent-a"), 8, 2),
        ];
        let ids = store.upsert_batch(&tags).await.unwrap();
        assert_eq!(ids.len(), 2);

        let count = store.count(None).await.unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn cooccurrence_through_composite() {
        let store = make_store("cooc", 8).await;

        let tag_a = make_tag("Alpha", Some("agent-a"), 8, 1);
        let tag_b = make_tag("Beta", Some("agent-a"), 8, 2);
        let tag_c = make_tag("Gamma", Some("agent-a"), 8, 3);

        let id_a = store.upsert(&tag_a).await.unwrap();
        let id_b = store.upsert(&tag_b).await.unwrap();
        let id_c = store.upsert(&tag_c).await.unwrap();

        store
            .record_cooccurrence(&[id_a.clone(), id_b.clone(), id_c.clone()])
            .await
            .unwrap();

        let coocs = store.cooccurrences(&id_a, 10).await.unwrap();
        assert_eq!(coocs.len(), 2);
    }

    #[tokio::test]
    async fn search_by_vector_through_composite() {
        let store = make_store("search", 8).await;

        let tag = make_tag("Rust ownership", Some("agent-a"), 8, 1);
        store.upsert(&tag).await.unwrap();

        let query_vec = random_vector(8, 1);
        let agent_id = AgentId::from_str("agent-a").unwrap();
        let results = store
            .search_by_vector(&query_vec, Some(&agent_id), 5)
            .await
            .unwrap();
        assert!(!results.is_empty());
    }

    #[tokio::test]
    async fn find_by_label_through_composite() {
        let store = make_store("find_label", 8).await;

        let tags = vec![
            make_tag("Rust ownership", Some("agent-a"), 8, 1),
            make_tag("Rust lifetimes", Some("agent-a"), 8, 2),
            make_tag("Python typing", Some("agent-a"), 8, 3),
        ];
        store.upsert_batch(&tags).await.unwrap();

        let agent_id = AgentId::from_str("agent-a").unwrap();
        let found = store
            .find_by_label("rust", Some(&agent_id), 10)
            .await
            .unwrap();
        assert_eq!(found.len(), 2);
    }

    #[tokio::test]
    async fn get_batch_returns_matching_tags() {
        let store = make_store("get_batch", 8).await;

        let tag1 = make_tag("Alpha", Some("agent-a"), 8, 1);
        let tag2 = make_tag("Beta", Some("agent-a"), 8, 2);
        let id1 = store.upsert(&tag1).await.unwrap();
        let id2 = store.upsert(&tag2).await.unwrap();

        let fetched = store.get_batch(&[id1, id2]).await.unwrap();
        assert_eq!(fetched.len(), 2);
    }
}
