//! Storage and service traits — the contracts between modules.
//!
//! These traits live in `umms-core` so that upper layers depend only on
//! abstract interfaces, never on concrete storage implementations.
//! Swapping LanceDB for Qdrant should require zero changes outside `umms-storage`.

use async_trait::async_trait;

use crate::error::Result;
use crate::types::*;

// ---------------------------------------------------------------------------
// Cache layer (L0/L1)
// ---------------------------------------------------------------------------

/// In-memory cache for L0 (sensory buffer) and L1 (working memory).
///
/// Implementations must be thread-safe (`Send + Sync`) because multiple
/// async tasks may read/write concurrently.
#[async_trait]
pub trait MemoryCache: Send + Sync {
    /// Insert or update a cache entry.
    async fn put(&self, agent_id: &AgentId, key: &MemoryId, entry: MemoryEntry);

    /// Retrieve a cached entry. Returns `None` if expired or absent.
    async fn get(&self, agent_id: &AgentId, key: &MemoryId) -> Option<MemoryEntry>;

    /// Remove a specific entry.
    async fn remove(&self, agent_id: &AgentId, key: &MemoryId);

    /// Evict all entries belonging to an agent. Used during agent switch.
    async fn evict_agent(&self, agent_id: &AgentId) -> Vec<MemoryEntry>;

    /// Return all entries for an agent (for snapshotting).
    async fn entries_for_agent(&self, agent_id: &AgentId) -> Vec<MemoryEntry>;

    /// Number of entries currently cached for an agent.
    async fn len(&self, agent_id: &AgentId) -> usize;
}

// ---------------------------------------------------------------------------
// Vector store (L2)
// ---------------------------------------------------------------------------

/// Persistent vector storage for episodic memory (L2).
///
/// All operations are scoped by `agent_id` — the implementation must filter
/// automatically. Calling code should never need to add agent_id filters manually.
#[async_trait]
pub trait VectorStore: Send + Sync {
    /// Insert a memory entry with its embedding vector.
    async fn insert(&self, entry: &MemoryEntry) -> Result<()>;

    /// Insert a batch of entries.
    async fn insert_batch(&self, entries: &[MemoryEntry]) -> Result<()>;

    /// Search by vector similarity, scoped to the given agent.
    /// If `include_shared` is true, also returns entries with `scope = Shared`.
    async fn search(
        &self,
        agent_id: &AgentId,
        vector: &[f32],
        top_k: usize,
        include_shared: bool,
    ) -> Result<Vec<ScoredMemory>>;

    /// Delete a specific entry.
    async fn delete(&self, id: &MemoryId) -> Result<()>;

    /// Get a single entry by ID.
    async fn get(&self, id: &MemoryId) -> Result<Option<MemoryEntry>>;

    /// Update metadata fields (importance, tags, access_count, etc.)
    async fn update_metadata(
        &self,
        id: &MemoryId,
        importance: Option<f32>,
        tags: Option<Vec<String>>,
        scope: Option<IsolationScope>,
    ) -> Result<()>;

    /// Count entries for an agent (optionally including shared).
    async fn count(&self, agent_id: &AgentId, include_shared: bool) -> Result<u64>;
}

// ---------------------------------------------------------------------------
// Knowledge graph (L3)
// ---------------------------------------------------------------------------

/// Knowledge graph storage for semantic memory (L3).
#[async_trait]
pub trait KnowledgeGraphStore: Send + Sync {
    /// Add a node. Returns the assigned `NodeId`.
    async fn add_node(&self, node: &KgNode) -> Result<NodeId>;

    /// Add an edge.
    async fn add_edge(&self, edge: &KgEdge) -> Result<EdgeId>;

    /// Find nodes by label substring, scoped to agent + shared.
    async fn find_nodes(
        &self,
        query: &str,
        agent_id: Option<&AgentId>,
        limit: usize,
    ) -> Result<Vec<KgNode>>;

    /// Get a node by ID.
    async fn get_node(&self, id: &NodeId) -> Result<Option<KgNode>>;

    /// Traverse the graph from a starting node up to `max_hops` edges away.
    async fn traverse(
        &self,
        start: &NodeId,
        max_hops: usize,
        agent_id: Option<&AgentId>,
    ) -> Result<(Vec<KgNode>, Vec<KgEdge>)>;

    /// Delete a node and all its incident edges.
    async fn delete_node(&self, id: &NodeId) -> Result<()>;

    /// Delete an edge.
    async fn delete_edge(&self, id: &EdgeId) -> Result<()>;
}

// ---------------------------------------------------------------------------
// Raw file store (L4)
// ---------------------------------------------------------------------------

/// Raw file storage for original content (L4).
#[async_trait]
pub trait RawFileStore: Send + Sync {
    /// Store a file and return its storage path.
    async fn store(
        &self,
        agent_id: &AgentId,
        filename: &str,
        data: &[u8],
    ) -> Result<String>;

    /// Read a file by its storage path.
    async fn read(&self, path: &str) -> Result<Vec<u8>>;

    /// Delete a file.
    async fn delete(&self, path: &str) -> Result<()>;

    /// Check if a file exists.
    async fn exists(&self, path: &str) -> Result<bool>;
}

// ---------------------------------------------------------------------------
// Agent context management
// ---------------------------------------------------------------------------

/// Snapshot data for suspending/resuming an agent's working context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSnapshot {
    pub agent_id: AgentId,
    pub l0_entries: Vec<MemoryEntry>,
    pub l1_entries: Vec<MemoryEntry>,
    pub state_json: serde_json::Value,
    pub snapshot_at: chrono::DateTime<chrono::Utc>,
}

/// Manages agent context lifecycle: snapshot, clean, restore.
///
/// Invariant: switching agents = snapshot(old) → clean caches → restore(new).
/// This is NOT destroy + create — the old agent's state must be fully recoverable.
#[async_trait]
pub trait AgentContextManager: Send + Sync {
    /// Snapshot the current agent's L0/L1 caches and execution state.
    #[must_use = "the snapshot must be persisted — dropping it loses agent state"]
    async fn snapshot(&self, agent_id: &AgentId) -> Result<AgentSnapshot>;

    /// Persist a snapshot to durable storage.
    async fn save_snapshot(&self, snapshot: &AgentSnapshot) -> Result<()>;

    /// Load a previously saved snapshot.
    async fn load_snapshot(&self, agent_id: &AgentId) -> Result<Option<AgentSnapshot>>;

    /// Perform a full agent switch: snapshot old → clean caches → restore new.
    async fn switch(&self, from: &AgentId, to: &AgentId) -> Result<()>;
}

use serde::{Serialize, Deserialize};
