//! Pluggable tokenization strategies for tag extraction and text analysis.
//!
//! Three implementations:
//! - `jieba`: Chinese word segmentation via jieba-rs + whitespace for English
//! - `llm`: LLM-based key term extraction (highest quality, costs API calls)
//! - `whitespace`: Simple whitespace splitting (zero cost, English-only)
//!
//! Select via `umms.toml`:
//! ```toml
//! [tag]
//! tokenizer = "jieba"  # or "llm" or "whitespace"
//! ```

mod jieba_tokenizer;
mod llm_tokenizer;
mod whitespace_tokenizer;

pub use jieba_tokenizer::JiebaTokenizer;
pub use llm_tokenizer::LlmTokenizer;
pub use whitespace_tokenizer::WhitespaceTokenizer;

/// Trait for text segmentation / key term extraction.
///
/// Implementations must be `Send + Sync` for use in async contexts.
/// The output is a list of meaningful terms extracted from input text.
pub trait Tokenizer: Send + Sync {
    /// Segment text into meaningful tokens/terms.
    ///
    /// The output should be filtered (no stopwords, no punctuation-only tokens)
    /// and ready for use as tag candidates.
    fn tokenize(&self, text: &str) -> Vec<String>;

    /// Name of this tokenizer (for logging/dashboard display).
    fn name(&self) -> &'static str;
}

/// Build a tokenizer from config string.
///
/// - `"jieba"` → `JiebaTokenizer` (default)
/// - `"llm"` → `LlmTokenizer` (requires encoder)
/// - `"whitespace"` → `WhitespaceTokenizer`
pub fn build_tokenizer(
    strategy: &str,
    encoder: Option<std::sync::Arc<dyn umms_core::traits::Encoder>>,
) -> Box<dyn Tokenizer> {
    match strategy {
        "llm" => {
            if let Some(enc) = encoder {
                Box::new(LlmTokenizer::new(enc))
            } else {
                tracing::warn!(
                    "LLM tokenizer requested but no encoder available, falling back to jieba"
                );
                Box::new(JiebaTokenizer::new())
            }
        }
        "whitespace" => Box::new(WhitespaceTokenizer::new()),
        _ => Box::new(JiebaTokenizer::new()), // default: jieba
    }
}
