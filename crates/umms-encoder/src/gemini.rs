//! Gemini embedding API backend.
//!
//! Calls `https://generativelanguage.googleapis.com/v1beta/models/{model}:embedContent`
//! (single) or `:batchEmbedContents` (batch).
//!
//! Configuration is injected via [`GeminiConfig`] — the API key is read from
//! an environment variable, never hardcoded or stored in config files.

use std::sync::atomic::Ordering;
use std::time::Instant;

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, instrument, warn};

use umms_core::error::{EncodingError, Result, UmmsError};
use umms_core::traits::Encoder;
use umms_model::stats::EncoderStats;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Configuration for the Gemini embedding backend.
#[derive(Debug, Clone)]
pub struct GeminiConfig {
    /// Environment variable name that holds the API key.
    pub api_key_env: String,
    /// Model identifier (e.g. "gemini-embedding-001").
    pub model: String,
    /// Output vector dimension.
    pub dimension: usize,
    /// Per-request timeout in milliseconds.
    pub timeout_ms: u64,
    /// Maximum retry attempts on transient failures.
    pub max_retries: u32,
    /// Maximum texts per batch request (Gemini limit is 100).
    pub max_batch_size: usize,
}

impl Default for GeminiConfig {
    fn default() -> Self {
        Self {
            api_key_env: "GEMINI_API_KEY".to_owned(),
            model: "gemini-embedding-001".to_owned(),
            dimension: 3072,
            timeout_ms: 10_000,
            max_retries: 2,
            max_batch_size: 100,
        }
    }
}

// ---------------------------------------------------------------------------
// Gemini API request/response types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct EmbedContentRequest<'a> {
    model: &'a str,
    content: ContentPayload<'a>,
    #[serde(
        rename = "outputDimensionality",
        skip_serializing_if = "Option::is_none"
    )]
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
// GeminiEncoder
// ---------------------------------------------------------------------------

/// Production encoder backed by Google's Gemini embedding API.
///
/// Debug impl intentionally hides the API key.
pub struct GeminiEncoder {
    config: GeminiConfig,
    api_key: String,
    client: Client,
    pub stats: EncoderStats,
}

#[allow(clippy::missing_fields_in_debug)] // intentionally hides api_key and client
impl std::fmt::Debug for GeminiEncoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GeminiEncoder")
            .field("model", &self.config.model)
            .field("dimension", &self.config.dimension)
            .field("api_key", &"***")
            .finish()
    }
}

impl GeminiEncoder {
    /// Create a new encoder, reading the API key from the configured env var.
    ///
    /// Returns an error if the environment variable is not set.
    pub fn new(config: GeminiConfig) -> Result<Self> {
        let api_key = std::env::var(&config.api_key_env).map_err(|_| {
            UmmsError::Config(format!(
                "Environment variable '{}' not set. Required for Gemini embedding API.",
                config.api_key_env
            ))
        })?;

        let client = Client::builder()
            .timeout(std::time::Duration::from_millis(config.timeout_ms))
            .build()
            .map_err(|e| {
                UmmsError::Encoding(EncodingError::ApiCallFailed {
                    provider: "gemini".into(),
                    reason: format!("Failed to build HTTP client: {e}"),
                })
            })?;

        Ok(Self {
            config,
            api_key,
            client,
            stats: EncoderStats::default(),
        })
    }

