//! Model pool — centralized registry routing requests to the appropriate model.
//!
//! The pool manages multiple [`ModelProvider`] instances and routes requests
//! based on task type. It also implements the [`Encoder`] trait so existing
//! code that depends on `Arc<dyn Encoder>` continues to work unchanged.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use chrono::Utc;
use serde::Serialize;
use tracing::{info, warn};

use umms_core::config::ModelPoolConfig;
use umms_core::error::{EncodingError, Result, UmmsError};
use umms_core::model::{ModelInfo, ModelProvider, ModelTask};
use umms_core::traits::Encoder;

use crate::gemini_provider::GeminiProvider;
use crate::stats::EncoderStats;
use crate::trace::{
    estimate_tokens, new_trace_id, truncate_preview, ModelTrace, TraceStore,
};

// ---------------------------------------------------------------------------
// Model activation status types
// ---------------------------------------------------------------------------

/// Status of a single model provider.
#[derive(Debug, Clone, Serialize)]
pub struct ModelStatus {
    pub id: String,
    pub provider: String,
    pub model_name: String,
    pub available: bool,
    pub tasks: Vec<String>,
    pub dimension: Option<usize>,
    pub stats: Option<ModelStats>,
}

/// Point-in-time statistics for a model provider.
#[derive(Debug, Clone, Serialize)]
pub struct ModelStats {
    pub total_requests: u64,
    pub total_errors: u64,
    pub avg_latency_ms: f64,
}

// ---------------------------------------------------------------------------
// ModelPool
// ---------------------------------------------------------------------------

/// Centralized model pool that routes requests to the right provider.
pub struct ModelPool {
    providers: HashMap<String, Arc<GeminiProvider>>,
    routing: HashMap<ModelTask, String>,
    /// Per-request trace store for debugging and monitoring.
    pub trace_store: Arc<TraceStore>,
}

impl std::fmt::Debug for ModelPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModelPool")
            .field("providers", &self.providers.keys().collect::<Vec<_>>())
            .field("routing", &self.routing)
            .field("trace_store", &self.trace_store)
            .finish()
    }
}

impl ModelPool {
    /// Build a model pool from configuration.
    ///
    /// Creates provider instances for each configured model. Providers that
    /// fail to initialize (e.g., missing API key) are logged as warnings and
    /// skipped rather than causing a hard failure. This allows the system to
    /// run in degraded mode when not all API keys are available.
    pub fn from_config(config: &ModelPoolConfig) -> Result<Self> {
        let mut providers: HashMap<String, Arc<GeminiProvider>> = HashMap::new();

        for model_config in &config.models {
            match model_config.provider.as_str() {
                "gemini" => match GeminiProvider::from_config(model_config) {
                    Ok(provider) => {
                        info!(
                            id = %model_config.id,
                            model = %model_config.model_name,
                            tasks = ?model_config.tasks,
                            "Registered Gemini model provider"
                        );
                        providers.insert(model_config.id.clone(), Arc::new(provider));
                    }
                    Err(e) => {
                        warn!(
                            id = %model_config.id,
                            error = %e,
                            "Failed to initialize model provider (skipping)"
                        );
                    }
                },
                other => {
                    warn!(
                        id = %model_config.id,
                        provider = %other,
                        "Unknown provider type (skipping). Currently supported: gemini"
                    );
                }
            }
        }

        // Build routing table
        let mut routing: HashMap<ModelTask, String> = HashMap::new();

        let route_pairs = [
            (ModelTask::Embedding, &config.routing.embedding),
            (ModelTask::Generation, &config.routing.generation),
            (ModelTask::Reranking, &config.routing.reranking),
            (ModelTask::EntityExtraction, &config.routing.entity_extraction),
            (ModelTask::Chat, &config.routing.chat),
        ];

        for (task, model_id) in route_pairs {
            if providers.contains_key(model_id) {
                routing.insert(task, model_id.clone());
            } else if !model_id.is_empty() {
                warn!(
                    task = %task,
                    model_id = %model_id,
                    "Routing target not available — task will be unrouted"
                );
            }
        }

        let trace_store = Arc::new(TraceStore::new(config.trace_max_size));

        Ok(Self {
            providers,
            routing,
            trace_store,
        })
    }

