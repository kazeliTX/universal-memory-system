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

    /// Update metadata fields (importance, tags, scope, agent_id, user_rating, etc.)
    async fn update_metadata(
        &self,
        id: &MemoryId,
        importance: Option<f32>,
        tags: Option<Vec<String>>,
        scope: Option<IsolationScope>,
        agent_id: Option<AgentId>,
    ) -> Result<()>;

    /// Update the user feedback rating for importance scoring.
    /// `rating` must be in `[-1.0, 1.0]`.
    async fn update_user_rating(
        &self,
        id: &MemoryId,
        rating: Option<f32>,
    ) -> Result<()>;

    /// Count entries for an agent (optionally including shared).
    async fn count(&self, agent_id: &AgentId, include_shared: bool) -> Result<u64>;

    /// List entries for an agent with pagination.
    /// Returns entries ordered by created_at descending (newest first).
    async fn list(
        &self,
        agent_id: &AgentId,
        offset: u64,
        limit: u64,
        include_shared: bool,
    ) -> Result<Vec<MemoryEntry>>;

    /// Delete all entries for an agent. If `include_shared` is true, also
    /// deletes entries with `scope = Shared`.
    ///
    /// Returns the number of entries deleted.
    async fn delete_all(&self, agent_id: &AgentId, include_shared: bool) -> Result<u64>;
}

// ---------------------------------------------------------------------------
// Knowledge graph (L3)
// ---------------------------------------------------------------------------

/// Knowledge graph storage for semantic memory (L3).
///
/// This trait is designed to be the **only** graph interface that upper layers
/// (M3 retrieval, M4 consolidation) interact with. The goal is to make backend
/// swaps (SQLite→Kùzu, petgraph→neo4j) a local change inside `umms-storage`.
///
/// If you find yourself needing a graph operation that this trait doesn't cover,
/// **add it here** rather than reaching into the implementation. The moment upper
/// layers bypass this trait, the abstraction is broken and migration cost explodes.
#[async_trait]
pub trait KnowledgeGraphStore: Send + Sync {
    // ----- Basic CRUD -----

    /// Add a node. Returns the assigned `NodeId`.
    async fn add_node(&self, node: &KgNode) -> Result<NodeId>;

    /// Add an edge.
    async fn add_edge(&self, edge: &KgEdge) -> Result<EdgeId>;

    /// Get a node by ID.
    async fn get_node(&self, id: &NodeId) -> Result<Option<KgNode>>;

    /// Delete a node and all its incident edges.
    async fn delete_node(&self, id: &NodeId) -> Result<()>;

    /// Delete an edge.
    async fn delete_edge(&self, id: &EdgeId) -> Result<()>;

    /// Update a node's properties, importance, or label.
    async fn update_node(
        &self,
        id: &NodeId,
        label: Option<&str>,
        properties: Option<&serde_json::Value>,
        importance: Option<f32>,
    ) -> Result<()>;

    // ----- Query -----

    /// Find nodes by label substring, scoped to agent + shared.
    async fn find_nodes(
        &self,
        query: &str,
        agent_id: Option<&AgentId>,
        limit: usize,
    ) -> Result<Vec<KgNode>>;

    /// Traverse the graph from a starting node up to `max_hops` edges away.
    /// Returns (visited nodes, traversed edges), scoped to agent + shared.
    async fn traverse(
        &self,
        start: &NodeId,
        max_hops: usize,
        agent_id: Option<&AgentId>,
    ) -> Result<(Vec<KgNode>, Vec<KgEdge>)>;

    /// Get all edges incident to a node (both incoming and outgoing).
    async fn edges_of(&self, node_id: &NodeId) -> Result<Vec<KgEdge>>;

    /// Get all nodes owned by an agent (plus shared nodes).
    /// Used by consolidation to scan an agent's entire knowledge subgraph.
    async fn nodes_for_agent(
        &self,
        agent_id: &AgentId,
        include_shared: bool,
    ) -> Result<Vec<KgNode>>;

    // ----- Consolidation / Evolution (M4 will need these) -----

    /// Merge two nodes into one. All edges pointing to `absorbed` are redirected
    /// to `surviving`. The `absorbed` node is deleted. Properties are merged
    /// according to the provided strategy.
    ///
    /// Returns the list of edge IDs that were redirected.
    ///
    /// This is an atomic operation — if any step fails, nothing changes.
    async fn merge_nodes(
        &self,
        surviving: &NodeId,
        absorbed: &NodeId,
        merged_properties: serde_json::Value,
    ) -> Result<Vec<EdgeId>>;

    /// Batch-update edge weights. Used during consolidation to strengthen
    /// frequently co-accessed relationships.
    async fn batch_update_edge_weights(
        &self,
        updates: &[(EdgeId, f32)],
    ) -> Result<()>;

    /// Find node pairs whose labels or embeddings are similar.
    /// Returns (node_a, node_b, similarity_score) tuples, ordered by similarity desc.
    /// `min_similarity` filters pairs below the threshold.
    ///
    /// This powers the "detect candidates for merge" step in graph evolution.
    /// The implementation can use label fuzzy matching, property overlap, or
    /// vector similarity — whatever the backend supports.
    async fn find_similar_node_pairs(
        &self,
        agent_id: Option<&AgentId>,
        min_similarity: f32,
        limit: usize,
    ) -> Result<Vec<(KgNode, KgNode, f32)>>;

    /// Count nodes and edges for an agent (for observability metrics).
    async fn stats(&self, agent_id: Option<&AgentId>) -> Result<GraphStats>;

