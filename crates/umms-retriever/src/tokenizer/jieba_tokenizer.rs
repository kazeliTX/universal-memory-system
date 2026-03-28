//! Jieba-based tokenizer for Chinese text with whitespace fallback for English.

use std::collections::HashSet;

use jieba_rs::Jieba;

use super::Tokenizer;

/// Chinese stopwords (common function words, particles, pronouns).
const ZH_STOPWORDS: &[&str] = &[
    "的", "了", "在", "是", "我", "有", "和", "就", "不", "人", "都", "一", "一个", "上", "也",
    "很", "到", "说", "要", "去", "你", "会", "着", "没有", "看", "好", "自己", "这", "他", "她",
    "它", "们", "那", "被", "从", "把", "对", "与", "为", "中", "等", "能", "以", "及", "其", "而",
    "之", "所", "或", "但", "如", "这个", "那个", "什么", "怎么", "可以", "已经", "因为", "所以",
    "如果", "虽然", "只是", "可能", "通过", "进行", "使用", "以及", "之间", "关于", "这些", "那些",
];

/// English stopwords.
const EN_STOPWORDS: &[&str] = &[
    "a", "an", "the", "and", "or", "but", "in", "on", "at", "to", "for", "of", "with", "by",
    "from", "as", "is", "was", "are", "were", "be", "been", "being", "have", "has", "had", "do",
    "does", "did", "will", "would", "could", "should", "may", "might", "shall", "can", "need",
    "it", "its", "this", "that", "these", "those", "he", "she", "they", "we", "you", "me", "him",
    "her", "us", "them", "my", "our", "your", "his", "their", "not", "no", "nor", "so", "if",
    "then", "than", "too", "very", "just", "about", "up", "out", "all", "also", "into",
];

/// Returns true if text contains CJK characters.
fn has_cjk(text: &str) -> bool {
    text.chars().any(|c| {
        ('\u{4E00}'..='\u{9FFF}').contains(&c)
            || ('\u{3400}'..='\u{4DBF}').contains(&c)
            || ('\u{F900}'..='\u{FAFF}').contains(&c)
    })
}

/// Jieba-based tokenizer: uses `cut_for_search` for CJK text,
/// whitespace splitting for pure ASCII text.
pub struct JiebaTokenizer {
    jieba: Jieba,
    en_stops: HashSet<&'static str>,
    zh_stops: HashSet<&'static str>,
}

impl JiebaTokenizer {
    pub fn new() -> Self {
        Self {
            jieba: Jieba::new(),
            en_stops: EN_STOPWORDS.iter().copied().collect(),
            zh_stops: ZH_STOPWORDS.iter().copied().collect(),
        }
    }
}

impl Default for JiebaTokenizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Tokenizer for JiebaTokenizer {
    fn tokenize(&self, text: &str) -> Vec<String> {
        let mut results = Vec::new();

        if has_cjk(text) {
            for word in self.jieba.cut_for_search(text, true) {
                let trimmed = word.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let char_count = trimmed.chars().count();
                if char_count < 2 {
                    continue;
                }
                if self.zh_stops.contains(trimmed)
                    || self.en_stops.contains(&trimmed.to_lowercase().as_str())
                {
                    continue;
                }
                if trimmed
                    .chars()
                    .all(|c| c.is_ascii_punctuation() || c.is_ascii_digit())
                {
                    continue;
                }
                results.push(trimmed.to_owned());
            }
        } else {
            for word in text.split_whitespace() {
                let clean = word.trim_matches(|c: char| !c.is_alphanumeric()).to_owned();
                if clean.len() >= 2 && !self.en_stops.contains(clean.to_lowercase().as_str()) {
                    results.push(clean);
                }
            }
        }

        results
    }

    fn name(&self) -> &'static str {
        "jieba"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chinese_segmentation() {
        let tok = JiebaTokenizer::new();
        let result = tok.tokenize("Rust异步运行时性能优化");
        assert!(
            result.iter().any(|w| w == "异步"),
            "Should find '异步': {result:?}"
        );
        assert!(
            result.iter().any(|w| w == "性能"),
            "Should find '性能': {result:?}"
        );
        assert!(
            result.iter().any(|w| w.contains("Rust")),
            "Should find 'Rust': {result:?}"
        );
    }

    #[test]
    fn chinese_stopwords_filtered() {
        let tok = JiebaTokenizer::new();
        let result = tok.tokenize("我们的知识图谱系统");
        assert!(
            !result.contains(&"的".to_owned()),
            "Should filter '的': {result:?}"
        );
        assert!(
            result.iter().any(|w| w.contains("知识")),
            "Should keep '知识': {result:?}"
        );
    }

    #[test]
    fn english_segmentation() {
        let tok = JiebaTokenizer::new();
        let result = tok.tokenize("Transformer attention mechanism for NLP");
        assert!(result.contains(&"Transformer".to_owned()));
        assert!(result.contains(&"attention".to_owned()));
        assert!(!result.contains(&"for".to_owned())); // stopword
    }

    #[test]
    fn mixed_text() {
        let tok = JiebaTokenizer::new();
        let result = tok.tokenize("使用Tokio进行高并发网络编程");
        assert!(
            result.iter().any(|w| w.contains("Tokio")),
            "Should find 'Tokio': {result:?}"
        );
        assert!(
            result.iter().any(|w| w.contains("并发")),
            "Should find '并发': {result:?}"
        );
    }
}
