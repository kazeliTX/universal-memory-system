//! Global configuration for UMMS.
//!
//! All behaviour parameters live here — no magic numbers elsewhere.
//! Loaded from `umms.toml` at startup, overridable via environment variables.
//!
//! Convention: `UMMS_<SECTION>__<KEY>` env var overrides the corresponding
//! toml key. For example `UMMS_ENCODER__TIMEOUT_MS=3000` overrides
//! `[encoder] timeout_ms`.

use serde::Deserialize;

/// Root configuration.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct UmmsConfig {
    pub cache: CacheConfig,
    pub promotion: PromotionConfig,
    pub decay: DecayConfig,
    pub graph_evolution: GraphEvolutionConfig,
    pub encoder: EncoderConfig,
    pub retriever: RetrieverConfig,
    pub storage: StorageConfig,
    pub tag: TagConfig,
    pub epa: EpaConfig,
    pub reshaping: ReshapingConfig,
    pub observe: ObserveConfig,
    pub model_pool: ModelPoolConfig,
    pub http: HttpConfig,
    pub scheduler: SchedulerConfig,
}

// ---------------------------------------------------------------------------
// Cache (L0 / L1)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CacheConfig {
    pub l0: CacheLayerConfig,
    pub l1: CacheLayerConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CacheLayerConfig {
    /// Maximum number of entries before eviction kicks in.
    pub max_capacity: u64,
    /// Eviction strategy: "fifo" | "lru".
    pub eviction: String,
    /// Whether to keep entries alive for the duration of an active session.
    /// When true, time-based expiry is disabled while a session is open.
    pub session_aware: bool,
}

impl Default for CacheLayerConfig {
    fn default() -> Self {
        Self {
            max_capacity: 100,
            eviction: "lru".to_owned(),
            session_aware: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Promotion (L1 → L2)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct PromotionConfig {
    /// Minimum importance score to be eligible for L1→L2 promotion.
    pub min_importance: f32,
    /// Minimum number of accesses before a memory can be promoted.
    pub min_access_count: u32,
    /// Minimum age in hours before promotion (avoid promoting ephemeral data).
    pub min_age_hours: u32,
}

// ---------------------------------------------------------------------------
// Decay / Forgetting
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DecayConfig {
    pub enabled: bool,
    /// Daily decay rate (0.05 = 5% per day).
    pub rate: f32,
    /// Importance floor — never decays below this value.
    pub floor: f32,
    /// Importance below this triggers archival.
    pub archive_threshold: f32,
    /// Days of no access before archiving.
    pub archive_after_days: u32,
    /// Whether to ever delete original L4 files. Default: false.
    pub delete_originals: bool,
}

// ---------------------------------------------------------------------------
// Graph Evolution (M4)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct GraphEvolutionConfig {
    /// Minimum similarity score (0.0..=1.0) for two nodes to be merge candidates.
    pub min_similarity: f32,
    /// Maximum number of merges per evolution run (to limit blast radius).
    pub max_merge_per_run: usize,
    /// Factor by which frequently co-accessed edge weights are boosted.
    pub edge_boost_factor: f32,
}

impl Default for GraphEvolutionConfig {
    fn default() -> Self {
        Self {
            min_similarity: 0.8,
            max_merge_per_run: 10,
            edge_boost_factor: 1.1,
        }
    }
}

// ---------------------------------------------------------------------------
// Encoder (M2)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct EncoderConfig {
    pub provider: String,
    pub model: String,
    pub dimension: usize,
    pub timeout_ms: u64,
    pub max_retries: u32,
    /// Environment variable name holding the API key.
    pub api_key_env: String,
}

// ---------------------------------------------------------------------------
// Retriever (M3)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RetrieverConfig {
    /// BM25 weight in hybrid recall (vector_weight = 1 - bm25_weight).
    pub bm25_weight: f32,
    /// Number of candidates from hybrid recall.
    pub top_k_recall: usize,
    /// Number of candidates after reranking.
    pub top_k_rerank: usize,
    /// Final number of results returned to caller.
    pub top_k_final: usize,
    /// Maximum hops for LIF graph diffusion.
    pub lif_hops: usize,
    /// Maximum nodes visited during diffusion (prevents runaway on dense graphs).
    pub lif_max_nodes: usize,
    /// Decay factor per hop for LIF diffusion scoring (0.0..=1.0).
    /// Score = seed_score × decay_factor^hops × node_importance.
    pub lif_decay_factor: f32,
    /// Edge weight for "follows" edges between consecutive chunks.
    pub graph_follows_weight: f32,
    /// Base edge weight for "shares_tag" edges (multiplied by shared tag count).
    pub graph_tag_weight: f32,
    /// Minimum score threshold — results below this are filtered out.
    pub min_score: f32,
    /// Whether to auto-escalate search depth when results are insufficient.
    pub auto_escalate: bool,
    /// Minimum results needed before escalation triggers.
    pub escalation_threshold: usize,
}

