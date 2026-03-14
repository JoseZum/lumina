# 03 - Cargo.toml & Dependencies

## Complete Cargo.toml

```toml
[package]
name = "lumina"
version = "0.1.0"
edition = "2021"
rust-version = "1.75"
description = "Semantic code search MCP server for Claude Code"
license = "Apache-2.0"
repository = "https://github.com/YOUR_USER/lumina"

[[bin]]
name = "lumina"
path = "src/main.rs"

[lib]
name = "lumina"
path = "src/lib.rs"

[dependencies]
# ─── CLI ────────────────────────────────────────────────
clap = { version = "4.5", features = ["derive"] }
# derive: enables #[derive(Parser)] for zero-boilerplate CLI definition

# ─── Async runtime (MINIMAL - only for LanceDB) ────────
tokio = { version = "1.43", features = ["rt", "macros"] }
# rt: single-threaded runtime (NOT rt-multi-thread)
# macros: for #[tokio::main]
# NOTE: We do NOT enable "io-std" or "io-util" — MCP server uses std::io

# ─── CPU parallelism ───────────────────────────────────
rayon = "1.10"
# Data parallelism for tree-sitter parsing and SHA-256 hashing
# rayon::par_iter is the correct tool for CPU-bound parallel work

# ─── Tree-sitter core ──────────────────────────────────
tree-sitter = "0.24"
# Core parsing library. Provides Parser, Tree, Node types.
# Version must be ABI-compatible with grammar crates below.

# ─── Tree-sitter language grammars ──────────────────────
tree-sitter-python = "0.23"
tree-sitter-javascript = "0.23"
tree-sitter-typescript = "0.23"
tree-sitter-rust = "0.23"
tree-sitter-go = "0.23"
tree-sitter-java = "0.23"
# Each grammar crate provides the Language struct for one language.
# Versions must be ABI-compatible with tree-sitter core.
# If a grammar doesn't compile, check ABI compatibility first.

# ─── Vector storage ────────────────────────────────────
lancedb = "0.15"
# Local vector database. Stores chunks + embeddings in .lance format.
# Async API (requires tokio), but we wrap it in sync via block_on().
# If the Rust SDK proves immature, see fallback plan in 07-DESIGN_DECISIONS.md

arrow-array = "53"
arrow-schema = "53"
# Arrow types for constructing RecordBatch to insert into LanceDB.
# Versions MUST match what lancedb re-exports. Check with:
#   cargo tree -i arrow-array
# If versions conflict, align to what lancedb pulls in.

# ─── BM25 keyword search ───────────────────────────────
tantivy = "0.22"
# Full-text search engine with BM25 scoring.
# Synchronous API. Used for keyword-based code search.
# Fields: chunk text (tokenized) + symbol name (for find_symbol)

# ─── HTTP client ────────────────────────────────────────
reqwest = { version = "0.12", features = ["json", "blocking"] }
# json: automatic JSON serialization/deserialization in requests
# blocking: synchronous HTTP client (no async needed for API calls)
# Used for: Voyage embedding API, reranker API
#
# WHY blocking instead of async:
# - Embedding calls happen during indexing (rayon threads, not tokio)
# - Reranker calls happen once per query (no concurrency benefit)
# - API rate limits prevent meaningful speedup from async anyway
# - blocking removes the need to pass tokio handles around

# ─── Serialization ──────────────────────────────────────
serde = { version = "1", features = ["derive"] }
# derive: #[derive(Serialize, Deserialize)] on all data types

serde_json = "1"
# JSON parsing for MCP protocol, API responses, config

bincode = "1.3"
# Compact binary serialization for hash cache file.
# 100K entries: JSON = ~15MB, bincode = ~3MB, 10x faster deser.

toml = "0.8"
# Parse .lumina/config.toml

# ─── Hashing ───────────────────────────────────────────
sha2 = "0.10"
# SHA-256 for file and chunk content hashing.
# Used for: incremental indexing (skip unchanged chunks)

hex = "0.4"
# Convert SHA-256 bytes to hex string for chunk IDs.

# ─── File walking ──────────────────────────────────────
ignore = "0.4"
# From the ripgrep project. Walks directories respecting:
# - .gitignore (including nested)
# - .ignore files
# - global gitignore (~/.config/git/ignore)
# Also handles: hidden files, symlinks, filesystem errors.
# Alternative: walkdir + manual gitignore parsing. Not worth it.

# ─── Error handling ────────────────────────────────────
thiserror = "2"
# Derive macro for error enums. Used in error.rs for LuminaError.
# Generates Display impl from #[error("...")] attributes.

anyhow = "1"
# Used ONLY in main.rs for top-level error wrapping.
# All internal code uses LuminaError. anyhow provides nice
# error chain formatting for CLI output.

# ─── Logging ───────────────────────────────────────────
tracing = "0.1"
# Structured logging. All log output goes to stderr
# (stdout is reserved for MCP JSON-RPC messages).

tracing-subscriber = { version = "0.3", features = ["env-filter"] }
# env-filter: control log level via RUST_LOG env var.
# Example: RUST_LOG=lumina=debug lumina mcp --repo .

[dev-dependencies]
tempfile = "3"
# Create temporary directories for test fixtures.
# Each test gets an isolated directory that's cleaned up automatically.

assert_cmd = "2"
# Test the lumina binary as a subprocess.
# Provides helpers for stdin/stdout/stderr assertions.

predicates = "3"
# Readable assertions for CLI test output.
# Example: cmd.assert().stdout(predicate::str::contains("indexed 5 files"))
```

