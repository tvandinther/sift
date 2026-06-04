mod cli;

use anyhow::{bail, Context, Result};
use clap::Parser;
use cli::{Cli, Command};
use sift::{config::Config, index, search};
use std::io::{self, Write};

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Load configuration
    let config = Config::load(cli.db.clone())?;
    let verbose = cli.verbose;

    match cli.command {
        None => {
            // Launch TUI
            let conn = index::db::open_connection(&config.db_path)?;
            sift::tui::run(&conn, &config.model_cache)
        }
        Some(Command::Index { command }) => match command {
            cli::IndexCommand::Add {
                paths,
                name,
                hidden,
                no_embeddings,
            } => cmd_index_add(
                &config,
                paths,
                name.as_deref(),
                hidden,
                !no_embeddings,
                verbose,
            ),
            cli::IndexCommand::List => cmd_index_list(&config),
            cli::IndexCommand::Delete { path_or_label, yes } => {
                cmd_index_delete(&config, &path_or_label, yes)
            }
            cli::IndexCommand::Prune => cmd_index_prune(&config),
            cli::IndexCommand::Refresh {
                source,
                force_embeddings,
                rebuild_vocab,
            } => cmd_index_refresh(
                &config,
                source.as_deref(),
                force_embeddings,
                rebuild_vocab,
                verbose,
            ),
        },
        Some(Command::Config) => cmd_config(&config),
        Some(Command::Version) => cmd_version(),
        Some(Command::Search {
            query,
            limit,
            lexical_only,
            semantic_only,
            fast,
            json,
        }) => {
            let mode = if lexical_only {
                search::SearchMode::LexicalOnly
            } else if semantic_only {
                search::SearchMode::SemanticOnly
            } else {
                search::SearchMode::Hybrid
            };

            cmd_search(&config, query, limit, mode, fast, json, verbose)
        }
    }
}

fn cmd_index_add(
    config: &Config,
    paths: Vec<std::path::PathBuf>,
    label: Option<&str>,
    hidden: bool,
    enable_embeddings: bool,
    verbose: bool,
) -> Result<()> {
    let conn = index::db::open_connection(&config.db_path)?;
    let stats = index::run_index(
        &conn,
        &paths,
        label,
        hidden,
        enable_embeddings,
        &config.model_cache,
        verbose,
    )?;

    println!(
        "\nIndexed {} files — {} added, {} updated, {} skipped, {} failed",
        stats.scanned, stats.added, stats.updated, stats.skipped, stats.failed
    );

    Ok(())
}

fn cmd_index_list(config: &Config) -> Result<()> {
    let conn = index::db::open_connection(&config.db_path)?;
    let sources = index::db::list_sources(&conn)?;

    if sources.is_empty() {
        println!("No indexed sources. Run 'sift index add <path>' to get started.");
        return Ok(());
    }

    // Print header
    println!(
        "{:<40} {:<12} {:<8} {:<10} {:<12} {}",
        "SOURCE", "LABEL", "FILES", "SIZE", "EMBEDDINGS", "INDEXED"
    );
    println!("{}", "-".repeat(110));

    // Print sources
    for source in sources {
        let label = source.label.as_deref().unwrap_or("—");
        let size = format_size(source.total_size_bytes);
        let indexed = format_timestamp(&source.indexed_at);
        let embeddings = if source.embedding_count > 0 {
            format!("{} chunks", source.embedding_count)
        } else {
            "none".to_string()
        };

        println!(
            "{:<40} {:<12} {:<8} {:<10} {:<12} {}",
            truncate(&source.path, 40),
            truncate(label, 12),
            source.file_count,
            size,
            embeddings,
            indexed
        );
    }

    Ok(())
}

fn cmd_index_delete(config: &Config, path_or_label: &str, yes: bool) -> Result<()> {
    let conn = index::db::open_connection(&config.db_path)?;

    // Confirm unless --yes
    if !yes {
        print!("Remove index for '{}'? (y/N) ", path_or_label);
        io::stdout().flush()?;

        let mut response = String::new();
        io::stdin().read_line(&mut response)?;

        if !response.trim().eq_ignore_ascii_case("y") {
            println!("Cancelled.");
            return Ok(());
        }
    }

    let deleted = index::db::delete_source(&conn, path_or_label)?;

    if deleted {
        println!("Removed index for '{}'", path_or_label);
    } else {
        bail!("No source found matching '{}'", path_or_label);
    }

    Ok(())
}

fn cmd_index_prune(config: &Config) -> Result<()> {
    let conn = index::db::open_connection(&config.db_path)?;
    let summary = index::db::gc(&conn)?;

    if summary.files_removed == 0 {
        println!("Nothing to remove — all indexed files still exist.");
    } else {
        println!(
            "Removed {} files ({} chunks) that no longer exist on disk.",
            summary.files_removed, summary.chunks_removed
        );
    }

    Ok(())
}

fn cmd_index_refresh(
    config: &Config,
    source: Option<&str>,
    force_embeddings: bool,
    rebuild_vocab: bool,
    verbose: bool,
) -> Result<()> {
    let conn = index::db::open_connection(&config.db_path)?;

    // Check if there are any sources to refresh
    let source_count: i64 = conn.query_row("SELECT COUNT(*) FROM sources", [], |row| row.get(0))?;
    if source_count == 0 {
        println!("No indexed sources. Run 'sift index add <path>' first.");
        return Ok(());
    }

    // --rebuild-vocab only makes sense when refreshing ALL sources
    if rebuild_vocab && source.is_some() {
        bail!("--rebuild-vocab can only be used when refreshing all sources (don't specify a source).\nThe term vocabulary is a global index built from all sources.");
    }

    let stats = index::run_refresh(
        &conn,
        source,
        force_embeddings,
        rebuild_vocab,
        &config.model_cache,
        verbose,
    )?;

    println!(
        "\nRefresh complete — {} scanned, {} added, {} updated, {} unchanged, {} removed, {} failed",
        stats.scanned, stats.added, stats.updated, stats.unchanged, stats.removed, stats.failed
    );

    Ok(())
}

