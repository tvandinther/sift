use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "sift")]
#[command(about = "Hybrid full-text and semantic search over local text content")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Path to the SQLite database
    #[arg(long, global = true)]
    pub db: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Manage indexed sources
    Index {
        #[command(subcommand)]
        command: IndexCommand,
    },

    /// Show resolved configuration
    Config,

    /// Search indexed content
    Search {
        /// Search query
        query: String,

        /// Maximum number of results to return
        #[arg(long, default_value = "10")]
        limit: usize,

        /// Use only lexical (FTS5) search
        #[arg(long)]
        lexical_only: bool,

        /// Use only semantic (vector) search
        #[arg(long)]
        semantic_only: bool,

        /// Output results as JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub enum IndexCommand {
    /// Add files or directories to the index
    Add {
        /// Paths to index (files or directories)
        #[arg(required = true)]
        paths: Vec<PathBuf>,

        /// Optional label for this source
        #[arg(long)]
        name: Option<String>,

        /// Include hidden files and directories
        #[arg(long)]
        hidden: bool,

        /// Disable embedding generation (lexical indexing only)
        #[arg(long)]
        no_embeddings: bool,
    },

    /// List all indexed sources
    List,

    /// Delete an indexed source
    Delete {
        /// Source path or label to delete
        path_or_label: String,

        /// Skip confirmation prompt
        #[arg(long)]
        yes: bool,
    },

    /// Remove index entries for files no longer on disk
    Prune,
}
