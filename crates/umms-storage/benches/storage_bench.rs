//! Criterion benchmarks for core storage paths.
//!
//! Run: cargo bench -p umms-storage
//!
//! These establish a performance baseline for the hot paths:
//! - L0/L1 cache read/write
//! - LanceDB vector insert + search
//! - SQLite graph traversal
//! - Agent context switch (snapshot → evict → restore)

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use tokio::runtime::Runtime;

use umms_core::traits::{AgentContextManager, KnowledgeGraphStore, MemoryCache, VectorStore};
use umms_core::types::*;
use umms_storage::cache::MokaMemoryCache;
use umms_storage::graph::SqliteGraphStore;
use umms_storage::isolation::{AgentSwitcher, SqliteAgentContextManager};
use umms_storage::vector::LanceVectorStore;

const BENCH_DIM: usize = 8;

fn make_entry(agent: &AgentId, idx: usize) -> MemoryEntry {
    let mut vec = vec![0.0f32; BENCH_DIM];
    for (d, v) in vec.iter_mut().enumerate() {
        *v = ((idx * 7 + d * 13) % 100) as f32 / 100.0;
    }
    MemoryEntryBuilder::new(agent.clone(), Modality::Text)
        .layer(MemoryLayer::WorkingMemory)
        .content_text(format!("bench entry {idx}"))
        .vector(vec)
        .importance(0.5)
        .build()
}

// ---------------------------------------------------------------------------
// Cache benchmarks
// ---------------------------------------------------------------------------

fn bench_cache_put_get(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let cache = MokaMemoryCache::new();
    let agent = AgentId::from_str("bench-agent").unwrap();

    // Pre-populate 100 entries
    let entries: Vec<MemoryEntry> = (0..100).map(|i| make_entry(&agent, i)).collect();
    for e in &entries {
        rt.block_on(cache.put(&agent, &e.id, e.clone()));
    }

    c.bench_function("cache_put", |b| {
        let entry = make_entry(&agent, 999);
        b.iter(|| {
            rt.block_on(cache.put(&agent, &entry.id, black_box(entry.clone())));
        });
    });

    c.bench_function("cache_get_hit", |b| {
        let id = entries[50].id.clone();
        b.iter(|| {
            rt.block_on(cache.get(&agent, black_box(&id)));
        });
    });

    c.bench_function("cache_get_miss", |b| {
        let missing = MemoryId::new();
        b.iter(|| {
            rt.block_on(cache.get(&agent, black_box(&missing)));
        });
    });
}

fn bench_cache_evict(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("cache_evict_100_entries", |b| {
        b.iter_with_setup(
            || {
                let cache = MokaMemoryCache::new();
                let agent = AgentId::from_str("evict-agent").unwrap();
                for i in 0..100 {
                    let e = make_entry(&agent, i);
                    let id = e.id.clone();
                    rt.block_on(cache.put(&agent, &id, e));
                }
                (cache, agent)
            },
            |(cache, agent)| {
                rt.block_on(cache.evict_agent(black_box(&agent)));
            },
        );
    });
}

// ---------------------------------------------------------------------------
// Vector store benchmarks
// ---------------------------------------------------------------------------

fn bench_vector_insert(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = rt
        .block_on(LanceVectorStore::new(
            dir.path().to_str().unwrap(),
            BENCH_DIM,
        ))
        .unwrap();
    let agent = AgentId::from_str("vec-bench").unwrap();

    c.bench_function("vector_insert_single", |b| {
        let mut idx = 0usize;
        b.iter(|| {
            let entry = make_entry(&agent, idx);
            rt.block_on(store.insert(black_box(&entry))).unwrap();
            idx += 1;
        });
    });
}

