use super::{ChunkResult, FileResult, ResultSource};
use std::collections::HashMap;

const RRF_CONSTANT: f32 = 60.0;

/// Merge two result lists using Reciprocal Rank Fusion (RRF).
///
/// RRF score for item i: sum(1 / (k + rank_i)) across all rankings
/// where k is a constant (typically 60).
pub fn merge_results(
    lexical_results: Vec<ChunkResult>,
    semantic_results: Vec<ChunkResult>,
) -> Vec<ChunkResult> {
    let mut rrf_scores: HashMap<i64, (ChunkResult, f32, ResultSource)> = HashMap::new();

    // Add lexical results
    for (rank, result) in lexical_results.into_iter().enumerate() {
        let rrf_score = 1.0 / (RRF_CONSTANT + rank as f32 + 1.0);
        rrf_scores.insert(
            result.chunk_id,
            (result, rrf_score, ResultSource::Lexical),
        );
    }

    // Add or merge semantic results
    for (rank, result) in semantic_results.into_iter().enumerate() {
        let rrf_score = 1.0 / (RRF_CONSTANT + rank as f32 + 1.0);

        rrf_scores
            .entry(result.chunk_id)
            .and_modify(|(_r, score, source)| {
                *score += rrf_score;
                *source = ResultSource::Both;
            })
            .or_insert((result, rrf_score, ResultSource::Semantic));
    }

    // Convert to vector and sort by RRF score
    let mut merged: Vec<ChunkResult> = rrf_scores
        .into_values()
        .map(|(mut result, score, source)| {
            result.score = score;
            result.source = source;
            result
        })
        .collect();

    merged.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());

    merged
}

/// Deduplicate chunk results to file level, keeping the highest-scoring chunk per file.
pub fn deduplicate_to_files(chunk_results: Vec<ChunkResult>) -> Vec<FileResult> {
    let mut file_map: HashMap<i64, (ChunkResult, f32)> = HashMap::new();

    for chunk in chunk_results {
        file_map
            .entry(chunk.file_id)
            .and_modify(|(best_chunk, best_score)| {
                if chunk.score > *best_score {
                    *best_chunk = chunk.clone();
                    *best_score = chunk.score;
                }
            })
            .or_insert((chunk.clone(), chunk.score));
    }

    let mut file_results: Vec<FileResult> = file_map
        .into_values()
        .map(|(chunk, _score)| FileResult {
            file_id: chunk.file_id,
            file_path: chunk.file_path.clone(),
            score: chunk.score,
            snippet: extract_snippet(&chunk.body, 150),
            source: chunk.source,
        })
        .collect();

    file_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());

    file_results
}

/// Extract a snippet from text, truncating to max_length words.
fn extract_snippet(text: &str, max_length: usize) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();

    if words.len() <= max_length {
        text.to_string()
    } else {
        format!("{}...", words[..max_length].join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_chunk(id: i64, file_id: i64, score: f32, source: ResultSource) -> ChunkResult {
        ChunkResult {
            chunk_id: id,
            file_id,
            file_path: PathBuf::from(format!("/test/file{}.txt", file_id)),
            chunk_seq: 0,
            body: "test body".to_string(),
            score,
            source,
        }
    }

    #[test]
    fn test_rrf_merge_disjoint() {
        let lex = vec![
            make_chunk(1, 1, 10.0, ResultSource::Lexical),
            make_chunk(2, 2, 9.0, ResultSource::Lexical),
        ];

        let sem = vec![
            make_chunk(3, 3, 0.95, ResultSource::Semantic),
            make_chunk(4, 4, 0.90, ResultSource::Semantic),
        ];

        let merged = merge_results(lex, sem);

        assert_eq!(merged.len(), 4);
        // All should have RRF scores based on their rank
        assert!(merged[0].score > 0.0);
    }

    #[test]
    fn test_rrf_merge_overlap() {
        let lex = vec![
            make_chunk(1, 1, 10.0, ResultSource::Lexical),
            make_chunk(2, 2, 9.0, ResultSource::Lexical),
        ];

        let sem = vec![
            make_chunk(1, 1, 0.95, ResultSource::Semantic), // Same chunk
            make_chunk(3, 3, 0.90, ResultSource::Semantic),
        ];

        let merged = merge_results(lex, sem);

        assert_eq!(merged.len(), 3);
        // Chunk 1 should have source=Both and highest score (sum of two RRF contributions)
        let chunk1 = merged.iter().find(|r| r.chunk_id == 1).unwrap();
        assert_eq!(chunk1.source, ResultSource::Both);
    }

    #[test]
    fn test_deduplicate() {
        let chunks = vec![
            make_chunk(1, 1, 0.9, ResultSource::Both),
            make_chunk(2, 1, 0.7, ResultSource::Lexical), // Same file, lower score
            make_chunk(3, 2, 0.8, ResultSource::Semantic),
        ];

        let files = deduplicate_to_files(chunks);

        assert_eq!(files.len(), 2);
        // File 1 should keep chunk 1 (highest score)
        let file1 = files.iter().find(|f| f.file_id == 1).unwrap();
        assert!((file1.score - 0.9).abs() < 1e-6);
    }

    #[test]
    fn test_extract_snippet() {
        let text = "one two three four five six seven eight nine ten";
        let snippet = extract_snippet(text, 5);
        assert_eq!(snippet, "one two three four five...");

        let short = "one two three";
        let snippet = extract_snippet(short, 5);
        assert_eq!(snippet, "one two three");
    }
}
