//! Moka-based in-memory cache implementing [`MemoryCache`] from `umms-core`.
//!
//! Two separate caches:
//! - **L0 (sensory buffer):** capacity 1000, TTL 30 seconds.
//! - **L1 (working memory):** capacity 100, TTI 300 seconds.
//!
//! Cache key is `(String, String)` representing `(agent_id, memory_id)`.

use std::time::Duration;

use async_trait::async_trait;
use moka::future::Cache;
use umms_core::traits::MemoryCache;
use umms_core::types::{AgentId, MemoryEntry, MemoryId, MemoryLayer};

/// Compound cache key: `(agent_id, memory_id)` as owned strings.
type CacheKey = (String, String);

/// Moka-backed implementation of [`MemoryCache`].
///
/// Thread-safe and fully async. Entries are automatically evicted by TTL (L0)
/// or TTI (L1) policies, plus capacity-based eviction.
#[derive(Clone)]
pub struct MokaMemoryCache {
    /// L0: sensory buffer — high throughput, very short lived.
    l0: Cache<CacheKey, MemoryEntry>,
    /// L1: working memory — smaller capacity, idle-timeout based.
    l1: Cache<CacheKey, MemoryEntry>,
}

impl MokaMemoryCache {
    /// Create a new cache with default configuration.
    ///
    /// - L0: capacity 1000, TTL 30s
    /// - L1: capacity 100, TTI 300s
    #[must_use]
    pub fn new() -> Self {
        let l0 = Cache::builder()
            .max_capacity(1000)
            .time_to_live(Duration::from_secs(30))
            .build();

        let l1 = Cache::builder()
            .max_capacity(100)
            .time_to_idle(Duration::from_secs(300))
            .build();

        Self { l0, l1 }
    }

    /// Create a cache with custom durations and capacities (for testing).
    #[cfg(test)]
    fn with_durations(
        l0_capacity: u64,
        l0_ttl: Duration,
        l1_capacity: u64,
        l1_tti: Duration,
    ) -> Self {
        let l0 = Cache::builder()
            .max_capacity(l0_capacity)
            .time_to_live(l0_ttl)
            .build();
        let l1 = Cache::builder()
            .max_capacity(l1_capacity)
            .time_to_idle(l1_tti)
            .build();
        Self { l0, l1 }
    }

    /// Build a compound cache key from typed IDs.
    fn key(agent_id: &AgentId, memory_id: &MemoryId) -> CacheKey {
        (agent_id.as_str().to_owned(), memory_id.as_str().to_owned())
    }

    /// Select the appropriate cache based on memory layer.
    fn cache_for(&self, layer: MemoryLayer) -> &Cache<CacheKey, MemoryEntry> {
        match layer {
            MemoryLayer::SensoryBuffer => &self.l0,
            _ => &self.l1,
        }
    }

    /// Collect all entries for a given agent from one cache without removing them.
    fn collect_agent_entries(
        cache: &Cache<CacheKey, MemoryEntry>,
        agent_id: &AgentId,
    ) -> Vec<MemoryEntry> {
        let agent_str = agent_id.as_str();
        cache
            .iter()
            .filter(|(k, _)| k.0 == agent_str)
            .map(|(_, v)| v)
            .collect()
    }

    /// Remove all entries for a given agent from one cache, returning them.
    async fn drain_agent_entries(
        cache: &Cache<CacheKey, MemoryEntry>,
        agent_id: &AgentId,
    ) -> Vec<MemoryEntry> {
        let agent_str = agent_id.as_str();
        let keys: Vec<CacheKey> = cache
            .iter()
            .filter(|(k, _)| k.0 == agent_str)
            .map(|(k, _)| k.as_ref().clone())
            .collect();

        let mut entries = Vec::with_capacity(keys.len());
        for key in &keys {
            if let Some(entry) = cache.get(key).await {
                entries.push(entry);
            }
            cache.invalidate(key).await;
        }
        entries
    }
}

impl Default for MokaMemoryCache {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MemoryCache for MokaMemoryCache {
    async fn put(&self, agent_id: &AgentId, key: &MemoryId, entry: MemoryEntry) {
        let cache_key = Self::key(agent_id, key);
        let cache = self.cache_for(entry.layer);
        tracing::debug!(
            agent_id = agent_id.as_str(),
            memory_id = key.as_str(),
            layer = ?entry.layer,
            "cache put"
        );
        cache.insert(cache_key, entry).await;
    }

