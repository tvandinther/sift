# Sift - Just recipes

# Show available commands
help:
    @just --list

# Build the project in debug mode
build:
    cargo build

# Build the project in release mode with optimizations
build-release:
    cargo build --release

# Run all tests
test:
    cargo test

# Run tests in quiet mode
test-quiet:
    cargo test --quiet

# Run ignored tests (e.g., embedding model tests)
test-all:
    cargo test -- --include-ignored

# Run the TUI (default command)
run *ARGS:
    cargo run -- {{ARGS}}

# Run with verbose output
run-verbose *ARGS:
    cargo run -- --verbose {{ARGS}}

# Clean build artifacts
clean:
    cargo clean

# Format code with rustfmt
fmt:
    cargo fmt

# Check formatting without modifying files
fmt-check:
    cargo fmt -- --check

# Run clippy lints
lint:
    cargo clippy -- -D warnings

# Fix clippy warnings automatically
fix:
    cargo clippy --fix --allow-dirty --allow-staged

# Build documentation
doc:
    cargo doc --no-deps --open

# Install the binary locally
install:
    cargo install --path .

# Install to a specific location
install-to PATH:
    cargo install --path . --root {{PATH}}

# Watch and rebuild on file changes (requires cargo-watch)
watch:
    cargo watch -x build

# Watch and run tests on file changes
watch-test:
    cargo watch -x test

# Run in release mode
run-release *ARGS:
    cargo run --release -- {{ARGS}}

# Build with Nix
nix-build:
    nix build

# Enter Nix development shell
nix-shell:
    nix develop

# Check Nix flake
nix-check:
    nix flake check

# Run benchmarks (if any exist)
bench:
    cargo bench

# Generate and view code coverage (requires cargo-tarpaulin)
coverage:
    cargo tarpaulin --out Html --output-dir target/coverage
    open target/coverage/index.html

# Check for outdated dependencies
outdated:
    cargo outdated

# Update dependencies
update:
    cargo update

# Audit dependencies for security vulnerabilities
audit:
    cargo audit

# Create a new release build and show the binary location
release: build-release
    @echo "Release binary built at: target/release/sift"
    @ls -lh target/release/sift

# Run integration tests with fixtures
test-integration:
    cargo test --test integration

# Quick check - format, lint, and test
check: fmt-check lint test
    @echo "✓ All checks passed!"

# Full CI check - format, lint, test, and build release
ci: fmt-check lint test build-release
    @echo "✓ CI checks passed!"

# Clean database (remove index.db)
clean-db:
    rm -f ~/.local/share/sift/index.db
    @echo "Database cleaned"

# Reset everything - clean build and database
reset: clean clean-db
    @echo "✓ All artifacts removed"