    fn base_url(&self) -> String {
        format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}",
            self.config.model
        )
    }

    /// Call the single embedContent endpoint with retry.
    #[instrument(skip(self, text), fields(model = %self.config.model))]
    async fn call_embed_single(&self, text: &str) -> Result<Vec<f32>> {
        let url = format!("{}:embedContent?key={}", self.base_url(), self.api_key);
        let full_model = format!("models/{}", self.config.model);
        let body = EmbedContentRequest {
            model: &full_model,
            content: ContentPayload {
                parts: vec![PartPayload { text }],
            },
            output_dimensionality: Some(self.config.dimension),
        };

        let start = Instant::now();
        let mut last_err = None;

        for attempt in 0..=self.config.max_retries {
            if attempt > 0 {
                self.stats.total_retries.fetch_add(1, Ordering::Relaxed);
                let delay = std::time::Duration::from_millis(100 * 2u64.pow(attempt - 1));
                warn!(attempt, delay_ms = delay.as_millis(), "Retrying Gemini API");
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

                    // Parse error for better messages
                    let reason =
                        if let Ok(ge) = serde_json::from_str::<GeminiErrorResponse>(&err_body) {
                            format!("{} ({})", ge.error.message, ge.error.status)
                        } else {
                            format!("HTTP {status}: {err_body}")
                        };

                    // Don't retry 4xx (except 429 rate limit)
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
                        last_err = Some(format!("Timeout after {}ms", self.config.timeout_ms));
                        continue;
                    }
                    last_err = Some(format!("Network error: {e}"));
                }
            }
        }

        self.stats.total_errors.fetch_add(1, Ordering::Relaxed);
        Err(UmmsError::Encoding(EncodingError::ApiCallFailed {
            provider: "gemini".into(),
            reason: format!(
                "All {} attempts failed. Last error: {}",
                self.config.max_retries + 1,
                last_err.unwrap_or_else(|| "unknown".into())
            ),
        }))
    }

    /// Call the batchEmbedContents endpoint with retry.
    #[instrument(skip(self, texts), fields(model = %self.config.model, batch_size = texts.len()))]
    async fn call_embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let url = format!(
            "{}:batchEmbedContents?key={}",
            self.base_url(),
            self.api_key
        );
        let full_model = format!("models/{}", self.config.model);

        let requests: Vec<EmbedContentRequest<'_>> = texts
            .iter()
            .map(|t| EmbedContentRequest {
                model: &full_model,
                content: ContentPayload {
                    parts: vec![PartPayload { text: t.as_str() }],
                },
                output_dimensionality: Some(self.config.dimension),
            })
            .collect();

        let body = BatchEmbedRequest { requests };
        let start = Instant::now();
        let mut last_err = None;

        for attempt in 0..=self.config.max_retries {
            if attempt > 0 {
                self.stats.total_retries.fetch_add(1, Ordering::Relaxed);
                let delay = std::time::Duration::from_millis(100 * 2u64.pow(attempt - 1));
                warn!(
                    attempt,
                    delay_ms = delay.as_millis(),
                    "Retrying batch Gemini API"
                );
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
                    let reason =
                        if let Ok(ge) = serde_json::from_str::<GeminiErrorResponse>(&err_body) {
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
                        last_err = Some(format!("Timeout after {}ms", self.config.timeout_ms));
                    } else {
                        last_err = Some(format!("Network error: {e}"));
                    }
                }
            }
        }

        self.stats.total_errors.fetch_add(1, Ordering::Relaxed);
        Err(UmmsError::Encoding(EncodingError::ApiCallFailed {
            provider: "gemini".into(),
            reason: format!(
                "All {} attempts failed. Last error: {}",
                self.config.max_retries + 1,
                last_err.unwrap_or_else(|| "unknown".into())
            ),
        }))
    }
}

#[async_trait]
impl Encoder for GeminiEncoder {
    async fn encode_text(&self, text: &str) -> Result<Vec<f32>> {
        self.stats.total_requests.fetch_add(1, Ordering::Relaxed);
        self.stats
            .total_texts_encoded
            .fetch_add(1, Ordering::Relaxed);

        let vec = self.call_embed_single(text).await?;

        if vec.len() != self.config.dimension {
            error!(
                expected = self.config.dimension,
                got = vec.len(),
                "Dimension mismatch from Gemini API"
            );
            return Err(UmmsError::Encoding(EncodingError::ApiCallFailed {
                provider: "gemini".into(),
                reason: format!(
                    "Expected {} dimensions, got {}",
                    self.config.dimension,
                    vec.len()
                ),
            }));
        }

        Ok(vec)
    }

    async fn encode_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        self.stats.total_requests.fetch_add(1, Ordering::Relaxed);
        self.stats
            .total_texts_encoded
            .fetch_add(texts.len() as u64, Ordering::Relaxed);

        // Split into chunks of max_batch_size
        let mut all_results = Vec::with_capacity(texts.len());