    async fn get(&self, agent_id: &AgentId, key: &MemoryId) -> Option<MemoryEntry> {
        let cache_key = Self::key(agent_id, key);

        // Check L0 first (most recent data), then L1.
        if let Some(entry) = self.l0.get(&cache_key).await {
            tracing::debug!(
                agent_id = agent_id.as_str(),
                memory_id = key.as_str(),
                "L0 cache hit"
            );
            return Some(entry);
        }
        if let Some(entry) = self.l1.get(&cache_key).await {
            tracing::debug!(
                agent_id = agent_id.as_str(),
                memory_id = key.as_str(),
                "L1 cache hit"
            );
            return Some(entry);
        }

        tracing::debug!(
            agent_id = agent_id.as_str(),
            memory_id = key.as_str(),
            "cache miss"
        );
        None
    }

    async fn remove(&self, agent_id: &AgentId, key: &MemoryId) {
        let cache_key = Self::key(agent_id, key);
        self.l0.invalidate(&cache_key).await;
        self.l1.invalidate(&cache_key).await;
        tracing::debug!(
            agent_id = agent_id.as_str(),
            memory_id = key.as_str(),
            "cache remove"
        );
    }

    async fn evict_agent(&self, agent_id: &AgentId) -> Vec<MemoryEntry> {
        let mut entries = Self::drain_agent_entries(&self.l0, agent_id).await;
        entries.extend(Self::drain_agent_entries(&self.l1, agent_id).await);
        tracing::debug!(
            agent_id = agent_id.as_str(),
            count = entries.len(),
            "evict_agent"
        );
        entries
    }

    async fn entries_for_agent(&self, agent_id: &AgentId) -> Vec<MemoryEntry> {
        let mut entries = Self::collect_agent_entries(&self.l0, agent_id);
        entries.extend(Self::collect_agent_entries(&self.l1, agent_id));
        tracing::debug!(
            agent_id = agent_id.as_str(),
            count = entries.len(),
            "entries_for_agent"
        );
        entries
    }