    /// Get the provider for a specific task.
    pub fn provider_for(&self, task: ModelTask) -> Option<&Arc<GeminiProvider>> {
        self.routing
            .get(&task)
            .and_then(|id| self.providers.get(id))
    }

    /// Get the model ID routed for a specific task.
    fn model_id_for(&self, task: ModelTask) -> Option<&str> {
        self.routing.get(&task).map(String::as_str)
    }

    /// List all registered models with their info.
    pub fn models(&self) -> Vec<ModelInfo> {
        self.providers
            .values()
            .map(|p| p.info())
            .collect()
    }

    /// Get activation status of all registered models (with statistics).
    pub fn status(&self) -> Vec<ModelStatus> {
        self.providers.values().map(|p| {
            let info = p.info();
            let snap = p.stats.snapshot();
            ModelStatus {
                id: info.id,
                provider: info.provider,
                model_name: info.model_name,
                available: info.available,
                tasks: info.tasks.iter().map(|t| t.to_string()).collect(),
                dimension: info.dimension,
                stats: Some(ModelStats {
                    total_requests: snap.total_requests,
                    total_errors: snap.total_errors,
                    avg_latency_ms: snap.avg_latency_ms,
                }),
            }
        }).collect()
    }

    /// Whether any providers are registered.
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }

    /// Whether a specific task has a routed provider.
    pub fn has_provider_for(&self, task: ModelTask) -> bool {
        self.provider_for(task).is_some()
    }

    // -----------------------------------------------------------------------
    // Traced convenience methods
    // -----------------------------------------------------------------------

    /// Convenience: embed text using the embedding model (with tracing).
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let provider = self.provider_for(ModelTask::Embedding).ok_or_else(|| {
            UmmsError::Encoding(EncodingError::ApiCallFailed {
                provider: "model_pool".into(),
                reason: "No model configured for embedding task".into(),
            })
        })?;

        let info = provider.info();
        let model_id = self.model_id_for(ModelTask::Embedding).unwrap_or("unknown").to_owned();
        let start = Instant::now();

        let result = provider.embed(text).await;

        let latency_ms = start.elapsed().as_millis() as u64;
        let trace = ModelTrace {
            id: new_trace_id(),
            timestamp: Utc::now(),
            model_id,
            model_name: info.model_name,
            provider: info.provider,
            task: "embedding".to_owned(),
            request_type: "embed".to_owned(),
            input_preview: truncate_preview(text, 200),
            input_tokens_estimate: estimate_tokens(text),
            success: result.is_ok(),
            error_message: result.as_ref().err().map(|e| format!("{e}")),
            output_preview: None,
            output_dimension: result.as_ref().ok().map(|v| v.len()),
            output_tokens_estimate: None,
            latency_ms,
            retry_count: 0,
            caller: "unknown".to_owned(),
        };
        self.trace_store.record(trace);

        result
    }

    /// Convenience: embed batch using the embedding model (with tracing).
    pub async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let provider = self.provider_for(ModelTask::Embedding).ok_or_else(|| {
            UmmsError::Encoding(EncodingError::ApiCallFailed {
                provider: "model_pool".into(),
                reason: "No model configured for embedding task".into(),
            })
        })?;

        let info = provider.info();
        let model_id = self.model_id_for(ModelTask::Embedding).unwrap_or("unknown").to_owned();
        let start = Instant::now();

        let result = provider.embed_batch(texts).await;

        let latency_ms = start.elapsed().as_millis() as u64;
        let input_preview = if texts.is_empty() {
            "(empty batch)".to_owned()
        } else {
            truncate_preview(
                &format!("[{} texts] {}", texts.len(), texts[0]),
                200,
            )
        };
        let input_tokens: usize = texts.iter().map(|t| estimate_tokens(t)).sum();
        let trace = ModelTrace {
            id: new_trace_id(),
            timestamp: Utc::now(),
            model_id,
            model_name: info.model_name,
            provider: info.provider,
            task: "embedding".to_owned(),
            request_type: "embed_batch".to_owned(),
            input_preview,
            input_tokens_estimate: input_tokens,
            success: result.is_ok(),
            error_message: result.as_ref().err().map(|e| format!("{e}")),
            output_preview: None,
            output_dimension: result.as_ref().ok().and_then(|v| v.first().map(|f| f.len())),
            output_tokens_estimate: None,
            latency_ms,
            retry_count: 0,
            caller: "unknown".to_owned(),
        };
        self.trace_store.record(trace);

        result
    }

    /// Convenience: generate text using the generation model (with tracing).
    pub async fn generate(&self, prompt: &str) -> Result<String> {
        let provider = self.provider_for(ModelTask::Generation).ok_or_else(|| {
            UmmsError::Encoding(EncodingError::ApiCallFailed {
                provider: "model_pool".into(),
                reason: "No model configured for generation task".into(),
            })
        })?;

        let info = provider.info();
        let model_id = self.model_id_for(ModelTask::Generation).unwrap_or("unknown").to_owned();
        let start = Instant::now();

        let result = provider.generate(prompt, None).await;

        let latency_ms = start.elapsed().as_millis() as u64;
        let trace = ModelTrace {
            id: new_trace_id(),
            timestamp: Utc::now(),
            model_id,
            model_name: info.model_name,
            provider: info.provider,
            task: "generation".to_owned(),
            request_type: "generate".to_owned(),
            input_preview: truncate_preview(prompt, 200),
            input_tokens_estimate: estimate_tokens(prompt),
            success: result.is_ok(),
            error_message: result.as_ref().err().map(|e| format!("{e}")),
            output_preview: result.as_ref().ok().map(|s| truncate_preview(s, 200)),
            output_dimension: None,
            output_tokens_estimate: result.as_ref().ok().map(|s| estimate_tokens(s)),
            latency_ms,
            retry_count: 0,
            caller: "unknown".to_owned(),
        };
        self.trace_store.record(trace);

        result
    }

    /// Convenience: generate text with explicit max tokens (with tracing).
    pub async fn generate_with_max_tokens(
        &self,
        prompt: &str,
        max_tokens: usize,
    ) -> Result<String> {
        let provider = self.provider_for(ModelTask::Generation).ok_or_else(|| {
            UmmsError::Encoding(EncodingError::ApiCallFailed {
                provider: "model_pool".into(),
                reason: "No model configured for generation task".into(),
            })
        })?;

        let info = provider.info();
        let model_id = self.model_id_for(ModelTask::Generation).unwrap_or("unknown").to_owned();
        let start = Instant::now();

        let result = provider.generate(prompt, Some(max_tokens)).await;

        let latency_ms = start.elapsed().as_millis() as u64;
        let trace = ModelTrace {
            id: new_trace_id(),
            timestamp: Utc::now(),
            model_id,
            model_name: info.model_name,
            provider: info.provider,
            task: "generation".to_owned(),
            request_type: "generate".to_owned(),
            input_preview: truncate_preview(prompt, 200),
            input_tokens_estimate: estimate_tokens(prompt),
            success: result.is_ok(),
            error_message: result.as_ref().err().map(|e| format!("{e}")),
            output_preview: result.as_ref().ok().map(|s| truncate_preview(s, 200)),
            output_dimension: None,
            output_tokens_estimate: result.as_ref().ok().map(|s| estimate_tokens(s)),
            latency_ms,
            retry_count: 0,
            caller: "unknown".to_owned(),
        };
        self.trace_store.record(trace);

        result
    }

    /// Convenience: generate using the chat model (with tracing).
    pub async fn chat(&self, prompt: &str) -> Result<String> {
        let provider = self.provider_for(ModelTask::Chat).ok_or_else(|| {
            UmmsError::Encoding(EncodingError::ApiCallFailed {
                provider: "model_pool".into(),
                reason: "No model configured for chat task".into(),
            })
        })?;

        let info = provider.info();
        let model_id = self.model_id_for(ModelTask::Chat).unwrap_or("unknown").to_owned();
        let start = Instant::now();

        let result = provider.generate(prompt, None).await;

        let latency_ms = start.elapsed().as_millis() as u64;
        let trace = ModelTrace {
            id: new_trace_id(),
            timestamp: Utc::now(),
            model_id,
            model_name: info.model_name,
            provider: info.provider,
            task: "chat".to_owned(),
            request_type: "generate".to_owned(),
            input_preview: truncate_preview(prompt, 200),
            input_tokens_estimate: estimate_tokens(prompt),
            success: result.is_ok(),
            error_message: result.as_ref().err().map(|e| format!("{e}")),
            output_preview: result.as_ref().ok().map(|s| truncate_preview(s, 200)),
            output_dimension: None,
            output_tokens_estimate: result.as_ref().ok().map(|s| estimate_tokens(s)),
            latency_ms,
            retry_count: 0,
            caller: "unknown".to_owned(),
        };
        self.trace_store.record(trace);

        result
    }

    /// Convenience: generate using the entity extraction model (with tracing).
    pub async fn extract(&self, prompt: &str) -> Result<String> {
        let provider = self.provider_for(ModelTask::EntityExtraction).ok_or_else(|| {
            UmmsError::Encoding(EncodingError::ApiCallFailed {
                provider: "model_pool".into(),
                reason: "No model configured for entity extraction task".into(),
            })
        })?;

        let info = provider.info();
        let model_id = self.model_id_for(ModelTask::EntityExtraction).unwrap_or("unknown").to_owned();
        let start = Instant::now();

        let result = provider.generate(prompt, None).await;

        let latency_ms = start.elapsed().as_millis() as u64;
        let trace = ModelTrace {
            id: new_trace_id(),
            timestamp: Utc::now(),
            model_id,
            model_name: info.model_name,
            provider: info.provider,
            task: "entity_extraction".to_owned(),
            request_type: "generate".to_owned(),
            input_preview: truncate_preview(prompt, 200),
            input_tokens_estimate: estimate_tokens(prompt),
            success: result.is_ok(),
            error_message: result.as_ref().err().map(|e| format!("{e}")),
            output_preview: result.as_ref().ok().map(|s| truncate_preview(s, 200)),
            output_dimension: None,
            output_tokens_estimate: result.as_ref().ok().map(|s| estimate_tokens(s)),
            latency_ms,
            retry_count: 0,
            caller: "unknown".to_owned(),
        };
        self.trace_store.record(trace);

        result
    }

    /// Get embedding dimension from the configured embedding model.
    pub fn embedding_dimension(&self) -> Option<usize> {
        self.provider_for(ModelTask::Embedding)
            .and_then(|p| p.embedding_dimension())
    }

    /// Get encoder stats from the embedding provider (for dashboard display).
    pub fn embedding_stats(&self) -> Option<&EncoderStats> {
        self.provider_for(ModelTask::Embedding)
            .map(|p| &p.stats)
    }

    /// Get encoder stats from the generation provider.
    pub fn generation_stats(&self) -> Option<&EncoderStats> {
        self.provider_for(ModelTask::Generation)
            .map(|p| &p.stats)
    }
}

