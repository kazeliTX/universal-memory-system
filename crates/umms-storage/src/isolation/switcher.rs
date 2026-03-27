//! [`AgentSwitcher`] — orchestrates agent context switches by coordinating
//! the in-memory cache with the persistent snapshot store.
//!
//! This is the concrete struct that higher-level code should use to perform
//! agent switches. It holds both a [`MokaMemoryCache`] and a
//! [`SqliteAgentContextManager`] and implements the full
//! snapshot-evict-restore cycle.

use chrono::Utc;

use umms_core::error::Result;
use umms_core::traits::{AgentContextManager, AgentSnapshot, MemoryCache};
use umms_core::types::{AgentId, MemoryLayer};

use crate::cache::MokaMemoryCache;
use super::agent_context::SqliteAgentContextManager;

/// Orchestrates agent context switches.
///
/// Invariant: at most one agent's working set is live in the cache at a time.
/// Switching from agent A to agent B:
/// 1. Snapshot A's L0/L1 entries from cache.
/// 2. Persist snapshot to SQLite.
/// 3. Evict A from cache.
/// 4. Load B's snapshot from SQLite (if any).
/// 5. Restore B's entries into cache.
pub struct AgentSwitcher {
    cache: MokaMemoryCache,
    context_db: SqliteAgentContextManager,
}

impl AgentSwitcher {
    /// Create a new switcher with the given cache and context database.
    pub fn new(cache: MokaMemoryCache, context_db: SqliteAgentContextManager) -> Self {
        Self { cache, context_db }
    }

    /// Read-only access to the underlying cache.
    pub fn cache(&self) -> &MokaMemoryCache {
        &self.cache
    }

    /// Read-only access to the underlying context database.
    pub fn context_db(&self) -> &SqliteAgentContextManager {
        &self.context_db
    }

    /// Perform a full agent context switch from `from` to `to`.
    ///
    /// 1. Snapshot `from`'s cache entries and persist to SQLite.
    /// 2. Evict `from` from the cache.
    /// 3. Load `to`'s snapshot (if any) and restore into cache.
    pub async fn switch(&self, from: &AgentId, to: &AgentId) -> Result<()> {
        tracing::info!(
            from = from.as_str(),
            to = to.as_str(),
            "agent context switch"
        );

        // 1. Snapshot the outgoing agent's cache entries.
        let all_entries = self.cache.entries_for_agent(from).await;
        let l0_entries: Vec<_> = all_entries
            .iter()
            .filter(|e| e.layer == MemoryLayer::SensoryBuffer)
            .cloned()
            .collect();
        let l1_entries: Vec<_> = all_entries
            .iter()
            .filter(|e| e.layer != MemoryLayer::SensoryBuffer)
            .cloned()
            .collect();

        let snapshot = AgentSnapshot {
            agent_id: from.clone(),
            l0_entries,
            l1_entries,
            state_json: serde_json::Value::Null,
            snapshot_at: Utc::now(),
        };

        // 2. Persist and evict.
        self.context_db.save_snapshot(&snapshot).await?;
        self.cache.evict_agent(from).await;

        tracing::debug!(
            agent_id = from.as_str(),
            "evicted from cache, snapshot saved"
        );

        // 3. Restore the incoming agent (if a snapshot exists).
        if let Some(restored) = self.context_db.load_snapshot(to).await? {
            for entry in &restored.l0_entries {
                self.cache.put(to, &entry.id, entry.clone()).await;
            }
            for entry in &restored.l1_entries {
                self.cache.put(to, &entry.id, entry.clone()).await;
            }
            tracing::debug!(
                agent_id = to.as_str(),
                l0 = restored.l0_entries.len(),
                l1 = restored.l1_entries.len(),
                "restored from snapshot"
            );
        } else {
            tracing::debug!(
                agent_id = to.as_str(),
                "cold start — no previous snapshot"
            );
        }

        Ok(())
    }

