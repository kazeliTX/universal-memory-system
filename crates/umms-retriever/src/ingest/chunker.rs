//! Text chunking strategies.
//!
//! Chunks are the atomic unit of storage — each chunk becomes one `MemoryEntry`.
//! Good chunking preserves semantic boundaries (paragraph, section) rather than
//! splitting mid-sentence.

/// A chunk of text with its position in the source document.
#[derive(Debug, Clone)]
pub struct Chunk {
    /// The actual text content.
    pub text: String,
    /// Zero-based index of this chunk in the document.
    pub index: usize,
    /// Character offset from the start of the document.
    pub char_offset: usize,
}

/// Configuration for the chunker.
#[derive(Debug, Clone)]
pub struct ChunkerConfig {
    /// Target chunk size in characters (not tokens — cheaper to compute).
    /// Actual chunks may be slightly larger to avoid splitting mid-sentence.
    pub target_size: usize,
    /// Overlap between adjacent chunks in characters.
    /// Ensures continuity at boundaries.
    pub overlap: usize,
}

impl Default for ChunkerConfig {
    fn default() -> Self {
        Self {
            target_size: 1500,
            overlap: 200,
        }
    }
}

/// Protected region that should not be split across chunks.
struct ProtectedRegion {
    _start: usize,
    _end: usize,
    placeholder: String,
    original: String,
}

/// Find all URLs, code blocks, and reference entries in text.
/// Returns (cleaned_text, regions) where regions can restore originals.
fn protect_regions(text: &str) -> (String, Vec<ProtectedRegion>) {
    let mut regions = Vec::new();
    let mut result = text.to_owned();
    let mut counter = 0;

    // Protect fenced code blocks: ```...```
    while let Some(start) = result.find("```") {
        if let Some(end_offset) = result[start + 3..].find("```") {
            let end = start + 3 + end_offset + 3;
            let original = result[start..end].to_owned();
            let placeholder = format!("__CODE_BLOCK_{counter}__");
            counter += 1;
            regions.push(ProtectedRegion {
                _start: start,
                _end: end,
                placeholder: placeholder.clone(),
                original,
            });
            result = format!("{}{}{}", &result[..start], placeholder, &result[end..]);
        } else {
            break;
        }
    }

    // Protect URLs: http(s)://... until whitespace or closing paren/bracket
    let url_pattern = regex_lite::Regex::new(r"https?://[^\s\)\]>]+").unwrap();
    let url_matches: Vec<(usize, usize, String)> = url_pattern
        .find_iter(&result)
        .map(|m| (m.start(), m.end(), m.as_str().to_owned()))
        .collect();
    // Replace in reverse order to preserve byte offsets
    for (start, end, original) in url_matches.into_iter().rev() {
        let placeholder = format!("__URL_{counter}__");
        counter += 1;
        regions.push(ProtectedRegion {
            _start: start,
            _end: end,
            placeholder: placeholder.clone(),
            original,
        });
        result = format!("{}{}{}", &result[..start], placeholder, &result[end..]);
    }

    (result, regions)
}

/// Restore protected regions in chunked text.
fn restore_regions(text: &str, regions: &[ProtectedRegion]) -> String {
    let mut result = text.to_owned();
    for region in regions {
        result = result.replace(&region.placeholder, &region.original);
    }
    result
}

/// Split text into chunks respecting sentence and structure boundaries.
///
/// Strategy:
/// 1. Protect URLs and code blocks from being split (ADR: structure-aware chunking)
/// 2. Split on paragraph breaks (`\n\n`) first
/// 3. If a paragraph exceeds `target_size`, split on sentence boundaries (`. `)
/// 4. Merge small consecutive paragraphs up to `target_size`
/// 5. Add `overlap` chars from the previous chunk as prefix
/// 6. Restore protected regions in final chunks
pub fn chunk_text(text: &str, config: &ChunkerConfig) -> Vec<Chunk> {
    if text.is_empty() {
        return Vec::new();
    }

    // Phase 0: Protect URLs and code blocks
    let (protected_text, regions) = protect_regions(text);
    let chunks = chunk_text_inner(&protected_text, config);

    // Restore protected content in each chunk
    chunks
        .into_iter()
        .map(|mut c| {
            c.text = restore_regions(&c.text, &regions);
            c
        })
        .collect()
}