// ---------------------------------------------------------------------------
// Encoder trait bridge — allows ModelPool to be used as Arc<dyn Encoder>
// ---------------------------------------------------------------------------

#[async_trait]
impl Encoder for ModelPool {
    async fn encode_text(&self, text: &str) -> Result<Vec<f32>> {
        self.embed(text).await
    }

    async fn encode_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        self.embed_batch(texts).await
    }

    fn dimension(&self) -> usize {
        self.embedding_dimension().unwrap_or(3072)
    }

    fn model_name(&self) -> &str {
        "model-pool"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umms_core::config::{ModelConfig, TaskRoutingConfig};

    fn make_test_config() -> ModelPoolConfig {
        ModelPoolConfig {
            models: vec![
                ModelConfig {
                    id: "embed-test".to_owned(),
                    provider: "gemini".to_owned(),
                    model_name: "gemini-embedding-001".to_owned(),
                    api_key_env: "UMMS_TEST_NONEXISTENT_EMBED_KEY".to_owned(),
                    tasks: vec!["embedding".to_owned()],
                    dimension: Some(3072),
                    max_tokens: None,
                    timeout_ms: Some(5000),
                    max_retries: Some(1),
                },
                ModelConfig {
                    id: "gen-test".to_owned(),
                    provider: "gemini".to_owned(),
                    model_name: "gemini-2.5-flash".to_owned(),
                    api_key_env: "UMMS_TEST_NONEXISTENT_GEN_KEY".to_owned(),
                    tasks: vec!["generation".to_owned(), "chat".to_owned()],
                    dimension: None,
                    max_tokens: Some(8192),
                    timeout_ms: Some(30000),
                    max_retries: Some(2),
                },
            ],
            routing: TaskRoutingConfig {
                embedding: "embed-test".to_owned(),
                generation: "gen-test".to_owned(),
                reranking: "embed-test".to_owned(),
                entity_extraction: "gen-test".to_owned(),
                chat: "gen-test".to_owned(),
            },
            trace_max_size: 1000,
        }
    }

    #[test]
    fn pool_from_config_graceful_without_keys() {
        let config = make_test_config();
        let pool = ModelPool::from_config(&config).unwrap();
        assert!(pool.is_empty());
        assert!(pool.models().is_empty());
    }

    #[test]
    fn pool_routing_when_no_providers() {
        let config = make_test_config();
        let pool = ModelPool::from_config(&config).unwrap();
        assert!(!pool.has_provider_for(ModelTask::Embedding));
        assert!(!pool.has_provider_for(ModelTask::Generation));
    }

    #[test]
    fn pool_default_config() {
        let config = ModelPoolConfig::default();
        assert_eq!(config.models.len(), 2);
        assert_eq!(config.models[0].id, "gemini-embed");
        assert_eq!(config.models[1].id, "gemini-flash");
        assert_eq!(config.routing.embedding, "gemini-embed");
        assert_eq!(config.routing.generation, "gemini-flash");
        assert_eq!(config.trace_max_size, 1000);
    }

    #[test]
    fn pool_unknown_provider_skipped() {
        let config = ModelPoolConfig {
            models: vec![ModelConfig {
                id: "unknown-model".to_owned(),
                provider: "unknown_provider".to_owned(),
                model_name: "some-model".to_owned(),
                api_key_env: "SOME_KEY".to_owned(),
                tasks: vec!["embedding".to_owned()],
                dimension: Some(768),
                max_tokens: None,
                timeout_ms: None,
                max_retries: None,
            }],
            routing: TaskRoutingConfig {
                embedding: "unknown-model".to_owned(),
                ..TaskRoutingConfig::default()
            },
            trace_max_size: 1000,
        };
        let pool = ModelPool::from_config(&config).unwrap();
        assert!(pool.is_empty());
    }

    #[test]
    fn encoder_trait_dimension_default() {
        let config = make_test_config();
        let pool = ModelPool::from_config(&config).unwrap();
        assert_eq!(pool.dimension(), 3072);
    }

    #[test]
    fn encoder_trait_model_name() {
        let config = make_test_config();
        let pool = ModelPool::from_config(&config).unwrap();
        assert_eq!(pool.model_name(), "model-pool");
    }

    #[test]
    fn pool_status_empty() {
        let config = make_test_config();
        let pool = ModelPool::from_config(&config).unwrap();
        let status = pool.status();
        assert!(status.is_empty());
    }

    #[test]
    fn pool_trace_store_created() {
        let config = make_test_config();
        let pool = ModelPool::from_config(&config).unwrap();
        assert_eq!(pool.trace_store.traces(100).len(), 0);
    }
}
