# 02 - Complete Project Structure

## File Tree

```
lumina/
├── Cargo.toml                        # Dependencies, metadata, build config
├── Cargo.lock                        # Pinned dependency versions (committed)
├── .gitignore                        # Ignore .lumina/, target/, etc.
├── PLAN.md                           # Master plan overview
├── LICENSE                           # Apache-2.0
├── mcp.json.example                  # Example MCP config for Claude Code
│
├── docs/                             # Implementation documentation
│   ├── 01-ARCHITECTURE.md
│   ├── 02-PROJECT_STRUCTURE.md       # This file
│   ├── 03-CARGO_TOML.md
│   ├── 04-MODULE_DESIGN.md
│   ├── 05-MCP_PROTOCOL.md
│   ├── 06-IMPLEMENTATION_ORDER.md
│   ├── 07-DESIGN_DECISIONS.md
│   └── 08-TESTING.md
│
├── src/
│   ├── main.rs                       # CLI entry point
│   ├── lib.rs                        # Public API re-exports
│   ├── error.rs                      # Error types
│   ├── types.rs                      # Shared data types
│   ├── config.rs                     # Configuration management
│   │
│   ├── chunker/
│   │   ├── mod.rs                    # Chunker trait definition
│   │   ├── treesitter.rs             # Tree-sitter implementation
│   │   └── languages.rs              # Language grammar configs
│   │
│   ├── embeddings/
│   │   ├── mod.rs                    # Embedder trait definition
│   │   └── voyage.rs                 # Voyage API client
│   │
│   ├── store/
│   │   ├── mod.rs                    # VectorStore + KeywordStore traits
│   │   ├── lance.rs                  # LanceDB implementation
│   │   └── tantivy_store.rs          # Tantivy BM25 implementation
│   │
│   ├── search/
│   │   ├── mod.rs                    # SearchEngine orchestrator
│   │   ├── rrf.rs                    # Reciprocal Rank Fusion
│   │   └── reranker.rs              # Reranker trait + implementations
│   │
│   ├── indexer/
│   │   ├── mod.rs                    # Indexing pipeline
│   │   └── hasher.rs                 # SHA-256 file/chunk hashing
│   │
│   └── mcp/
│       ├── mod.rs                    # MCP server main loop
│       ├── protocol.rs               # JSON-RPC message types
│       ├── handler.rs                # Request dispatcher
│       └── tools.rs                  # Tool definitions + handlers
│
├── tests/
│   ├── common/
│   │   └── mod.rs                    # Shared test utilities
│   ├── fixtures/
│   │   ├── sample_rust/              # Fixture: small Rust project
│   │   │   ├── src/
│   │   │   │   ├── main.rs
│   │   │   │   ├── lib.rs
│   │   │   │   └── user.rs
│   │   │   └── Cargo.toml
│   │   ├── sample_python/            # Fixture: small Python project
│   │   │   ├── app/
│   │   │   │   ├── __init__.py
│   │   │   │   ├── models.py
│   │   │   │   ├── auth.py
│   │   │   │   └── utils.py
│   │   │   └── requirements.txt
│   │   └── sample_mixed/             # Fixture: multi-language project
│   │       ├── backend/
│   │       │   └── main.go
│   │       ├── frontend/
│   │       │   ├── App.tsx
│   │       │   └── api.ts
│   │       └── README.md
│   ├── test_chunker.rs              # Chunker unit/integration tests
│   ├── test_indexer.rs              # Indexer pipeline tests
│   ├── test_search.rs               # Search + RRF tests
│   └── test_mcp.rs                  # MCP server protocol tests
│
└── .lumina/                          # Runtime data directory (gitignored)
    ├── config.toml                   # User configuration
    ├── index.lance/                  # LanceDB data files
    ├── tantivy_index/                # Tantivy index files
    └── hashes.bin                    # SHA-256 hash cache (bincode)
```

## File-by-File Detail

### `src/main.rs` — CLI Entry Point

**Purpose**: Parse CLI arguments, wire together all modules, dispatch commands.
This file should contain ZERO business logic — only plumbing.