// ---------------------------------------------------------------------------
// Storage paths
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct StorageConfig {
    /// Base directory for all persistent data.
    pub data_dir: String,
    /// LanceDB directory (relative to data_dir).
    pub vector_dir: String,
    /// SQLite graph database filename.
    pub graph_db: String,
    /// Agent context snapshots database filename.
    pub context_db: String,
    /// Raw file storage directory.
    pub files_dir: String,
    /// Graph backend: "cozo" (default) or "sqlite".
    pub graph_backend: String,
}

// ---------------------------------------------------------------------------
// Model pool (M5)
// ---------------------------------------------------------------------------

/// Configuration for the model pool — manages multiple LLM backends.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ModelPoolConfig {
    /// Model configurations.
    pub models: Vec<ModelConfig>,
    /// Task routing: which model ID to use for each task type.
    pub routing: TaskRoutingConfig,
}

/// Configuration for a single model backend.
#[derive(Debug, Clone, Deserialize)]
pub struct ModelConfig {
    /// Unique identifier for this model (e.g., "gemini-embed", "gemini-flash").
    pub id: String,
    /// Provider name (e.g., "gemini", "openai").
    pub provider: String,
    /// Actual model name sent to the API (e.g., "gemini-embedding-001").
    pub model_name: String,
    /// Environment variable name holding the API key.
    pub api_key_env: String,
    /// Task types this model supports (e.g., ["embedding"], ["generation", "chat"]).
    pub tasks: Vec<String>,
    /// Embedding dimension (for embedding models).
    pub dimension: Option<usize>,
    /// Maximum output tokens (for generative models).
    pub max_tokens: Option<usize>,
    /// Request timeout in milliseconds.
    pub timeout_ms: Option<u64>,
    /// Maximum retry attempts on transient failures.
    pub max_retries: Option<u32>,
}

/// Task-to-model routing configuration.
///
/// Each field is a model ID that should handle the corresponding task type.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TaskRoutingConfig {
    /// Model ID for embedding tasks.
    pub embedding: String,
    /// Model ID for text generation tasks.
    pub generation: String,
    /// Model ID for reranking tasks.
    pub reranking: String,
    /// Model ID for entity extraction tasks.
    pub entity_extraction: String,
    /// Model ID for chat/conversation tasks.
    pub chat: String,
}

impl Default for ModelPoolConfig {
    fn default() -> Self {
        Self {
            models: vec![
                ModelConfig {
                    id: "gemini-embed".to_owned(),
                    provider: "gemini".to_owned(),
                    model_name: "gemini-embedding-001".to_owned(),
                    api_key_env: "GEMINI_API_KEY".to_owned(),
                    tasks: vec!["embedding".to_owned()],
                    dimension: Some(3072),
                    max_tokens: None,
                    timeout_ms: Some(10_000),
                    max_retries: Some(2),
                },
                ModelConfig {
                    id: "gemini-flash".to_owned(),
                    provider: "gemini".to_owned(),
                    model_name: "gemini-2.0-flash".to_owned(),
                    api_key_env: "GEMINI_API_KEY".to_owned(),
                    tasks: vec![
                        "generation".to_owned(),
                        "entity_extraction".to_owned(),
                        "chat".to_owned(),
                    ],
                    dimension: None,
                    max_tokens: Some(8192),
                    timeout_ms: Some(30_000),
                    max_retries: Some(2),
                },
            ],
            routing: TaskRoutingConfig::default(),
        }
    }
}

impl Default for TaskRoutingConfig {
    fn default() -> Self {
        Self {
            embedding: "gemini-embed".to_owned(),
            generation: "gemini-flash".to_owned(),
            reranking: "gemini-embed".to_owned(),
            entity_extraction: "gemini-flash".to_owned(),
            chat: "gemini-flash".to_owned(),
        }
    }
}

// ---------------------------------------------------------------------------
// HTTP server
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct HttpConfig {
    /// Bind address for the HTTP server.
    pub host: String,
    /// TCP port for the HTTP server.
    pub port: u16,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_owned(),
            port: 8720,
        }
    }
}

