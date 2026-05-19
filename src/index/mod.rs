pub mod db;
pub mod embed;
pub mod read;

use anyhow::{Context, Result};
use ignore::WalkBuilder;
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Statistics from an indexing operation.
#[derive(Debug, Default)]
pub struct IndexStats {
    pub scanned: usize,
    pub added: usize,
    pub updated: usize,
    pub skipped: usize,
    pub failed: usize,
}

/// Statistics from a refresh operation.
#[derive(Debug, Default)]
pub struct RefreshStats {
    pub scanned: usize,
    pub added: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub removed: usize,
    pub failed: usize,
}

/// Index a path (file or directory) into the database.
pub fn run_index(
    conn: &Connection,
    paths: &[PathBuf],
    label: Option<&str>,
    include_hidden: bool,
    enable_embeddings: bool,
    model_cache: &Path,
    verbose: bool,
) -> Result<IndexStats> {
    let mut stats = IndexStats::default();

    // Load embedding model if enabled
    let embedding_model = if enable_embeddings {
        match embed::EmbeddingModel::load(model_cache, verbose) {
            Ok(model) => {
                if verbose {
                    println!("Model loaded successfully.");
                }
                Some(Arc::new(model))
            }
            Err(e) => {
                eprintln!("Warning: failed to load embedding model: {}", e);
                eprintln!("Continuing with lexical indexing only.");
                None
            }
        }
    } else {
        None
    };

    for path in paths {
        let canonical_path = normalize_path(path)?;
        let canonical_str = path_to_string(&canonical_path)?;

        // Upsert the source
        let source_id = db::upsert_source(conn, &canonical_str, label)?;

        // Collect all files to index
        let files = collect_files(&canonical_path, include_hidden)?;
        let total_files = files.len();

        println!("Indexing {} files from {}...", total_files, canonical_str);

        for (i, file_path) in files.iter().enumerate() {
            print!("\r[{}/{}] indexing {}    ", i + 1, total_files, file_path.display());
            std::io::Write::flush(&mut std::io::stdout())?;

            match index_file(conn, source_id, file_path, embedding_model.as_ref()) {
                Ok(file_stats) => {
                    stats.scanned += 1;
                    match file_stats {
                        FileIndexResult::Added => stats.added += 1,
                        FileIndexResult::Updated => stats.updated += 1,
                        FileIndexResult::Skipped => stats.skipped += 1,
                    }
                }
                Err(e) => {
                    eprintln!("\nWarning: failed to index {}: {}", file_path.display(), e);
                    stats.failed += 1;
                }
            }
        }

        println!();
    }

    Ok(stats)
}

#[derive(Debug)]
enum FileIndexResult {
    Added,
    Updated,
    Skipped,
}

/// Index a single file.
fn index_file(
    conn: &Connection,
    source_id: i64,
    path: &Path,
    embedding_model: Option<&Arc<embed::EmbeddingModel>>,
) -> Result<FileIndexResult> {
    let canonical_path = normalize_path(path)?;
    let canonical_str = path_to_string(&canonical_path)?;

    // Read file content
    let text = match read::read_text_file(&canonical_path)? {
        Some(text) => text,
        None => {
            // Binary file, skip silently
            return Ok(FileIndexResult::Skipped);
        }
    };

    // Compute checksum
    let checksum = compute_checksum(&text);

    // Check if file has changed
    if let Some(existing_checksum) = db::get_file_checksum(conn, &canonical_str)? {
        if existing_checksum == checksum {
            return Ok(FileIndexResult::Skipped);
        }
    }

    // Get file size
    let metadata = std::fs::metadata(&canonical_path)?;
    let size_bytes = metadata.len();

    // Chunk the text
    let chunks = read::chunk_text(&text);
    let chunk_count = chunks.len();

    // Determine if this is an add or update
    let is_new = db::get_file_checksum(conn, &canonical_str)?.is_none();

    // Upsert file
    let file_id = db::upsert_file(conn, source_id, &canonical_str, &checksum, size_bytes, chunk_count)?;

    // Insert chunks and embeddings
    for (seq, chunk_body) in chunks.iter().enumerate() {
        db::upsert_chunk(conn, file_id, seq, chunk_body)?;
        let chunk_id = conn.last_insert_rowid();

        // Generate and store embedding if model is available
        if let Some(model) = embedding_model {
            match model.embed(chunk_body) {
                Ok(embedding) => {
                    db::insert_embedding(conn, chunk_id, &embedding)?;
                }
                Err(e) => {
                    eprintln!("\nWarning: failed to generate embedding for chunk {}: {}", chunk_id, e);
                }
            }
        }
    }

    if is_new {
        Ok(FileIndexResult::Added)
    } else {
        Ok(FileIndexResult::Updated)
    }
}

