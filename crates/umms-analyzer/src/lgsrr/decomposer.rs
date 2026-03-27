//! `LgsrrDecomposer` — heuristic five-layer query decomposition.
//!
//! All analysis is rule-based (no LLM call) for sub-millisecond latency.
//! Supports both English and Chinese queries.

use super::{
    ExpectedAnswer, GrammaticalLayer, LexicalLayer, LgsrrDecomposition, QueryRelation, QueryType,
    ReasoningLayer, RelationalLayer, RetrievalHints, SemanticLayer, UserIntent,
};

/// Heuristic LGSRR decomposer.
///
/// Stateless — all methods are pure functions on the query string.
pub struct LgsrrDecomposer;

// ---------------------------------------------------------------------------
// English stopwords (small set for key-term filtering)
// ---------------------------------------------------------------------------

const EN_STOPWORDS: &[&str] = &[
    "a", "an", "the", "is", "are", "was", "were", "be", "been", "being", "have", "has", "had",
    "do", "does", "did", "will", "would", "shall", "should", "may", "might", "must", "can",
    "could", "to", "of", "in", "for", "on", "with", "at", "by", "from", "as", "into", "through",
    "during", "before", "after", "above", "below", "between", "out", "off", "over", "under",
    "again", "further", "then", "once", "here", "there", "when", "where", "why", "how", "all",
    "each", "every", "both", "few", "more", "most", "other", "some", "such", "no", "nor", "not",
    "only", "own", "same", "so", "than", "too", "very", "just", "because", "but", "and", "or",
    "if", "while", "about", "up", "its", "it", "i", "me", "my", "we", "our", "you", "your",
    "he", "him", "his", "she", "her", "they", "them", "their", "this", "that", "these", "those",
    "what", "which", "who", "whom", "am",
];

// ---------------------------------------------------------------------------
// Chinese stopwords (common functional words)
// ---------------------------------------------------------------------------

const ZH_STOPWORDS: &[&str] = &[
    "的", "了", "在", "是", "我", "有", "和", "就", "不", "人", "都", "一", "一个", "上", "也",
    "很", "到", "说", "要", "去", "你", "会", "着", "没有", "看", "好", "自己", "这", "他", "她",
    "它", "那", "被", "从", "把", "还", "吗", "呢", "吧", "啊", "呀", "吧", "嗯", "哦",
];

impl LgsrrDecomposer {
    /// Decompose a query into five semantic layers.
    ///
    /// This uses heuristic analysis (no LLM call) for speed.
    pub fn decompose(query: &str) -> LgsrrDecomposition {
        let lexical = Self::analyze_lexical(query);
        let grammatical = Self::analyze_grammatical(query);
        let semantic = Self::analyze_semantic(query, &lexical);
        let relational = Self::analyze_relational(query, &lexical);
        let reasoning =
            Self::analyze_reasoning(query, &lexical, &grammatical, &semantic);

        LgsrrDecomposition {
            query: query.to_owned(),
            lexical,
            grammatical,
            semantic,
            relational,
            reasoning,
        }
    }

    // -----------------------------------------------------------------------
    // L: Lexical
    // -----------------------------------------------------------------------

    fn analyze_lexical(query: &str) -> LexicalLayer {
        let language = detect_language(query);
        let tokens = tokenize(query);
        let token_count = tokens.len();

        // Filter stopwords to get key terms
        let key_terms: Vec<String> = tokens
            .iter()
            .filter(|t| t.chars().count() > 1)
            .filter(|t| {
                let lower = t.to_lowercase();
                !EN_STOPWORDS.contains(&lower.as_str()) && !ZH_STOPWORDS.contains(&lower.as_str())
            })
            .cloned()
            .collect();

        // Detect named entities: capitalised words (for English)
        let entities: Vec<String> = tokens
            .iter()
            .filter(|t| {
                let first = t.chars().next();
                first.is_some_and(|c| c.is_uppercase())
                    && t.chars().count() > 1
                    && !is_sentence_start(query, t)
            })
            .cloned()
            .collect();

        LexicalLayer {
            key_terms,
            entities,
            language,
            token_count,
        }
    }

