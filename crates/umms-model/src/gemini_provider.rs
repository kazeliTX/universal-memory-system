//! Gemini model provider — wraps embedding and generation APIs.
//!
//! For embedding: calls `embedContent` / `batchEmbedContents` on the Gemini API.
//! For generation: calls `generateContent` on the Gemini API.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, instrument, warn};

use umms_core::config::ModelConfig;
use umms_core::error::{EncodingError, Result, UmmsError};
use umms_core::model::{ModelInfo, ModelProvider, ModelTask};

use crate::stats::EncoderStats;

// ---------------------------------------------------------------------------
// Gemini generateContent API types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct GenerateContentRequest<'a> {
    contents: Vec<GenerateContent<'a>>,
    #[serde(rename = "generationConfig", skip_serializing_if = "Option::is_none")]
    generation_config: Option<GenerationConfig>,
}

#[derive(Serialize)]
struct GenerateContent<'a> {
    parts: Vec<GeneratePart<'a>>,
}

#[derive(Serialize)]
struct GeneratePart<'a> {
    text: &'a str,
}

#[derive(Serialize)]
struct GenerationConfig {
    #[serde(rename = "maxOutputTokens", skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<usize>,
}

#[derive(Deserialize)]
struct GenerateContentResponse {
    candidates: Vec<Candidate>,
}

#[derive(Deserialize)]
struct Candidate {
    content: CandidateContent,
}

#[derive(Deserialize)]
struct CandidateContent {
    parts: Vec<CandidatePart>,
}

#[derive(Deserialize)]
struct CandidatePart {
    text: String,
}

// Embed API types
#[derive(Serialize)]
struct EmbedContentRequest<'a> {
    model: &'a str,
    content: ContentPayload<'a>,
    #[serde(rename = "outputDimensionality", skip_serializing_if = "Option::is_none")]
    output_dimensionality: Option<usize>,
}

#[derive(Serialize)]
struct BatchEmbedRequest<'a> {
    requests: Vec<EmbedContentRequest<'a>>,
}

#[derive(Serialize)]
struct ContentPayload<'a> {
    parts: Vec<PartPayload<'a>>,
}

#[derive(Serialize)]
struct PartPayload<'a> {
    text: &'a str,
}

#[derive(Deserialize)]
struct EmbedContentResponse {
    embedding: EmbeddingValue,
}

#[derive(Deserialize)]
struct BatchEmbedResponse {
    embeddings: Vec<EmbeddingValue>,
}

#[derive(Deserialize)]
struct EmbeddingValue {
    values: Vec<f32>,
}

#[derive(Deserialize)]
struct GeminiErrorResponse {
    error: GeminiErrorDetail,
}

#[derive(Deserialize)]
struct GeminiErrorDetail {
    message: String,
    #[serde(default)]
    status: String,
}

// ---------------------------------------------------------------------------
// GeminiProvider
// ---------------------------------------------------------------------------

/// A Gemini-backed model provider supporting embedding and generation.
pub struct GeminiProvider {
    id: String,
    model_name: String,
    api_key: String,
    tasks: Vec<ModelTask>,
    dimension: Option<usize>,
    max_tokens: Option<usize>,
    timeout: Duration,
    max_retries: u32,
    client: Client,
    /// Maximum texts per batch embed request (Gemini limit is 100).
    max_batch_size: usize,
    /// Runtime statistics for observability.
    pub stats: EncoderStats,
}

impl std::fmt::Debug for GeminiProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GeminiProvider")
            .field("id", &self.id)
            .field("model_name", &self.model_name)
            .field("tasks", &self.tasks)
            .field("api_key", &"***")
            .finish()
    }
}

impl GeminiProvider {
    /// Create a new Gemini provider from model configuration.
    ///
    /// Returns an error if the API key environment variable is not set.
    pub fn from_config(config: &ModelConfig) -> Result<Self> {
        let api_key = std::env::var(&config.api_key_env).map_err(|_| {
            UmmsError::Config(format!(
                "Environment variable '{}' not set. Required for Gemini model '{}'.",
                config.api_key_env, config.id
            ))
        })?;

        let timeout = Duration::from_millis(config.timeout_ms.unwrap_or(10_000));

        let client = Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| {
                UmmsError::Encoding(EncodingError::ApiCallFailed {
                    provider: "gemini".into(),
                    reason: format!("Failed to build HTTP client: {e}"),
                })
            })?;