/// Collect all files under a path, respecting .gitignore.
fn collect_files(path: &Path, include_hidden: bool) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    if path.is_file() {
        files.push(path.to_path_buf());
        return Ok(files);
    }

    let walker = WalkBuilder::new(path)
        .hidden(!include_hidden)
        .build();

    for entry in walker {
        let entry = entry?;
        if entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
            files.push(entry.into_path());
        }
    }

    Ok(files)
}

/// Normalize a path to an absolute, canonical form.
fn normalize_path(path: &Path) -> Result<PathBuf> {
    path.canonicalize()
        .with_context(|| format!("Failed to canonicalize path: {}", path.display()))
}

/// Convert a PathBuf to a String.
fn path_to_string(path: &Path) -> Result<String> {
    path.to_str()
        .map(|s| s.to_string())
        .context("Path contains invalid UTF-8")
}

/// Compute SHA256 checksum of text content.
fn compute_checksum(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Refresh indexes: check hashes, add new files, remove missing files.
pub fn run_refresh(
    conn: &Connection,
    source_identifier: Option<&str>,
    force_embeddings: bool,
    model_cache: &Path,
    verbose: bool,
) -> Result<RefreshStats> {
    let mut stats = RefreshStats::default();

    // Get sources to refresh
    let sources_to_refresh = if let Some(identifier) = source_identifier {
        // Refresh specific source
        if let Some((id, path, label)) = db::get_source_by_identifier(conn, identifier)? {
            vec![(id, path, label)]
        } else {
            anyhow::bail!("No source found matching '{}'", identifier);
        }
    } else {
        // Refresh all sources
        db::list_sources(conn)?
            .into_iter()
            .map(|s| (s.id, s.path, s.label))
            .collect()
    };

    // Load embedding model if needed
    let needs_embeddings = force_embeddings || {
        // Check if any source has embeddings - if so, maintain them
        sources_to_refresh.iter().any(|(id, _, _)| {
            conn.query_row(
                "SELECT COUNT(*) FROM chunk_embeddings WHERE chunk_id IN (SELECT c.id FROM chunks c JOIN files f ON f.id = c.file_id WHERE f.source_id = ?1)",
                params![id],
                |row| row.get::<_, i64>(0)
            ).unwrap_or(0) > 0
        })
    };

    let embedding_model = if needs_embeddings {
        match embed::EmbeddingModel::load(model_cache, verbose) {
            Ok(model) => {
                if verbose {
                    println!("Model loaded for embedding generation.");
                }
                Some(Arc::new(model))
            }
            Err(e) => {
                if verbose {
                    eprintln!("Warning: failed to load embedding model: {}", e);
                    eprintln!("Continuing without embeddings.");
                }
                None
            }
        }
    } else {
        None
    };

    for (source_id, source_path, _label) in sources_to_refresh {
        let canonical_path = normalize_path(&PathBuf::from(&source_path))?;

        if verbose {
            println!("Refreshing {}...", source_path);
        }

        // Get current files in DB for this source
        let db_files: std::collections::HashSet<String> =
            db::get_source_files(conn, source_id)?.into_iter().collect();

        // Collect current files on disk
        let disk_files = collect_files(&canonical_path, false)?;
        let disk_files_set: std::collections::HashSet<String> = disk_files
            .iter()
            .filter_map(|p| normalize_path(p).ok().and_then(|pb| path_to_string(&pb).ok()))
            .collect();

        // Process files on disk
        for file_path in disk_files {
            match index_file(conn, source_id, &file_path, embedding_model.as_ref()) {
                Ok(result) => {
                    stats.scanned += 1;
                    match result {
                        FileIndexResult::Added => {
                            stats.added += 1;
                            if verbose {
                                println!("  Added: {}", file_path.display());
                            }
                        }
                        FileIndexResult::Updated => {
                            stats.updated += 1;
                            if verbose {
                                println!("  Updated: {}", file_path.display());
                            }
                        }
                        FileIndexResult::Skipped => stats.unchanged += 1,
                    }
                }
                Err(e) => {
                    if verbose {
                        eprintln!("Warning: failed to index {}: {}", file_path.display(), e);
                    }
                    stats.failed += 1;
                }
            }
        }

        // Remove files that are in DB but not on disk
        for db_file in db_files {
            if !disk_files_set.contains(&db_file) {
                db::delete_file(conn, &db_file)?;
                stats.removed += 1;
                if verbose {
                    println!("  Removed: {}", db_file);
                }
            }
        }
    }

    Ok(stats)
}
