//! Query analysis API handler — LGSRR decomposition endpoint.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;

use umms_analyzer::lgsrr::LgsrrDecomposer;

use crate::AppState;
use crate::response::{
    LgsrrGrammaticalResponse, LgsrrLexicalResponse, LgsrrReasoningResponse, LgsrrRelationResponse,
    LgsrrRelationalResponse, LgsrrResponse, LgsrrRetrievalHintsResponse, LgsrrSemanticResponse,
};

/// POST /api/analyze — run LGSRR five-layer decomposition on a query.
pub async fn analyze_query(
    State(_state): State<Arc<AppState>>,
    Json(body): Json<AnalyzeRequest>,
) -> Result<Json<LgsrrResponse>, String> {
    let decomposition = LgsrrDecomposer::decompose(&body.query);

    let response = LgsrrResponse {
        query: decomposition.query,
        lexical: LgsrrLexicalResponse {
            key_terms: decomposition.lexical.key_terms,
            entities: decomposition.lexical.entities,
            language: decomposition.lexical.language,
            token_count: decomposition.lexical.token_count,
        },
        grammatical: LgsrrGrammaticalResponse {
            query_type: serde_json::to_value(&decomposition.grammatical.query_type)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_else(|| "conversational".to_owned()),
            is_comparison: decomposition.grammatical.is_comparison,
            is_negated: decomposition.grammatical.is_negated,
            temporal_reference: decomposition.grammatical.temporal_reference,
        },
        semantic: LgsrrSemanticResponse {
            domains: decomposition.semantic.domains,
            specificity: decomposition.semantic.specificity,
            complexity: decomposition.semantic.complexity,
        },
        relational: LgsrrRelationalResponse {
            relations: decomposition
                .relational
                .relations
                .into_iter()
                .map(|r| LgsrrRelationResponse {
                    subject: r.subject,
                    predicate: r.predicate,
                    object: r.object,
                })
                .collect(),
        },
        reasoning: LgsrrReasoningResponse {
            intent: serde_json::to_value(&decomposition.reasoning.intent)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_else(|| "explore".to_owned()),
            expected_answer: serde_json::to_value(&decomposition.reasoning.expected_answer)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_else(|| "explanation".to_owned()),
            confidence: decomposition.reasoning.confidence,
            retrieval_hints: LgsrrRetrievalHintsResponse {
                min_score_adjustment: decomposition.reasoning.retrieval_hints.min_score_adjustment,
                top_k_multiplier: decomposition.reasoning.retrieval_hints.top_k_multiplier,
                bm25_weight_adjustment: decomposition
                    .reasoning
                    .retrieval_hints
                    .bm25_weight_adjustment,
                enable_diffusion: decomposition.reasoning.retrieval_hints.enable_diffusion,
                diffusion_hops: decomposition.reasoning.retrieval_hints.diffusion_hops,
            },
        },
    };

    Ok(Json(response))
}

#[derive(serde::Deserialize)]
pub struct AnalyzeRequest {
    pub query: String,
}
