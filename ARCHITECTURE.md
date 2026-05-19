# sift Architecture

## Overview

`sift` is a hybrid full-text and semantic search engine for local text content. It combines traditional BM25 lexical search (via SQLite FTS5) with modern semantic vector search (via embedding models) to provide accurate, context-aware search results over personal knowledge bases.

**Key characteristics:**
- Single binary, no daemon
- On-disk SQLite index with hybrid search
- In-process embedding model (Candle)
- TUI for interactive use, CLI for scripting
- Works entirely offline after model download

## High-Level Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                         User                                 │
└──────────┬───────────────────────────┬──────────────────────┘
           │                           │
           │ TUI (interactive)         │ CLI (scripting)
           ▼                           ▼
┌──────────────────────────────────────────────────────────────┐
│                     sift binary                               │
│  ┌────────────────┐  ┌────────────────┐  ┌─────────────────┐│
│  │   Indexing     │  │    Search      │  │      TUI        ││
│  │   Pipeline     │  │    Engine      │  │   (ratatui)     ││
│  └───────┬────────┘  └────────┬───────┘  └─────────────────┘│
│          │                    │                               │
│          ▼                    ▼                               │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │              Embedding Model (Candle)                   │ │
│  │         sentence-transformers/all-MiniLM-L6-v2         │ │
│  └─────────────────────────────────────────────────────────┘ │
│          │                    │                               │
│          ▼                    ▼                               │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │                   SQLite Database                        │ │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────────────────┐  │ │
│  │  │ Metadata │  │  FTS5    │  │ Vector Embeddings    │  │ │
│  │  │ (files)  │  │ (lexical)│  │    (semantic)        │  │ │
│  │  └──────────┘  └──────────┘  └──────────────────────┘  │ │
│  └─────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────┘
           │                    │
           ▼                    ▼
    ~/.local/share/sift/   Source Files
      - index.db           (read-only)
      - models/
```

## Core Components

### 1. Indexing Pipeline

The indexing pipeline transforms source files into searchable chunks with both lexical and semantic representations.

**Process:**
1. **File Discovery** (`index/mod.rs`)
   - Uses `ignore` crate to walk directories
   - Respects `.gitignore`, `.ignore` files
   - Filters hidden files unless `--hidden`
   - Computes SHA256 checksums for change detection

2. **Text Extraction** (`index/read.rs`)
   - Detects binary files (presence of null bytes in first 8KB)
   - Reads as UTF-8 with replacement characters for invalid bytes
   - No format-specific parsing (treats all as plain text)

3. **Chunking** (`index/read.rs::chunk_text`)
   - Splits large files into ~512-token chunks
   - Uses overlapping windows to preserve context
   - Each chunk indexed independently

4. **Embedding Generation** (`index/embed.rs`)
   - Loads model on first use (cached thereafter)
   - Generates 384-dimensional vectors per chunk
   - Uses Metal acceleration on Apple Silicon when available
   - Falls back to CPU otherwise

5. **Database Storage** (`index/db.rs`)
   - Upserts source, file, and chunk records
   - Inserts into FTS5 virtual table for lexical search
   - Stores embeddings as BLOB for semantic search
   - Cascading deletes for data consistency

**Incremental updates:**
- Skip files with unchanged checksums
- Delete old chunks when file changes
- Remove missing files on refresh/prune

### 2. Search Engine

Hybrid search combines lexical precision with semantic understanding.

**Search Modes:**

1. **Lexical-Only** (`search/lexical.rs`)
   - SQLite FTS5 full-text search
   - BM25 ranking algorithm
   - Fast, exact keyword matching
   - Case-insensitive, supports phrase queries

2. **Semantic-Only** (`search/semantic.rs`)
   - Embeds query text (384-dim vector)
   - Cosine similarity against all chunk embeddings
   - Finds conceptually similar text
   - Works with paraphrases and synonyms

3. **Hybrid** (default) (`search/fusion.rs`)
   - Runs both lexical and semantic searches
   - Merges results using Reciprocal Rank Fusion (RRF)
   - Formula: `score = Σ 1/(k + rank)` where k=60
   - Deduplicates to file level (highest-scoring chunk wins)
   - Balances precision and recall

**Query Flow:**
```
User Query
    │
    ├─→ Lexical Search (FTS5) ──→ Ranked chunk list
    │                              (BM25 scores)
    │
    └─→ Semantic Search ────────→ Ranked chunk list
        (embed + cosine)           (similarity scores)
                │
                ▼
         Reciprocal Rank Fusion
                │
                ▼
         File-level deduplication
                │
                ▼
         Top N results with snippets
