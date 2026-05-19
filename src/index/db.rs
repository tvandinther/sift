use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection};
use std::path::Path;

/// Summary of an indexed source for display.
#[derive(Debug, Clone)]
pub struct SourceSummary {
    pub id: i64,
    pub path: String,
    pub label: Option<String>,
    pub indexed_at: String,
    pub file_count: usize,
    pub total_size_bytes: u64,
}

/// Summary of garbage collection operation.
#[derive(Debug, Clone)]
pub struct GcSummary {
    pub files_removed: usize,
    pub chunks_removed: usize,
}

/// Open a connection to the database, initialize schema, and configure WAL mode.
pub fn open_connection<P: AsRef<Path>>(path: P) -> Result<Connection> {
    let path = path.as_ref();

    // Create parent directory if it doesn't exist
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .context("Failed to create database directory")?;
    }

    let conn = Connection::open(path)
        .context("Failed to open database connection")?;

    // Enable WAL mode for concurrent reads
    conn.pragma_update(None, "journal_mode", "WAL")?;

    // Run migrations
    init_schema(&conn)?;

    Ok(conn)
}

/// Initialize database schema.
fn init_schema(conn: &Connection) -> Result<()> {
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
        "#
    )?;

    // Create embeddings table
    // Using regular BLOB storage for now - can migrate to vec0 extension later for better performance
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS chunk_embeddings (
            chunk_id    INTEGER PRIMARY KEY REFERENCES chunks(id) ON DELETE CASCADE,
            embedding   BLOB NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_chunk_embeddings_chunk_id ON chunk_embeddings(chunk_id);
        "#
    )?;

    Ok(())
}

/// Insert or update a source, returning its id.
pub fn upsert_source(conn: &Connection, path: &str, label: Option<&str>) -> Result<i64> {
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO sources (path, label, indexed_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(path) DO UPDATE SET label = ?2, indexed_at = ?3",
        params![path, label, now],
    )?;

    let id = conn.last_insert_rowid();

    // If we updated an existing row, get its id
    if id == 0 {
        let id: i64 = conn.query_row(
            "SELECT id FROM sources WHERE path = ?1",
            params![path],
            |row| row.get(0),
        )?;
        Ok(id)
    } else {
        Ok(id)
    }
}

/// Insert or update a file, returning its id.
pub fn upsert_file(
    conn: &Connection,
    source_id: i64,
    path: &str,
    checksum: &str,
    size_bytes: u64,
    chunk_count: usize,
) -> Result<i64> {
    let now = Utc::now().to_rfc3339();

    // First, delete old chunks if file already exists
    conn.execute(
        "DELETE FROM chunks WHERE file_id IN (SELECT id FROM files WHERE path = ?1)",
        params![path],
    )?;

    conn.execute(
        "INSERT INTO files (source_id, path, checksum, indexed_at, size_bytes, chunk_count)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(path) DO UPDATE SET
            source_id = ?1,
            checksum = ?3,
            indexed_at = ?4,
            size_bytes = ?5,
            chunk_count = ?6",
        params![source_id, path, checksum, now, size_bytes as i64, chunk_count as i64],
    )?;

    let id = conn.last_insert_rowid();

    // If we updated an existing row, get its id
    if id == 0 {
        let id: i64 = conn.query_row(
            "SELECT id FROM files WHERE path = ?1",
            params![path],
            |row| row.get(0),
        )?;
        Ok(id)
    } else {
        Ok(id)
    }
}

/// Insert a chunk and update the FTS index.
pub fn upsert_chunk(conn: &Connection, file_id: i64, seq: usize, body: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO chunks (file_id, seq, body) VALUES (?1, ?2, ?3)",
        params![file_id, seq as i64, body],
    )?;

    let chunk_id = conn.last_insert_rowid();

    // Insert into FTS index
    conn.execute(
        "INSERT INTO chunks_fts (rowid, body) VALUES (?1, ?2)",
        params![chunk_id, body],
    )?;

    Ok(())
}

/// Delete a source and all its files (cascades to chunks and FTS).
pub fn delete_source(conn: &Connection, path_or_label: &str) -> Result<bool> {
    let rows_affected = conn.execute(
        "DELETE FROM sources WHERE path = ?1 OR label = ?1",
        params![path_or_label],
    )?;

    Ok(rows_affected > 0)
}

/// List all indexed sources with file counts and sizes.
pub fn list_sources(conn: &Connection) -> Result<Vec<SourceSummary>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
            s.id,
            s.path,
            s.label,
            s.indexed_at,
            COUNT(f.id) as file_count,
            COALESCE(SUM(f.size_bytes), 0) as total_size
        FROM sources s
        LEFT JOIN files f ON f.source_id = s.id
        GROUP BY s.id
        ORDER BY s.indexed_at DESC
        "#
    )?;

    let sources = stmt.query_map([], |row| {
        Ok(SourceSummary {
            id: row.get(0)?,
            path: row.get(1)?,
            label: row.get(2)?,
            indexed_at: row.get(3)?,
            file_count: row.get::<_, i64>(4)? as usize,
            total_size_bytes: row.get::<_, i64>(5)? as u64,
        })
    })?
    .collect::<Result<Vec<_>, _>>()?;

    Ok(sources)
}

/// Remove index entries for files that no longer exist on disk.
pub fn gc(conn: &Connection) -> Result<GcSummary> {
    let mut stmt = conn.prepare("SELECT id, path FROM files")?;
    let files: Vec<(i64, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;

    let mut files_removed = 0;
    let mut chunks_removed = 0;

    for (file_id, path) in files {
        if !Path::new(&path).exists() {
            // Count chunks before deletion
            let chunk_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM chunks WHERE file_id = ?1",
                params![file_id],
                |row| row.get(0),
            )?;

            // Delete file (cascades to chunks)
            conn.execute("DELETE FROM files WHERE id = ?1", params![file_id])?;

            files_removed += 1;
            chunks_removed += chunk_count as usize;
        }
    }

    Ok(GcSummary {
        files_removed,
        chunks_removed,
    })
}

/// Get the checksum of a file if it exists in the database.
pub fn get_file_checksum(conn: &Connection, path: &str) -> Result<Option<String>> {
    let result = conn.query_row(
        "SELECT checksum FROM files WHERE path = ?1",
        params![path],
        |row| row.get(0),
    );

    match result {
        Ok(checksum) => Ok(Some(checksum)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Insert an embedding vector for a chunk.
pub fn insert_embedding(conn: &Connection, chunk_id: i64, embedding: &[f32]) -> Result<()> {
    if embedding.len() != 768 {
        anyhow::bail!(
            "Invalid embedding dimension: expected 768, got {}",
            embedding.len()
        );
    }

    // Convert to bytes for vec0
    let embedding_bytes: Vec<u8> = embedding
        .iter()
        .flat_map(|f| f.to_le_bytes())
        .collect();

    conn.execute(
        "INSERT INTO chunk_embeddings (chunk_id, embedding) VALUES (?1, ?2)",
        params![chunk_id, embedding_bytes],
    )?;

    Ok(())
}