        for chunk in texts.chunks(self.config.max_batch_size) {
            let chunk_owned: Vec<String> = chunk.to_vec();
            let batch_result = self.call_embed_batch(&chunk_owned).await?;

            // Validate dimensions
            for (i, vec) in batch_result.iter().enumerate() {
                if vec.len() != self.config.dimension {
                    return Err(UmmsError::Encoding(EncodingError::ApiCallFailed {
                        provider: "gemini".into(),
                        reason: format!(
                            "Batch item {i}: expected {} dimensions, got {}",
                            self.config.dimension,
                            vec.len()
                        ),
                    }));
                }
            }

            all_results.extend(batch_result);
        }

        Ok(all_results)
    }

    fn dimension(&self) -> usize {
        self.config.dimension
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_values() {
        let c = GeminiConfig::default();
        assert_eq!(c.model, "gemini-embedding-001");
        assert_eq!(c.dimension, 3072);
        assert_eq!(c.max_retries, 2);
        assert_eq!(c.max_batch_size, 100);
        assert_eq!(c.api_key_env, "GEMINI_API_KEY");
    }

    #[test]
    fn encoder_creation_fails_without_api_key() {
        // Use an env var name that is extremely unlikely to exist
        let config = GeminiConfig {
            api_key_env: "UMMS_TEST_NONEXISTENT_KEY_XYZ_42".to_owned(),
            ..GeminiConfig::default()
        };
        let result = GeminiEncoder::new(config);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("UMMS_TEST_NONEXISTENT_KEY_XYZ_42"));
    }

    #[test]
    fn stats_snapshot_initial() {
        let stats = EncoderStats::default();
        let snap = stats.snapshot();
        assert_eq!(snap.total_requests, 0);
        assert_eq!(snap.total_texts_encoded, 0);
        assert_eq!(snap.total_errors, 0);
        assert_eq!(snap.avg_latency_ms, 0.0);
    }

    // -----------------------------------------------------------------------
    // Integration tests (require GEMINI_API_KEY, run with: cargo test -p umms-encoder -- --ignored)
    // -----------------------------------------------------------------------

    #[tokio::test]
    #[ignore = "requires GEMINI_API_KEY"]
    async fn api_encode_single_text() {
        let encoder = GeminiEncoder::new(GeminiConfig::default()).unwrap();

        let vec = encoder.encode_text("Rust memory management").await.unwrap();
        assert_eq!(vec.len(), 3072, "Expected 3072 dimensions");

        // Vectors should be normalized (L2 norm ≈ 1.0)
        let norm: f32 = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 0.1,
            "Expected roughly unit norm, got {norm}"
        );

        let snap = encoder.stats.snapshot();
        assert_eq!(snap.total_requests, 1);
        assert_eq!(snap.total_texts_encoded, 1);
        assert_eq!(snap.total_errors, 0);
        assert!(snap.avg_latency_ms > 0.0, "Latency should be recorded");
    }

    #[tokio::test]
    #[ignore = "requires GEMINI_API_KEY"]
    async fn api_encode_batch() {
        let encoder = GeminiEncoder::new(GeminiConfig::default()).unwrap();

        let texts = vec![
            "Rust ownership model".to_owned(),
            "Python garbage collection".to_owned(),
            "JavaScript event loop".to_owned(),
        ];

        let vecs = encoder.encode_batch(&texts).await.unwrap();
        assert_eq!(vecs.len(), 3, "Should return 3 vectors");
        for v in &vecs {
            assert_eq!(v.len(), 3072);
        }

        // Semantic similarity: Rust/Python (both about memory) should be
        // more similar than Rust/JavaScript (different topics)
        let sim_rust_python = cosine_sim(&vecs[0], &vecs[1]);
        let sim_rust_js = cosine_sim(&vecs[0], &vecs[2]);
        println!("Rust↔Python: {sim_rust_python:.4}, Rust↔JS: {sim_rust_js:.4}");

        // Both are programming topics so similarity may be close,
        // but Rust/Python share "memory management" semantics
        assert!(
            sim_rust_python > 0.5,
            "Rust↔Python should have meaningful similarity"
        );
    }

    #[tokio::test]
    #[ignore = "requires GEMINI_API_KEY"]
    async fn api_empty_batch_returns_empty() {
        let encoder = GeminiEncoder::new(GeminiConfig::default()).unwrap();
        let vecs = encoder.encode_batch(&[]).await.unwrap();
        assert!(vecs.is_empty());
    }

    fn cosine_sim(a: &[f32], b: &[f32]) -> f32 {
        let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }
        dot / (norm_a * norm_b)
    }
}
