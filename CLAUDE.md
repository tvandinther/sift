# CLAUDE.md — sift

`sift` is a personal Rust CLI for hybrid full-text and semantic search over arbitrary local text content. It indexes directories or individual files, stores the index in SQLite, and provides both a TUI (default) and a non-interactive CLI mode. It is a search tool only — it never writes to the source content.

---

## Project goals

- Single self-contained binary — no daemon, no server, no external dependencies at runtime
- Index any text content: plain text, markdown, code, logs, or any file with readable text
- Hybrid search: BM25 full-text (FTS5) + semantic vector search, merged with Reciprocal Rank Fusion
- On-disk index stored as a single SQLite file at a configurable path
- Embedding model runs in-process via Candle — downloaded on first run, cached locally
- TUI when run interactively (`sift`); non-interactive subcommands for scripting and Claude Code integration
- Fast enough to feel instant for personal-scale corpora (thousands of files)

---

## What is out of scope

- Writing, editing, or modifying source files
- Note-taking features
- Summarisation or LLM integration
- Server or daemon mode
- Multi-user or remote index
- Any GUI beyond the TUI

---

## Architecture

```
sift/
├── src/
│   ├── main.rs              # Entry point — TUI if no subcommand, else dispatch
│   ├── cli.rs               # clap argument definitions
│   ├── config.rs            # Config resolution (flags → config file → defaults)
│   ├── index/
│   │   ├── mod.rs           # Indexing orchestration
│   │   ├── read.rs          # File reading and text extraction
│   │   ├── embed.rs         # Embedding generation via Candle
│   │   └── db.rs            # SQLite schema, FTS5, sqlite-vec, index management
│   ├── search/
│   │   ├── mod.rs           # Search orchestration
│   │   ├── lexical.rs       # FTS5 query and result handling
│   │   ├── semantic.rs      # Vector query and ANN result handling
│   │   └── fusion.rs        # Reciprocal Rank Fusion merge and rerank
│   └── tui/
│       ├── mod.rs           # TUI entry point and event loop
│       ├── search.rs        # Interactive search view
│       ├── indexes.rs       # Index list and management view
│       └── theme.rs         # Colours and styles
├── tests/
│   └── fixtures/            # Sample text files for integration tests
├── CLAUDE.md
├── Cargo.toml
└── README.md
```

---

## CLI surface

```
sift                                        # Launch TUI (default)
sift search <query>                         # Non-interactive search
sift search <query> --lexical-only          # FTS5 only
sift search <query> --semantic-only         # Vector only
sift search <query> --limit <n>             # Default 10
sift index add <path> [<path>...]           # Index a file or directory (recursive)
sift index add <path> --name <label>        # Optionally label this index
sift index list                             # List all indexed sources with stats
sift index delete <path-or-name>            # Remove index entries for a path or label
sift index prune                            # Remove index entries for files no longer on disk
sift config                                 # Show resolved config
```

Config resolution order: CLI flags → `~/.config/sift/config.toml` → defaults.

Default `--db` is `~/.local/share/sift/index.db`.

Multiple directories or files can be indexed independently and co-exist in the same database. Each indexed path is tracked as a source.

---

## Key dependencies

| Crate | Purpose |
|---|---|
| `clap` | CLI argument parsing (derive feature) |
| `ratatui` | TUI framework |
| `crossterm` | Terminal backend for ratatui |
| `rusqlite` | SQLite — FTS5 and metadata |
| `sqlite-vec` | Vector storage and ANN search extension |
| `candle-core` + `candle-transformers` | In-process embedding model inference |
| `candle-nn` | Required by candle-transformers |
| `hf-hub` | Download model weights from Hugging Face on first run |
| `tokenizers` | Tokenisation for the embedding model |
| `serde` + `toml` | Config file parsing |
| `walkdir` | Directory traversal |
| `sha2` | SHA256 checksums for change detection |
| `anyhow` | Error handling |
| `tokio` | Async runtime |
| `ignore` | Respects .gitignore rules during traversal (same as ripgrep) |