    // -----------------------------------------------------------------------
    // G: Grammatical
    // -----------------------------------------------------------------------

    fn analyze_grammatical(query: &str) -> GrammaticalLayer {
        let lower = query.to_lowercase();
        let query_type = detect_query_type(&lower, query);
        let is_comparison = detect_comparison(&lower, query);
        let is_negated = detect_negation(&lower, query);
        let temporal_reference = detect_temporal(&lower, query);

        // Override query_type if comparison detected
        let query_type = if is_comparison && query_type == QueryType::Conversational {
            QueryType::Comparative
        } else {
            query_type
        };

        GrammaticalLayer {
            query_type,
            is_comparison,
            is_negated,
            temporal_reference,
        }
    }

    // -----------------------------------------------------------------------
    // S: Semantic
    // -----------------------------------------------------------------------

    fn analyze_semantic(query: &str, lexical: &LexicalLayer) -> SemanticLayer {
        let domains = detect_domains(query, &lexical.key_terms);

        // Specificity: more key terms + longer query = more specific
        let term_factor = (lexical.key_terms.len() as f32 / 8.0).clamp(0.0, 1.0);
        let length_factor = (query.chars().count() as f32 / 50.0).clamp(0.3, 1.0);
        let specificity = (term_factor * length_factor).clamp(0.0, 1.0);

        // Complexity: multiple domains + comparison patterns + temporal = more complex
        let domain_factor = (domains.len() as f32 / 3.0).clamp(0.0, 1.0);
        let length_complexity = (query.chars().count() as f32 / 100.0).clamp(0.0, 1.0);
        let complexity = ((domain_factor * 0.4 + length_complexity * 0.3 + term_factor * 0.3))
            .clamp(0.0, 1.0);

        SemanticLayer {
            domains,
            specificity,
            complexity,
        }
    }

    // -----------------------------------------------------------------------
    // R: Relational
    // -----------------------------------------------------------------------

    fn analyze_relational(query: &str, lexical: &LexicalLayer) -> RelationalLayer {
        let relations = detect_relations(query, lexical);
        RelationalLayer { relations }
    }

    // -----------------------------------------------------------------------
    // R: Reasoning
    // -----------------------------------------------------------------------

    fn analyze_reasoning(
        _query: &str,
        lexical: &LexicalLayer,
        grammatical: &GrammaticalLayer,
        semantic: &SemanticLayer,
    ) -> ReasoningLayer {
        let intent = infer_intent(grammatical, semantic);
        let expected_answer = infer_expected_answer(&intent, grammatical, semantic);
        let confidence = compute_confidence(lexical, grammatical);
        let retrieval_hints = compute_retrieval_hints(semantic, grammatical, &intent);

        ReasoningLayer {
            intent,
            expected_answer,
            confidence,
            retrieval_hints,
        }
    }
}

// ===========================================================================
// Helper functions
// ===========================================================================

/// Detect language by CJK character ratio.
fn detect_language(query: &str) -> String {
    let total = query.chars().filter(|c| !c.is_whitespace()).count();
    if total == 0 {
        return "en".to_owned();
    }
    let cjk = query
        .chars()
        .filter(|c| is_cjk_char(*c))
        .count();
    let ratio = cjk as f32 / total as f32;
    if ratio > 0.5 {
        "zh".to_owned()
    } else if ratio > 0.1 {
        "mixed".to_owned()
    } else {
        "en".to_owned()
    }
}

fn is_cjk_char(c: char) -> bool {
    matches!(c,
        '\u{4E00}'..='\u{9FFF}'   // CJK Unified Ideographs
        | '\u{3400}'..='\u{4DBF}' // CJK Extension A
        | '\u{F900}'..='\u{FAFF}' // CJK Compatibility Ideographs
        | '\u{3000}'..='\u{303F}' // CJK Symbols and Punctuation
    )
}