**Contains**:
- `fn main()` with `#[tokio::main(flavor = "current_thread")]`
  - Note: `current_thread` flavor because we only need tokio for LanceDB.
    No need for multi-threaded runtime.
- `#[derive(Parser)] struct Cli` with subcommands:
  - `lumina index [--repo PATH] [--force]` — index or re-index a repo
  - `lumina query <QUERY> [--k NUM]` — search from CLI (for testing)
  - `lumina mcp [--repo PATH]` — start MCP server on stdio
  - `lumina status` — show index stats
- Each subcommand calls into `lib.rs` functions

**Key decision**: `main.rs` uses `anyhow::Result` for top-level error handling.
All internal modules use `LuminaError`. The conversion happens at the boundary.

### `src/lib.rs` — Public API

**Purpose**: Re-export the public interface. Also serves as the integration point
for anyone using Lumina as a library (not just CLI).

**Contains**:
```rust
pub mod error;
pub mod types;
pub mod config;
pub mod chunker;
pub mod embeddings;
pub mod store;
pub mod search;
pub mod indexer;
pub mod mcp;
```

Plus convenience functions:
- `pub fn create_indexer(config: &LuminaConfig) -> Result<Indexer>`
- `pub fn create_search_engine(config: &LuminaConfig) -> Result<SearchEngine>`

These factory functions wire together the concrete implementations (TreeSitterChunker,
VoyageEmbedder, LanceStore, TantivyStore) behind the trait interfaces. This is the
only place where concrete types are named — everything else works with trait objects.

### `src/error.rs` — Error Types

