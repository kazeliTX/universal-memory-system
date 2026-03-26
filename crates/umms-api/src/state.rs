//! Shared application state — the single source of truth for all storage backends.
//!
//! Both Tauri Commands and Axum Handlers receive `Arc<AppState>`. This ensures
//! zero data synchronisation overhead: one process, one state, two access paths.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use umms_core::config;
use umms_core::error::UmmsError;
use umms_core::traits::{Encoder, TagStore};
use umms_encoder::{GeminiConfig, GeminiEncoder};
use umms_observe::AuditLog;
use umms_retriever::pipeline::RetrievalPipeline;
use umms_retriever::recall::Bm25Index;
use umms_storage::cache::MokaMemoryCache;
use umms_storage::file::LocalFileStore;
use umms_storage::graph::SqliteGraphStore;
use umms_storage::tag::CompositeTagStore;
use umms_storage::vector::LanceVectorStore;

/// Configuration for initialising [`AppState`].
///
/// All paths are derived from a single `data_dir` root. This avoids scattered
/// config and makes it trivial to wipe state: `rm -rf {data_dir}`.
pub struct AppConfig {
    /// Root directory for all persistent data (e.g. `~/.umms`).
    pub data_dir: PathBuf,
    /// Vector embedding dimension. Must match the encoder output.
    pub vector_dim: usize,
    /// Audit log ring buffer capacity.
    pub audit_capacity: usize,
}

impl AppConfig {
    /// Sensible defaults for development.
    ///
    /// Uses 3072-dim vectors (Gemini embedding-001 native output).
    /// Old 8-dim data is incompatible — call `/api/demo/clear` after upgrading.
    #[must_use]
    pub fn dev() -> Self {
        let home = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .unwrap_or_else(|_| ".".to_owned());

        Self {
            data_dir: PathBuf::from(home).join(".umms").join("dev"),
            vector_dim: 3072,
            audit_capacity: 10_000,
        }
    }

    /// Production config with a custom data directory.
    #[must_use]
    pub fn production(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            vector_dim: 3072,
            audit_capacity: 50_000,
        }
    }
}

/// Central application state shared across all access paths (GUI, HTTP, background tasks).
///
/// Invariant: constructed once at startup, never replaced. Storage backends are
/// internally thread-safe (`Send + Sync`), so concurrent access from Tauri IPC
/// and Axum handlers is safe without external locking.
pub struct AppState {
    pub cache: MokaMemoryCache,
    pub vector: Arc<LanceVectorStore>,
    pub graph: Arc<SqliteGraphStore>,
    pub files: LocalFileStore,
    pub audit: AuditLog,
    /// Encoder is `None` when `GEMINI_API_KEY` is not set (dev mode without API).
    pub encoder: Option<Arc<GeminiEncoder>>,
    /// BM25 full-text index (always available).
    pub bm25: Arc<Bm25Index>,
    /// Tag store is `None` when tag system is disabled.
    pub tag_store: Option<Arc<dyn TagStore>>,
    /// Retrieval pipeline is `None` when encoder is unavailable.
    pub retriever: Option<RetrievalPipeline>,
    pub metrics_registry: prometheus_client::registry::Registry,
    pub started_at: Instant,
    pub config: AppConfig,
}

impl AppState {
    /// Initialise all storage backends from the given config.
    ///
    /// Creates directories as needed. Fails fast on any backend init error —
    /// there is no point in running with a broken storage layer.
    pub async fn new(config: AppConfig) -> Result<Self, UmmsError> {
        std::fs::create_dir_all(&config.data_dir)
            .map_err(|e| UmmsError::Config(format!("cannot create data dir: {e}")))?;

        tracing::info!(data_dir = ?config.data_dir, "initialising storage backends");

        let cache = MokaMemoryCache::new();

        let vector = Arc::new(LanceVectorStore::new(
            config.data_dir.join("lance").to_str().ok_or_else(|| {
                UmmsError::Config("data_dir contains non-UTF-8 characters".into())
            })?,
            config.vector_dim,
        )
        .await?);

        let graph = Arc::new(SqliteGraphStore::new(&config.data_dir.join("graph.sqlite"))?);

        let files = LocalFileStore::new(config.data_dir.join("files")).await?;

        let audit = AuditLog::with_capacity(config.audit_capacity);

        // Encoder: attempt to initialise from env var. Not a fatal error if missing —
        // dev mode can run without an API key, using pre-seeded fake vectors.
        let encoder: Option<Arc<GeminiEncoder>> = match GeminiEncoder::new(GeminiConfig {
            dimension: config.vector_dim,
            ..GeminiConfig::default()
        }) {
            Ok(enc) => {
                tracing::info!(model = enc.model_name(), dim = enc.dimension(), "encoder ready");
                Some(Arc::new(enc))
            }
            Err(e) => {
                tracing::warn!("Encoder not available: {e}. Encoding API will be disabled.");
                None
            }
        };

        // BM25 index (always initialised, even without encoder)
        let bm25 = Arc::new(
            Bm25Index::new().map_err(|e| UmmsError::Internal(format!("BM25 init failed: {e}")))?,
        );

        // Tag store (initialise when tag system is enabled)
        let umms_config = config::load_config();
        let tag_store: Option<Arc<dyn TagStore>> = if umms_config.tag.enabled {
            let tag_lance_path = config.data_dir.join(&umms_config.tag.vector_dir);
            let tag_cooc_path = config.data_dir.join(&umms_config.tag.cooc_db);
            match CompositeTagStore::open(
                tag_lance_path.to_str().ok_or_else(|| {
                    UmmsError::Config("tag vector_dir contains non-UTF-8 characters".into())
                })?,
                tag_cooc_path.to_str().ok_or_else(|| {
                    UmmsError::Config("tag cooc_db path contains non-UTF-8 characters".into())
                })?,
                config.vector_dim,
            )
            .await
            {
                Ok(store) => {
                    tracing::info!("Tag store initialised");
                    Some(Arc::new(store) as Arc<dyn TagStore>)
                }
                Err(e) => {
                    tracing::warn!("Tag store init failed: {e}. Tags will be disabled.");
                    None
                }
            }
        } else {
            None
        };

        // Retrieval pipeline (requires encoder for query encoding)
        let retriever = encoder.as_ref().map(|enc| {
            let enc_arc: Arc<dyn Encoder> = Arc::clone(enc) as Arc<dyn Encoder>;
            let vec_arc: Arc<dyn umms_core::traits::VectorStore> = Arc::clone(&vector) as _;
            let graph_arc: Arc<dyn umms_core::traits::KnowledgeGraphStore> =
                Arc::clone(&graph) as _;
            let mut pipeline = RetrievalPipeline::new(
                Arc::clone(&bm25),
                vec_arc,
                enc_arc,
                graph_arc,
                umms_config.retriever,
            );
            // Attach EPA if tag store is available
            if let Some(ref ts) = tag_store {
                pipeline = pipeline.with_epa(
                    Arc::clone(ts),
                    umms_config.epa,
                    umms_config.reshaping,
                );
            }
            pipeline
        });

        let metrics_registry = umms_observe::init_metrics();

        Ok(Self {
            cache,
            vector,
            graph,
            files,
            audit,
            encoder,
            bm25,
            tag_store,
            retriever,
            metrics_registry,
            started_at: Instant::now(),
            config,
        })
    }

    /// Convenience: wrap in Arc for sharing.
    pub async fn shared(config: AppConfig) -> Result<Arc<Self>, UmmsError> {
        Ok(Arc::new(Self::new(config).await?))
    }
}