fn cmd_config(config: &Config) -> Result<()> {
    println!("{}", config.to_toml()?);
    Ok(())
}

fn cmd_version() -> Result<()> {
    println!("sift {}", env!("CARGO_PKG_VERSION"));
    Ok(())
}

fn cmd_search(
    config: &Config,
    query: String,
    limit: usize,
    mode: search::SearchMode,
    fast: bool,
    json: bool,
    verbose: bool,
) -> Result<()> {
    let conn = index::db::open_connection(&config.db_path)?;

    // Check if index is empty
    let source_count: i64 = conn.query_row("SELECT COUNT(*) FROM sources", [], |row| row.get(0))?;
    if source_count == 0 {
        println!("No indexed sources. Run 'sift index add <path>' first.");
        return Ok(());
    }

    // Perform search based on mode
    let file_results = match mode {
        search::SearchMode::LexicalOnly => {
            if verbose && !fast {
                println!("Query expansion skipped (--lexical-only mode)");
            }
            let chunk_results = search::lexical::search(&conn, &query, limit * 3)?;
            search::fusion::deduplicate_to_files(chunk_results)
        }
        search::SearchMode::SemanticOnly => {
            if verbose && !fast {
                println!("Query expansion skipped (--semantic-only mode)");
            }
            // Load embedding model
            let model = index::embed::EmbeddingModel::load(&config.model_cache, verbose)?;

            let chunk_results = search::semantic::search(&conn, &model, &query, limit * 3)?;
            search::fusion::deduplicate_to_files(chunk_results)
        }
        search::SearchMode::Hybrid => {
            // Load embedding model
            let model = index::embed::EmbeddingModel::load(&config.model_cache, verbose)?;

            if fast {
                // Fast mode: single-pass hybrid search
                if verbose {
                    println!("Query expansion skipped (--fast)");
                }

                let lex_results = search::lexical::search(&conn, &query, limit * 3)?;
                let sem_results = search::semantic::search(&conn, &model, &query, limit * 3)?;

                let merged = search::fusion::merge_results(lex_results, sem_results);
                search::fusion::deduplicate_to_files(merged)
            } else {
                // Default: two-phase search with semantic term expansion

                // Embed query once; reuse for both semantic search and term expansion
                let query_embedding = model
                    .embed(&query)
                    .context("Failed to generate query embedding")?;

                // Phase 1: Seed search with original query
                let seed_lex = search::lexical::search(&conn, &query, 100)?;
                let seed_sem = search::semantic::search_by_embedding(&conn, &query_embedding, 100)?;
                let seed_merged = search::fusion::merge_results(seed_lex, seed_sem);

                // Find nearest-neighbour terms in the vocabulary embedding space
                let expansion_terms = search::expand::find_expansion_terms_by_embedding(
                    &query_embedding,
                    &conn,
                    &query,
                    10,
                )?;

                if verbose && !expansion_terms.is_empty() {
                    println!("Expanded query with: {}", expansion_terms.join(", "));
                } else if verbose {
                    println!("No expansion terms found");
                }

                // Phase 2: Expansion search with nearest-neighbour terms appended
                let expanded_chunk_results = if !expansion_terms.is_empty() {
                    // Quote each expansion term for FTS5 to prevent column name interpretation
                    let quoted_terms: Vec<String> = expansion_terms
                        .iter()
                        .map(|term| format!("\"{}\"", term.replace("\"", "\"\"")))
                        .collect();
                    let expanded_query = format!("{} {}", query, quoted_terms.join(" "));

                    let exp_lex = search::lexical::search(&conn, &expanded_query, 100)?;
                    let exp_sem =
                        search::semantic::search_by_embedding(&conn, &query_embedding, 100)?;

                    search::fusion::merge_results(exp_lex, exp_sem)
                } else {
                    Vec::new()
                };

                // Merge seed and expansion results with weighted RRF
                // k=60 for seed (higher weight), k=80 for expansion (lower weight)
                let final_merged = search::fusion::merge_results_weighted(
                    seed_merged,
                    expanded_chunk_results,
                    60.0,
                    80.0,
                );

                search::fusion::deduplicate_to_files(final_merged)
            }
        }
    };

    // Truncate to limit
    let file_results: Vec<_> = file_results.into_iter().take(limit).collect();

    if file_results.is_empty() {
        println!("No results found.");
        return Ok(());
    }

    // Output results
    if json {
        println!("{}", serde_json::to_string_pretty(&file_results)?);
    } else {
        for result in file_results {
            println!(
                "{:<60} {:>8}  {}",
                truncate(&result.file_path.display().to_string(), 60),
                result.source,
                truncate(&result.snippet, 80)
            );
        }
    }

    Ok(())
}

/// Format bytes as human-readable size.
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Format ISO8601 timestamp as date and time.
fn format_timestamp(timestamp: &str) -> String {
    // Try to parse and format nicely, fall back to raw string
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(timestamp) {
        dt.format("%Y-%m-%d %H:%M").to_string()
    } else {
        timestamp.to_string()
    }
}

/// Truncate a string to a maximum length, adding ellipsis if needed.
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}