/// Simple tokenizer: split on whitespace and punctuation.
/// For CJK text, treat each character as a token.
fn tokenize(query: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();

    for c in query.chars() {
        if is_cjk_char(c) {
            // Flush any accumulated Latin token
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            // Each CJK char is its own token
            tokens.push(c.to_string());
        } else if c.is_alphanumeric() || c == '_' || c == '-' {
            current.push(c);
        } else {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// Rough check: is `word` at the start of a sentence in `query`?
fn is_sentence_start(query: &str, word: &str) -> bool {
    if let Some(pos) = query.find(word) {
        if pos == 0 {
            return true;
        }
        let before = &query[..pos];
        let trimmed = before.trim_end();
        trimmed.ends_with('.') || trimmed.ends_with('?') || trimmed.ends_with('!')
    } else {
        false
    }
}

// ---------------------------------------------------------------------------
// Query type detection
// ---------------------------------------------------------------------------

fn detect_query_type(lower: &str, _raw: &str) -> QueryType {
    // Chinese patterns — order matters: check "为什么" before "什么"
    if contains_any(lower, &["为什么", "为何", "原因", "为啥"]) {
        return QueryType::Causal;
    }
    if contains_any(lower, &["如何", "怎么", "怎样", "怎么办", "步骤"]) {
        return QueryType::Procedural;
    }
    if contains_any(lower, &["什么", "是什么", "哪些", "哪个", "哪里", "谁"]) {
        return QueryType::Factual;
    }
    if contains_any(lower, &["对比", "区别", "比较", "和…的区别", "和…的不同"]) {
        return QueryType::Comparative;
    }
    if contains_any(lower, &["推荐", "建议", "应该", "该不该", "值不值"]) {
        return QueryType::Evaluative;
    }

    // English patterns
    if starts_with_any(lower, &["what ", "what's ", "which ", "who ", "who's ", "where "]) {
        return QueryType::Factual;
    }
    if starts_with_any(lower, &["how to ", "how do ", "how can ", "how should "]) {
        return QueryType::Procedural;
    }
    if starts_with_any(lower, &["why ", "how come "]) {
        return QueryType::Causal;
    }
    if contains_any(lower, &[" vs ", " vs. ", " versus ", " compared to ", " difference between "]) {
        return QueryType::Comparative;
    }
    if starts_with_any(lower, &["should ", "recommend ", "best ", "suggest "])
        || contains_any(lower, &[" should i ", " recommend ", " best "])
    {
        return QueryType::Evaluative;
    }

    // Question mark heuristic
    if lower.ends_with('?') {
        return QueryType::Factual;
    }

    QueryType::Conversational
}

fn detect_comparison(lower: &str, raw: &str) -> bool {
    contains_any(lower, &[
        " vs ", " vs. ", " versus ", " compared to ", " difference between ",
        " differ from ", " better than ", " worse than ",
    ]) || contains_any(raw, &[
        "对比", "区别", "比较", "和…的区别", "A和B",
    ]) || {
        // Pattern: "X 和 Y 的区别"
        raw.contains('和') && (raw.contains("区别") || raw.contains("不同") || raw.contains("差异"))
    }
}

fn detect_negation(lower: &str, raw: &str) -> bool {
    contains_any(lower, &[
        "not ", "don't ", "doesn't ", "didn't ", "won't ", "wouldn't ",
        "can't ", "cannot ", "isn't ", "aren't ", "wasn't ", "weren't ",
        " no ", "never ",
    ]) || contains_any(raw, &["不", "没", "没有", "别", "非", "无"])
}

fn detect_temporal(lower: &str, raw: &str) -> Option<String> {
    // English temporal patterns
    let en_temporal = [
        "yesterday", "today", "tomorrow", "last week", "last month", "last year",
        "this week", "this month", "this year", "next week", "next month", "next year",
        "recently", "lately", "earlier", "before",
    ];
    for pattern in &en_temporal {
        if lower.contains(pattern) {
            return Some((*pattern).to_owned());
        }
    }

    // Chinese temporal patterns
    let zh_temporal = [
        "昨天", "今天", "明天", "上周", "上个月", "去年",
        "这周", "这个月", "今年", "下周", "下个月", "明年",
        "最近", "以前", "之前",
    ];
    for pattern in &zh_temporal {
        if raw.contains(pattern) {
            return Some((*pattern).to_owned());
        }
    }

    // Year patterns (e.g. "2024", "2024年")
    // Simple: look for 4-digit numbers starting with 19 or 20
    let bytes = lower.as_bytes();
    for i in 0..lower.len().saturating_sub(3) {
        if bytes[i].is_ascii_digit()
            && bytes[i + 1].is_ascii_digit()
            && bytes[i + 2].is_ascii_digit()
            && bytes[i + 3].is_ascii_digit()
        {
            let year_str = &lower[i..i + 4];
            if let Ok(year) = year_str.parse::<u32>() {
                if (1900..=2100).contains(&year) {
                    return Some(year_str.to_owned());
                }
            }
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Domain detection
// ---------------------------------------------------------------------------

fn detect_domains(query: &str, key_terms: &[String]) -> Vec<String> {
    let lower = query.to_lowercase();
    let mut domains = Vec::new();

    let domain_patterns: &[(&str, &[&str])] = &[
        ("programming", &[
            "code", "coding", "program", "function", "variable", "algorithm", "debug",
            "compile", "rust", "python", "javascript", "typescript", "java", "api",
            "database", "sql", "git", "docker", "kubernetes", "linux", "server",
            "编程", "代码", "函数", "变量", "算法", "调试", "编译", "数据库",
        ]),
        ("science", &[
            "physics", "chemistry", "biology", "math", "mathematics", "equation",
            "theorem", "experiment", "hypothesis", "research", "quantum", "molecule",
            "物理", "化学", "生物", "数学", "方程", "实验", "研究", "量子",
        ]),
        ("writing", &[
            "write", "writing", "essay", "article", "story", "novel", "poetry",
            "grammar", "sentence", "paragraph", "draft", "edit",
            "写作", "文章", "故事", "小说", "诗", "语法", "段落",
        ]),
        ("business", &[
            "business", "company", "market", "finance", "investment", "startup",
            "management", "strategy", "revenue", "profit", "customer",
            "商业", "公司", "市场", "金融", "投资", "管理", "策略",
        ]),
        ("health", &[
            "health", "medical", "doctor", "disease", "symptom", "treatment",
            "medicine", "diagnosis", "therapy", "surgery",
            "健康", "医疗", "医生", "疾病", "症状", "治疗", "药",
        ]),
        ("education", &[
            "learn", "learning", "study", "education", "course", "tutorial",
            "teach", "student", "school", "university",
            "学习", "教育", "课程", "教程", "教学", "学生", "学校",
        ]),
    ];

    for (domain, keywords) in domain_patterns {
        let has_match = keywords.iter().any(|kw| lower.contains(kw))
            || key_terms.iter().any(|t| {
                let tl = t.to_lowercase();
                keywords.iter().any(|kw| tl.contains(kw))
            });
        if has_match {
            domains.push((*domain).to_owned());
        }
    }

    domains
}

// ---------------------------------------------------------------------------
// Relation detection
// ---------------------------------------------------------------------------

fn detect_relations(query: &str, lexical: &LexicalLayer) -> Vec<QueryRelation> {
    let mut relations = Vec::new();

    // Pattern: "X vs Y" or "X versus Y"
    for sep in &[" vs ", " vs. ", " versus "] {
        if let Some(pos) = query.to_lowercase().find(sep) {
            let subject = query[..pos].trim().to_owned();
            let object = query[pos + sep.len()..].trim().to_owned();
            if !subject.is_empty() && !object.is_empty() {
                relations.push(QueryRelation {
                    subject: last_n_words(&subject, 3),
                    predicate: "compared_to".to_owned(),
                    object: first_n_words(&object, 3),
                });
            }
        }
    }

    // Pattern: "X 和 Y 的区别" (Chinese comparison)
    if query.contains('和') && (query.contains("区别") || query.contains("不同") || query.contains("差异")) {
        if let Some(he_pos) = query.find('和') {
            let subject = query[..he_pos].trim().to_owned();
            let rest = &query[he_pos + '和'.len_utf8()..];
            let object = rest
                .split(|c: char| c == '的' || c == '有')
                .next()
                .unwrap_or("")
                .trim()
                .to_owned();
            if !subject.is_empty() && !object.is_empty() {
                relations.push(QueryRelation {
                    subject,
                    predicate: "compared_to".to_owned(),
                    object,
                });
            }
        }
    }

    // Pattern: "X causes Y" / "X 导致 Y"
    let lower = query.to_lowercase();
    for pattern in &[" causes ", " leads to ", " results in "] {
        if let Some(pos) = lower.find(pattern) {
            let subject = query[..pos].trim().to_owned();
            let object = query[pos + pattern.len()..].trim().to_owned();
            if !subject.is_empty() && !object.is_empty() {
                relations.push(QueryRelation {
                    subject: last_n_words(&subject, 3),
                    predicate: "causes".to_owned(),
                    object: first_n_words(&object, 3),
                });
            }
        }
    }
    if let Some(pos) = query.find("导致") {
        let subject = query[..pos].trim().to_owned();
        let object = query[pos + "导致".len()..].trim().to_owned();
        if !subject.is_empty() && !object.is_empty() {
            relations.push(QueryRelation {
                subject,
                predicate: "causes".to_owned(),
                object,
            });
        }
    }

    // Pattern: "X is Y" — only if there are exactly two key terms
    if relations.is_empty() && lexical.key_terms.len() == 2 {
        if lower.contains(" is ") || query.contains('是') {
            relations.push(QueryRelation {
                subject: lexical.key_terms[0].clone(),
                predicate: "is".to_owned(),
                object: lexical.key_terms[1].clone(),
            });
        }
    }

    relations
}

fn last_n_words(s: &str, n: usize) -> String {
    let words: Vec<&str> = s.split_whitespace().collect();
    let start = words.len().saturating_sub(n);
    words[start..].join(" ")
}

fn first_n_words(s: &str, n: usize) -> String {
    let words: Vec<&str> = s.split_whitespace().collect();
    let end = n.min(words.len());
    words[..end].join(" ")
}

// ---------------------------------------------------------------------------
// Reasoning helpers
// ---------------------------------------------------------------------------

fn infer_intent(grammatical: &GrammaticalLayer, semantic: &SemanticLayer) -> UserIntent {
    match grammatical.query_type {
        QueryType::Factual => UserIntent::Learn,
        QueryType::Procedural => {
            if semantic.domains.iter().any(|d| d == "programming") {
                UserIntent::Create
            } else {
                UserIntent::Solve
            }
        }
        QueryType::Causal => UserIntent::Learn,
        QueryType::Comparative => UserIntent::Compare,
        QueryType::Evaluative => UserIntent::Compare,
        QueryType::Conversational => {
            if semantic.specificity < 0.2 {
                UserIntent::Converse
            } else {
                UserIntent::Explore
            }
        }
    }
}

fn infer_expected_answer(
    intent: &UserIntent,
    grammatical: &GrammaticalLayer,
    semantic: &SemanticLayer,
) -> ExpectedAnswer {
    match intent {
        UserIntent::Learn => {
            if grammatical.query_type == QueryType::Causal {
                ExpectedAnswer::Explanation
            } else {
                ExpectedAnswer::FactualAnswer
            }
        }
        UserIntent::Solve => ExpectedAnswer::StepByStep,
        UserIntent::Create => {
            if semantic.domains.iter().any(|d| d == "programming") {
                ExpectedAnswer::CodeSnippet
            } else {
                ExpectedAnswer::StepByStep
            }
        }
        UserIntent::Compare => ExpectedAnswer::Comparison,
        UserIntent::Recall => ExpectedAnswer::FactualAnswer,
        UserIntent::Explore => ExpectedAnswer::Explanation,
        UserIntent::Converse => ExpectedAnswer::Acknowledgment,
    }
}

fn compute_confidence(lexical: &LexicalLayer, grammatical: &GrammaticalLayer) -> f32 {
    let mut confidence = 0.5_f32;

    // More key terms → higher confidence
    confidence += (lexical.key_terms.len() as f32 * 0.05).min(0.2);

    // Non-conversational → higher confidence (more structure detected)
    if grammatical.query_type != QueryType::Conversational {
        confidence += 0.15;
    }

    // Longer queries → slightly higher confidence
    if lexical.token_count > 5 {
        confidence += 0.1;
    }

    confidence.clamp(0.0, 1.0)
}

fn compute_retrieval_hints(
    semantic: &SemanticLayer,
    grammatical: &GrammaticalLayer,
    intent: &UserIntent,
) -> RetrievalHints {
    let mut hints = RetrievalHints::default();

    // Specificity-based adjustments
    if semantic.specificity > 0.7 {
        hints.min_score_adjustment = 0.05;
        hints.top_k_multiplier = 0.7;
    } else if semantic.specificity < 0.3 {
        hints.min_score_adjustment = -0.1;
        hints.top_k_multiplier = 1.5;
    }

    // Comparative queries benefit from diffusion
    if grammatical.query_type == QueryType::Comparative || grammatical.is_comparison {
        hints.enable_diffusion = true;
        hints.diffusion_hops = 3;
    }

    // Code/programming queries benefit from exact keyword matching
    if semantic.domains.iter().any(|d| d == "programming") {
        hints.bm25_weight_adjustment = 0.15;
    }

    // Conversational queries need less retrieval
    if *intent == UserIntent::Converse {
        hints.top_k_multiplier = 0.3;
        hints.min_score_adjustment = 0.1;
    }

    // Exploratory queries need broader search
    if *intent == UserIntent::Explore {
        hints.enable_diffusion = true;
        hints.diffusion_hops = 2;
        hints.top_k_multiplier = 1.3;
    }

    hints
}

// ---------------------------------------------------------------------------
// Utility
// ---------------------------------------------------------------------------

fn contains_any(text: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|p| text.contains(p))
}

fn starts_with_any(text: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|p| text.starts_with(p))
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factual_english_query() {
        let result = LgsrrDecomposer::decompose("What is Rust ownership?");
        assert_eq!(result.grammatical.query_type, QueryType::Factual);
        assert_eq!(result.reasoning.intent, UserIntent::Learn);
        assert_eq!(result.reasoning.expected_answer, ExpectedAnswer::FactualAnswer);
        assert!(result.lexical.key_terms.iter().any(|t| t.to_lowercase().contains("rust")));
        assert!(result.semantic.domains.contains(&"programming".to_owned()));
    }

    #[test]
    fn procedural_chinese_query() {
        let result = LgsrrDecomposer::decompose("如何用Rust编写异步代码");
        assert_eq!(result.grammatical.query_type, QueryType::Procedural);
        assert!(result.lexical.language == "mixed" || result.lexical.language == "zh");
        assert!(result.semantic.domains.contains(&"programming".to_owned()));
        assert_eq!(result.reasoning.expected_answer, ExpectedAnswer::CodeSnippet);
    }

    #[test]
    fn causal_english_query() {
        let result = LgsrrDecomposer::decompose("Why does memory leak happen in C++?");
        assert_eq!(result.grammatical.query_type, QueryType::Causal);
        assert_eq!(result.reasoning.intent, UserIntent::Learn);
        assert_eq!(result.reasoning.expected_answer, ExpectedAnswer::Explanation);
    }

    #[test]
    fn comparative_english_query() {
        let result = LgsrrDecomposer::decompose("Rust vs Go for web development");
        assert_eq!(result.grammatical.query_type, QueryType::Comparative);
        assert!(result.grammatical.is_comparison);
        assert_eq!(result.reasoning.intent, UserIntent::Compare);
        assert_eq!(result.reasoning.expected_answer, ExpectedAnswer::Comparison);
        // Should have a "compared_to" relation
        assert!(!result.relational.relations.is_empty());
        assert_eq!(result.relational.relations[0].predicate, "compared_to");
        // Comparative → diffusion enabled
        assert!(result.reasoning.retrieval_hints.enable_diffusion);
    }

    #[test]
    fn comparative_chinese_query() {
        let result = LgsrrDecomposer::decompose("Python和JavaScript的区别");
        assert!(result.grammatical.is_comparison);
        assert!(!result.relational.relations.is_empty());
        assert_eq!(result.relational.relations[0].predicate, "compared_to");
    }

    #[test]
    fn evaluative_english_query() {
        let result = LgsrrDecomposer::decompose("Should I use React or Vue for my project?");
        assert_eq!(result.grammatical.query_type, QueryType::Evaluative);
        assert_eq!(result.reasoning.intent, UserIntent::Compare);
    }

    #[test]
    fn conversational_short_query() {
        let result = LgsrrDecomposer::decompose("hello");
        assert_eq!(result.grammatical.query_type, QueryType::Conversational);
        assert_eq!(result.reasoning.intent, UserIntent::Converse);
        assert_eq!(result.reasoning.expected_answer, ExpectedAnswer::Acknowledgment);
        // Conversational → lower top_k
        assert!(result.reasoning.retrieval_hints.top_k_multiplier < 1.0);
    }

    #[test]
    fn negation_detected() {
        let result = LgsrrDecomposer::decompose("Why doesn't my code compile?");
        assert!(result.grammatical.is_negated);
    }

    #[test]
    fn temporal_detected() {
        let result = LgsrrDecomposer::decompose("What happened last week in the project?");
        assert!(result.grammatical.temporal_reference.is_some());
        assert_eq!(
            result.grammatical.temporal_reference.as_deref(),
            Some("last week")
        );
    }

    #[test]
    fn high_specificity_adjusts_retrieval() {
        // Long specific query → higher specificity → tighter retrieval
        let result = LgsrrDecomposer::decompose(
            "How to implement async trait methods with lifetime bounds in Rust 2024 edition",
        );
        assert!(result.semantic.specificity > 0.5);
        assert!(result.reasoning.retrieval_hints.min_score_adjustment >= 0.0);
    }

    #[test]
    fn programming_query_boosts_bm25() {
        let result = LgsrrDecomposer::decompose("How to fix Rust borrow checker error E0502?");
        assert!(result.semantic.domains.contains(&"programming".to_owned()));
        assert!(result.reasoning.retrieval_hints.bm25_weight_adjustment > 0.0);
    }

    #[test]
    fn language_detection() {
        assert_eq!(detect_language("hello world"), "en");
        assert_eq!(detect_language("你好世界"), "zh");
        assert_eq!(detect_language("Rust编程入门"), "mixed");
    }

    #[test]
    fn causal_chinese_query() {
        let result = LgsrrDecomposer::decompose("为什么Rust没有垃圾回收");
        assert_eq!(result.grammatical.query_type, QueryType::Causal);
        assert!(result.grammatical.is_negated);
    }
}
