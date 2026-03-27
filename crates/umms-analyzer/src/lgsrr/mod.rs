//! LGSRR — Lexical, Grammatical, Semantic, Relational, Reasoning.
//!
//! Five-layer semantic decomposition of queries. Each layer extracts
//! progressively higher-level features from the raw query text, producing
//! a structured analysis that drives retrieval parameter tuning.

pub mod decomposer;

pub use decomposer::LgsrrDecomposer;

use serde::Serialize;

// ---------------------------------------------------------------------------
// Top-level decomposition result
// ---------------------------------------------------------------------------

/// Five-layer semantic decomposition of a query.
#[derive(Debug, Clone, Serialize)]
pub struct LgsrrDecomposition {
    /// Raw query text.
    pub query: String,

    /// L: Lexical — key terms and named entities extracted from the query.
    pub lexical: LexicalLayer,

    /// G: Grammatical — sentence structure and query type.
    pub grammatical: GrammaticalLayer,

    /// S: Semantic — core meaning and topic domains.
    pub semantic: SemanticLayer,

    /// R: Relational — relationships between concepts in the query.
    pub relational: RelationalLayer,

    /// R: Reasoning — inferred user intent and expected answer type.
    pub reasoning: ReasoningLayer,
}

// ---------------------------------------------------------------------------
// L: Lexical layer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct LexicalLayer {
    /// Key terms extracted (via simple extraction / stopword filtering).
    pub key_terms: Vec<String>,
    /// Named entities detected (capitalised words, CJK proper nouns).
    pub entities: Vec<String>,
    /// Language detected ("zh", "en", "mixed").
    pub language: String,
    /// Query length in tokens (estimated).
    pub token_count: usize,
}

// ---------------------------------------------------------------------------
// G: Grammatical layer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct GrammaticalLayer {
    /// Query type classification.
    pub query_type: QueryType,
    /// Whether the query is a comparison ("A vs B").
    pub is_comparison: bool,
    /// Whether the query is negated ("not", "don't", "没有").
    pub is_negated: bool,
    /// Temporal reference if any ("yesterday", "last week", "2024年").
    pub temporal_reference: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum QueryType {
    /// Asking for facts ("what is X", "X是什么")
    Factual,
    /// Asking for instructions ("how to X", "如何X")
    Procedural,
    /// Asking for explanation ("why X", "为什么X")
    Causal,
    /// Asking for comparison ("X vs Y", "X和Y的区别")
    Comparative,
    /// Asking for opinion/recommendation ("should I", "推荐")
    Evaluative,
    /// General/conversational
    Conversational,
}

// ---------------------------------------------------------------------------
// S: Semantic layer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct SemanticLayer {
    /// Primary topic domain(s) detected.
    pub domains: Vec<String>,
    /// Specificity: how narrow/broad is the query (0.0 = very broad, 1.0 = very specific).
    pub specificity: f32,
    /// Complexity estimate (0.0 = simple, 1.0 = very complex).
    pub complexity: f32,
}

// ---------------------------------------------------------------------------
// R: Relational layer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct RelationalLayer {
    /// Concept pairs and their relationships detected in the query.
    pub relations: Vec<QueryRelation>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QueryRelation {
    pub subject: String,
    pub predicate: String, // "is", "causes", "compared_to", "affects"
    pub object: String,
}

// ---------------------------------------------------------------------------
// R: Reasoning layer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ReasoningLayer {
    /// Inferred user intent.
    pub intent: UserIntent,
    /// Expected answer type.
    pub expected_answer: ExpectedAnswer,
    /// Confidence in the analysis (0.0-1.0).
    pub confidence: f32,
    /// Suggested retrieval strategy adjustments based on analysis.
    pub retrieval_hints: RetrievalHints,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum UserIntent {
    /// Wants to understand something.
    Learn,
    /// Wants to fix a problem.
    Solve,
    /// Wants to build something.
    Create,
    /// Wants to evaluate options.
    Compare,
    /// Wants to find something previously stored.
    Recall,
    /// Browsing/discovering.
    Explore,
    /// Casual chat.
    Converse,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ExpectedAnswer {
    /// Detailed explanation.
    Explanation,
    /// Ordered instructions.
    StepByStep,
    /// Code example.
    CodeSnippet,
    /// Pros/cons table.
    Comparison,
    /// Short factual response.
    FactualAnswer,
    /// Suggested choice.
    Recommendation,
    /// Simple response (for chat).
    Acknowledgment,
}

#[derive(Debug, Clone, Serialize)]
pub struct RetrievalHints {
    /// Suggested min_score adjustment (e.g. -0.1 for broad, +0.1 for precise).
    pub min_score_adjustment: f32,
    /// Suggested top_k multiplier (e.g. 2.0 for exploratory, 0.5 for precise).
    pub top_k_multiplier: f32,
    /// Suggested BM25 weight adjustment (e.g. +0.2 for keyword-heavy queries).
    pub bm25_weight_adjustment: f32,
    /// Whether to enable LIF diffusion.
    pub enable_diffusion: bool,
    /// Suggested diffusion hops.
    pub diffusion_hops: usize,
}

impl Default for RetrievalHints {
    fn default() -> Self {
        Self {
            min_score_adjustment: 0.0,
            top_k_multiplier: 1.0,
            bm25_weight_adjustment: 0.0,
            enable_diffusion: false,
            diffusion_hops: 2,
        }
    }
}
