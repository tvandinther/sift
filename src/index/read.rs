use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

const BINARY_DETECTION_SIZE: usize = 8192;
const CHUNK_SIZE_WORDS: usize = 400;
const CHUNK_OVERLAP_WORDS: usize = 50;

/// Read file content as UTF-8 text, or return None if it's a binary file.
pub fn read_text_file<P: AsRef<Path>>(path: P) -> Result<Option<String>> {
    let bytes = fs::read(path.as_ref())
        .with_context(|| format!("Failed to read file: {}", path.as_ref().display()))?;

    // Binary detection: check first 8KB for null bytes
    let check_size = BINARY_DETECTION_SIZE.min(bytes.len());
    if bytes[..check_size].contains(&0) {
        return Ok(None);
    }

    // Convert to UTF-8 with lossy fallback
    let text = String::from_utf8_lossy(&bytes).into_owned();

    Ok(Some(text))
}

/// Split text into overlapping chunks by word boundaries.
///
/// Chunks are approximately `CHUNK_SIZE_WORDS` words each with `CHUNK_OVERLAP_WORDS`
/// words of overlap. If the text is shorter than `CHUNK_SIZE_WORDS`, returns a single chunk.
pub fn chunk_text(text: &str) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();

    if words.len() <= CHUNK_SIZE_WORDS {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let step = CHUNK_SIZE_WORDS - CHUNK_OVERLAP_WORDS;
    let mut start = 0;

    while start < words.len() {
        let end = (start + CHUNK_SIZE_WORDS).min(words.len());
        let chunk = words[start..end].join(" ");
        chunks.push(chunk);

        if end == words.len() {
            break;
        }

        start += step;
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_text_empty() {
        let chunks = chunk_text("");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "");
    }

    #[test]
    fn test_chunk_text_small() {
        let text = "word ".repeat(100);
        let chunks = chunk_text(&text);
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn test_chunk_text_exact_boundary() {
        let text = "word ".repeat(CHUNK_SIZE_WORDS);
        let chunks = chunk_text(&text);
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn test_chunk_text_multiple_chunks() {
        let text = "word ".repeat(600);
        let chunks = chunk_text(&text);
        assert!(chunks.len() > 1);

        // Verify overlap exists between consecutive chunks
        if chunks.len() >= 2 {
            let first_words: Vec<&str> = chunks[0].split_whitespace().collect();
            let second_words: Vec<&str> = chunks[1].split_whitespace().collect();
            let overlap_start = &first_words[first_words.len() - CHUNK_OVERLAP_WORDS..];
            let second_start = &second_words[..CHUNK_OVERLAP_WORDS.min(second_words.len())];
            assert_eq!(overlap_start, second_start);
        }
    }

    #[test]
    fn test_chunk_text_preserves_word_boundaries() {
        let text = "one two three four five";
        let chunks = chunk_text(text);
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].contains("one two three four five"));
    }
}