**Purpose**: Single error enum for the entire crate. Every module's errors are
variants here. No per-module error types (that's over-engineering for this codebase size).

**Contains**:
- `enum LuminaError` with ~10 variants (see Module Design doc)
- `type Result<T> = std::result::Result<T, LuminaError>`
- `From` impls for `std::io::Error`, `serde_json::Error`

### `src/types.rs` — Shared Data Types

**Purpose**: Data structures used across modules. No logic, just `struct` definitions
with `Serialize`/`Deserialize` derives.

**Contains**:
- `struct Chunk` — the core unit of indexed code
- `enum SymbolKind` — function, method, class, struct, etc.
- `struct SearchResult` — a chunk with relevance score
- `enum SearchSource` — where the result came from (vector, keyword, fused, reranked)
- `struct SymbolInfo` — symbol name + location for find_symbol
- `struct IndexStats` — counters from an index run
- `struct FileMetadata` — file path, language, line count, hash

### `src/config.rs` — Configuration

**Purpose**: Load config from `.lumina/config.toml`, env vars, and CLI args.
Precedence: CLI args > env vars > config file > defaults.

**Contains**:
- `struct LuminaConfig` with fields:
  - `repo_root: PathBuf`
  - `data_dir: PathBuf` (default: `{repo_root}/.lumina`)
  - `voyage_api_key: Option<String>` (from env `VOYAGE_API_KEY`)
  - `voyage_model: String` (default: `"voyage-code-3"`)
  - `reranker_api_key: Option<String>` (from env `RERANKER_API_KEY`)
  - `reranker_model: String` (default: `"jina-reranker-v2-base-multilingual"`)
  - `max_chunk_tokens: usize` (default: 500)
  - `min_chunk_tokens: usize` (default: 50)
  - `max_file_size: usize` (default: 1MB)
  - `embedding_batch_size: usize` (default: 128)
  - `search_k_vector: usize` (default: 30, candidates for RRF)
  - `search_k_keyword: usize` (default: 30)
  - `rrf_k: u32` (default: 60)
  - `response_token_budget: usize` (default: 2000)
- `impl LuminaConfig { pub fn load(repo_root: PathBuf) -> Result<Self> }`

### `src/chunker/mod.rs` — Chunker Trait

**Purpose**: Define the interface for code chunking. Any future chunker
(regex-based, language-server-based) implements this trait.

**Contains**:
- `trait Chunker: Send + Sync`
  - `fn chunk_file(&self, path: &Path, content: &str) -> Result<Vec<Chunk>>`
  - `fn supported_extensions(&self) -> &[&str]`
- `pub use treesitter::TreeSitterChunker`

### `src/chunker/treesitter.rs` — Tree-Sitter Chunker

**Purpose**: The main chunker implementation. Uses tree-sitter to parse source
code into an AST, then extracts semantic nodes (functions, classes, methods)
as chunks.

**Contains**:
- `struct TreeSitterChunker` with language configs
- `impl Chunker for TreeSitterChunker`
- Private helpers:
  - `fn parse_with_language(&self, content: &str, lang: &LanguageConfig) -> Result<Tree>`
  - `fn extract_chunks_from_tree(&self, tree: &Tree, content: &str, ...) -> Vec<Chunk>`
  - `fn merge_small_nodes(chunks: Vec<Chunk>, min_tokens: usize) -> Vec<Chunk>`
  - `fn split_large_node(chunk: Chunk, max_tokens: usize) -> Vec<Chunk>`
  - `fn estimate_tokens(text: &str) -> usize`

**This is the most complex module.** Each language has different AST node names
for the same concepts. Python uses `function_definition`, Rust uses `function_item`,
Go uses `function_declaration`. The `languages.rs` file abstracts this.

### `src/chunker/languages.rs` — Language Configurations

**Purpose**: Map file extensions to tree-sitter grammars and define extraction
queries for each language.

**Contains**:
- `struct LanguageConfig` with grammar + queries
- `fn get_language_config(extension: &str) -> Option<&'static LanguageConfig>`
- `fn all_configs() -> &'static [LanguageConfig]`
- One `LanguageConfig` instance per supported language

Each config includes tree-sitter S-expression queries that match the semantic
nodes to extract. These queries are the language-specific part; everything else
is language-agnostic.

### `src/embeddings/mod.rs` — Embedder Trait

**Purpose**: Define the interface for text embedding.

**Contains**:
- `trait Embedder: Send + Sync`
  - `fn embed_texts(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>`
  - `fn embed_query(&self, query: &str) -> Result<Vec<f32>>`
  - `fn dimensions(&self) -> usize`
- `pub use voyage::VoyageEmbedder`

The `embed_query` vs `embed_texts` distinction matters: Voyage uses different
`input_type` values ("query" vs "document") which affect the embedding quality.

### `src/embeddings/voyage.rs` — Voyage API Client

**Purpose**: Call Voyage AI's embedding API.

**Contains**:
- `struct VoyageEmbedder` with `reqwest::blocking::Client`, API key, model name
- `impl Embedder for VoyageEmbedder`
- Private helpers:
  - `fn call_api(&self, texts: &[&str], input_type: &str) -> Result<Vec<Vec<f32>>>`
  - Response deserialization types (`VoyageResponse`, `VoyageData`)

### `src/store/mod.rs` — Store Traits

**Purpose**: Define interfaces for vector storage and keyword storage.
Two separate traits because they're fundamentally different systems.

**Contains**:
- `trait VectorStore: Send + Sync`
  - `fn upsert_chunks(&self, chunks: &[Chunk]) -> Result<()>`
  - `fn vector_search(&self, embedding: &[f32], k: usize) -> Result<Vec<SearchResult>>`
  - `fn delete_by_file(&self, file: &str) -> Result<()>`
  - `fn list_files(&self) -> Result<Vec<FileMetadata>>`
  - `fn chunk_exists(&self, id: &str) -> Result<bool>`
- `trait KeywordStore: Send + Sync`
  - `fn upsert_chunks(&self, chunks: &[Chunk]) -> Result<()>`
  - `fn keyword_search(&self, query: &str, k: usize) -> Result<Vec<SearchResult>>`
  - `fn symbol_search(&self, name: &str) -> Result<Vec<SymbolInfo>>`
  - `fn delete_by_file(&self, file: &str) -> Result<()>`

### `src/store/lance.rs` — LanceDB Vector Store

**Purpose**: Persist chunks with embeddings in LanceDB format, provide ANN vector search.

**Contains**:
- `struct LanceStore` with `lancedb::Connection`, table handle, tokio `Runtime`
- `impl VectorStore for LanceStore`
- Private helpers:
  - `fn chunks_to_record_batch(chunks: &[Chunk]) -> Result<RecordBatch>`
  - `fn record_batch_to_results(batch: RecordBatch) -> Vec<SearchResult>`
  - `fn create_schema() -> Schema` (Arrow schema definition)

The tokio `Runtime` is created once in `LanceStore::open()` and used for all
async operations via `self.rt.block_on(...)`. This keeps async contained.

### `src/store/tantivy_store.rs` — Tantivy BM25 Store

**Purpose**: Full-text keyword search using tantivy's BM25 scoring.

**Contains**:
- `struct TantivyStore` with `tantivy::Index`, `IndexReader`
- `impl KeywordStore for TantivyStore`
- Private helpers:
  - `fn build_schema() -> tantivy::schema::Schema`
  - `fn chunk_to_document(chunk: &Chunk) -> tantivy::Document`

Tantivy is fully synchronous. No async anywhere.

### `src/search/mod.rs` — Search Engine

**Purpose**: Orchestrate the full search pipeline: embed query → dual retrieval
→ RRF fusion → optional reranking → token budget enforcement.

**Contains**:
- `struct SearchEngine` with references to stores, embedder, reranker
- Public methods:
  - `fn semantic_search(&self, query: &str, k: usize) -> Result<Vec<SearchResult>>`
  - `fn find_symbol(&self, name: &str) -> Result<Vec<SymbolInfo>>`
  - `fn get_file_span(&self, file: &str, start: u32, end: u32) -> Result<String>`
  - `fn list_files(&self) -> Result<Vec<FileMetadata>>`
  - `fn format_results(&self, results: &[SearchResult], budget: usize) -> String`

### `src/search/rrf.rs` — Reciprocal Rank Fusion

**Purpose**: Combine two ranked result lists into one using RRF scoring.
This is a pure function, no state.

**Contains**:
- `pub fn rrf_merge(list_a: Vec<SearchResult>, list_b: Vec<SearchResult>, k: u32) -> Vec<SearchResult>`

About 30 lines of code. The formula: `score(d) = Σ 1/(k + rank_i(d))` where k=60.

### `src/search/reranker.rs` — Reranker

**Purpose**: Optional cross-encoder reranking of search results.

**Contains**:
- `trait Reranker: Send + Sync`
  - `fn rerank(&self, query: &str, results: Vec<SearchResult>, k: usize) -> Result<Vec<SearchResult>>`
- `struct NoopReranker` — passes results through unchanged
- `struct ApiReranker` — calls Jina/Mixedbread API

### `src/indexer/mod.rs` — Indexing Pipeline

**Purpose**: Orchestrate the full indexing process: walk → filter → parse →
chunk → embed → store.

**Contains**:
- `struct Indexer` with all dependencies (chunker, embedder, stores, hasher)
- `pub fn index(&mut self) -> Result<IndexStats>`
- Private helpers:
  - `fn walk_repo(&self) -> Vec<PathBuf>`
  - `fn filter_changed(&self, files: Vec<PathBuf>) -> Vec<(PathBuf, String)>`
  - `fn process_files(&self, files: Vec<(PathBuf, String)>) -> Vec<Chunk>`
  - `fn embed_new_chunks(&self, chunks: &mut Vec<Chunk>) -> Result<()>`
  - `fn store_chunks(&self, chunks: &[Chunk]) -> Result<()>`

### `src/indexer/hasher.rs` — SHA-256 Hashing

**Purpose**: Hash files and chunks for incremental indexing. Persist hash cache.

**Contains**:
- `struct FileHasher` with `HashMap<PathBuf, String>` cache
- `pub fn hash_content(content: &str) -> String`
- `pub fn load(path: &Path) -> Result<Self>`
- `pub fn save(&self, path: &Path) -> Result<()>`
- `pub fn is_changed(&self, path: &Path, current_hash: &str) -> bool`
- `pub fn update(&mut self, path: PathBuf, hash: String)`

### `src/mcp/mod.rs` — MCP Server

**Purpose**: Main loop for the MCP server. Read stdin, dispatch to handler,
write to stdout. Zero business logic here.

**Contains**:
- `struct McpServer` with `SearchEngine` and `initialized: bool`
- `pub fn run(&mut self) -> Result<()>` — the main loop

### `src/mcp/protocol.rs` — JSON-RPC Types

**Purpose**: Serde-compatible structs for the JSON-RPC 2.0 protocol used by MCP.

**Contains**:
- `struct JsonRpcMessage` (incoming request/notification)
- `struct JsonRpcResponse` (outgoing response)
- `struct JsonRpcError` (protocol error)
- `struct ToolResult` (tool execution result)
- `struct ContentBlock` (text content in tool results)
- `struct ToolDefinition` (tool schema for tools/list)
- `struct InputSchema` (JSON Schema for tool parameters)
- Error code constants

### `src/mcp/handler.rs` — Request Dispatcher

**Purpose**: Route incoming JSON-RPC methods to the right handler function.

**Contains**:
- `pub fn handle_request(server: &McpServer, msg: &JsonRpcMessage) -> JsonRpcResponse`
  - Routes "initialize" → `handle_initialize()`
  - Routes "tools/list" → `handle_tools_list()`
  - Routes "tools/call" → `handle_tools_call()`
  - Routes unknown → METHOD_NOT_FOUND error
- `pub fn handle_notification(server: &mut McpServer, msg: &JsonRpcMessage)`
  - Routes "notifications/initialized" → set `server.initialized = true`
  - Routes unknown → ignore silently (per JSON-RPC spec)

### `src/mcp/tools.rs` — Tool Definitions & Handlers

**Purpose**: Define the 4 MCP tools and implement each one's logic.

**Contains**:
- `pub fn tool_definitions() -> Vec<ToolDefinition>` — returns all 4 tool schemas
- `pub fn handle_tool_call(engine: &SearchEngine, name: &str, args: Value) -> ToolResult`
  - Dispatches to:
    - `fn handle_semantic_search(engine: &SearchEngine, args: Value) -> ToolResult`
    - `fn handle_get_file_span(engine: &SearchEngine, args: Value) -> ToolResult`
    - `fn handle_find_symbol(engine: &SearchEngine, args: Value) -> ToolResult`
    - `fn handle_list_indexed_files(engine: &SearchEngine, args: Value) -> ToolResult`
- `fn format_search_response(results: Vec<SearchResult>, budget: usize) -> String`
- `fn estimate_tokens(text: &str) -> usize`

## Runtime Data Directory (`.lumina/`)

Created automatically on first `lumina index`. Structure:

```
.lumina/
├── config.toml          # Optional user config overrides
│                        # Created by `lumina init` (future feature)
│
├── index.lance/         # LanceDB storage
│   ├── _versions/       # Lance version metadata
│   ├── _indices/        # ANN index data (if created)
│   └── data/            # Arrow IPC files with chunk data + embeddings
│
├── tantivy_index/       # Tantivy storage
│   ├── meta.json        # Index metadata
│   └── *.managed.json   # Segment files
│
└── hashes.bin           # Bincode-serialized HashMap<PathBuf, String>
                         # Maps file paths to their SHA-256 hashes
                         # Used for incremental indexing
```

This directory should be added to `.gitignore`. It contains derived data
that can be regenerated from the source code. Committing it would bloat
the repo (embeddings are ~4KB per chunk).

## `.gitignore`

```
/target
/.lumina
*.lance
```

## `mcp.json.example`

```json
{
  "mcpServers": {
    "lumina": {
      "command": "lumina",
      "args": ["mcp", "--repo", "."],
      "env": {
        "VOYAGE_API_KEY": "pa-YOUR_KEY_HERE"
      }
    }
  }
}
```