```

### 3. TUI (Terminal User Interface)

Interactive search interface built with `ratatui`.

**Views:**
- **Search View** (`tui/search.rs`)
  - Input bar with cursor positioning
  - Live result list
  - Snippet previews
  - Keyboard navigation (↑/↓, Enter, o for system open)
  
- **Indexes View** (`tui/indexes.rs`)
  - List of indexed sources
  - File counts, sizes, embedding status
  - Refresh, prune, delete operations
  - Confirmation dialogs for destructive actions

- **Help View**
  - Keyboard shortcuts
  - Mode explanations

**Key Features:**
- Input focus mode (hotkeys disabled when typing)
- Pending operation pattern (deferred execution for smooth UI)
- $PAGER integration (Enter to view file)
- System default app integration (o to open)
- Error handling with user feedback

### 4. Database Schema

SQLite with FTS5 and BLOB storage for vectors.

**Tables:**

```sql
-- Indexed sources (directories or files)
sources (
    id INTEGER PRIMARY KEY,
    path TEXT UNIQUE,
    label TEXT,
    indexed_at TEXT
)

-- Files within sources
files (
    id INTEGER PRIMARY KEY,
    source_id INTEGER REFERENCES sources(id) ON DELETE CASCADE,
    path TEXT UNIQUE,
    checksum TEXT,      -- SHA256 for change detection
    indexed_at TEXT,
    size_bytes INTEGER,
    chunk_count INTEGER
)

-- Text chunks (for large files)
chunks (
    id INTEGER PRIMARY KEY,
    file_id INTEGER REFERENCES files(id) ON DELETE CASCADE,
    seq INTEGER,        -- Chunk sequence number
    body TEXT
)

-- FTS5 virtual table (lexical search)
chunks_fts (
    body TEXT,
    content='chunks',   -- External content table
    content_rowid='id'
)

-- Embeddings (semantic search)
chunk_embeddings (
    chunk_id INTEGER PRIMARY KEY REFERENCES chunks(id) ON DELETE CASCADE,
    embedding BLOB      -- 384 f32 values (1536 bytes)
)
```

**Rationale:**
- FTS5 for fast lexical search
- BLOB storage for embeddings (simple, works everywhere)
- Cascading deletes maintain referential integrity
- Normalized schema avoids duplication

### 5. Configuration

Config resolution order: CLI flags → `~/.config/sift/config.toml` → defaults

```toml
db_path = "/Users/user/.local/share/sift/index.db"
model_cache = "/Users/user/.local/share/sift/models"
```

**Model cache structure:**
```
~/.local/share/sift/models/
└── sentence-transformers-all-MiniLM-L6-v2/
    ├── config.json
    ├── tokenizer.json
    └── model.safetensors