    async fn len(&self, agent_id: &AgentId) -> usize {
        let agent_str = agent_id.as_str();
        let l0_count = self.l0.iter().filter(|(k, _)| k.0 == agent_str).count();
        let l1_count = self.l1.iter().filter(|(k, _)| k.0 == agent_str).count();
        l0_count + l1_count
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use umms_core::types::{MemoryEntryBuilder, MemoryLayer, Modality};

    /// Helper: create a sensory-buffer (L0) entry.
    fn l0_entry(agent: &AgentId, id: &MemoryId, text: &str) -> MemoryEntry {
        MemoryEntryBuilder::new(agent.clone(), Modality::Text)
            .id(id.clone())
            .layer(MemoryLayer::SensoryBuffer)
            .content_text(text)
            .build()
    }

    /// Helper: create a working-memory (L1) entry.
    fn l1_entry(agent: &AgentId, id: &MemoryId, text: &str) -> MemoryEntry {
        MemoryEntryBuilder::new(agent.clone(), Modality::Text)
            .id(id.clone())
            .layer(MemoryLayer::WorkingMemory)
            .content_text(text)
            .build()
    }

    fn agent(name: &str) -> AgentId {
        AgentId::from_str(name).unwrap()
    }

    fn mem_id(name: &str) -> MemoryId {
        MemoryId::from_str(name).unwrap()
    }

    // -- put / get -----------------------------------------------------------

    #[tokio::test]
    async fn put_and_get_l0() {
        let cache = MokaMemoryCache::new();
        let a = agent("agent-a");
        let id = mem_id("mem-1");
        let entry = l0_entry(&a, &id, "hello sensory");

        cache.put(&a, &id, entry.clone()).await;
        let got = cache.get(&a, &id).await;

        assert!(got.is_some());
        assert_eq!(got.unwrap().content_text.as_deref(), Some("hello sensory"));
    }

    #[tokio::test]
    async fn put_and_get_l1() {
        let cache = MokaMemoryCache::new();
        let a = agent("agent-a");
        let id = mem_id("mem-1");
        let entry = l1_entry(&a, &id, "hello working");

        cache.put(&a, &id, entry.clone()).await;
        let got = cache.get(&a, &id).await;

        assert!(got.is_some());
        assert_eq!(got.unwrap().content_text.as_deref(), Some("hello working"));
    }

    #[tokio::test]
    async fn get_missing_returns_none() {
        let cache = MokaMemoryCache::new();
        let a = agent("agent-a");
        let id = mem_id("nonexistent");

        assert!(cache.get(&a, &id).await.is_none());
    }

    // -- remove --------------------------------------------------------------

    #[tokio::test]
    async fn remove_deletes_entry() {
        let cache = MokaMemoryCache::new();
        let a = agent("agent-a");
        let id = mem_id("mem-1");

        cache.put(&a, &id, l0_entry(&a, &id, "temp")).await;
        assert!(cache.get(&a, &id).await.is_some());

        cache.remove(&a, &id).await;
        assert!(cache.get(&a, &id).await.is_none());
    }

    // -- agent isolation -----------------------------------------------------

    #[tokio::test]
    async fn agent_isolation() {
        let cache = MokaMemoryCache::new();
        let a = agent("agent-a");
        let b = agent("agent-b");
        let id = mem_id("mem-1");

        cache.put(&a, &id, l0_entry(&a, &id, "a-data")).await;
        cache.put(&b, &id, l0_entry(&b, &id, "b-data")).await;

        // Each agent sees only its own data.
        let got_a = cache.get(&a, &id).await.unwrap();
        assert_eq!(got_a.content_text.as_deref(), Some("a-data"));

        let got_b = cache.get(&b, &id).await.unwrap();
        assert_eq!(got_b.content_text.as_deref(), Some("b-data"));

        // Agent C sees nothing.
        let c = agent("agent-c");
        assert!(cache.get(&c, &id).await.is_none());
    }

    // -- len -----------------------------------------------------------------

    #[tokio::test]
    async fn len_counts_per_agent() {
        let cache = MokaMemoryCache::new();
        let a = agent("agent-a");
        let b = agent("agent-b");

        cache
            .put(&a, &mem_id("m1"), l0_entry(&a, &mem_id("m1"), "1"))
            .await;
        cache
            .put(&a, &mem_id("m2"), l1_entry(&a, &mem_id("m2"), "2"))
            .await;
        cache
            .put(&b, &mem_id("m3"), l0_entry(&b, &mem_id("m3"), "3"))
            .await;

        assert_eq!(cache.len(&a).await, 2);
        assert_eq!(cache.len(&b).await, 1);
    }

    // -- evict_agent ---------------------------------------------------------

    #[tokio::test]
    async fn evict_agent_returns_all_and_removes() {
        let cache = MokaMemoryCache::new();
        let a = agent("agent-a");
        let b = agent("agent-b");

        cache
            .put(&a, &mem_id("m1"), l0_entry(&a, &mem_id("m1"), "a-l0"))
            .await;
        cache
            .put(&a, &mem_id("m2"), l1_entry(&a, &mem_id("m2"), "a-l1"))
            .await;
        cache
            .put(&b, &mem_id("m3"), l0_entry(&b, &mem_id("m3"), "b-l0"))
            .await;

        let evicted = cache.evict_agent(&a).await;
        assert_eq!(evicted.len(), 2);

        // All of agent A's entries are gone.
        assert!(cache.get(&a, &mem_id("m1")).await.is_none());
        assert!(cache.get(&a, &mem_id("m2")).await.is_none());
        assert_eq!(cache.len(&a).await, 0);

        // Agent B is untouched.
        assert!(cache.get(&b, &mem_id("m3")).await.is_some());
        assert_eq!(cache.len(&b).await, 1);
    }

    #[tokio::test]
    async fn evict_agent_returns_correct_content() {
        let cache = MokaMemoryCache::new();
        let a = agent("agent-a");

        cache
            .put(&a, &mem_id("m1"), l0_entry(&a, &mem_id("m1"), "first"))
            .await;
        cache
            .put(&a, &mem_id("m2"), l1_entry(&a, &mem_id("m2"), "second"))
            .await;

        let evicted = cache.evict_agent(&a).await;
        let mut texts: Vec<String> = evicted
            .iter()
            .filter_map(|e| e.content_text.clone())
            .collect();
        texts.sort();
        assert_eq!(texts, vec!["first", "second"]);
    }

    // -- entries_for_agent (snapshotting) ------------------------------------

    #[tokio::test]
    async fn entries_for_agent_returns_snapshot_without_removing() {
        let cache = MokaMemoryCache::new();
        let a = agent("agent-a");

        cache
            .put(&a, &mem_id("m1"), l0_entry(&a, &mem_id("m1"), "snap-1"))
            .await;
        cache
            .put(&a, &mem_id("m2"), l1_entry(&a, &mem_id("m2"), "snap-2"))
            .await;

        let snapshot = cache.entries_for_agent(&a).await;
        assert_eq!(snapshot.len(), 2);

        // Entries are still present after snapshotting.
        assert!(cache.get(&a, &mem_id("m1")).await.is_some());
        assert!(cache.get(&a, &mem_id("m2")).await.is_some());
        assert_eq!(cache.len(&a).await, 2);
    }

    #[tokio::test]
    async fn entries_for_agent_does_not_include_other_agents() {
        let cache = MokaMemoryCache::new();
        let a = agent("agent-a");
        let b = agent("agent-b");

        cache
            .put(&a, &mem_id("m1"), l0_entry(&a, &mem_id("m1"), "a-data"))
            .await;
        cache
            .put(&b, &mem_id("m2"), l0_entry(&b, &mem_id("m2"), "b-data"))
            .await;

        let a_entries = cache.entries_for_agent(&a).await;
        assert_eq!(a_entries.len(), 1);
        assert_eq!(
            a_entries[0].content_text.as_deref(),
            Some("a-data")
        );
    }

    // -- TTL expiry (L0) — short durations with real sleeps ----------------

    #[tokio::test]
    async fn l0_entry_expires_after_ttl() {
        // Use a 1-second TTL so the test is fast.
        let cache = MokaMemoryCache::with_durations(
            1000,
            Duration::from_secs(1),
            100,
            Duration::from_secs(300),
        );
        let a = agent("agent-a");
        let id = mem_id("mem-ttl");

        cache.put(&a, &id, l0_entry(&a, &id, "ephemeral")).await;
        assert!(cache.get(&a, &id).await.is_some());

        // Wait past the TTL.
        tokio::time::sleep(Duration::from_millis(1200)).await;
        cache.l0.run_pending_tasks().await;

        assert!(
            cache.get(&a, &id).await.is_none(),
            "L0 entry should have expired after TTL"
        );
    }

    #[tokio::test]
    async fn l0_entry_alive_before_ttl() {
        let cache = MokaMemoryCache::with_durations(
            1000,
            Duration::from_secs(2),
            100,
            Duration::from_secs(300),
        );
        let a = agent("agent-a");
        let id = mem_id("mem-alive");

        cache.put(&a, &id, l0_entry(&a, &id, "still here")).await;

        // Check well before the TTL.
        tokio::time::sleep(Duration::from_millis(200)).await;
        cache.l0.run_pending_tasks().await;

        assert!(
            cache.get(&a, &id).await.is_some(),
            "L0 entry should still be alive before TTL"
        );
    }

    // -- TTI expiry (L1) — short durations with real sleeps ----------------

    #[tokio::test]
    async fn l1_entry_expires_after_idle() {
        let cache = MokaMemoryCache::with_durations(
            1000,
            Duration::from_secs(30),
            100,
            Duration::from_secs(1), // 1s TTI
        );
        let a = agent("agent-a");
        let id = mem_id("mem-tti");

        cache.put(&a, &id, l1_entry(&a, &id, "idle test")).await;
        assert!(cache.get(&a, &id).await.is_some());

        // Wait past the TTI without accessing.
        tokio::time::sleep(Duration::from_millis(1200)).await;
        cache.l1.run_pending_tasks().await;

        assert!(
            cache.get(&a, &id).await.is_none(),
            "L1 entry should have expired after idle timeout"
        );
    }

    #[tokio::test]
    async fn l1_entry_stays_alive_with_access() {
        let cache = MokaMemoryCache::with_durations(
            1000,
            Duration::from_secs(30),
            100,
            Duration::from_secs(1), // 1s TTI
        );
        let a = agent("agent-a");
        let id = mem_id("mem-active");

        cache
            .put(&a, &id, l1_entry(&a, &id, "keep alive"))
            .await;

        // Access every 500ms (before the 1s TTI) three times.
        for _ in 0..3 {
            tokio::time::sleep(Duration::from_millis(500)).await;
            cache.l1.run_pending_tasks().await;
            assert!(
                cache.get(&a, &id).await.is_some(),
                "L1 entry should stay alive when accessed before TTI"
            );
        }
    }

    // -- mixed L0/L1 ---------------------------------------------------------

    #[tokio::test]
    async fn put_overwrite_same_key() {
        let cache = MokaMemoryCache::new();
        let a = agent("agent-a");
        let id = mem_id("mem-1");

        cache
            .put(&a, &id, l0_entry(&a, &id, "version-1"))
            .await;
        cache
            .put(&a, &id, l0_entry(&a, &id, "version-2"))
            .await;

        let got = cache.get(&a, &id).await.unwrap();
        assert_eq!(got.content_text.as_deref(), Some("version-2"));
    }

    #[tokio::test]
    async fn same_memory_id_different_layers() {
        let cache = MokaMemoryCache::new();
        let a = agent("agent-a");
        let id = mem_id("shared-id");

        // Put to L0 and L1 with same agent+memory_id.
        // They go to different caches, so `get` returns L0 first.
        cache
            .put(&a, &id, l0_entry(&a, &id, "l0-version"))
            .await;
        cache
            .put(&a, &id, l1_entry(&a, &id, "l1-version"))
            .await;

        // get() checks L0 first.
        let got = cache.get(&a, &id).await.unwrap();
        assert_eq!(got.content_text.as_deref(), Some("l0-version"));

        // After removing L0 entry, L1 entry is visible.
        cache.l0.invalidate(&MokaMemoryCache::key(&a, &id)).await;
        let got = cache.get(&a, &id).await.unwrap();
        assert_eq!(got.content_text.as_deref(), Some("l1-version"));
    }
}
