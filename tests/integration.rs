use anyhow::Result;
use rusqlite::Connection;
use std::path::PathBuf;

// Re-export the modules we need for testing
mod index {
    pub use sift::index::*;
}

#[test]
fn test_index_fixtures() -> Result<()> {
    // Create an in-memory database
    let conn = Connection::open_in_memory()?;

    // Initialize schema manually since we're not using open_connection
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS sources (
            id          INTEGER PRIMARY KEY,
            path        TEXT NOT NULL UNIQUE,
            label       TEXT,
            indexed_at  TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS files (
            id          INTEGER PRIMARY KEY,
            source_id   INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
            path        TEXT NOT NULL UNIQUE,
            checksum    TEXT NOT NULL,
            indexed_at  TEXT NOT NULL,
            size_bytes  INTEGER NOT NULL,
            chunk_count INTEGER NOT NULL DEFAULT 1
        );

        CREATE TABLE IF NOT EXISTS chunks (
            id        INTEGER PRIMARY KEY,
            file_id   INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
            seq       INTEGER NOT NULL,
            body      TEXT NOT NULL
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
            body,
            content='chunks',
            content_rowid='id'
        );

        CREATE TABLE IF NOT EXISTS chunk_embeddings (
            chunk_id    INTEGER PRIMARY KEY REFERENCES chunks(id) ON DELETE CASCADE,
            embedding   BLOB NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_chunk_embeddings_chunk_id ON chunk_embeddings(chunk_id);
        "#,
    )?;

    // Get the fixtures directory path
    let fixtures_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures");

    // Temporary model cache for testing (not actually used since embeddings are disabled)
    let model_cache = PathBuf::from("/tmp/sift-test-models");

    // Index the fixtures (without embeddings for speed)
    let stats = index::run_index(
        &conn,
        std::slice::from_ref(&fixtures_path),
        None,
        false,
        false,
        &model_cache,
        false,
    )?;

    // Verify that files were indexed
    assert!(stats.scanned > 0, "Should have scanned files");
    assert!(stats.added > 0, "Should have added files");
    assert_eq!(stats.failed, 0, "Should have no failures");

    // List sources and verify
    let sources = index::db::list_sources(&conn)?;
    assert_eq!(sources.len(), 1, "Should have one source");
    assert!(sources[0].file_count > 0, "Source should have files");
    assert!(sources[0].total_size_bytes > 0, "Source should have size");

    // Test FTS search for a term that appears in the fixtures
    let mut stmt = conn.prepare("SELECT COUNT(*) FROM chunks_fts WHERE chunks_fts MATCH ?1")?;
    let count: i64 = stmt.query_row(["crossplane"], |row| row.get(0))?;
    assert!(count > 0, "Should find results for 'crossplane'");

    // Test another search term
    let count: i64 = stmt.query_row(["kubernetes"], |row| row.get(0))?;
    assert!(count > 0, "Should find results for 'kubernetes'");

    // Run gc and verify nothing is removed (all files still exist)
    let gc_summary = index::db::gc(&conn)?;
    assert_eq!(
        gc_summary.files_removed, 0,
        "GC should not remove existing files"
    );

    // Get the source path for deletion
    let source_path = sources[0].path.clone();

    // Delete the source
    let deleted = index::db::delete_source(&conn, &source_path)?;
    assert!(deleted, "Should successfully delete source");

    // Verify source is gone
    let sources_after = index::db::list_sources(&conn)?;
    assert_eq!(
        sources_after.len(),
        0,
        "Should have no sources after deletion"
    );

    // Verify files are gone (cascade delete)
    let file_count: i64 = conn.query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))?;
    assert_eq!(file_count, 0, "All files should be deleted via cascade");

    Ok(())
}

#[test]
fn test_chunking_edge_cases() -> Result<()> {
    use sift::index::read::chunk_text;

    // Empty text
    let chunks = chunk_text("");
    assert_eq!(chunks.len(), 1);

    // Small text (less than chunk size)
    let small_text = "word ".repeat(100);
    let chunks = chunk_text(&small_text);
    assert_eq!(chunks.len(), 1);

    // Text exactly at chunk boundary
    let boundary_text = "word ".repeat(400);
    let chunks = chunk_text(&boundary_text);
    assert_eq!(chunks.len(), 1);

    // Large text requiring multiple chunks
    let large_text = "word ".repeat(1000);
    let chunks = chunk_text(&large_text);
    assert!(
        chunks.len() > 1,
        "Large text should produce multiple chunks"
    );

    Ok(())
}