Do not add dependencies without good reason. Prefer crates already in the tree.

---

## Embedding model

Default model: `nomic-ai/nomic-embed-text-v1` from Hugging Face.

Weights are downloaded on first `sift index` run to `~/.local/share/sift/models/`. Subsequent runs use the cached weights. Never re-download unless `--refresh-model` is passed explicitly.

Embedding dimensions: 768. All vectors stored as `f32`.

The model runs on CPU by default. On Apple Silicon, enable Metal acceleration if available — detect at runtime via candle's device detection, not a compile-time flag.

---

## File handling

`sift` treats any file as a bag of text. There is no special handling for frontmatter, file types, or formats. Files are read as UTF-8 text; non-UTF-8 bytes are replaced with the replacement character rather than failing. Binary files (detected by the presence of null bytes in the first 8KB) are silently skipped.

The `ignore` crate is used for directory traversal — this means `.gitignore`, `.ignore`, and similar files are respected automatically. Hidden files and directories are skipped by default; pass `--hidden` to include them.

Each indexed file is stored with its absolute path. Paths are normalised (resolved symlinks, no trailing slashes) before storage.

---

## SQLite schema

```sql
-- Indexed sources (directories or files passed to `sift index`)
CREATE TABLE IF NOT EXISTS sources (
    id          INTEGER PRIMARY KEY,
    path        TEXT NOT NULL UNIQUE,   -- absolute, normalised path
    label       TEXT,                   -- optional --name label
    indexed_at  TEXT NOT NULL           -- ISO8601 timestamp of last full index
);

-- Indexed files
CREATE TABLE IF NOT EXISTS files (
    id          INTEGER PRIMARY KEY,
    source_id   INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    path        TEXT NOT NULL UNIQUE,   -- absolute path
    checksum    TEXT NOT NULL,          -- SHA256 of file content
    indexed_at  TEXT NOT NULL,          -- ISO8601 timestamp
    size_bytes  INTEGER NOT NULL,
    chunk_count INTEGER NOT NULL DEFAULT 1
);

-- Text chunks (one or more per file for large files)
CREATE TABLE IF NOT EXISTS chunks (
    id        INTEGER PRIMARY KEY,
    file_id   INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    seq       INTEGER NOT NULL,         -- chunk sequence within file (0-based)
    body      TEXT NOT NULL             -- chunk text content
);

-- FTS5 virtual table over chunks
CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
    body,
    content='chunks',
    content_rowid='id'
);

-- Vector table via sqlite-vec (one embedding per chunk)
CREATE VIRTUAL TABLE IF NOT EXISTS chunk_embeddings USING vec0(
    chunk_id  INTEGER PRIMARY KEY,
    embedding FLOAT[768]
);
```

Large files are split into overlapping chunks of ~512 tokens before embedding. Each chunk is independently searchable. Search results de-duplicate to file level before display.

---

## Indexing behaviour

`sift index <path>` walks the path, and for each readable text file:

1. Compute SHA256 of file content
2. If checksum matches existing record, skip
3. Split into chunks if file exceeds token threshold
4. Generate embedding for each chunk
5. Upsert into `sources`, `files`, `chunks`, `chunks_fts`, `chunk_embeddings`
6. Delete stale chunks from a previous version of the file if chunk count changed

Print a live progress indicator during indexing (number of files processed, current file). Print a summary on completion: scanned, added, updated, skipped, failed.

---

## Index management

### `sift index list`

Tabular output:

```
SOURCE                          LABEL       FILES    SIZE      INDEXED
/Users/tom/vault                vault       142      8.4 MB    2025-05-18 14:32
/Users/tom/projects/sift        —           38       1.1 MB    2025-05-18 09:11
```

### `sift index delete <path-or-label>`

Removes all database entries for the given source (cascades to files, chunks, FTS, embeddings). Prompts for confirmation unless `--yes` is passed. Does not touch source files on disk.

