pub mod fusion;
pub mod lexical;
pub mod semantic;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A search result for a chunk of text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkResult {
    pub chunk_id: i64,
    pub file_id: i64,
    pub file_path: PathBuf,
    pub chunk_seq: usize,
    pub body: String,
    pub score: f32,
    pub source: ResultSource,
}

/// A search result deduplicated to file level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileResult {
    pub file_id: i64,
    pub file_path: PathBuf,
    pub score: f32,
    pub snippet: String,
    pub source: ResultSource,
}

/// Source of a search result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResultSource {
    Lexical,
    Semantic,
    Both,
}

impl std::fmt::Display for ResultSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResultSource::Lexical => write!(f, "[lex]"),
            ResultSource::Semantic => write!(f, "[sem]"),
            ResultSource::Both => write!(f, "[lex+sem]"),
        }
    }
}

/// Search mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    Hybrid,
    LexicalOnly,
    SemanticOnly,
}

/// Search query parameters.
#[derive(Debug, Clone)]
pub struct SearchQuery {
    pub query: String,
    pub limit: usize,
    pub mode: SearchMode,
}

impl SearchQuery {
    pub fn new(query: String, limit: usize, mode: SearchMode) -> Self {
        Self { query, limit, mode }
    }
}