```

Models are organized by name to allow multiple models in the future.

## Embedding Model Decision

### Current Model: sentence-transformers/all-MiniLM-L6-v2

**Why this model?**
1. **Fast and lightweight** - 90 MB download, runs on CPU
2. **Good quality** - Competitive performance on semantic similarity tasks
3. **Broad compatibility** - BERT architecture, well-supported by Candle
4. **384 dimensions** - Smaller vectors = faster search, less storage

**Trade-offs accepted:**
- Limited to 512 tokens context (fine for chunked search)
- Not state-of-the-art (but good enough for personal use)
- English-focused (acceptable for target use case)

### Alternative Considered: nomic-embed-text-v1.5

During development, we considered switching to Nomic's embedding model but decided against it.

**Comparison:**

| Feature | all-MiniLM-L6-v2 (chosen) | nomic-embed-text-v1.5 |
|---------|--------------------------|----------------------|
| **Dimensions** | 384 | 768 |
| **Model size** | ~90 MB | ~274 MB |
| **Architecture** | BERT encoder | Custom with rotary embeddings |
| **Context length** | 512 tokens | 8192 tokens |
| **Speed** | Fast (smaller) | Slower (2x dimensions) |
| **Quality** | Good | Better (MTEB benchmarks) |
| **VRAM/RAM** | Low | Higher |
| **Loading time** | Fast | Slower |
| **Storage per chunk** | 1536 bytes | 3072 bytes |
| **Search speed** | Fast | Slower (2x comparisons) |

**Why we stayed with all-MiniLM-L6-v2:**

1. **Performance** - Personal search doesn't need the extra 30-50ms per search that nomic would add
2. **Resource usage** - 2x memory and storage overhead isn't justified for typical use cases
3. **User experience** - Faster model download (90 MB vs 274 MB) lowers barrier to entry
4. **Simplicity** - BERT architecture is simpler, well-tested in Candle
5. **Chunking strategy** - We chunk at ~512 tokens anyway, so 8K context isn't utilized

**When nomic-embed-text might make sense:**
- Searching technical documentation with long code blocks
- Multi-lingual content (nomic has better multilingual support)
- Large corpus where quality trumps speed
- Powerful hardware (M3 Max, etc.) where 274 MB model doesn't matter

**Migration path:**
The architecture supports model swapping. To use nomic-embed-text:
1. Change `MODEL_NAME` and `BASE_URL` in `embed.rs`
2. Update `EMBEDDING_DIM` to 768
3. Update schema to match new dimension
4. Re-index all content with new embeddings

## File Organization

```
sift/
├── src/
│   ├── main.rs              # Entry point, CLI dispatch
│   ├── cli.rs               # clap definitions
│   ├── config.rs            # Config loading
│   ├── index/
│   │   ├── mod.rs           # Indexing orchestration
│   │   ├── read.rs          # File reading, chunking
│   │   ├── embed.rs         # Embedding model (Candle)
│   │   └── db.rs            # SQLite schema, operations
│   ├── search/
│   │   ├── mod.rs           # Search orchestration
│   │   ├── lexical.rs       # FTS5 queries
│   │   ├── semantic.rs      # Vector similarity
│   │   └── fusion.rs        # RRF merge, dedup
│   └── tui/
│       ├── mod.rs           # TUI app state, event loop
│       ├── search.rs        # Search view
│       ├── indexes.rs       # Index management view
│       └── theme.rs         # Color scheme
├── tests/
│   ├── integration.rs       # End-to-end tests
│   └── fixtures/            # Sample files for testing
├── CLAUDE.md                # Project instructions
├── ARCHITECTURE.md          # This file
├── README.md                # User documentation
├── Cargo.toml               # Dependencies
├── flake.nix                # Nix development environment
└── justfile                 # Development recipes

~/.local/share/sift/         # Runtime data
├── index.db                 # SQLite database
├── index.db-wal             # WAL file (write-ahead log)
└── models/                  # Embedding models
    └── sentence-transformers-all-MiniLM-L6-v2/
        ├── config.json
        ├── tokenizer.json
        └── model.safetensors