        let tasks: Vec<ModelTask> = config
            .tasks
            .iter()
            .filter_map(|s| ModelTask::from_str_loose(s))
            .collect();

        Ok(Self {
            id: config.id.clone(),
            model_name: config.model_name.clone(),
            api_key,
            tasks,
            dimension: config.dimension,
            max_tokens: config.max_tokens,
            timeout,
            max_retries: config.max_retries.unwrap_or(2),
            client,
            max_batch_size: 100,
            stats: EncoderStats::default(),
        })
    }

    fn base_url(&self) -> String {
        format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}",
            self.model_name
        )
    }

    /// Call the single embedContent endpoint with retry.
    #[instrument(skip(self, text), fields(model = %self.model_name))]
    async fn call_embed_single(&self, text: &str) -> Result<Vec<f32>> {
        let url = format!("{}:embedContent?key={}", self.base_url(), self.api_key);
        let full_model = format!("models/{}", self.model_name);
        let body = EmbedContentRequest {
            model: &full_model,
            content: ContentPayload {
                parts: vec![PartPayload { text }],
            },
            output_dimensionality: self.dimension,
        };

        let start = Instant::now();
        let mut last_err = None;

        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                self.stats.total_retries.fetch_add(1, Ordering::Relaxed);
                let delay = Duration::from_millis(100 * 2u64.pow(attempt - 1));
                warn!(attempt, delay_ms = delay.as_millis(), "Retrying Gemini embed API");
                tokio::time::sleep(delay).await;
            }

            match self.client.post(&url).json(&body).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        let parsed: EmbedContentResponse = resp.json().await.map_err(|e| {
                            UmmsError::Encoding(EncodingError::ApiCallFailed {
                                provider: "gemini".into(),
                                reason: format!("Failed to parse response: {e}"),
                            })
                        })?;

                        let elapsed = start.elapsed();
                        self.stats
                            .total_duration_us
                            .fetch_add(elapsed.as_micros() as u64, Ordering::Relaxed);

                        debug!(
                            dim = parsed.embedding.values.len(),
                            latency_ms = elapsed.as_millis(),
                            "Embedding generated"
                        );

                        return Ok(parsed.embedding.values);
                    }

                    let status = resp.status();
                    let err_body = resp.text().await.unwrap_or_default();
                    let reason = if let Ok(ge) =
                        serde_json::from_str::<GeminiErrorResponse>(&err_body)
                    {
                        format!("{} ({})", ge.error.message, ge.error.status)
                    } else {
                        format!("HTTP {status}: {err_body}")
                    };

                    if status.as_u16() == 429 || status.is_server_error() {
                        last_err = Some(reason);
                        continue;
                    }

                    self.stats.total_errors.fetch_add(1, Ordering::Relaxed);
                    return Err(UmmsError::Encoding(EncodingError::ApiCallFailed {
                        provider: "gemini".into(),
                        reason,
                    }));
                }
                Err(e) => {
                    if e.is_timeout() {
                        last_err = Some(format!("Timeout after {}ms", self.timeout.as_millis()));
                    } else {
                        last_err = Some(format!("Network error: {e}"));
                    }
                    continue;
                }
            }
        }

        self.stats.total_errors.fetch_add(1, Ordering::Relaxed);
        Err(UmmsError::Encoding(EncodingError::ApiCallFailed {
            provider: "gemini".into(),
            reason: format!(
                "All {} attempts failed. Last error: {}",
                self.max_retries + 1,
                last_err.unwrap_or_else(|| "unknown".into())
            ),
        }))
    }

    /// Call the batchEmbedContents endpoint with retry.
    #[instrument(skip(self, texts), fields(model = %self.model_name, batch_size = texts.len()))]
    async fn call_embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let url = format!(
            "{}:batchEmbedContents?key={}",
            self.base_url(),
            self.api_key
        );
        let full_model = format!("models/{}", self.model_name);

        let requests: Vec<EmbedContentRequest<'_>> = texts
            .iter()
            .map(|t| EmbedContentRequest {
                model: &full_model,
                content: ContentPayload {
                    parts: vec![PartPayload { text: t.as_str() }],
                },
                output_dimensionality: self.dimension,
            })
            .collect();

        let body = BatchEmbedRequest { requests };
        let start = Instant::now();
        let mut last_err = None;

        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                self.stats.total_retries.fetch_add(1, Ordering::Relaxed);
                let delay = Duration::from_millis(100 * 2u64.pow(attempt - 1));
                warn!(attempt, delay_ms = delay.as_millis(), "Retrying batch Gemini API");
                tokio::time::sleep(delay).await;
            }

            match self.client.post(&url).json(&body).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        let parsed: BatchEmbedResponse = resp.json().await.map_err(|e| {
                            UmmsError::Encoding(EncodingError::ApiCallFailed {
                                provider: "gemini".into(),
                                reason: format!("Failed to parse batch response: {e}"),
                            })
                        })?;

                        let elapsed = start.elapsed();
                        self.stats
                            .total_duration_us
                            .fetch_add(elapsed.as_micros() as u64, Ordering::Relaxed);

                        debug!(
                            count = parsed.embeddings.len(),
                            latency_ms = elapsed.as_millis(),
                            "Batch embeddings generated"
                        );

                        return Ok(parsed.embeddings.into_iter().map(|e| e.values).collect());
                    }

                    let status = resp.status();
                    let err_body = resp.text().await.unwrap_or_default();
                    let reason = if let Ok(ge) =
                        serde_json::from_str::<GeminiErrorResponse>(&err_body)
                    {
                        format!("{} ({})", ge.error.message, ge.error.status)
                    } else {
                        format!("HTTP {status}: {err_body}")
                    };

                    if status.as_u16() == 429 || status.is_server_error() {
                        last_err = Some(reason);
                        continue;
                    }

                    self.stats.total_errors.fetch_add(1, Ordering::Relaxed);
                    return Err(UmmsError::Encoding(EncodingError::ApiCallFailed {
                        provider: "gemini".into(),
                        reason,
                    }));
                }
                Err(e) => {
                    if e.is_timeout() {
                        last_err = Some(format!("Timeout after {}ms", self.timeout.as_millis()));
                    } else {
                        last_err = Some(format!("Network error: {e}"));
                    }
                    continue;
                }
            }
        }

        self.stats.total_errors.fetch_add(1, Ordering::Relaxed);
        Err(UmmsError::Encoding(EncodingError::ApiCallFailed {
            provider: "gemini".into(),
            reason: format!(
                "All {} attempts failed. Last error: {}",
                self.max_retries + 1,
                last_err.unwrap_or_else(|| "unknown".into())
            ),
        }))
    }

    /// Call the generateContent endpoint with retry.
    #[instrument(skip(self, prompt), fields(model = %self.model_name))]
    async fn call_generate(&self, prompt: &str, max_tokens: Option<usize>) -> Result<String> {
        let url = format!(
            "{}:generateContent?key={}",
            self.base_url(),
            self.api_key
        );

        let gen_config = max_tokens
            .or(self.max_tokens)
            .map(|mt| GenerationConfig {
                max_output_tokens: Some(mt),
            });

        let body = GenerateContentRequest {
            contents: vec![GenerateContent {
                parts: vec![GeneratePart { text: prompt }],
            }],
            generation_config: gen_config,
        };

        let start = Instant::now();
        let mut last_err = None;

        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                self.stats.total_retries.fetch_add(1, Ordering::Relaxed);
                let delay = Duration::from_millis(100 * 2u64.pow(attempt - 1));
                warn!(attempt, delay_ms = delay.as_millis(), "Retrying Gemini generate API");
                tokio::time::sleep(delay).await;
            }

            match self.client.post(&url).json(&body).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        let parsed: GenerateContentResponse =
                            resp.json().await.map_err(|e| {
                                UmmsError::Encoding(EncodingError::ApiCallFailed {
                                    provider: "gemini".into(),
                                    reason: format!("Failed to parse generate response: {e}"),
                                })
                            })?;

                        let elapsed = start.elapsed();
                        self.stats
                            .total_duration_us
                            .fetch_add(elapsed.as_micros() as u64, Ordering::Relaxed);

                        let text = parsed
                            .candidates
                            .into_iter()
                            .next()
                            .and_then(|c| c.content.parts.into_iter().next())
                            .map(|p| p.text)
                            .unwrap_or_default();

                        debug!(
                            chars = text.len(),
                            latency_ms = elapsed.as_millis(),
                            "Text generated"
                        );

                        return Ok(text);
                    }

                    let status = resp.status();
                    let err_body = resp.text().await.unwrap_or_default();
                    let reason = if let Ok(ge) =
                        serde_json::from_str::<GeminiErrorResponse>(&err_body)
                    {
                        format!("{} ({})", ge.error.message, ge.error.status)
                    } else {
                        format!("HTTP {status}: {err_body}")
                    };

                    if status.as_u16() == 429 || status.is_server_error() {
                        last_err = Some(reason);
                        continue;
                    }

                    self.stats.total_errors.fetch_add(1, Ordering::Relaxed);
                    return Err(UmmsError::Encoding(EncodingError::ApiCallFailed {
                        provider: "gemini".into(),
                        reason,
                    }));
                }
                Err(e) => {
                    if e.is_timeout() {
                        last_err = Some(format!("Timeout after {}ms", self.timeout.as_millis()));
                    } else {
                        last_err = Some(format!("Network error: {e}"));
                    }
                    continue;
                }
            }
        }

        self.stats.total_errors.fetch_add(1, Ordering::Relaxed);
        Err(UmmsError::Encoding(EncodingError::ApiCallFailed {
            provider: "gemini".into(),
            reason: format!(
                "All {} attempts failed. Last error: {}",
                self.max_retries + 1,
                last_err.unwrap_or_else(|| "unknown".into())
            ),
        }))
    }
}

