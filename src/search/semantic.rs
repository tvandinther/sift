use super::{ChunkResult, ResultSource};
use crate::index::embed::EmbeddingModel;
use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::PathBuf;

/// Perform semantic (vector) search.
pub fn search(
    conn: &Connection,
    model: &EmbeddingModel,
    query: &str,
    limit: usize,
) -> Result<Vec<ChunkResult>> {
    let query_embedding = model
        .embed(query)
        .context("Failed to generate query embedding")?;
    search_by_embedding(conn, &query_embedding, limit)
}

/// Perform semantic search using a pre-computed query embedding.
pub fn search_by_embedding(
    conn: &Connection,
    query_embedding: &[f32],
    limit: usize,
) -> Result<Vec<ChunkResult>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
            chunks.id,
            chunks.file_id,
            files.path,
            chunks.seq,
            chunks.body,
            chunk_embeddings.embedding
        FROM chunks
        JOIN files ON files.id = chunks.file_id
        JOIN chunk_embeddings ON chunk_embeddings.chunk_id = chunks.id
        "#,
    )?;

    let mut results: Vec<ChunkResult> = stmt
        .query_map([], |row| {
            let chunk_id: i64 = row.get(0)?;
            let file_id: i64 = row.get(1)?;
            let file_path: String = row.get(2)?;
            let chunk_seq: i64 = row.get(3)?;
            let body: String = row.get(4)?;
            let embedding_blob: Vec<u8> = row.get(5)?;

            let embedding: Vec<f32> = embedding_blob
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect();

            let similarity = cosine_similarity(query_embedding, &embedding);

            Ok(ChunkResult {
                chunk_id,
                file_id,
                file_path: PathBuf::from(file_path),
                chunk_seq: chunk_seq as usize,
                body,
                score: similarity,
                source: ResultSource::Semantic,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    results.truncate(limit);

    Ok(results)
}

/// Compute cosine similarity between two vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "Vectors must have same length");

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot_product / (norm_a * norm_b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity() {
        // Identical vectors
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 1e-6);

        // Orthogonal vectors
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-6);

        // Opposite vectors
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![-1.0, -2.0, -3.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim + 1.0).abs() < 1e-6);
    }
}