## Dependency Justification Table

| Dependency | Size Impact | Why Not Alternative | Can Drop Later? |
|-----------|-------------|--------------------|--------------------|
| `tokio` | Medium (~2MB) | LanceDB requires it. Only use `rt` (no multi-thread). | Yes, if LanceDB adds sync API |
| `rayon` | Small (~200KB) | Nothing else does CPU parallelism this well in Rust | No, core to performance |
| `tree-sitter` | Small (~500KB) | Only production-grade multi-language parser available | No, core to the product |
| `tree-sitter-*` | ~2MB each | Each grammar is a compiled C library. No alternative. | Can drop unused languages |
| `lancedb` | Large (~5MB+) | Purpose-built for local vector search. See fallback plan. | Yes, brute-force fallback exists |
| `arrow-*` | Medium (~3MB) | Required by LanceDB for data format | Drops with LanceDB |
| `tantivy` | Large (~4MB) | Only serious BM25 engine in Rust. SQLite FTS5 is worse. | No, core to hybrid search |
| `reqwest` | Medium (~2MB) | Standard HTTP client in Rust ecosystem | No, needed for APIs |
| `serde` + `serde_json` | Small | Universal Rust serialization. No alternative. | No |
| `bincode` | Tiny (~50KB) | Fastest serialization for hash cache | Could use JSON (slower) |
| `sha2` | Tiny (~100KB) | Standard SHA-256 impl via RustCrypto | No |
| `ignore` | Small (~200KB) | From ripgrep, handles all gitignore edge cases | Could use walkdir (worse) |
| `thiserror` | Zero runtime | Compile-time only. Generates error Display impls. | Could write manually |
| `tracing` | Small (~300KB) | Standard structured logging for Rust | Could use `log` (less features) |

## Total Binary Size Estimate

Release build with `strip` and LTO:
- Estimated: **15-25 MB** (dominated by tree-sitter grammars and tantivy)
- With `opt-level = "z"` and `lto = true`: **10-18 MB**

This is acceptable for a CLI tool. Cursor's extension is 50MB+.

## Cargo.toml Profile Tuning

```toml
[profile.release]
lto = "thin"        # Faster than "fat" LTO, still good size reduction
strip = true        # Strip debug symbols from binary
codegen-units = 1   # Better optimization, slower compile
opt-level = 3       # Max speed (default for release)

[profile.dev]
opt-level = 1       # Slightly optimize dev builds (tree-sitter is slow at opt-0)
```

## Version Pinning Strategy

**Pin major + minor, allow patch**: `"0.22"` means `>=0.22.0, <0.23.0`.
This balances stability (no breaking changes) with getting security fixes.

**Exception**: `arrow-*` crates must be pinned to EXACT major version matching
what `lancedb` uses internally. Check with:

```bash
cargo tree -i arrow-array
```

If `lancedb` uses `arrow-array 53.x` but we specified `54.x`, we get duplicate
types that don't convert between each other. This is the #1 cause of compile
errors when using Arrow-based crates.

## Feature Flags Rationale

### Why `tokio` has only `["rt", "macros"]`

- `rt`: The single-threaded runtime. Enough for our needs.
- `macros`: For `#[tokio::main]` on `fn main()`.
- NOT `rt-multi-thread`: We don't need a multi-threaded async runtime. rayon handles parallelism.
- NOT `io-std`: We use `std::io::stdin()` for MCP, not tokio's async stdin.
- NOT `io-util`: No async IO utilities needed.
- NOT `net`: No TCP/HTTP server. MCP uses stdio.
- NOT `time`: No sleeps or timeouts needed in async context.
- NOT `sync`: No async mutexes or channels. We use std sync primitives.
- NOT `fs`: No async file IO. `std::fs` is fine for local files (SSD is fast enough).

Every tokio feature we DON'T enable is compile time saved and binary size saved.

### Why `reqwest` has `["json", "blocking"]`

- `json`: Avoids manual `serde_json::to_string()` + content-type header setting.
- `blocking`: Synchronous client for use in rayon threads and simple code paths.
- NOT `rustls-tls`: reqwest includes TLS by default. `rustls-tls` would replace
  OpenSSL with a Rust TLS impl. Both work, default is fine.
- NOT `cookies`: We don't need cookie management for API calls.
- NOT `gzip`/`brotli`: API responses are small JSON. Compression overhead > benefit.

### Why `clap` has `["derive"]`

- `derive`: `#[derive(Parser)]` macro for declarative CLI definition.
- NOT `env`: We handle env vars manually in config.rs (more control).
- NOT `unicode`: Default ASCII is fine for our CLI.
- NOT `wrap_help`: We keep help text short enough to not need wrapping.
