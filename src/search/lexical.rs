use super::{ChunkResult, ResultSource};
use anyhow::Result;
use rusqlite::{params, Connection};
use std::path::PathBuf;

/// Perform lexical (FTS5) search.
pub fn search(conn: &Connection, query: &str, limit: usize) -> Result<Vec<ChunkResult>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
            chunks.id,
            chunks.file_id,
            files.path,
            chunks.seq,
            chunks.body,
            chunks_fts.rank
        FROM chunks_fts
        JOIN chunks ON chunks.id = chunks_fts.rowid
        JOIN files ON files.id = chunks.file_id
        WHERE chunks_fts MATCH ?1
        ORDER BY rank
        LIMIT ?2
        "#,
    )?;

    let results = stmt
        .query_map(params![query, limit as i64], |row| {
            Ok(ChunkResult {
                chunk_id: row.get(0)?,
                file_id: row.get(1)?,
                file_path: PathBuf::from(row.get::<_, String>(2)?),
                chunk_seq: row.get::<_, i64>(3)? as usize,
                body: row.get(4)?,
                score: row.get::<_, f64>(5)? as f32,
                source: ResultSource::Lexical,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lexical_search() -> Result<()> {
        // Create an in-memory database with test data
        let conn = Connection::open_in_memory()?;

        // Initialize schema
        conn.execute_batch(
            r#"
            CREATE TABLE sources (
                id INTEGER PRIMARY KEY,
                path TEXT NOT NULL UNIQUE,
                label TEXT,
                indexed_at TEXT NOT NULL
            );

            CREATE TABLE files (
                id INTEGER PRIMARY KEY,
                source_id INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
                path TEXT NOT NULL UNIQUE,
                checksum TEXT NOT NULL,
                indexed_at TEXT NOT NULL,
                size_bytes INTEGER NOT NULL,
                chunk_count INTEGER NOT NULL DEFAULT 1
            );

            CREATE TABLE chunks (
                id INTEGER PRIMARY KEY,
                file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
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

        // Insert test data
        conn.execute(
            "INSERT INTO sources (path, indexed_at) VALUES (?1, ?2)",
            params!["/test", "2024-01-01T00:00:00Z"],
        )?;
        let source_id = conn.last_insert_rowid();

        conn.execute(
            "INSERT INTO files (source_id, path, checksum, indexed_at, size_bytes, chunk_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![source_id, "/test/file1.txt", "abc123", "2024-01-01T00:00:00Z", 100, 1],
        )?;
        let file_id = conn.last_insert_rowid();

        conn.execute(
            "INSERT INTO chunks (file_id, seq, body) VALUES (?1, ?2, ?3)",
            params![file_id, 0, "This is a test document about Kubernetes and containers"],
        )?;
        let chunk_id = conn.last_insert_rowid();

        conn.execute(
            "INSERT INTO chunks_fts (rowid, body) VALUES (?1, ?2)",
            params![chunk_id, "This is a test document about Kubernetes and containers"],
        )?;

        // Search
        let results = search(&conn, "kubernetes", 10)?;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file_path, PathBuf::from("/test/file1.txt"));
        assert!(results[0].body.contains("Kubernetes"));

        Ok(())
    }
}
