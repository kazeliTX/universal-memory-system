//! Quick query intent classification.
//!
//! A lighter-weight alternative to full LGSRR decomposition for fast
//! decisions: should we retrieve? What kind of response is expected?
//! Replaces the old `is_greeting()` hack with proper intent analysis.

use serde::Serialize;

use crate::lgsrr::{LgsrrDecomposer, UserIntent};

/// Quick intent classification result.
#[derive(Debug, Clone, Serialize)]
pub struct QueryIntent {
    /// The classified intent.
    pub intent: UserIntent,
    /// Should we search memories for this query?
    pub needs_retrieval: bool,
    /// Should we call an LLM to generate a response?
    pub needs_generation: bool,
    /// Confidence in the classification (0.0-1.0).
    pub confidence: f32,
}

/// Fast query intent classifier.
///
/// Uses a subset of LGSRR heuristics for speed. No LLM calls.
pub struct IntentClassifier;

// Greetings and acknowledgments that do not need retrieval
const GREETING_PATTERNS: &[&str] = &[
    "hello", "hi", "hey", "good morning", "good afternoon", "good evening",
    "thanks", "thank you", "ok", "okay", "got it", "sure", "yes", "no",
    "bye", "goodbye", "see you",
    "你好", "嗨", "谢谢", "好的", "可以", "再见", "早上好", "晚上好",
    "嗯", "哦", "行",
];

impl IntentClassifier {
    /// Returns `true` for greetings, acknowledgments, and very short messages
    /// that do not warrant a retrieval pipeline call.
    pub fn is_non_retrieval_intent(query: &str) -> bool {
        let trimmed = query.trim();
        // Very short messages (< 3 chars) are never retrieval-worthy
        if trimmed.chars().count() < 3 {
            return true;
        }
        let result = Self::classify(trimmed);
        !result.needs_retrieval
    }

    /// Returns `true` when the query is a simple greeting or farewell.
    pub fn is_greeting(query: &str) -> bool {
        let trimmed = query.trim();
        let lower = trimmed.to_lowercase();
        trimmed.chars().count() <= 10 && is_greeting(&lower)
    }

    /// Returns `true` when the query is a simple acknowledgment (ok, thanks, etc.).
    pub fn is_acknowledgment(query: &str) -> bool {
        let lower = query.trim().to_lowercase();
        matches!(
            lower.as_str(),
            "ok" | "okay" | "got it" | "sure" | "yes" | "no"
                | "thanks" | "thank you" | "好的" | "可以" | "嗯" | "哦" | "行" | "谢谢"
        )
    }

    /// Classify a query's intent quickly.
    pub fn classify(query: &str) -> QueryIntent {
        let trimmed = query.trim();
        let lower = trimmed.to_lowercase();

        // Fast path: check for greetings / short acknowledgments
        if trimmed.chars().count() <= 10 && is_greeting(&lower) {
            return QueryIntent {
                intent: UserIntent::Converse,
                needs_retrieval: false,
                needs_generation: true,
                confidence: 0.9,
            };
        }

        // Use LGSRR grammatical analysis for the rest
        let decomposition = LgsrrDecomposer::decompose(query);
        let intent = decomposition.reasoning.intent.clone();

        let needs_retrieval = match &intent {
            UserIntent::Converse => false,
            _ => true,
        };

        let needs_generation = true; // Almost always need to generate

        QueryIntent {
            intent,
            needs_retrieval,
            needs_generation,
            confidence: decomposition.reasoning.confidence,
        }
    }
}

fn is_greeting(lower: &str) -> bool {
    GREETING_PATTERNS.iter().any(|p| lower == *p || lower.starts_with(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greeting_does_not_need_retrieval() {
        let intent = IntentClassifier::classify("hello");
        assert_eq!(intent.intent, UserIntent::Converse);
        assert!(!intent.needs_retrieval);
        assert!(intent.needs_generation);
    }

    #[test]
    fn chinese_greeting_does_not_need_retrieval() {
        let intent = IntentClassifier::classify("你好");
        assert_eq!(intent.intent, UserIntent::Converse);
        assert!(!intent.needs_retrieval);
    }

    #[test]
    fn knowledge_query_needs_retrieval() {
        let intent = IntentClassifier::classify("What is the Rust borrow checker?");
        assert!(intent.needs_retrieval);
        assert_eq!(intent.intent, UserIntent::Learn);
    }

    #[test]
    fn procedural_query_needs_retrieval() {
        let intent = IntentClassifier::classify("How to deploy a Docker container?");
        assert!(intent.needs_retrieval);
    }

    #[test]
    fn acknowledgment_does_not_need_retrieval() {
        let intent = IntentClassifier::classify("ok");
        assert!(!intent.needs_retrieval);
    }
}