    /// Cold-start an agent with no prior context. This is a no-op; it exists
    /// for symmetry and logging.
    pub async fn cold_start(&self, agent_id: &AgentId) -> Result<()> {
        tracing::info!(
            agent_id = agent_id.as_str(),
            "agent cold start — fresh context"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    use umms_core::types::{MemoryEntryBuilder, MemoryId, MemoryLayer, Modality};

    fn agent(name: &str) -> AgentId {
        AgentId::from_str(name).unwrap()
    }

    fn mem_id(name: &str) -> MemoryId {
        MemoryId::from_str(name).unwrap()
    }

    fn l0_entry(agent_id: &AgentId, id: &MemoryId, text: &str) -> umms_core::types::MemoryEntry {
        MemoryEntryBuilder::new(agent_id.clone(), Modality::Text)
            .id(id.clone())
            .layer(MemoryLayer::SensoryBuffer)
            .content_text(text)
            .build()
    }

    fn l1_entry(agent_id: &AgentId, id: &MemoryId, text: &str) -> umms_core::types::MemoryEntry {
        MemoryEntryBuilder::new(agent_id.clone(), Modality::Text)
            .id(id.clone())
            .layer(MemoryLayer::WorkingMemory)
            .content_text(text)
            .build()
    }

    fn make_switcher() -> AgentSwitcher {
        let cache = MokaMemoryCache::new();
        let ctx = SqliteAgentContextManager::new(":memory:").unwrap();
        AgentSwitcher::new(cache, ctx)
    }

    // -- switch from A to B: A's entries disappear, B starts empty ----------

    #[tokio::test]
    async fn switch_evicts_old_agent_cold_starts_new() {
        let sw = make_switcher();
        let a = agent("agent-a");
        let b = agent("agent-b");
        let m1 = mem_id("m1");
        let m2 = mem_id("m2");

        // Populate A's cache.
        sw.cache.put(&a, &m1, l0_entry(&a, &m1, "a-l0")).await;
        sw.cache.put(&a, &m2, l1_entry(&a, &m2, "a-l1")).await;
        assert_eq!(sw.cache.len(&a).await, 2);

        // Switch A -> B.
        sw.switch(&a, &b).await.unwrap();

        // A's entries are gone from cache.
        assert_eq!(sw.cache.len(&a).await, 0);
        assert!(sw.cache.get(&a, &m1).await.is_none());

        // B has no prior data, so cache is empty for B.
        assert_eq!(sw.cache.len(&b).await, 0);
    }

    // -- switch A -> B -> A: A's entries are restored -----------------------

    #[tokio::test]
    async fn switch_round_trip_restores_entries() {
        let sw = make_switcher();
        let a = agent("agent-a");
        let b = agent("agent-b");
        let m1 = mem_id("m1");
        let m2 = mem_id("m2");

        // Populate A.
        sw.cache.put(&a, &m1, l0_entry(&a, &m1, "a-l0")).await;
        sw.cache.put(&a, &m2, l1_entry(&a, &m2, "a-l1")).await;

        // Switch A -> B.
        sw.switch(&a, &b).await.unwrap();
        assert_eq!(sw.cache.len(&a).await, 0);

        // Switch B -> A (B has nothing to snapshot).
        sw.switch(&b, &a).await.unwrap();

        // A's entries are back.
        assert_eq!(sw.cache.len(&a).await, 2);
        let got = sw.cache.get(&a, &m1).await.unwrap();
        assert_eq!(got.content_text.as_deref(), Some("a-l0"));
        let got = sw.cache.get(&a, &m2).await.unwrap();
        assert_eq!(got.content_text.as_deref(), Some("a-l1"));
    }

    // -- switch preserves B's separately saved data -------------------------

    #[tokio::test]
    async fn switch_preserves_other_agents_snapshot() {
        let sw = make_switcher();
        let a = agent("agent-a");
        let b = agent("agent-b");
        let ma = mem_id("ma");
        let mb = mem_id("mb");

        // Populate A and B.
        sw.cache.put(&a, &ma, l0_entry(&a, &ma, "a-data")).await;
        sw.cache.put(&b, &mb, l1_entry(&b, &mb, "b-data")).await;

        // Switch A -> B: A is snapshotted and evicted, B's snapshot (none)
        // is loaded but B already has live cache entries.
        // First, save B's state via a separate switch to capture it.
        sw.switch(&b, &a).await.unwrap();
        // Now B's data is in the snapshot DB, A is restored.

        // Switch A -> B: A is saved again, B is restored.
        sw.switch(&a, &b).await.unwrap();

        // B's data should be restored from snapshot.
        let got = sw.cache.get(&b, &mb).await.unwrap();
        assert_eq!(got.content_text.as_deref(), Some("b-data"));
    }

    // -- agent isolation: after switch, new agent cannot see old entries -----

    #[tokio::test]
    async fn agent_isolation_after_switch() {
        let sw = make_switcher();
        let a = agent("agent-a");
        let b = agent("agent-b");
        let m1 = mem_id("m1");

        // Only A has data.
        sw.cache.put(&a, &m1, l0_entry(&a, &m1, "secret")).await;

        // Switch A -> B.
        sw.switch(&a, &b).await.unwrap();

        // B cannot see A's entry (it's been evicted from cache).
        assert!(sw.cache.get(&a, &m1).await.is_none());
        assert!(sw.cache.get(&b, &m1).await.is_none());

        // B adds its own entry.
        let m2 = mem_id("m2");
        sw.cache.put(&b, &m2, l0_entry(&b, &m2, "b-own")).await;
        assert_eq!(sw.cache.len(&b).await, 1);

        // A still has nothing in cache.
        assert_eq!(sw.cache.len(&a).await, 0);
    }
}
