use anyhow::Result;
use rusqlite::{params, Connection};
use std::collections::HashMap;

/// Stop words to exclude from expansion terms.
const STOP_WORDS: &[&str] = &[
    "the", "and", "for", "are", "but", "not", "you", "all", "can", "had", "her", "was", "one",
    "our", "out", "day", "get", "has", "him", "his", "how", "its", "may", "new", "now", "old",
    "see", "two", "who", "did", "use", "way", "with", "this", "that", "from", "they", "will",
    "have", "been", "when", "what", "your", "each", "she", "there", "their", "which", "also",
    "into", "more", "than", "then", "them", "these", "some", "would", "other", "about", "after",
];

/// Extract expansion terms from seed chunks using TF-IDF.
///
/// Takes the top 5 seed chunk bodies, computes TF-IDF scores for all terms,
/// and returns the top 8-10 terms by score after filtering.
///
/// Filters out:
/// - Stop words
/// - Terms already in the original query
/// - Terms shorter than 3 characters
pub fn extract_expansion_terms(
    seed_chunks: &[String],
    conn: &Connection,
    original_query: &str,
) -> Result<Vec<String>> {
    if seed_chunks.is_empty() {
        return Ok(Vec::new());
    }

    // Tokenize the original query for exclusion
    let query_tokens: Vec<String> = tokenize(original_query)
        .into_iter()
        .map(|s| s.to_lowercase())
        .collect();

    // Combine all seed chunk bodies
    let combined_text = seed_chunks.join(" ");

    // Compute term frequencies in seed chunks
    let tf_scores = compute_tf(&combined_text);

    // Get total chunk count for IDF computation
    let total_chunks: i64 = conn.query_row("SELECT COUNT(*) FROM chunks", [], |row| row.get(0))?;

    if total_chunks == 0 {
        return Ok(Vec::new());
    }

    // Compute IDF for each term
    let idf_scores = compute_idf(conn, &tf_scores, total_chunks)?;

    // Compute TF-IDF scores
    let mut tf_idf_scores: Vec<(String, f64)> = tf_scores
        .into_iter()
        .filter_map(|(term, tf)| {
            // Filter out stop words
            if STOP_WORDS.contains(&term.as_str()) {
                return None;
            }

            // Filter out terms already in query
            if query_tokens.contains(&term) {
                return None;
            }

            // Filter out terms shorter than 3 characters
            if term.len() < 3 {
                return None;
            }

            // Get IDF score
            let idf = idf_scores.get(&term)?;

            // Compute TF-IDF
            let tf_idf = tf * idf;

            Some((term, tf_idf))
        })
        .collect();

    // Sort by TF-IDF score descending
    tf_idf_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Take top 8-10 terms
    let expansion_terms: Vec<String> = tf_idf_scores
        .into_iter()
        .take(10)
        .map(|(term, _score)| term)
        .collect();

    Ok(expansion_terms)
}

/// Compute term frequency (TF) for all terms in text.
///
/// Returns a map of term -> count.
fn compute_tf(text: &str) -> HashMap<String, f64> {
    let tokens = tokenize(text);
    let mut tf: HashMap<String, f64> = HashMap::new();

    for token in tokens {
        let term = token.to_lowercase();
        *tf.entry(term).or_insert(0.0) += 1.0;
    }

    tf
}

/// Compute inverse document frequency (IDF) for terms.
///
/// IDF = ln(total_chunks / (1 + doc_count))
///
/// Returns a map of term -> IDF score.
fn compute_idf(
    conn: &Connection,
    tf_scores: &HashMap<String, f64>,
    total_chunks: i64,
) -> Result<HashMap<String, f64>> {
    let mut idf_scores: HashMap<String, f64> = HashMap::new();

    // Prepare statement for counting term occurrences
    let mut stmt = conn.prepare("SELECT COUNT(*) FROM chunks_fts WHERE chunks_fts MATCH ?")?;

    for term in tf_scores.keys() {
        // Quote the term for FTS5 to treat it as a literal string, not a column or operator
        let quoted_term = format!("\"{}\"", term.replace("\"", "\"\""));

        // Query FTS5 for document frequency
        let doc_count: i64 = stmt
            .query_row(params![quoted_term], |row| row.get(0))
            .unwrap_or_default();

        // Compute IDF: ln(total / (1 + doc_count))
        let idf = ((total_chunks as f64) / (1.0 + doc_count as f64)).ln();

        idf_scores.insert(term.clone(), idf);
    }

    Ok(idf_scores)
}

/// Find expansion terms by nearest-neighbour lookup in the term vocabulary embedding space.
///
/// Embeds the query, finds the top `n` nearest terms in the pre-built vocabulary index,
/// and returns them after filtering out query terms and stop words.
/// Returns an empty vec if the vocabulary is not yet built.
pub fn find_expansion_terms_by_embedding(
    query_embedding: &[f32],
    conn: &Connection,
    original_query: &str,
    n: usize,
) -> Result<Vec<String>> {
    let term_embeddings = crate::index::db::load_term_embeddings(conn)?;

    if term_embeddings.is_empty() {
        return Ok(Vec::new());
    }

    let query_tokens: std::collections::HashSet<String> = tokenize(original_query)
        .into_iter()
        .map(|s| s.to_lowercase())
        .collect();

    let mut scored: Vec<(String, f32)> = term_embeddings
        .into_iter()
        .filter_map(|(term, embedding)| {
            if query_tokens.contains(&term) || STOP_WORDS.contains(&term.as_str()) {
                return None;
            }
            if embedding.len() != query_embedding.len() {
                return None;
            }
            let sim = embedding_cosine_similarity(query_embedding, &embedding);
            Some((term, sim))
        })
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    Ok(scored.into_iter().take(n).map(|(t, _)| t).collect())
}

