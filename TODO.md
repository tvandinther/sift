# TODO

## Phase 1: Core Indexing ✓ Complete

- [x] Project scaffolding with Cargo
- [x] Config resolution (CLI → file → defaults)
- [x] SQLite schema with FTS5
- [x] File reading and binary detection
- [x] Text chunking (400 words, 50 word overlap)
- [x] SHA256-based change detection
- [x] .gitignore-aware traversal
- [x] Index management commands (add, list, delete, prune)
- [x] Integration tests with realistic fixtures
- [x] CLI restructure with nested commands

## Phase 2: Embeddings & Vector Search (In Progress)

- [x] Integrate Candle embedding model
  - [x] Model download and caching logic (HF Hub)
  - [x] Load `nomic-embed-text-v1` from HuggingFace
  - [x] Metal acceleration on Apple Silicon (auto-detect)
  - [x] Generate embeddings during indexing
- [x] Embedding storage
  - [x] Create `chunk_embeddings` table (BLOB storage)
  - [x] Store embeddings as BLOB (768 f32 values)
  - [ ] **TODO:** Migrate to sqlite-vec vec0 for ANN performance
- [x] Update indexing pipeline
  - [x] Generate embeddings for each chunk
  - [x] Store vectors alongside text
  - [x] Handle embedding errors gracefully
  - [x] Add `--no-embeddings` flag for lexical-only indexing
- [ ] Semantic search implementation
  - [ ] Embed query string
  - [ ] Compute cosine similarity (in-memory for now)
  - [ ] Rank results by similarity
- [ ] Tests
  - [x] Unit test embedding generation (ignored until model downloaded)
  - [ ] Integration test with vector storage

## Phase 3: Search Implementation

- [ ] Lexical search (FTS5)
  - [ ] Query parser for FTS5 syntax
  - [ ] Rank and score results
  - [ ] Snippet extraction with highlighting
- [ ] Semantic search
  - [ ] Embed query string
  - [ ] KNN search via sqlite-vec
  - [ ] Cosine similarity scoring
- [ ] Hybrid search with RRF
  - [ ] Reciprocal Rank Fusion implementation
  - [ ] Merge lexical + semantic results
  - [ ] File-level deduplication
  - [ ] Configurable fusion weights
- [ ] Non-interactive `sift search` command
  - [ ] Text output with snippets
  - [ ] JSON output for scripting
  - [ ] Mode flags (--lexical-only, --semantic-only)
- [ ] Tests
  - [ ] Unit test RRF fusion logic
  - [ ] Integration test search quality

## Phase 4: TUI

- [ ] Ratatui setup
  - [ ] Event loop with crossterm
  - [ ] Layout: search bar + results + status
  - [ ] Theme and colors
- [ ] Interactive search view
  - [ ] Live search with debouncing
  - [ ] Scrollable results
  - [ ] Result selection and preview
  - [ ] Snippet highlighting
- [ ] Index management view
  - [ ] List sources
  - [ ] Delete sources
  - [ ] Run prune
- [ ] Key bindings
  - [ ] Search input
  - [ ] Result navigation
  - [ ] Mode toggle (hybrid/lexical/semantic)
  - [ ] View switching (search ↔ indexes)
  - [ ] File opening in $EDITOR
- [ ] Help overlay

## Phase 5: Polish & Optimization

- [ ] Performance
  - [ ] Benchmark indexing speed
  - [ ] Optimize chunking strategy
  - [ ] Connection pooling if needed
- [ ] Error handling
  - [ ] Better error messages
  - [ ] Graceful degradation
  - [ ] Recovery from partial failures
- [ ] Documentation
  - [ ] User guide
  - [ ] API documentation
  - [ ] Architecture decision records
- [ ] Distribution
  - [ ] Release builds
  - [ ] Binary distribution
  - [ ] Homebrew formula (optional)

## Future Enhancements (Nice to Have)

- [ ] Incremental re-indexing (watch mode)
- [ ] Multi-index search across sources
- [ ] Query history and saved searches
- [ ] Export results to various formats
- [ ] Custom stop words and stemming
- [ ] File type filters
- [ ] Date range filters
- [ ] Fuzzy matching
- [ ] Parallel indexing
- [ ] Compressed storage
