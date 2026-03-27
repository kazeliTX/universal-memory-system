//! Model pool types and traits for multi-model LLM management.
//!
//! Defines the core abstractions for routing requests to different LLM backends
//! based on task type (embedding, generation, reranking, etc.). The actual pool
//! implementation lives in `umms-encoder` to avoid circular dependencies.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::Result;

/// Task types that can be routed to different models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelTask {
    /// Text to vector embedding.
    Embedding,
    /// Text generation (summarization, entity extraction, skeleton extraction).
    Generation,
    /// Cross-encoder reranking.
    Reranking,
    /// Entity/relationship extraction from text.
    EntityExtraction,
    /// Chat/conversation.
    Chat,
}

impl ModelTask {
    /// Parse from a string (used for config deserialization).
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "embedding" => Some(Self::Embedding),
            "generation" => Some(Self::Generation),
            "reranking" => Some(Self::Reranking),
            "entity_extraction" => Some(Self::EntityExtraction),
            "chat" => Some(Self::Chat),
            _ => None,
        }
    }

    /// All known task variants.
    pub fn all() -> &'static [ModelTask] {
        &[
            Self::Embedding,
            Self::Generation,
            Self::Reranking,
            Self::EntityExtraction,
            Self::Chat,
        ]
    }
}

impl std::fmt::Display for ModelTask {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Embedding => write!(f, "embedding"),
            Self::Generation => write!(f, "generation"),
            Self::Reranking => write!(f, "reranking"),
            Self::EntityExtraction => write!(f, "entity_extraction"),
            Self::Chat => write!(f, "chat"),
        }
    }
}

/// Information about a registered model.
#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    /// Unique identifier for this model configuration.
    pub id: String,
    /// Provider name (e.g., "gemini", "openai", "local").
    pub provider: String,
    /// Actual model name (e.g., "gemini-embedding-001", "gemini-2.5-flash").
    pub model_name: String,
    /// What tasks this model supports.
    pub tasks: Vec<ModelTask>,
    /// Embedding dimension (for embedding models).
    pub dimension: Option<usize>,
    /// Maximum output tokens (for generative models).
    pub max_tokens: Option<usize>,
    /// Whether this model is currently available (API key set, etc.).
    pub available: bool,
}

/// Trait for a model provider that can handle multiple task types.
///
/// Each provider wraps a single model configuration. The model pool
/// routes requests to the appropriate provider based on task type.
#[async_trait]
pub trait ModelProvider: Send + Sync {
    /// Get info about this provider's model.
    fn info(&self) -> ModelInfo;

    /// Check if this provider supports a given task.
    fn supports(&self, task: ModelTask) -> bool;

    /// Generate embeddings for text.
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;

    /// Generate embeddings for a batch of texts.
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;

    /// Generate text (for generation/chat/entity extraction tasks).
    async fn generate(&self, prompt: &str, max_tokens: Option<usize>) -> Result<String>;

    /// Get the embedding dimension (if applicable).
    fn embedding_dimension(&self) -> Option<usize>;

    /// Generate embedding for an image (base64 encoded).
    ///
    /// Only supported by multimodal-capable embedding models (e.g., gemini-embedding-002).
    /// Default implementation returns an error — only providers that support multimodal
    /// embedding need to override this.
    async fn embed_image(&self, _image_base64: &str, _mime_type: &str) -> Result<Vec<f32>> {
        Err(crate::error::UmmsError::Internal(
            "Image embedding not supported by this provider".into(),
        ))
    }

    /// Transcribe audio to text.
    ///
    /// Uses a generative model with audio input support (e.g., Gemini 2.5 Flash).
    /// Default implementation returns an error — only providers that support audio
    /// input need to override this.
    async fn transcribe_audio(&self, _audio_base64: &str, _mime_type: &str) -> Result<String> {
        Err(crate::error::UmmsError::Internal(
            "Audio transcription not supported by this provider".into(),
        ))
    }
}

/// Type alias for a shared provider reference.
pub type SharedProvider = Arc<dyn ModelProvider>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_task_from_str_loose() {
        assert_eq!(ModelTask::from_str_loose("embedding"), Some(ModelTask::Embedding));
        assert_eq!(ModelTask::from_str_loose("GENERATION"), Some(ModelTask::Generation));
        assert_eq!(ModelTask::from_str_loose("entity_extraction"), Some(ModelTask::EntityExtraction));
        assert_eq!(ModelTask::from_str_loose("chat"), Some(ModelTask::Chat));
        assert_eq!(ModelTask::from_str_loose("reranking"), Some(ModelTask::Reranking));
        assert_eq!(ModelTask::from_str_loose("unknown"), None);
    }

    #[test]
    fn model_task_display() {
        assert_eq!(ModelTask::Embedding.to_string(), "embedding");
        assert_eq!(ModelTask::Generation.to_string(), "generation");
        assert_eq!(ModelTask::EntityExtraction.to_string(), "entity_extraction");
    }

    #[test]
    fn model_task_all_variants() {
        assert_eq!(ModelTask::all().len(), 5);
    }

    // -----------------------------------------------------------------------
    // Tests for default trait method implementations (embed_image, transcribe_audio)
    // -----------------------------------------------------------------------

    /// Minimal provider that only implements the required methods.
    struct StubProvider;

    #[async_trait]
    impl ModelProvider for StubProvider {
        fn info(&self) -> ModelInfo {
            ModelInfo {
                id: "stub".to_string(),
                provider: "stub".to_string(),
                model_name: "stub-model".to_string(),
                tasks: vec![],
                dimension: None,
                max_tokens: None,
                available: false,
            }
        }
        fn supports(&self, _task: ModelTask) -> bool { false }
        async fn embed(&self, _text: &str) -> Result<Vec<f32>> {
            Ok(vec![0.0; 8])
        }
        async fn embed_batch(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>> {
            Ok(vec![])
        }
        async fn generate(&self, _prompt: &str, _max_tokens: Option<usize>) -> Result<String> {
            Ok(String::new())
        }
        fn embedding_dimension(&self) -> Option<usize> { Some(8) }
    }

    #[tokio::test]
    async fn embed_image_default_returns_error() {
        let provider = StubProvider;
        let result = provider.embed_image("base64data", "image/png").await;
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("Image embedding not supported"));
    }

    #[tokio::test]
    async fn transcribe_audio_default_returns_error() {
        let provider = StubProvider;
        let result = provider.transcribe_audio("base64data", "audio/wav").await;
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("Audio transcription not supported"));
    }

    #[test]
    fn model_info_serializes() {
        let info = ModelInfo {
            id: "test".to_string(),
            provider: "gemini".to_string(),
            model_name: "gemini-embedding-001".to_string(),
            tasks: vec![ModelTask::Embedding],
            dimension: Some(3072),
            max_tokens: None,
            available: true,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("gemini-embedding-001"));
        assert!(json.contains("3072"));
    }
}
