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
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct UmmsConfig {
    pub cache: CacheConfig,
    pub promotion: PromotionConfig,
    pub decay: DecayConfig,
    pub encoder: EncoderConfig,
    pub retriever: RetrieverConfig,
    pub storage: StorageConfig,
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
}

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

impl Default for UmmsConfig {
    fn default() -> Self {
        Self {
            cache: CacheConfig::default(),
            promotion: PromotionConfig::default(),
            decay: DecayConfig::default(),
            encoder: EncoderConfig::default(),
            retriever: RetrieverConfig::default(),
            storage: StorageConfig::default(),
        }
    }
}

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
            model: "text-embedding-004".to_owned(),
            dimension: 768,
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
        assert_eq!(cfg.encoder.dimension, 768);
        assert!((cfg.retriever.bm25_weight - 0.3).abs() < f32::EPSILON);
        assert_eq!(cfg.retriever.top_k_recall, 200);
        assert_eq!(cfg.retriever.lif_hops, 2);
        assert!(cfg.retriever.auto_escalate);
    }

    #[test]
    fn load_config_returns_defaults_without_file() {
        let cfg = load_config();
        assert_eq!(cfg.encoder.provider, "gemini");
    }
}