// ---------------------------------------------------------------------------
// Scheduler
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SchedulerConfig {
    /// Whether the scheduler engine is active.
    pub enabled: bool,
    /// Interval in seconds between scheduler poll cycles.
    pub check_interval_secs: u64,
    /// SQLite database filename for scheduler state (relative to data_dir).
    pub db: String,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            check_interval_secs: 30,
            db: "scheduler.sqlite".to_owned(),
        }
    }
}

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------


impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            l0: CacheLayerConfig {
                max_capacity: 50,
                eviction: "fifo".to_owned(),
                session_aware: false,
            },
            l1: CacheLayerConfig {
                max_capacity: 200,
                eviction: "lru".to_owned(),
                session_aware: true,
            },
        }
    }
}

impl Default for PromotionConfig {
    fn default() -> Self {
        Self {
            min_importance: 0.7,
            min_access_count: 3,
            min_age_hours: 24,
        }
    }
}

impl Default for DecayConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            rate: 0.05,
            floor: 0.01,
            archive_threshold: 0.05,
            archive_after_days: 90,
            delete_originals: false,
        }
    }
}

impl Default for EncoderConfig {
    fn default() -> Self {
        Self {
            provider: "gemini".to_owned(),
            model: "gemini-embedding-001".to_owned(),
            dimension: 3072,
            timeout_ms: 5000,
            max_retries: 2,
            api_key_env: "GEMINI_API_KEY".to_owned(),
        }
    }
}

impl Default for RetrieverConfig {
    fn default() -> Self {
        Self {
            bm25_weight: 0.3,
            top_k_recall: 200,
            top_k_rerank: 20,
            top_k_final: 10,
            lif_hops: 2,
            lif_max_nodes: 100,
            lif_decay_factor: 0.5,
            graph_follows_weight: 1.0,
            graph_tag_weight: 0.5,
            min_score: 0.0,
            auto_escalate: true,
            escalation_threshold: 3,
        }
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            data_dir: ".umms".to_owned(),
            vector_dir: "vectors".to_owned(),
            graph_db: "graph.sqlite".to_owned(),
            context_db: "context.sqlite".to_owned(),
            files_dir: "files".to_owned(),
            graph_backend: "cozo".to_owned(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tag system
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TagConfig {
    /// Whether the tag system is active.
    pub enabled: bool,
    /// Whether to auto-extract tags during document ingestion.
    pub auto_extract: bool,
    /// LanceDB directory for tag vectors (relative to data_dir).
    pub vector_dir: String,
    /// SQLite database for co-occurrence matrix (relative to data_dir).
    pub cooc_db: String,
    /// Tokenizer strategy: "jieba" (default, Chinese+English),
    /// "llm" (LLM-based key term extraction), "whitespace" (English only).
    pub tokenizer: String,
}

impl Default for TagConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            auto_extract: true,
            vector_dir: "tag_vectors".to_owned(),
            cooc_db: "tag_cooc.sqlite".to_owned(),
            tokenizer: "jieba".to_owned(),
        }
    }
}

// ---------------------------------------------------------------------------
// EPA (Embedding Projection Analysis)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct EpaConfig {
    /// Whether EPA is active.
    pub enabled: bool,
    /// Number of nearest tags to consider for activation.
    pub activation_top_k: usize,
    /// Minimum cosine similarity for a tag to be "activated".
    pub activation_threshold: f32,
    /// Number of K-Means clusters.
    pub num_clusters: usize,
    /// K-Means max iterations.
    pub kmeans_iterations: usize,
    /// Minimum cluster weight fraction to count as "significant".
    pub cluster_significance_threshold: f32,
    /// Number of PCA semantic axes to extract.
    pub num_axes: usize,
    /// Power iteration count for PCA.
    pub pca_iterations: usize,
    /// Alpha blending parameters.
    pub alpha_base: f32,
    pub alpha_depth_weight: f32,
    pub alpha_resonance_weight: f32,
    pub alpha_importance_weight: f32,
    pub alpha_min: f32,
    pub alpha_max: f32,
}

impl Default for EpaConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            activation_top_k: 50,
            activation_threshold: 0.3,
            num_clusters: 5,
            kmeans_iterations: 30,
            cluster_significance_threshold: 0.1,
            num_axes: 3,
            pca_iterations: 50,
            alpha_base: 0.05,
            alpha_depth_weight: 0.15,
            alpha_resonance_weight: 0.10,
            alpha_importance_weight: 0.10,
            alpha_min: 0.05,
            alpha_max: 0.40,
        }
    }
}