#[async_trait]
impl ModelProvider for GeminiProvider {
    fn info(&self) -> ModelInfo {
        ModelInfo {
            id: self.id.clone(),
            provider: "gemini".to_owned(),
            model_name: self.model_name.clone(),
            tasks: self.tasks.clone(),
            dimension: self.dimension,
            max_tokens: self.max_tokens,
            available: true,
        }
    }

    fn supports(&self, task: ModelTask) -> bool {
        self.tasks.contains(&task)
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        self.stats.total_requests.fetch_add(1, Ordering::Relaxed);
        self.stats.total_texts_encoded.fetch_add(1, Ordering::Relaxed);

        let vec = self.call_embed_single(text).await?;

        if let Some(dim) = self.dimension {
            if vec.len() != dim {
                error!(expected = dim, got = vec.len(), "Dimension mismatch from Gemini API");
                return Err(UmmsError::Encoding(EncodingError::ApiCallFailed {
                    provider: "gemini".into(),
                    reason: format!("Expected {dim} dimensions, got {}", vec.len()),
                }));
            }
        }

        Ok(vec)
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        self.stats.total_requests.fetch_add(1, Ordering::Relaxed);
        self.stats
            .total_texts_encoded
            .fetch_add(texts.len() as u64, Ordering::Relaxed);

        let mut all_results = Vec::with_capacity(texts.len());

        for chunk in texts.chunks(self.max_batch_size) {
            let chunk_owned: Vec<String> = chunk.to_vec();
            let batch_result = self.call_embed_batch(&chunk_owned).await?;

            if let Some(dim) = self.dimension {
                for (i, vec) in batch_result.iter().enumerate() {
                    if vec.len() != dim {
                        return Err(UmmsError::Encoding(EncodingError::ApiCallFailed {
                            provider: "gemini".into(),
                            reason: format!(
                                "Batch item {i}: expected {dim} dimensions, got {}",
                                vec.len()
                            ),
                        }));
                    }
                }
            }

            all_results.extend(batch_result);
        }

        Ok(all_results)
    }