    /// Delete all nodes and edges. If `agent_id` is `Some`, only deletes that
    /// agent's data. If `None`, deletes everything (including shared).
    ///
    /// Returns (nodes_deleted, edges_deleted).
    async fn clear(&self, agent_id: Option<&AgentId>) -> Result<(u64, u64)>;
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

    /// List all files stored for an agent. Returns relative paths.
    async fn list(&self, agent_id: &AgentId) -> Result<Vec<String>>;
}

// ---------------------------------------------------------------------------
// Tag store
// ---------------------------------------------------------------------------

/// Storage for semantic tags with embeddings and co-occurrence tracking.
///
/// Tags are first-class entities in their own index, separate from memories.
/// The implementation uses LanceDB for vector search over tag embeddings
/// and SQLite for co-occurrence statistics and metadata.
///
/// EPA and query reshaping depend on this trait to access the tag space.
#[async_trait]
pub trait TagStore: Send + Sync {
    /// Upsert a tag. If a tag with the same canonical label + agent scope
    /// already exists, updates its frequency and vector (running average).
    /// Returns the tag's ID (existing or newly created).
    async fn upsert(&self, tag: &Tag) -> Result<TagId>;

    /// Batch upsert tags (used during document ingestion).
    async fn upsert_batch(&self, tags: &[Tag]) -> Result<Vec<TagId>>;

    /// Search tags by vector similarity. Returns top-k most similar tags.
    /// If `agent_id` is `Some`, includes that agent's private tags + shared tags.
    async fn search_by_vector(
        &self,
        vector: &[f32],
        agent_id: Option<&AgentId>,
        top_k: usize,
    ) -> Result<Vec<TagMatch>>;

    /// Find tags by label prefix/substring (for autocomplete and dedup checks).
    async fn find_by_label(
        &self,
        query: &str,
        agent_id: Option<&AgentId>,
        limit: usize,
    ) -> Result<Vec<Tag>>;

    /// Record that a set of tags co-occurred on the same memory entry.
    /// Updates the co-occurrence counts and PMI values incrementally.
    async fn record_cooccurrence(&self, tag_ids: &[TagId]) -> Result<()>;

    /// Get co-occurring tags for a given tag, ordered by PMI descending.
    async fn cooccurrences(&self, tag_id: &TagId, limit: usize) -> Result<Vec<TagCooccurrence>>;

    /// Get all tags for an agent (including shared). Used by EPA for clustering.
    async fn all_tags(&self, agent_id: Option<&AgentId>) -> Result<Vec<Tag>>;

    /// Get a single tag by ID.
    async fn get(&self, id: &TagId) -> Result<Option<Tag>>;

    /// Get multiple tags by ID.
    async fn get_batch(&self, ids: &[TagId]) -> Result<Vec<Tag>>;

    /// Count tags for an agent (including shared if agent_id is Some).
    async fn count(&self, agent_id: Option<&AgentId>) -> Result<u64>;
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

// ---------------------------------------------------------------------------
// Encoder (M2)
// ---------------------------------------------------------------------------

/// Text-to-vector encoding service.
///
/// The sole contract between the encoding layer and the rest of the system.
/// Upper layers call `encode_text` / `encode_batch` and receive vectors.
/// They never know (or care) whether the implementation calls Gemini, OpenAI,
/// or a local ONNX model.
///
/// Invariant: every vector returned has exactly `dimension()` elements.
#[async_trait]
pub trait Encoder: Send + Sync {
    /// Encode a single text into a vector.
    async fn encode_text(&self, text: &str) -> Result<Vec<f32>>;

    /// Encode a batch of texts. More efficient than calling `encode_text` in a loop
    /// because it batches API calls.
    ///
    /// Returns vectors in the same order as the input texts.
    async fn encode_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;

    /// The dimensionality of vectors produced by this encoder.
    /// Used by the storage layer to validate and configure indices.
    fn dimension(&self) -> usize;

    /// Human-readable name of the backend (e.g. "gemini-embedding-001").
    fn model_name(&self) -> &str;
}

// ---------------------------------------------------------------------------
// Retriever (M3)
// ---------------------------------------------------------------------------

/// Result of a retrieval query, including diffusion-discovered entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalResult {
    /// Final ranked results (after recall → rerank → diffusion merge).
    pub entries: Vec<ScoredMemory>,
    /// Entries discovered via LIF graph diffusion (subset that were not
    /// found by direct vector/BM25 search).
    pub diffusion_entries: Vec<ScoredMemory>,
    /// Time spent in each stage.
    pub latency: RetrievalLatency,
}

/// Per-stage latency breakdown.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RetrievalLatency {
    pub encode_ms: u64,
    pub epa_ms: u64,
    pub reshape_ms: u64,
    pub recall_ms: u64,
    pub rerank_ms: u64,
    pub diffusion_ms: u64,
    pub total_ms: u64,
}

/// Full retrieval pipeline: encode → recall → rerank → diffuse.
///
/// The pipeline auto-escalates search depth (ADR-012):
/// L1 cache → L2 vector+BM25 → L3 graph diffusion → archived scan.
/// Each stage only runs if the previous stage returned insufficient results.
#[async_trait]
pub trait Retriever: Send + Sync {
    /// Execute the full retrieval pipeline.
    async fn retrieve(
        &self,
        query: &str,
        agent_id: &AgentId,
    ) -> Result<RetrievalResult>;

    /// Recall only (skip rerank and diffusion). Useful for testing.
    async fn recall_only(
        &self,
        query: &str,
        agent_id: &AgentId,
        top_k: usize,
    ) -> Result<Vec<ScoredMemory>>;
}