// ---------------------------------------------------------------------------
// Query reshaping
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ReshapingConfig {
    /// Whether query reshaping is active.
    pub enabled: bool,
    /// Residual pyramid: number of tags at level 0 (fine, highest weight).
    pub level0_count: usize,
    /// Number of tags at level 1 (medium weight).
    pub level1_count: usize,
    /// Number of co-occurring tags to expand at level 2.
    pub cooc_expansion_k: usize,
    /// Pyramid level weights [level0, level1, level2]. Should sum to 1.0.
    pub pyramid_weights: [f32; 3],
}

impl Default for ReshapingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            level0_count: 5,
            level1_count: 15,
            cooc_expansion_k: 10,
            pyramid_weights: [0.6, 0.3, 0.1],
        }
    }
}

// ---------------------------------------------------------------------------
// Observe (tracing / logging)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ObserveConfig {
    /// Whether Prometheus metrics collection is active.
    pub metrics_enabled: bool,
    /// Whether the tracing subscriber is initialised.
    pub tracing_enabled: bool,
    /// Dashboard HTTP port. 0 = disabled.
    pub dashboard_port: u16,
    /// `EnvFilter`-compatible directive string.
    ///
    /// Examples: `"info"`, `"info,lance=warn,lancedb=warn"`,
    /// `"umms=debug,info"`.
    pub log_level: String,
    /// Output format: `"json"` for structured JSON lines, `"pretty"` for
    /// human-readable coloured output.
    pub log_format: String,
    /// Log file path. Empty string means stdout.
    pub log_file: String,
}

impl Default for ObserveConfig {
    fn default() -> Self {
        Self {
            metrics_enabled: true,
            tracing_enabled: true,
            dashboard_port: 8721,
            log_level: "info,lance=warn,lancedb=warn".to_owned(),
            log_format: "pretty".to_owned(),
            log_file: String::new(),
        }
    }
}

/// Load configuration from `umms.toml` (if present) merged with env vars.
///
/// Priority: env vars > umms.toml > defaults.
pub fn load_config() -> UmmsConfig {
    // Search for umms.toml: CWD first, then walk up to find project root.
    let config_path = find_config_file("umms.toml");

    let mut builder = config::Config::builder();

    if let Some(path) = &config_path {
        builder = builder.add_source(
            config::File::from(path.as_path())
                .format(config::FileFormat::Toml)
                .required(false),
        );
    } else {
        builder = builder.add_source(
            config::File::with_name("umms")
                .format(config::FileFormat::Toml)
                .required(false),
        );
    }

    builder = builder.add_source(
        config::Environment::with_prefix("UMMS")
            .separator("__")
            .try_parsing(true),
    );

    match builder.build() {
        Ok(cfg) => cfg.try_deserialize().unwrap_or_default(),
        Err(_) => UmmsConfig::default(),
    }
}

/// Walk up from CWD to find a config file (handles Tauri launching from src-tauri/).
fn find_config_file(filename: &str) -> Option<std::path::PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let candidate = dir.join(filename);
        if candidate.exists() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_sane() {
        let cfg = UmmsConfig::default();
        assert_eq!(cfg.cache.l0.max_capacity, 50);
        assert_eq!(cfg.cache.l1.max_capacity, 200);
        assert!((cfg.promotion.min_importance - 0.7).abs() < f32::EPSILON);
        assert!(!cfg.decay.enabled);
        assert!(!cfg.decay.delete_originals);
        assert_eq!(cfg.encoder.dimension, 3072);
        assert!((cfg.retriever.bm25_weight - 0.3).abs() < f32::EPSILON);
        assert_eq!(cfg.retriever.top_k_recall, 200);
        assert_eq!(cfg.retriever.lif_hops, 2);
        assert!(cfg.retriever.auto_escalate);
        // observe
        assert!(cfg.observe.metrics_enabled);
        assert!(cfg.observe.tracing_enabled);
        assert_eq!(cfg.observe.dashboard_port, 8721);
        assert_eq!(cfg.observe.log_format, "pretty");
        assert!(cfg.observe.log_level.contains("lance=warn"));
        assert!(cfg.observe.log_file.is_empty());
        // model pool
        assert_eq!(cfg.model_pool.models.len(), 2);
        assert_eq!(cfg.model_pool.routing.embedding, "gemini-embed");
        assert_eq!(cfg.model_pool.routing.generation, "gemini-flash");
    }

    #[test]
    fn load_config_returns_defaults_without_file() {
        let cfg = load_config();
        assert_eq!(cfg.encoder.provider, "gemini");
    }
}
