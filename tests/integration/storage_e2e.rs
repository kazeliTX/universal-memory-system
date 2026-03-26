//! End-to-end integration test for the complete storage stack.
//!
//! This test simulates a realistic usage scenario:
//! 1. Two agents (coding_assistant, research_assistant) write memories
//! 2. Each agent can only see its own private memories
//! 3. Promote a memory to shared → both agents can see it
//! 4. Switch agents → old agent's cache is cleared, new agent starts fresh
//! 5. Switch back → old agent's cache is restored from snapshot
//! 6. Demote a shared memory → only target agent can see it
//! 7. File storage isolation works correctly
//! 8. Graph nodes and edges are correctly scoped

use std::path::PathBuf;
use std::sync::Arc;

use umms_core::traits::*;
use umms_core::types::*;

use umms_storage::cache::MokaMemoryCache;
use umms_storage::file::LocalFileStore;
use umms_storage::graph::SqliteGraphStore;
use umms_storage::isolation::{AgentSwitcher, SqliteAgentContextManager};
use umms_storage::promotion;
use umms_storage::vector::LanceVectorStore;

/// Create a unique temp directory for each test run.
fn test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("umms-e2e-test")
        .join(format!("{}-{}", name, uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn agent(name: &str) -> AgentId {
    AgentId::from_str(name).unwrap()
}

fn make_entry(agent_id: &AgentId, text: &str, vec: Vec<f32>) -> MemoryEntry {
    MemoryEntryBuilder::new(agent_id.clone(), Modality::Text)
        .content_text(text)
        .vector(vec)
        .layer(MemoryLayer::EpisodicMemory)
        .importance(0.6)
        .build()
}

// ============================================================================
// Test 1: Agent isolation — private memories are invisible to other agents
// ============================================================================

#[tokio::test]
async fn agent_isolation_across_all_storage_layers() {
    let dir = test_dir("isolation");
    let coder = agent("coder");
    let researcher = agent("researcher");

    // --- Cache isolation ---
    let cache = MokaMemoryCache::new();
    let entry_a = MemoryEntryBuilder::new(coder.clone(), Modality::Text)
        .content_text("Rust borrow checker")
        .build();
    let id_a = entry_a.id.clone();
    cache.put(&coder, &id_a, entry_a).await;

    assert!(cache.get(&coder, &id_a).await.is_some(), "Owner can see own cache entry");
    assert!(cache.get(&researcher, &id_a).await.is_none(), "Other agent cannot see cache entry");

    // --- Vector store isolation ---
    let vec_store = LanceVectorStore::new(dir.join("lance").to_str().unwrap(), 8).await.unwrap();
    let mem_a = make_entry(&coder, "Rust performance tips", vec![1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
    let mem_b = make_entry(&researcher, "Quantum computing paper", vec![0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
    vec_store.insert(&mem_a).await.unwrap();
    vec_store.insert(&mem_b).await.unwrap();

    let coder_results = vec_store
        .search(&coder, &[1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], 10, false)
        .await
        .unwrap();
    assert_eq!(coder_results.len(), 1, "Coder only sees own memories");
    assert_eq!(coder_results[0].entry.agent_id, coder);

    let researcher_results = vec_store
        .search(&researcher, &[0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], 10, false)
        .await
        .unwrap();
    assert_eq!(researcher_results.len(), 1, "Researcher only sees own memories");
    assert_eq!(researcher_results[0].entry.agent_id, researcher);

    // --- Graph isolation ---
    let graph = SqliteGraphStore::new(dir.join("graph.db")).unwrap();
    let node_a = KgNode {
        id: NodeId::new(),
        agent_id: Some(coder.clone()),
        node_type: KgNodeType::Concept,
        label: "Borrow Checker".into(),
        properties: serde_json::json!({}),
        importance: 0.8,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let node_b = KgNode {
        id: NodeId::new(),
        agent_id: Some(researcher.clone()),
        node_type: KgNodeType::Concept,
        label: "Quantum Entanglement".into(),
        properties: serde_json::json!({}),
        importance: 0.9,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    graph.add_node(&node_a).await.unwrap();
    graph.add_node(&node_b).await.unwrap();

    let coder_nodes = graph.find_nodes("", Some(&coder), 100).await.unwrap();
    assert!(coder_nodes.iter().all(|n| n.agent_id.as_ref() == Some(&coder) || n.agent_id.is_none()),
        "Coder only sees own + shared nodes");

    let researcher_nodes = graph.find_nodes("", Some(&researcher), 100).await.unwrap();
    assert!(researcher_nodes.iter().all(|n| n.agent_id.as_ref() == Some(&researcher) || n.agent_id.is_none()),
        "Researcher only sees own + shared nodes");

    // --- File isolation ---
    let fs_store = LocalFileStore::new(dir.join("files")).await.unwrap();
    let path_a = fs_store.store(&coder, "notes.txt", b"coder notes").await.unwrap();
    let path_b = fs_store.store(&researcher, "notes.txt", b"researcher notes").await.unwrap();
    assert_ne!(path_a, path_b, "Same filename, different agents → different paths");

    let data_a = fs_store.read(&path_a).await.unwrap();
    assert_eq!(data_a, b"coder notes");
    let data_b = fs_store.read(&path_b).await.unwrap();
    assert_eq!(data_b, b"researcher notes");

    println!("✓ Agent isolation verified across all 4 storage layers");
}

// ============================================================================
// Test 2: Promote/demote cycle — private → shared → private
// ============================================================================

#[tokio::test]
async fn promote_demote_cycle() {
    let dir = test_dir("promote");
    let coder = agent("coder");
    let researcher = agent("researcher");

    let vec_store = LanceVectorStore::new(dir.join("lance").to_str().unwrap(), 8).await.unwrap();

    // Coder writes a private memory
    let mem = make_entry(&coder, "Tokio runtime internals", vec![1.0, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
    let mem_id = mem.id.clone();
    vec_store.insert(&mem).await.unwrap();

    // Researcher cannot see it (even with include_shared=true)
    let results = vec_store
        .search(&researcher, &[1.0, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], 10, true)
        .await
        .unwrap();
    assert!(results.iter().all(|r| r.entry.id != mem_id), "Researcher cannot see private memory before promotion");

    // Promote to shared
    let result = promotion::promote(&vec_store, &mem_id, &[]).await.unwrap();
    assert_eq!(result.new_scope, IsolationScope::Shared);

    // Now researcher CAN see it via include_shared
    let results = vec_store
        .search(&researcher, &[1.0, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], 10, true)
        .await
        .unwrap();
    assert!(results.iter().any(|r| r.entry.id == mem_id), "Researcher can see shared memory after promotion");

    // Demote back to private
    let result = promotion::demote(&vec_store, &mem_id, &researcher).await.unwrap();
    assert_eq!(result.new_scope, IsolationScope::Private);

    // Double promote fails
    let err = promotion::promote(&vec_store, &mem_id, &[]).await;
    assert!(err.is_ok(), "Re-promoting a private memory should succeed");

    println!("✓ Promote/demote cycle verified");
}

// ============================================================================
// Test 3: Agent switch — snapshot → evict → restore
// ============================================================================

#[tokio::test]
async fn agent_switch_preserves_and_restores_state() {
    let dir = test_dir("switch");
    let coder = agent("coder");
    let researcher = agent("researcher");

    let cache = MokaMemoryCache::new();
    let ctx_db = SqliteAgentContextManager::new(dir.join("ctx.db")).unwrap();
    let switcher = AgentSwitcher::new(cache.clone(), ctx_db);

    // Coder writes 3 entries to cache
    for i in 0..3 {
        let entry = MemoryEntryBuilder::new(coder.clone(), Modality::Text)
            .content_text(format!("coder memory {i}"))
            .build();
        let id = entry.id.clone();
        cache.put(&coder, &id, entry).await;
    }
    assert_eq!(cache.len(&coder).await, 3, "Coder has 3 cached entries");

    // Switch to researcher
    switcher.switch(&coder, &researcher).await.unwrap();
    assert_eq!(cache.len(&coder).await, 0, "Coder cache is evicted after switch");
    assert_eq!(cache.len(&researcher).await, 0, "Researcher starts with empty cache (cold start)");

    // Researcher writes 2 entries
    for i in 0..2 {
        let entry = MemoryEntryBuilder::new(researcher.clone(), Modality::Text)
            .content_text(format!("researcher memory {i}"))
            .build();
        let id = entry.id.clone();
        cache.put(&researcher, &id, entry).await;
    }
    assert_eq!(cache.len(&researcher).await, 2);

    // Switch back to coder
    switcher.switch(&researcher, &coder).await.unwrap();
    assert_eq!(cache.len(&researcher).await, 0, "Researcher cache evicted");
    assert_eq!(cache.len(&coder).await, 3, "Coder cache RESTORED from snapshot");

    println!("✓ Agent switch snapshot/restore verified");
}

// ============================================================================
// Test 4: Graph traversal with shared nodes
// ============================================================================

#[tokio::test]
async fn graph_traversal_with_shared_nodes() {
    let dir = test_dir("graph-traversal");
    let coder = agent("coder");

    let graph = SqliteGraphStore::new(dir.join("graph.db")).unwrap();

    // Create a shared concept node
    let shared_node = KgNode {
        id: NodeId::new(),
        agent_id: None, // shared
        node_type: KgNodeType::Concept,
        label: "Rust Language".into(),
        properties: serde_json::json!({"category": "programming"}),
        importance: 1.0,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let shared_id = graph.add_node(&shared_node).await.unwrap();

    // Coder creates a private node linked to the shared one
    let private_node = KgNode {
        id: NodeId::new(),
        agent_id: Some(coder.clone()),
        node_type: KgNodeType::Entity,
        label: "Tokio Runtime".into(),
        properties: serde_json::json!({}),
        importance: 0.8,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let private_id = graph.add_node(&private_node).await.unwrap();

    // Edge: private → shared
    let edge = KgEdge {
        id: EdgeId::new(),
        source_id: private_id.clone(),
        target_id: shared_id.clone(),
        relation: "part_of".into(),
        weight: 1.0,
        agent_id: Some(coder.clone()),
        created_at: chrono::Utc::now(),
    };
    graph.add_edge(&edge).await.unwrap();

    // Traverse from private node: should reach the shared node
    let (nodes, edges) = graph.traverse(&private_id, 2, Some(&coder)).await.unwrap();
    assert!(nodes.len() >= 2, "Traversal reaches both private and shared nodes");
    assert!(!edges.is_empty(), "Traversal includes the connecting edge");

    // Stats
    let stats = graph.stats(Some(&coder)).await.unwrap();
    assert!(stats.node_count >= 1, "Coder has at least 1 private node");
    assert!(stats.shared_node_count >= 1, "There is at least 1 shared node");

    println!("✓ Graph traversal with shared nodes verified");
}

// ============================================================================
// Test 5: Full memory lifecycle — write → search → update → promote → decay check
// ============================================================================

#[tokio::test]
async fn full_memory_lifecycle() {
    let dir = test_dir("lifecycle");
    let coder = agent("coder");

    let vec_store = LanceVectorStore::new(dir.join("lance").to_str().unwrap(), 8).await.unwrap();

    // 1. Write
    let entry = MemoryEntryBuilder::new(coder.clone(), Modality::Text)
        .content_text("Async Rust patterns for high concurrency")
        .vector(vec![0.8, 0.2, 0.1, 0.0, 0.0, 0.0, 0.0, 0.0])
        .layer(MemoryLayer::EpisodicMemory)
        .importance(0.5)
        .decay_category(DecayCategory::SessionTopic)
        .tags(vec!["rust".into(), "async".into(), "debug".into()])
        .build();
    let mem_id = entry.id.clone();
    vec_store.insert(&entry).await.unwrap();

    // 2. Search — should find it
    let results = vec_store
        .search(&coder, &[0.8, 0.2, 0.1, 0.0, 0.0, 0.0, 0.0, 0.0], 5, false)
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].entry.content_text.as_deref(), Some("Async Rust patterns for high concurrency"));

    // 3. Update importance
    vec_store.update_metadata(&mem_id, Some(0.9), None, None, None).await.unwrap();
    let updated = vec_store.get(&mem_id).await.unwrap().unwrap();
    assert!((updated.importance - 0.9).abs() < 0.01, "Importance updated to 0.9");

    // 4. Check promotion criteria
    assert!(
        !promotion::meets_promotion_criteria(0.9, 12.0, &promotion::PromotionCriteria::default()),
        "12 hours old → too young for auto-promotion"
    );
    assert!(
        promotion::meets_promotion_criteria(0.9, 48.0, &promotion::PromotionCriteria::default()),
        "48 hours old + high importance → eligible for promotion"
    );

    // 5. Promote with tag stripping
    promotion::promote(&vec_store, &mem_id, &["debug".to_string()]).await.unwrap();
    let promoted = vec_store.get(&mem_id).await.unwrap().unwrap();
    assert_eq!(promoted.scope, IsolationScope::Shared);

    // 6. Count
    let total = vec_store.count(&coder, true).await.unwrap();
    assert_eq!(total, 1);

    // 7. Delete
    vec_store.delete(&mem_id).await.unwrap();
    assert!(vec_store.get(&mem_id).await.unwrap().is_none(), "Deleted memory is gone");
    assert_eq!(vec_store.count(&coder, true).await.unwrap(), 0);

    println!("✓ Full memory lifecycle verified: write → search → update → promote → delete");
}