    async fn generate(&self, prompt: &str, max_tokens: Option<usize>) -> Result<String> {
        self.stats.total_requests.fetch_add(1, Ordering::Relaxed);
        self.call_generate(prompt, max_tokens).await
    }

    fn embedding_dimension(&self) -> Option<usize> {
        self.dimension
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_creation_fails_without_api_key() {
        let config = ModelConfig {
            id: "test".to_owned(),
            provider: "gemini".to_owned(),
            model_name: "gemini-embedding-001".to_owned(),
            api_key_env: "UMMS_TEST_NONEXISTENT_KEY_XYZ_99".to_owned(),
            tasks: vec!["embedding".to_owned()],
            dimension: Some(3072),
            max_tokens: None,
            timeout_ms: Some(5000),
            max_retries: Some(2),
        };
        let result = GeminiProvider::from_config(&config);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("UMMS_TEST_NONEXISTENT_KEY_XYZ_99"));
    }

    #[test]
    fn provider_parses_tasks() {
        let tasks: Vec<ModelTask> = vec!["embedding", "generation", "chat"]
            .iter()
            .filter_map(|s| ModelTask::from_str_loose(s))
            .collect();

        assert_eq!(tasks.len(), 3);
        assert!(tasks.contains(&ModelTask::Embedding));
        assert!(tasks.contains(&ModelTask::Generation));
        assert!(tasks.contains(&ModelTask::Chat));
    }
}