```

## Key Design Decisions

### Why SQLite?

1. **Single file** - Easy backup, no server setup
2. **FTS5** - Production-ready full-text search
3. **ACID** - Data integrity for concurrent access
4. **Mature** - Battle-tested, excellent documentation
5. **Portable** - Works everywhere Rust does

Alternative considered: Custom inverted index
- Rejected: Too much work, SQLite FTS5 is excellent

### Why not a vector database?

**Current approach:** SQLite with BLOB storage

**Alternatives considered:**
- Qdrant, Milvus, Weaviate (vector databases)
- sqlite-vss, sqlite-vec (SQLite extensions)

**Why BLOBs work:**
1. **Scale** - Personal search = thousands of vectors, not millions
2. **Linear scan is fast** - 10K vectors × 384 dims = ~3.8M comparisons, ~10-20ms on CPU
3. **Simplicity** - No external dependencies, pure Rust
4. **Portability** - Works in WASM, embedded systems, etc.
5. **Good enough** - Sub-second search is fine for interactive use

**When to reconsider:**
- Corpus > 100K documents (>1M chunks)
- Search latency > 500ms becomes noticeable
- Need approximate nearest neighbors (ANN)

At that scale, consider:
- `sqlite-vec` extension for ANN
- Separate vector index (HNSW/IVF) alongside SQLite

### Why Reciprocal Rank Fusion?

RRF is a simple, effective algorithm for combining ranked lists.

**Formula:** `score(item) = Σ 1/(k + rank)` across all result lists

**Why it works:**
1. **No score normalization** - Works with different scoring systems (BM25 vs cosine)
2. **Position-based** - Emphasizes top results from both methods
3. **Robust** - Not sensitive to score scale differences
4. **Simple** - Easy to understand and debug

**Alternatives considered:**
- Weighted sum (requires score normalization)
- Borda count (sensitive to list lengths)
- CombSUM/CombMNZ (require comparable score ranges)

**k=60 chosen empirically:**
- Balances influence of top results vs. tail
- Standard value in literature
- Works well in practice

### Why In-Process Model?

Embedding model runs in the same process (via Candle).

**Advantages:**
1. **No server** - One binary, no daemon management
2. **Fast startup** - No network overhead
3. **Simple deployment** - No separate services
4. **Offline** - Works without internet after download

**Trade-offs:**
1. **Memory** - Model stays loaded (~200 MB RAM)
2. **First search slow** - Model loading adds 1-2s latency
3. **No sharing** - Each sift instance loads own model

**Lazy loading strategy:**
- Model loaded on first semantic search
- Stays resident for session lifetime
- Acceptable for interactive CLI tool

## Performance Characteristics

**Indexing:**
- ~1000 files/minute (text, no embeddings)
- ~100 files/minute (with embeddings, CPU)
- ~300 files/minute (with embeddings, Metal M1)

**Search:**
- Lexical: 5-20ms (10K chunks)
- Semantic: 10-50ms (10K chunks)
- Hybrid: 20-80ms (combined)

**First search penalty:**
- Model loading: 1-2 seconds
- Subsequent searches: instant

**Storage:**
- ~2KB per chunk (text + metadata)
- ~1.5KB per embedding (384 × 4 bytes)
- FTS5 index: ~1.5× text size

Example: 1000 markdown files, 5MB total text
- ~10K chunks
- ~20 MB text + metadata
- ~15 MB embeddings
- ~7.5 MB FTS5 index
- **Total: ~45 MB database**

## Future Considerations

### Potential Enhancements

1. **Better chunking**
   - Respect paragraph/section boundaries
   - Overlap for context preservation
   - Adaptive chunk sizes based on content

2. **Query expansion**
   - Synonym expansion
   - Related term suggestions
   - Query rewriting

3. **Filters**
   - File type filters
   - Date ranges
   - Source/label filters
   - Custom metadata

4. **Incremental indexing**
   - Watch mode (inotify/FSEvents)
   - Auto-refresh on file changes
   - Background indexing

5. **Export/Import**
   - Share indexes between machines
   - Backup/restore workflows
   - Merge indexes

### Non-Goals

Explicitly out of scope to maintain focus:

1. **Writing to source files** - Read-only operation
2. **Note-taking features** - Use Obsidian, Logseq, etc.
3. **LLM integration** - Separate tool for that
4. **Server/daemon mode** - CLI-first design
5. **Multi-user** - Single-user tool
6. **Real-time sync** - Local-first

## Appendix: Dependencies

**Core:**
- `candle-core`, `candle-nn`, `candle-transformers` - ML inference
- `rusqlite` - SQLite bindings
- `tokenizers` - HuggingFace tokenizer

**CLI/TUI:**
- `clap` - Argument parsing
- `ratatui` - Terminal UI framework
- `crossterm` - Terminal backend

**Utilities:**
- `anyhow` - Error handling
- `serde`, `serde_json`, `toml` - Serialization
- `ignore` - Gitignore-aware traversal
- `walkdir` - Directory walking
- `sha2` - Checksums
- `ureq` - HTTP downloads

**Why these choices?**
- Mature, well-maintained crates
- Minimal dependency tree
- Good documentation
- Active communities
