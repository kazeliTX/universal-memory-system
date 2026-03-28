//! Shared application state — the single source of truth for all storage backends.
//!
//! Both Tauri Commands and Axum Handlers receive `Arc<AppState>`. This ensures
//! zero data synchronisation overhead: one process, one state, two access paths.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use umms_core::config;
use umms_core::error::UmmsError;
use umms_core::traits::{Encoder, KnowledgeGraphStore, TagStore};
use umms_model::ModelPool;
use umms_observe::AuditLog;
use umms_persona::{DiaryStore, PersonaStore};

use crate::prompt::{PromptEngine, PromptStore};
use crate::session::SessionStore;
use umms_retriever::pipeline::RetrievalPipeline;
use umms_retriever::recall::Bm25Index;
use umms_storage::cache::MokaMemoryCache;
use umms_storage::file::LocalFileStore;
use umms_storage::graph::{CozoGraphStore, SqliteGraphStore};
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
    pub graph: Arc<dyn KnowledgeGraphStore>,
    pub files: LocalFileStore,
    pub audit: AuditLog,
    /// Unified encoder via ModelPool. `None` when no model providers initialized.
    pub encoder: Option<Arc<dyn Encoder>>,
    /// Model pool for multi-model management (M5).
    /// `None` when no model providers could be initialized.
    pub model_pool: Option<Arc<ModelPool>>,
    /// BM25 full-text index (always available).
    pub bm25: Arc<Bm25Index>,
    /// Tag store is `None` when tag system is disabled.
    pub tag_store: Option<Arc<dyn TagStore>>,
    /// Retrieval pipeline is `None` when encoder is unavailable.
    pub retriever: Option<RetrievalPipeline>,
    /// Persona store for agent identity management (M7).
    pub persona_store: Arc<PersonaStore>,
    /// Agent diary store — per-agent user observation notebook.
    pub diary_store: Arc<DiaryStore>,
    /// Chat session store — persistent conversation history.
    pub session_store: Arc<SessionStore>,
    /// Centralised prompt construction engine.
    pub prompt_engine: PromptEngine,
    /// SQLite-backed prompt configuration store (three-mode system).
    pub prompt_store: Arc<PromptStore>,
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

        let umms_cfg = config::load_config();
        let cache = MokaMemoryCache::from_config(&umms_cfg.cache.l0, &umms_cfg.cache.l1);

        let vector = Arc::new(LanceVectorStore::new(
            config.data_dir.join("lance").to_str().ok_or_else(|| {
                UmmsError::Config("data_dir contains non-UTF-8 characters".into())
            })?,
            config.vector_dim,
        )
        .await?);

        let graph: Arc<dyn KnowledgeGraphStore> =
            if umms_cfg.storage.graph_backend == "sqlite" {
                tracing::info!("using SQLite graph backend");
                Arc::new(SqliteGraphStore::new(
                    &config.data_dir.join("graph.sqlite"),
                )?)
            } else {
                tracing::info!("using CozoDB graph backend");
                Arc::new(CozoGraphStore::new(
                    config.data_dir.join("graph.cozo"),
                )?)
            };

        let files = LocalFileStore::new(config.data_dir.join("files")).await?;

        let audit = AuditLog::with_capacity(config.audit_capacity);

        // Load global config early so all subsystems can reference it.
        let umms_config = config::load_config();

        // Encoder: attempt to initialise from env var. Not a fatal error if missing —
        // dev mode can run without an API key, using pre-seeded fake vectors.
        // Model pool (M5): unified model management
        let model_pool = {
            let pool_config = &umms_config.model_pool;
            match ModelPool::from_config(pool_config) {
                Ok(pool) if !pool.is_empty() => {
                    tracing::info!(
                        models = pool.models().len(),
                        "Model pool ready"
                    );
                    Some(Arc::new(pool))
                }
                Ok(_) => {
                    tracing::warn!("Model pool has no available providers. Generation API will be disabled.");
                    None
                }
                Err(e) => {
                    tracing::warn!("Model pool init failed: {e}. Generation API will be disabled.");
                    None
                }
            }
        };

        // BM25 index (always initialised, even without encoder)
        // Use persistent directory from config when available.
        let bm25_path = if umms_cfg.storage.bm25_dir.is_empty() {
            None
        } else {
            Some(config.data_dir.join(&umms_cfg.storage.bm25_dir))
        };
        let bm25 = Arc::new(
            Bm25Index::open(bm25_path.as_ref())
                .map_err(|e| UmmsError::Internal(format!("BM25 init failed: {e}")))?,
        );
        tracing::info!(persistent = bm25_path.is_some(), "BM25 index initialised");

        // Tag store (initialise when tag system is enabled)
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

        // Persona store (M7)
        let persona_store = Arc::new(
            PersonaStore::new(config.data_dir.join("personas.sqlite")).map_err(|e| {
                UmmsError::Config(format!("persona store init failed: {e}"))
            })?,
        );

        // Seed/update default personas — ensures system_prompt is always current
        {
            for persona in umms_persona::default_personas() {
                let existing = persona_store
                    .get(&persona.agent_id)
                    .await
                    .ok()
                    .flatten();
                let should_upsert = match &existing {
                    None => true,
                    Some(e) => e.system_prompt.is_empty() || !e.system_prompt.contains(&persona.name),
                };
                if should_upsert {
                    tracing::info!(agent_id = persona.agent_id.as_str(), "seeding/updating default persona");
                    let _ = persona_store.save(&persona).await;
                }
            }
        }

        // Diary store
        let diary_db = config
            .data_dir
            .join(&umms_config.diary.db);
        let diary_store = Arc::new(
            DiaryStore::new(&diary_db).map_err(|e| {
                UmmsError::Config(format!("diary store init failed: {e}"))
            })?,
        );
        tracing::info!("Diary store initialised");

        // Session store
        let session_store = Arc::new(
            SessionStore::new(config.data_dir.join("sessions.sqlite")).map_err(|e| {
                UmmsError::Config(format!("session store init failed: {e}"))
            })?,
        );
        tracing::info!("Session store initialised");

        // Prompt engine (with default chat template)
        let prompt_engine = PromptEngine::with_defaults();

        // Prompt store (three-mode prompt system)
        let prompt_store = Arc::new(
            PromptStore::new(config.data_dir.join("prompts.sqlite")).map_err(|e| {
                UmmsError::Config(format!("prompt store init failed: {e}"))
            })?,
        );
        tracing::info!("Prompt store initialised");

        // Retrieval pipeline (requires encoder for query encoding)
        let retriever = model_pool.as_ref().map(|pool| {
            let enc_arc: Arc<dyn Encoder> = Arc::clone(pool) as Arc<dyn Encoder>;
            let vec_arc: Arc<dyn umms_core::traits::VectorStore> = Arc::clone(&vector) as _;
            let graph_arc: Arc<dyn umms_core::traits::KnowledgeGraphStore> =
                Arc::clone(&graph) as _;
            let mut pipeline = RetrievalPipeline::with_importance(
                Arc::clone(&bm25),
                vec_arc,
                enc_arc,
                graph_arc,
                umms_config.retriever,
                umms_config.importance,
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

        // Derive encoder from model_pool (ModelPool implements Encoder trait)
        let encoder: Option<Arc<dyn Encoder>> = model_pool
            .as_ref()
            .map(|pool| Arc::clone(pool) as Arc<dyn Encoder>);

        Ok(Self {
            cache,
            vector,
            graph,
            files,
            audit,
            encoder,
            model_pool,
            bm25,
            tag_store,
            retriever,
            persona_store,
            diary_store,
            session_store,
            prompt_engine,
            prompt_store,
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
