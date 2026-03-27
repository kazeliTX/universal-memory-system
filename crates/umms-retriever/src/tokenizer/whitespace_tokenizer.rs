//! Simple whitespace-based tokenizer.
//!
//! Zero dependencies, zero cost. Suitable for English text only.
//! Use as a lightweight fallback when jieba or LLM is unavailable.

use super::Tokenizer;

const STOPWORDS: &[&str] = &[
    "a", "an", "the", "and", "or", "but", "in", "on", "at", "to", "for",
    "of", "with", "by", "from", "as", "is", "was", "are", "were", "be",
    "been", "being", "have", "has", "had", "do", "does", "did", "will",
    "would", "could", "should", "may", "might", "shall", "can", "need",
    "it", "its", "this", "that", "these", "those", "not", "no", "so",
];

/// Simple whitespace tokenizer with English stopword filtering.
pub struct WhitespaceTokenizer;

impl WhitespaceTokenizer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WhitespaceTokenizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Tokenizer for WhitespaceTokenizer {
    fn tokenize(&self, text: &str) -> Vec<String> {
        let stops: std::collections::HashSet<&str> = STOPWORDS.iter().copied().collect();
        text.split_whitespace()
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_owned())
            .filter(|w| w.len() >= 2 && !stops.contains(w.to_lowercase().as_str()))
            .collect()
    }

    fn name(&self) -> &'static str {
        "whitespace"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_splitting() {
        let tok = WhitespaceTokenizer::new();
        let result = tok.tokenize("Rust async runtime optimization");
        assert_eq!(result, vec!["Rust", "async", "runtime", "optimization"]);
    }

    #[test]
    fn filters_stopwords() {
        let tok = WhitespaceTokenizer::new();
        let result = tok.tokenize("the quick brown fox and the lazy dog");
        assert!(!result.contains(&"the".to_owned()));
        assert!(!result.contains(&"and".to_owned()));
        assert!(result.contains(&"quick".to_owned()));
    }
}