### `sift index prune`

Walks every file path in the `files` table. For each path that no longer exists on disk, removes the file record (cascades to chunks, FTS, embeddings). Prints a summary of what was removed. Safe to run at any time.

---

## Search behaviour

### Lexical (FTS5)

```sql
SELECT chunks.id, chunks.file_id, rank
FROM chunks_fts
JOIN chunks ON chunks.id = chunks_fts.rowid
WHERE chunks_fts MATCH ?
ORDER BY rank
LIMIT 100;
```

### Semantic

Embed the query string. Query `chunk_embeddings` for top-100 nearest neighbours by cosine similarity via sqlite-vec. Join back to `chunks` and `files`.

### Fusion

Apply Reciprocal Rank Fusion across chunk-level results, then de-duplicate to file level (keep the highest-scoring chunk per file as the representative snippet). Return top `--limit` files.

```
rrf_score(item) = Σ 1 / (60 + rank)
```

### Non-interactive output

One result per line:

```
/Users/tom/vault/crossplane-composition-latency.md    [lex+sem]  "…composition function latency was traced to…"
/Users/tom/vault/istio-authorizationpolicy.md         [sem]      "…namespace isolation via AuthorizationPolicy…"
/Users/tom/projects/sift/src/search/fusion.rs         [lex]      "…reciprocal rank fusion implementation…"
```

`[lex+sem]` — appeared in both result lists. `[lex]` or `[sem]` — appeared in one only.

With `--json`, output a JSON array of result objects for scripting.

---

## TUI

Launched by running `sift` with no subcommand.

### Layout

```
┌─ sift ──────────────────────────────────────────────────────┐
│ > _                                          [hybrid] [10]  │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  Results appear here as you type                            │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│ /Users/tom/vault · 142 files   [?] help  [i] indexes  [q]  │
└─────────────────────────────────────────────────────────────┘
```

- **Search bar** at top — live search as you type (debounced ~200ms)
- **Results pane** — scrollable list of results with snippets
- **Status bar** at bottom — active source, file count, key hints

### Key bindings

| Key | Action |
|---|---|
| Type | Update query, trigger search |
| `↑` / `↓` | Navigate results |
| `Enter` | Open selected file in `$EDITOR` |
| `Tab` | Toggle between hybrid / lexical / semantic mode |
| `i` | Switch to index list view |
| `?` | Show help overlay |
| `q` / `Esc` | Quit |

### Index list view (`i`)

Shows the same output as `sift list` in a navigable table. Allows deleting a source (`d`) and triggering gc (`g`) without leaving the TUI.

---

## Error handling

Use `anyhow` throughout. Errors at the CLI boundary must be human-readable.

- No index found → `No index found. Run 'sift index <path>' first.`
- Model weights missing, search without `--lexical-only` → prompt to download or suggest `--lexical-only`
- File read error during indexing → log warning, continue, report in summary
- Binary file detected → silently skip

---

## Code style

- Idiomatic Rust — no `unwrap()` in non-test code
- Keep `main.rs` thin — dispatch only
- Each module owns its domain
- Prefer `?` for error propagation
- Doc comment on every public function
- No `unsafe` without a comment explaining soundness

---

## Testing

- Unit test text chunking with edge cases (empty file, file smaller than chunk size, exact chunk boundary)
- Unit test RRF fusion with known inputs and expected output order
- Integration test: index `tests/fixtures/`, run searches, assert result ordering and file-level deduplication
- Unit tests use SQLite in-memory database (`:memory:`)
- Fixture files should be realistic text content across varied formats (`.md`, `.txt`, `.rs`, `.toml`) so search tests are meaningful

---

## What to avoid

- Do not special-case file formats or parse structure (no frontmatter extraction, no AST)
- Do not write to source files under any circumstances
- Do not add a server or HTTP API
- Do not store relative paths — always absolute, normalised
- Do not silently skip files with read errors — warn and continue
- Do not assume a specific directory layout in source content