/// Inner chunking logic operating on protected text.
#[allow(clippy::assigning_clones)]
fn chunk_text_inner(text: &str, config: &ChunkerConfig) -> Vec<Chunk> {
    if text.is_empty() {
        return Vec::new();
    }

    // If text fits in one chunk, return as-is
    if text.len() <= config.target_size {
        return vec![Chunk {
            text: text.to_owned(),
            index: 0,
            char_offset: 0,
        }];
    }

    let paragraphs: Vec<&str> = text.split("\n\n").collect();
    let mut chunks: Vec<Chunk> = Vec::new();
    let mut current_text = String::new();
    let mut current_offset: usize = 0;
    let mut chunk_start_offset: usize = 0;

    for para in &paragraphs {
        let para_trimmed = para.trim();
        if para_trimmed.is_empty() {
            current_offset += para.len() + 2; // +2 for "\n\n"
            continue;
        }

        // If adding this paragraph would exceed target, flush current chunk
        if !current_text.is_empty()
            && current_text.len() + para_trimmed.len() + 2 > config.target_size
        {
            chunks.push(Chunk {
                text: current_text.clone(),
                index: chunks.len(),
                char_offset: chunk_start_offset,
            });

            // Start new chunk with overlap from the end of previous
            let overlap_start = find_char_boundary(&current_text, config.overlap);
            let overlap_text = find_word_boundary(&current_text[overlap_start..]).to_owned();
            current_text = overlap_text;
            chunk_start_offset = current_offset.saturating_sub(config.overlap);
        }

        // If a single paragraph exceeds target, split by sentences
        if para_trimmed.len() > config.target_size {
            let sentences = split_sentences(para_trimmed);
            for sentence in &sentences {
                if current_text.len() + sentence.len() + 1 > config.target_size
                    && !current_text.is_empty()
                {
                    chunks.push(Chunk {
                        text: current_text.clone(),
                        index: chunks.len(),
                        char_offset: chunk_start_offset,
                    });
                    let overlap_start = find_char_boundary(&current_text, config.overlap);
                    let overlap_text =
                        find_word_boundary(&current_text[overlap_start..]).to_owned();
                    current_text = overlap_text;
                    chunk_start_offset = current_offset.saturating_sub(config.overlap);
                }
                if !current_text.is_empty() {
                    current_text.push(' ');
                }
                current_text.push_str(sentence);
            }
        } else {
            if current_text.is_empty() {
                chunk_start_offset = current_offset;
            } else {
                current_text.push_str("\n\n");
            }
            current_text.push_str(para_trimmed);
        }

        current_offset += para.len() + 2;
    }

    // Flush remaining
    if !current_text.is_empty() {
        chunks.push(Chunk {
            text: current_text,
            index: chunks.len(),
            char_offset: chunk_start_offset,
        });
    }

    chunks
}

/// Split text into sentences (rough heuristic).
fn split_sentences(text: &str) -> Vec<&str> {
    let mut sentences = Vec::new();
    let mut start = 0;

    for (i, c) in text.char_indices() {
        if (c == '.' || c == '!' || c == '?' || c == '。' || c == '！' || c == '？')
            && i + c.len_utf8() < text.len()
        {
            let next_char_start = i + c.len_utf8();
            if let Some(next) = text[next_char_start..].chars().next() {
                if next.is_whitespace() || next == '"' || next == '\'' {
                    let end = next_char_start;
                    let sentence = text[start..end].trim();
                    if !sentence.is_empty() {
                        sentences.push(sentence);
                    }
                    start = next_char_start;
                }
            }
        }
    }

    // Remaining text
    let remaining = text[start..].trim();
    if !remaining.is_empty() {
        sentences.push(remaining);
    }

    sentences
}

/// Find a valid UTF-8 char boundary for overlap, counting back `overlap` bytes
/// from the end of the string. Returns a byte index that is safe to slice at.
fn find_char_boundary(s: &str, overlap: usize) -> usize {
    if s.len() <= overlap {
        return 0;
    }
    let mut idx = s.len() - overlap;
    // Walk forward to the nearest char boundary
    while idx < s.len() && !s.is_char_boundary(idx) {
        idx += 1;
    }
    idx
}

/// Find the nearest word boundary (space) at or after the given position.
fn find_word_boundary(s: &str) -> &str {
    if let Some(pos) = s.find(' ') {
        &s[pos + 1..]
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_short_text_returns_one_chunk() {
        let chunks = chunk_text("Hello world", &ChunkerConfig::default());
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text, "Hello world");
        assert_eq!(chunks[0].index, 0);
    }

    #[test]
    fn empty_text_returns_empty() {
        let chunks = chunk_text("", &ChunkerConfig::default());
        assert!(chunks.is_empty());
    }

    #[test]
    fn long_text_splits_into_multiple_chunks() {
        let para = "This is a test sentence. ".repeat(100); // ~2500 chars
        let config = ChunkerConfig {
            target_size: 500,
            overlap: 50,
        };
        let chunks = chunk_text(&para, &config);
        assert!(chunks.len() > 1);

        // All chunks should be non-empty
        for chunk in &chunks {
            assert!(!chunk.text.is_empty());
        }

        // Indices should be sequential
        for (i, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk.index, i);
        }
    }

    #[test]
    fn respects_paragraph_boundaries() {
        let text = format!("{}\n\n{}", "Short paragraph one.", "Short paragraph two.");
        let config = ChunkerConfig {
            target_size: 5000,
            overlap: 0,
        };
        let chunks = chunk_text(&text, &config);
        // Both paragraphs fit in one chunk
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].text.contains("one"));
        assert!(chunks[0].text.contains("two"));
    }

    #[test]
    fn sentence_splitting_basic() {
        let sentences = split_sentences("First sentence. Second sentence. Third.");
        assert_eq!(sentences.len(), 3);
    }

    #[test]
    fn chinese_text_does_not_panic() {
        let text = "脑神经科学与大模型交叉领域前沿研究全景。\n\n\
                    神经科学中的记忆巩固、脉冲编码、预测编码等机制为AI架构创新提供了方向。\n\n\
                    这是第三段，测试中文分块不会在多字节字符中间切割。";
        let config = ChunkerConfig {
            target_size: 50,
            overlap: 20,
        };
        // Should not panic on UTF-8 multi-byte chars
        let chunks = chunk_text(text, &config);
        assert!(!chunks.is_empty());
        for chunk in &chunks {
            assert!(!chunk.text.is_empty());
        }
    }

    #[test]
    fn find_char_boundary_on_chinese() {
        let s = "你好世界Hello";
        // "你好世界" = 12 bytes, "Hello" = 5 bytes, total = 17
        let idx = find_char_boundary(s, 10);
        assert!(s.is_char_boundary(idx));
    }
}
