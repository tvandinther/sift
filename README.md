# sift

Hybrid full-text and semantic search over local text content.

## Overview

`sift` is a Rust CLI tool for indexing and searching arbitrary text files using hybrid search (BM25 + semantic vectors). It stores everything in a single SQLite database with no external dependencies.

**Current status:** Core indexing and management commands are implemented. Search functionality and TUI coming in the next iteration.

## Features

- **Index any text content** — markdown, code, logs, config files, meeting notes
- **Smart file handling** — respects `.gitignore`, skips binary files, handles non-UTF8 gracefully
- **Efficient storage** — SHA256-based change detection, chunked storage for large files
- **Simple CLI** — non-interactive commands for scripting and automation
- **SQLite-backed** — single database file with FTS5 full-text search (vector search coming soon)

## Installation

```bash
cargo build --release
cp target/release/sift ~/.local/bin/  # or your preferred bin directory
```

## Usage

### Add files to the index

```bash
# Index a directory
sift index add ~/notes

# Index with a label
sift index add ~/projects/myapp --name myapp

# Include hidden files
sift index add ~/dotfiles --hidden

# Index multiple paths at once
sift index add ~/notes ~/docs ~/projects
```

### List indexed sources

```bash
sift index list
```

### Delete an index

```bash
sift index delete ~/notes

# Or delete by label
sift index delete myapp

# Skip confirmation
sift index delete ~/notes --yes
```

### Prune missing files

Remove index entries for files that no longer exist:

```bash
sift index prune
```

### View configuration

```bash
sift config
```

## Configuration

Default configuration:

```toml
db_path = "~/.local/share/sift/index.db"
model_cache = "~/.local/share/sift/models"
```

Override via `~/.config/sift/config.toml` or CLI flags:

```bash
sift --db /custom/path/index.db index add ~/notes
```

## Architecture

```
sift/
├── src/
│   ├── main.rs              # CLI entry point
│   ├── lib.rs               # Library interface
│   ├── cli.rs               # clap argument parsing
│   ├── config.rs            # Config resolution
│   └── index/
│       ├── mod.rs           # Indexing orchestration
│       ├── read.rs          # File reading and chunking
│       └── db.rs            # SQLite schema and queries
└── tests/
    ├── fixtures/            # Sample files for testing
    └── integration.rs       # Integration tests
```

## How it works

1. **File discovery** — Uses `ignore` crate (same as ripgrep) to walk directories and respect `.gitignore`
2. **Binary detection** — Checks first 8KB for null bytes, skips binary files
3. **Text reading** — UTF-8 with lossy fallback for non-UTF8 content
4. **Chunking** — Splits large files into ~400 word chunks with 50 word overlap
5. **Change detection** — SHA256 checksum comparison, only re-indexes changed files
6. **Storage** — SQLite with FTS5 for full-text search, cascading deletes for cleanup

## Coming soon

- [ ] Semantic search with in-process embedding model (Candle)
- [ ] Hybrid search with Reciprocal Rank Fusion
- [ ] Interactive TUI with live search
- [ ] `sift search` non-interactive command
- [ ] Result highlighting and snippet extraction

## Development

### Running tests

```bash
cargo test
```

### Building

```bash
cargo build
```

### Linting

```bash
cargo clippy
```

## License

MIT