fn bench_vector_search(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = rt
        .block_on(LanceVectorStore::new(
            dir.path().to_str().unwrap(),
            BENCH_DIM,
        ))
        .unwrap();
    let agent = AgentId::from_str("vec-bench").unwrap();

    // Seed 500 entries
    let entries: Vec<MemoryEntry> = (0..500).map(|i| make_entry(&agent, i)).collect();
    rt.block_on(store.insert_batch(&entries)).unwrap();

    let query_vec: Vec<f32> = (0..BENCH_DIM)
        .map(|d| (d as f32) / BENCH_DIM as f32)
        .collect();

    c.bench_function("vector_search_top10_in_500", |b| {
        b.iter(|| {
            rt.block_on(store.search(black_box(&agent), &query_vec, 10, false))
                .unwrap();
        });
    });

    c.bench_function("vector_search_top10_include_shared", |b| {
        b.iter(|| {
            rt.block_on(store.search(black_box(&agent), &query_vec, 10, true))
                .unwrap();
        });
    });
}

// ---------------------------------------------------------------------------
// Graph store benchmarks
// ---------------------------------------------------------------------------

fn bench_graph_traverse(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("bench.sqlite");
    let graph = SqliteGraphStore::new(&db_path).unwrap();

    // Build a small graph: 50 nodes, ~100 edges
    let mut node_ids = Vec::new();
    for i in 0..50 {
        let node = KgNode {
            id: NodeId::new(),
            agent_id: None,
            node_type: KgNodeType::Concept,
            label: format!("concept-{i}"),
            properties: serde_json::json!({}),
            importance: 0.5,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let nid = rt.block_on(graph.add_node(&node)).unwrap();
        node_ids.push(nid);
    }

    for i in 0..50 {
        for offset in [1, 3, 7] {
            let tgt = (i + offset) % 50;
            if i == tgt {
                continue;
            }
            let edge = KgEdge {
                id: EdgeId::new(),
                source_id: node_ids[i].clone(),
                target_id: node_ids[tgt].clone(),
                relation: "related_to".to_owned(),
                weight: 1.0,
                agent_id: None,
                created_at: chrono::Utc::now(),
            };
            let _ = rt.block_on(graph.add_edge(&edge));
        }
    }

    let start = node_ids[0].clone();

    c.bench_function("graph_traverse_2_hops_50_nodes", |b| {
        b.iter(|| {
            rt.block_on(graph.traverse(black_box(&start), 2, None))
                .unwrap();
        });
    });

    c.bench_function("graph_traverse_3_hops_50_nodes", |b| {
        b.iter(|| {
            rt.block_on(graph.traverse(black_box(&start), 3, None))
                .unwrap();
        });
    });

    c.bench_function("graph_find_nodes", |b| {
        b.iter(|| {
            rt.block_on(graph.find_nodes(black_box("concept-2"), None, 10))
                .unwrap();
        });
    });
}

// ---------------------------------------------------------------------------
// Agent switch benchmark
// ---------------------------------------------------------------------------

fn bench_agent_switch(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("agent_switch_20_entries", |b| {
        b.iter_with_setup(
            || {
                let cache = MokaMemoryCache::new();
                let dir = tempfile::tempdir().unwrap();
                let ctx_db = SqliteAgentContextManager::new(dir.path().join("ctx.sqlite")).unwrap();
                let switcher = AgentSwitcher::new(cache, ctx_db);
                let from = AgentId::from_str("agent-a").unwrap();
                let to = AgentId::from_str("agent-b").unwrap();

                // Populate agent-a with 20 entries
                for i in 0..20 {
                    let e = make_entry(&from, i);
                    let id = e.id.clone();
                    rt.block_on(switcher.cache().put(&from, &id, e));
                }

                (switcher, from, to, dir)
            },
            |(switcher, from, to, _dir)| {
                rt.block_on(switcher.switch(black_box(&from), black_box(&to)))
                    .unwrap();
            },
        );
    });
}

// ---------------------------------------------------------------------------
// Groups
// ---------------------------------------------------------------------------

criterion_group!(cache_benches, bench_cache_put_get, bench_cache_evict,);

criterion_group!(vector_benches, bench_vector_insert, bench_vector_search,);

criterion_group!(graph_benches, bench_graph_traverse,);

criterion_group!(agent_benches, bench_agent_switch,);

criterion_main!(cache_benches, vector_benches, graph_benches, agent_benches);