fn embedding_cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot / (norm_a * norm_b)
}

/// Simple tokenizer: split on whitespace and strip punctuation.
fn tokenize(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|word| {
            word.trim_matches(|c: char| !c.is_alphanumeric())
                .to_string()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize() {
        let text = "Hello, world! This is a test.";
        let tokens = tokenize(text);
        assert_eq!(tokens, vec!["Hello", "world", "This", "is", "a", "test"]);
    }

    #[test]
    fn test_compute_tf() {
        let text = "kubernetes kubernetes istio mesh mesh mesh";
        let tf = compute_tf(text);

        assert_eq!(tf.get("kubernetes"), Some(&2.0));
        assert_eq!(tf.get("istio"), Some(&1.0));
        assert_eq!(tf.get("mesh"), Some(&3.0));
    }

    #[test]
    fn test_extract_expansion_terms_filters_stopwords() -> Result<()> {
        // Create in-memory database
        let conn = Connection::open_in_memory()?;

        conn.execute_batch(
            r#"
            CREATE TABLE chunks (
                id INTEGER PRIMARY KEY,
                file_id INTEGER NOT NULL,
                seq INTEGER NOT NULL,
                body TEXT NOT NULL
            );

            CREATE VIRTUAL TABLE chunks_fts USING fts5(
                body,
                content='chunks',
                content_rowid='id'
            );
            "#,
        )?;

        // Insert test chunks
        conn.execute(
            "INSERT INTO chunks (file_id, seq, body) VALUES (1, 0, 'the quick brown fox')",
            [],
        )?;
        let chunk_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO chunks_fts (rowid, body) VALUES (?1, 'the quick brown fox')",
            params![chunk_id],
        )?;

        let seed_chunks = vec!["the quick brown fox jumps with the lazy dog".to_string()];

        let expansion_terms = extract_expansion_terms(&seed_chunks, &conn, "search")?;

        // Should not include stop words like "the", "with"
        assert!(!expansion_terms.contains(&"the".to_string()));
        assert!(!expansion_terms.contains(&"with".to_string()));

        Ok(())
    }

    #[test]
    fn test_extract_expansion_terms_excludes_query_terms() -> Result<()> {
        let conn = Connection::open_in_memory()?;

        conn.execute_batch(
            r#"
            CREATE TABLE chunks (
                id INTEGER PRIMARY KEY,
                file_id INTEGER NOT NULL,
                seq INTEGER NOT NULL,
                body TEXT NOT NULL
            );

            CREATE VIRTUAL TABLE chunks_fts USING fts5(
                body,
                content='chunks',
                content_rowid='id'
            );
            "#,
        )?;

        // Insert test chunk
        conn.execute(
            "INSERT INTO chunks (file_id, seq, body) VALUES (1, 0, 'kubernetes cluster')",
            [],
        )?;
        let chunk_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO chunks_fts (rowid, body) VALUES (?1, 'kubernetes cluster')",
            params![chunk_id],
        )?;

        let seed_chunks = vec!["kubernetes istio service mesh".to_string()];

        let expansion_terms = extract_expansion_terms(&seed_chunks, &conn, "kubernetes istio")?;

        // Should not include "kubernetes" or "istio" as they're in the query
        assert!(!expansion_terms.contains(&"kubernetes".to_string()));
        assert!(!expansion_terms.contains(&"istio".to_string()));

        Ok(())
    }

    #[test]
    fn test_extract_expansion_terms_filters_short_terms() -> Result<()> {
        let conn = Connection::open_in_memory()?;

        conn.execute_batch(
            r#"
            CREATE TABLE chunks (
                id INTEGER PRIMARY KEY,
                file_id INTEGER NOT NULL,
                seq INTEGER NOT NULL,
                body TEXT NOT NULL
            );

            CREATE VIRTUAL TABLE chunks_fts USING fts5(
                body,
                content='chunks',
                content_rowid='id'
            );
            "#,
        )?;

        // Insert test chunk
        conn.execute(
            "INSERT INTO chunks (file_id, seq, body) VALUES (1, 0, 'a bb ccc dddd')",
            [],
        )?;
        let chunk_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO chunks_fts (rowid, body) VALUES (?1, 'a bb ccc dddd')",
            params![chunk_id],
        )?;

        let seed_chunks = vec!["a bb ccc dddd".to_string()];

        let expansion_terms = extract_expansion_terms(&seed_chunks, &conn, "test")?;

        // Should only include terms >= 3 characters
        assert!(!expansion_terms.contains(&"a".to_string()));
        assert!(!expansion_terms.contains(&"bb".to_string()));
        assert!(
            expansion_terms.contains(&"ccc".to_string())
                || expansion_terms.contains(&"dddd".to_string())
        );

        Ok(())
    }
}
