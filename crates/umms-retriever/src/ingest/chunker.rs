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

/// Split text into chunks respecting sentence boundaries.
///
/// Strategy:
/// 1. Split on paragraph breaks (`\n\n`) first
/// 2. If a paragraph exceeds `target_size`, split on sentence boundaries (`. `)
/// 3. Merge small consecutive paragraphs up to `target_size`
/// 4. Add `overlap` chars from the previous chunk as prefix
pub fn chunk_text(text: &str, config: &ChunkerConfig) -> Vec<Chunk> {
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
            let overlap_start = if current_text.len() > config.overlap {
                current_text.len() - config.overlap
            } else {
                0
            };
            // Find a word boundary for overlap
            let overlap_text = find_word_boundary(&current_text[overlap_start..]);
            current_text = overlap_text.to_owned();
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
                    let overlap_start = if current_text.len() > config.overlap {
                        current_text.len() - config.overlap
                    } else {
                        0
                    };
                    let overlap_text = find_word_boundary(&current_text[overlap_start..]);
                    current_text = overlap_text.to_owned();
                    chunk_start_offset = current_offset.saturating_sub(config.overlap);
                }
                if !current_text.is_empty() {
                    current_text.push(' ');
                }
                current_text.push_str(sentence);
            }
        } else {
            if !current_text.is_empty() {
                current_text.push_str("\n\n");
            } else {
                chunk_start_offset = current_offset;
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
        let text = format!(
            "{}\n\n{}",
            "Short paragraph one.",
            "Short paragraph two."
        );
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
}
