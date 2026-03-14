# 04 - Module Design (Types, Traits, Functions)

This document specifies every public type, trait, and function signature in Lumina.
It's the contract that the implementation must satisfy.

---

## `src/error.rs`

```rust
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum LuminaError {
    // ── Parsing ────────────────────────────────────────
    #[error("Failed to parse {file}: {reason}")]
    ParseError { file: String, reason: String },

    #[error("Unsupported file extension: .{extension}")]
    UnsupportedLanguage { extension: String },

    #[error("Tree-sitter query error for {language}: {reason}")]
    QueryError { language: String, reason: String },

    // ── Embedding ──────────────────────────────────────
    #[error("Embedding API error: {0}")]
    EmbeddingError(String),

    #[error("Embedding API rate limited. Retry after {retry_after_secs}s")]
    EmbeddingRateLimited { retry_after_secs: u64 },

    #[error("No embedding API key configured. Set VOYAGE_API_KEY env var.")]
    MissingApiKey,

    // ── Reranker ───────────────────────────────────────
    #[error("Reranker API error: {0}")]
    RerankerError(String),

    // ── Storage ────────────────────────────────────────
    #[error("Vector store error: {0}")]
    VectorStoreError(String),

    #[error("Keyword store error: {0}")]
    KeywordStoreError(String),

    // ── Index ──────────────────────────────────────────
    #[error("Index not found at {path}. Run `lumina index` first.")]
    IndexNotFound { path: PathBuf },

    #[error("Index is empty. Run `lumina index` to build it.")]
    IndexEmpty,

    // ── Config ─────────────────────────────────────────
    #[error("Configuration error: {0}")]
    ConfigError(String),

    // ── File Operations ────────────────────────────────
    #[error("File not found: {0}")]
    FileNotFound(PathBuf),

    #[error("File too large ({size_bytes} bytes, max {max_bytes}): {path}")]
    FileTooLarge {
        path: PathBuf,
        size_bytes: u64,
        max_bytes: u64,
    },

    // ── MCP Protocol ───────────────────────────────────
    #[error("MCP protocol error ({code}): {message}")]
    McpProtocol { code: i32, message: String },

    #[error("Unknown MCP tool: {0}")]
    UnknownTool(String),

    // ── Standard library wrappers ──────────────────────
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("HTTP request error: {0}")]
    Http(#[from] reqwest::Error),
}

/// Convenience alias used throughout the codebase
pub type Result<T> = std::result::Result<T, LuminaError>;
```

**Design decisions:**
- Single error enum, not one per module. This codebase is ~3K lines total.
  Per-module error types add boilerplate without benefit at this scale.
- Each variant has enough context to debug (file path, size, code).
- `#[from]` on std errors for ergonomic `?` operator usage.

---

## `src/types.rs`

```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The atomic unit of indexed code.
/// One Chunk = one semantic code block (function, class, method, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    /// SHA-256 hash of `self.text`. Serves as unique ID.
    /// Deterministic: same code content → same ID, regardless of file path.
    pub id: String,

    /// File path relative to repository root.
    /// Example: "src/auth/middleware.rs"
    pub file: String,

    /// Name of the symbol this chunk represents.
    /// For functions: "authenticate_user"
    /// For methods: "UserService.create"
    /// For top-level code: "" (empty string)
    pub symbol: String,

    /// What kind of symbol this chunk represents.
    pub kind: SymbolKind,

    /// First line of this chunk in the source file (1-indexed).
    pub start_line: u32,

    /// Last line of this chunk in the source file (1-indexed, inclusive).
    pub end_line: u32,

    /// Programming language identifier.
    /// Values: "python", "rust", "typescript", "javascript", "go", "java"
    pub language: String,

    /// Raw source code text of this chunk.
    /// Includes the complete function/class/method body.
    pub text: String,

    /// Embedding vector from Voyage code-3 (1024 dimensions).
    /// None before embedding, Some after.
    /// Stored as Vec<f32> for flexibility, not [f32; 1024] because
    /// different embedding models have different dimensions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
}

impl Chunk {
    /// Estimate token count for this chunk's text.
    /// Heuristic: ~4 characters per token for code.
    /// Accurate within ~15% of tiktoken for code content.
    pub fn estimated_tokens(&self) -> usize {
        (self.text.len() + 3) / 4
    }

    /// Number of lines in this chunk.
    pub fn line_count(&self) -> u32 {
        self.end_line - self.start_line + 1
    }
}

/// Classification of a code symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Function,
    Method,
    Class,
    Struct,
    Enum,
    Trait,
    Interface,   // TypeScript/Java/Go interfaces
    Impl,        // Rust impl blocks
    Module,      // Python module-level, Go package-level
    Constant,    // Constants and static values
    TypeAlias,
    TopLevel,    // Code that doesn't belong to any symbol
}

impl SymbolKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Method => "method",
            Self::Class => "class",
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::Trait => "trait",
            Self::Interface => "interface",
            Self::Impl => "impl",
            Self::Module => "module",
            Self::Constant => "constant",
            Self::TypeAlias => "type_alias",
            Self::TopLevel => "top_level",
        }
    }
}

impl std::fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A search result: a Chunk with a relevance score and source info.
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// The chunk ID (SHA-256 hash)
    pub chunk_id: String,

    /// File path relative to repo root
    pub file: String,

    /// Symbol name
    pub symbol: String,

    /// Symbol classification
    pub kind: SymbolKind,

    /// Line range in the source file
    pub start_line: u32,
    pub end_line: u32,

    /// Programming language
    pub language: String,

    /// Source code text
    pub text: String,

    /// Relevance score, normalized to [0.0, 1.0].
    /// Higher = more relevant.
    pub score: f32,

    /// Where this result came from in the pipeline.
    pub source: SearchSource,
}

/// Tracks where a SearchResult originated in the retrieval pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchSource {
    /// From LanceDB vector search
    Vector,
    /// From tantivy BM25 keyword search
    Keyword,
    /// After RRF fusion of vector + keyword
    Fused,
    /// After cross-encoder reranking
    Reranked,
}

/// Symbol information for the find_symbol tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolInfo {
    /// Symbol name (e.g., "UserService", "authenticate")
    pub name: String,

    /// Symbol kind
    pub kind: SymbolKind,

    /// File path relative to repo root
    pub file: String,

    /// Line range
    pub start_line: u32,
    pub end_line: u32,

    /// Just the signature/declaration, not the full body.
    /// For a function: "pub fn authenticate(token: &str) -> Result<Claims>"
    /// For a class: "class UserService:"
    /// For a struct: "pub struct Config { ... }"
    pub signature: String,
}

/// Metadata about an indexed file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    /// File path relative to repo root
    pub path: String,

    /// Programming language
    pub language: String,

    /// Number of lines in the file
    pub line_count: u32,

    /// Number of chunks this file was split into
    pub chunk_count: u32,

    /// Total number of symbols extracted
    pub symbol_count: u32,

    /// SHA-256 hash of file content (for change detection)
    pub content_hash: String,
}

/// Statistics from an indexing run.
#[derive(Debug, Clone, Default)]
pub struct IndexStats {
    /// Total files found in the repository
    pub files_scanned: usize,

    /// Files whose content changed since last index
    pub files_changed: usize,

    /// Files skipped (unchanged, binary, too large, unsupported)
    pub files_skipped: usize,

    /// Files skipped specifically because they haven't changed
    pub files_unchanged: usize,

    /// Total chunks in the index after this run
    pub chunks_total: usize,

    /// Chunks that needed new embeddings this run
    pub chunks_embedded: usize,

    /// Chunks whose embeddings were reused (content hash matched)
    pub chunks_cached: usize,

    /// Number of API calls made to the embedding service
    pub embedding_api_calls: usize,

    /// Total time for the index operation
    pub duration: std::time::Duration,
}

impl std::fmt::Display for IndexStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Indexed {} files ({} changed, {} skipped) → {} chunks ({} embedded, {} cached) in {:.1}s",
            self.files_scanned,
            self.files_changed,
            self.files_skipped,
            self.chunks_total,
            self.chunks_embedded,
            self.chunks_cached,
            self.duration.as_secs_f64()
        )
    }
}
```

---

## `src/config.rs`

```rust
use crate::error::{LuminaError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LuminaConfig {
    /// Root directory of the repository to index
    #[serde(skip)]
    pub repo_root: PathBuf,

    /// Directory for index data (default: {repo_root}/.lumina)
    #[serde(skip)]
    pub data_dir: PathBuf,

    // ── Embedding config ──
    /// Voyage AI API key (from VOYAGE_API_KEY env var)
    #[serde(skip)]
    pub voyage_api_key: Option<String>,

    /// Voyage model name
    #[serde(default = "default_voyage_model")]
    pub voyage_model: String,

    /// Max texts per embedding API call
    #[serde(default = "default_embedding_batch_size")]
    pub embedding_batch_size: usize,

    // ── Reranker config ──
    /// Reranker API key (from RERANKER_API_KEY env var)
    #[serde(skip)]
    pub reranker_api_key: Option<String>,

    /// Reranker model name
    #[serde(default = "default_reranker_model")]
    pub reranker_model: String,

    // ── Chunking config ──
    /// Maximum tokens per chunk (chunks larger than this are split)
    #[serde(default = "default_max_chunk_tokens")]
    pub max_chunk_tokens: usize,

    /// Minimum tokens per chunk (chunks smaller than this are merged)
    #[serde(default = "default_min_chunk_tokens")]
    pub min_chunk_tokens: usize,

    /// Maximum file size to index (in bytes)
    #[serde(default = "default_max_file_size")]
    pub max_file_size: u64,

    // ── Search config ──
    /// Number of candidates to retrieve from vector search before RRF
    #[serde(default = "default_search_k")]
    pub search_k_vector: usize,

    /// Number of candidates to retrieve from keyword search before RRF
    #[serde(default = "default_search_k")]
    pub search_k_keyword: usize,

    /// RRF constant (standard value is 60)
    #[serde(default = "default_rrf_k")]
    pub rrf_k: u32,

    /// Maximum tokens in MCP tool responses
    #[serde(default = "default_token_budget")]
    pub response_token_budget: usize,
}

// Default value functions for serde
fn default_voyage_model() -> String { "voyage-code-3".to_string() }
fn default_reranker_model() -> String { "jina-reranker-v2-base-multilingual".to_string() }
fn default_embedding_batch_size() -> usize { 128 }
fn default_max_chunk_tokens() -> usize { 500 }
fn default_min_chunk_tokens() -> usize { 50 }
fn default_max_file_size() -> u64 { 1_048_576 } // 1 MB
fn default_search_k() -> usize { 30 }
fn default_rrf_k() -> u32 { 60 }
fn default_token_budget() -> usize { 2000 }

impl LuminaConfig {
    /// Load configuration with precedence: env vars > config file > defaults.
    ///
    /// 1. Start with defaults
    /// 2. If .lumina/config.toml exists, override with file values
    /// 3. Override API keys from environment variables
    pub fn load(repo_root: PathBuf) -> Result<Self> {
        let data_dir = repo_root.join(".lumina");
        let config_path = data_dir.join("config.toml");

        let mut config = if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            toml::from_str(&content)
                .map_err(|e| LuminaError::ConfigError(e.to_string()))?
        } else {
            Self::default()
        };

        config.repo_root = repo_root;
        config.data_dir = data_dir;
        config.voyage_api_key = std::env::var("VOYAGE_API_KEY").ok();
        config.reranker_api_key = std::env::var("RERANKER_API_KEY").ok();

        Ok(config)
    }

    /// Returns path to the LanceDB data directory
    pub fn lance_path(&self) -> PathBuf {
        self.data_dir.join("index.lance")
    }

    /// Returns path to the tantivy index directory
    pub fn tantivy_path(&self) -> PathBuf {
        self.data_dir.join("tantivy_index")
    }

    /// Returns path to the hash cache file
    pub fn hashes_path(&self) -> PathBuf {
        self.data_dir.join("hashes.bin")
    }
}

impl Default for LuminaConfig {
    fn default() -> Self {
        Self {
            repo_root: PathBuf::new(),
            data_dir: PathBuf::new(),
            voyage_api_key: None,
            voyage_model: default_voyage_model(),
            embedding_batch_size: default_embedding_batch_size(),
            reranker_api_key: None,
            reranker_model: default_reranker_model(),
            max_chunk_tokens: default_max_chunk_tokens(),
            min_chunk_tokens: default_min_chunk_tokens(),
            max_file_size: default_max_file_size(),
            search_k_vector: default_search_k(),
            search_k_keyword: default_search_k(),
            rrf_k: default_rrf_k(),
            response_token_budget: default_token_budget(),
        }
    }
}
```

---

## `src/chunker/mod.rs` — Chunker Trait

```rust
use crate::error::Result;
use crate::types::Chunk;
use std::path::Path;

/// Trait for code chunking strategies.
///
/// A Chunker takes a source file and splits it into semantic chunks.
/// Each chunk represents a logical unit of code (function, class, method)
/// that can be independently embedded and retrieved.
pub trait Chunker: Send + Sync {
    /// Parse a source file and extract semantic chunks.
    ///
    /// # Arguments
    /// - `path`: File path relative to repo root (used for metadata, not reading)
    /// - `content`: The actual file content to parse
    ///
    /// # Returns
    /// A vector of Chunks. Each chunk has:
    /// - `id`: SHA-256 of the chunk text (computed by this method)
    /// - `file`: the path argument, as string
    /// - `symbol`: extracted symbol name
    /// - `kind`: symbol classification
    /// - `start_line`, `end_line`: line range in the original file
    /// - `language`: detected from file extension
    /// - `text`: the source code for this chunk
    /// - `embedding`: None (not set by the chunker)
    fn chunk_file(&self, path: &Path, content: &str) -> Result<Vec<Chunk>>;

    /// List of file extensions this chunker can handle.
    /// Example: ["py", "rs", "ts", "tsx", "js", "jsx", "go", "java"]
    fn supported_extensions(&self) -> &[&str];

    /// Check if this chunker supports a given file extension.
    fn supports(&self, extension: &str) -> bool {
        self.supported_extensions().contains(&extension)
    }
}

pub mod languages;
pub mod treesitter;

pub use treesitter::TreeSitterChunker;
```

---

## `src/chunker/languages.rs` — Language Configurations

```rust
use tree_sitter::Language;

/// Configuration for a single programming language.
pub struct LanguageConfig {
    /// Language identifier (e.g., "python", "rust")
    pub name: &'static str,

    /// Tree-sitter Language object (from grammar crate)
    pub language: Language,

    /// File extensions that map to this language
    pub extensions: &'static [&'static str],

    /// Tree-sitter query to match extractable code blocks.
    /// Each @chunk capture becomes a Chunk.
    /// Each @name capture extracts the symbol name.
    ///
    /// These queries use tree-sitter's S-expression query syntax.
    /// Reference: https://tree-sitter.github.io/tree-sitter/using-parsers#pattern-matching-with-queries
    pub chunk_query: &'static str,
}

/// Get the language config for a file extension.
/// Returns None if the extension is not supported.
pub fn get_config(extension: &str) -> Option<&'static LanguageConfig>;

/// Get all supported language configs.
pub fn all_configs() -> &'static [LanguageConfig];

/// Get all supported file extensions.
pub fn all_extensions() -> Vec<&'static str>;
```

### Tree-sitter Queries Per Language

These are the exact S-expression queries for extracting semantic code blocks.

**Python:**
```scheme
;; Functions (including async)
(function_definition
  name: (identifier) @name) @chunk

;; Classes
(class_definition
  name: (identifier) @name) @chunk

;; Decorated definitions (catches @decorator + def/class)
(decorated_definition) @chunk
```

**Rust:**
```scheme
;; Functions
(function_item
  name: (identifier) @name) @chunk

;; Methods and associated functions (inside impl)
(impl_item) @chunk

;; Structs
(struct_item
  name: (type_identifier) @name) @chunk

;; Enums
(enum_item
  name: (type_identifier) @name) @chunk

;; Traits
(trait_item
  name: (type_identifier) @name) @chunk

;; Type aliases
(type_item
  name: (type_identifier) @name) @chunk
```

**TypeScript / JavaScript:**
```scheme
;; Function declarations
(function_declaration
  name: (identifier) @name) @chunk

;; Arrow functions assigned to variables
(lexical_declaration
  (variable_declarator
    name: (identifier) @name
    value: (arrow_function))) @chunk

;; Class declarations
(class_declaration
  name: (identifier) @name) @chunk

;; Export default function/class
(export_statement
  declaration: (function_declaration
    name: (identifier) @name)) @chunk

(export_statement
  declaration: (class_declaration
    name: (identifier) @name)) @chunk

;; Interface declarations (TypeScript)
(interface_declaration
  name: (type_identifier) @name) @chunk

;; Type alias declarations (TypeScript)
(type_alias_declaration
  name: (type_identifier) @name) @chunk
```

**Go:**
```scheme
;; Function declarations
(function_declaration
  name: (identifier) @name) @chunk

;; Method declarations
(method_declaration
  name: (field_identifier) @name) @chunk

;; Type declarations (struct, interface)
(type_declaration
  (type_spec
    name: (type_identifier) @name)) @chunk
```

**Java:**
```scheme
;; Class declarations
(class_declaration
  name: (identifier) @name) @chunk

;; Method declarations
(method_declaration
  name: (identifier) @name) @chunk

;; Interface declarations
(interface_declaration
  name: (identifier) @name) @chunk

;; Enum declarations
(enum_declaration
  name: (identifier) @name) @chunk

;; Constructor declarations
(constructor_declaration
  name: (identifier) @name) @chunk
```

**Important notes on queries:**
- The `@chunk` capture marks the full node to extract as a chunk.
- The `@name` capture extracts the symbol name from within the chunk.
- If `@name` is missing (e.g., anonymous functions), the symbol is "".
- Rust `impl_item` captures the whole impl block. The chunker then splits
  it into individual methods if it exceeds `max_chunk_tokens`.

---

## `src/chunker/treesitter.rs` — Tree-Sitter Chunker Implementation

```rust
use crate::chunker::languages::{self, LanguageConfig};
use crate::chunker::Chunker;
use crate::error::{LuminaError, Result};
use crate::types::{Chunk, SymbolKind};
use sha2::{Digest, Sha256};
use std::path::Path;
use tree_sitter::{Parser, Query, QueryCursor};

pub struct TreeSitterChunker {
    /// Maximum tokens per chunk. Chunks larger than this are split.
    max_chunk_tokens: usize,

    /// Minimum tokens per chunk. Chunks smaller than this are merged.
    min_chunk_tokens: usize,
}

impl TreeSitterChunker {
    pub fn new(max_tokens: usize, min_tokens: usize) -> Self;

    /// Parse source code with tree-sitter and extract chunks using queries.
    ///
    /// Algorithm:
    /// 1. Determine language from file extension
    /// 2. Create parser, set language
    /// 3. Parse content → Tree
    /// 4. Run chunk_query on Tree → list of captured nodes
    /// 5. For each captured node:
    ///    a. Extract text from content using byte range
    ///    b. Extract symbol name from @name capture
    ///    c. Determine SymbolKind from node type
    ///    d. Compute line range from byte offset
    ///    e. Create Chunk with SHA-256 id
    /// 6. Handle gaps: code between captured nodes becomes TopLevel chunks
    /// 7. Merge small chunks with adjacent siblings
    /// 8. Split oversized chunks into sub-chunks
    fn extract_chunks(
        &self,
        content: &str,
        path: &Path,
        config: &LanguageConfig,
    ) -> Result<Vec<Chunk>>;

    /// Merge adjacent small chunks into larger ones.
    /// Chunks smaller than min_chunk_tokens are merged with the next sibling.
    fn merge_small_chunks(&self, chunks: Vec<Chunk>) -> Vec<Chunk>;

    /// Split a chunk that exceeds max_chunk_tokens.
    /// Strategy: find child nodes in the AST and split there.
    /// Fallback: split at line boundaries at ~max_tokens intervals.
    fn split_large_chunk(&self, chunk: Chunk) -> Vec<Chunk>;

    /// Determine SymbolKind from a tree-sitter node type string.
    fn node_type_to_kind(node_type: &str) -> SymbolKind;

    /// Extract the first line of a function/class as its "signature".
    /// For Python: "def authenticate(token: str) -> bool:"
    /// For Rust: "pub fn authenticate(token: &str) -> Result<Claims>"
    fn extract_signature(text: &str, kind: SymbolKind) -> String;
}

impl Chunker for TreeSitterChunker {
    fn chunk_file(&self, path: &Path, content: &str) -> Result<Vec<Chunk>> {
        let extension = path.extension()
            .and_then(|e| e.to_str())
            .ok_or_else(|| LuminaError::UnsupportedLanguage {
                extension: "none".to_string()
            })?;

        let config = languages::get_config(extension)
            .ok_or_else(|| LuminaError::UnsupportedLanguage {
                extension: extension.to_string()
            })?;

        self.extract_chunks(content, path, config)
    }

    fn supported_extensions(&self) -> &[&str] {
        &["py", "rs", "ts", "tsx", "js", "jsx", "go", "java"]
    }
}

/// Compute SHA-256 hash of text content, return as hex string.
pub fn content_hash(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    hex::encode(hasher.finalize())
}

/// Estimate token count for code text.
/// Heuristic: ~4 characters per token.
/// More accurate for code than for natural language because code has
/// more short tokens (operators, brackets, keywords).
pub fn estimate_tokens(text: &str) -> usize {
    (text.len() + 3) / 4
}
```

---

## `src/embeddings/mod.rs` — Embedder Trait

```rust
use crate::error::Result;

/// Trait for text embedding models.
///
/// Implementations may call external APIs (Voyage, OpenAI) or run
/// local models (via ONNX Runtime, candle).
pub trait Embedder: Send + Sync {
    /// Embed multiple document texts in a single batch.
    ///
    /// The `texts` slice may be larger than the API's batch limit.
    /// Implementations must handle batching internally.
    ///
    /// Returns one embedding vector per input text, in the same order.
    fn embed_texts(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;

    /// Embed a single query text.
    ///
    /// Some embedding models use different encoding for queries vs documents
    /// (e.g., Voyage uses input_type "query" vs "document").
    /// This method handles that distinction.
    fn embed_query(&self, query: &str) -> Result<Vec<f32>>;

    /// Number of dimensions in the embedding vectors.
    /// Voyage code-3: 1024
    fn dimensions(&self) -> usize;
}

pub mod voyage;
pub use voyage::VoyageEmbedder;
```

---

## `src/embeddings/voyage.rs` — Voyage API Client

```rust
use crate::embeddings::Embedder;
use crate::error::{LuminaError, Result};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

const VOYAGE_API_URL: &str = "https://api.voyageai.com/v1/embeddings";
const VOYAGE_DIMENSIONS: usize = 1024;

pub struct VoyageEmbedder {
    client: Client,
    api_key: String,
    model: String,
    batch_size: usize,
}

impl VoyageEmbedder {
    pub fn new(api_key: String, model: String, batch_size: usize) -> Self {
        Self {
            client: Client::new(),
            api_key,
            model,
            batch_size,
        }
    }
}

impl Embedder for VoyageEmbedder {
    fn embed_texts(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let mut all_embeddings = Vec::with_capacity(texts.len());

        for batch in texts.chunks(self.batch_size) {
            debug!("Embedding batch of {} texts", batch.len());
            let embeddings = self.call_api(batch, "document")?;
            all_embeddings.extend(embeddings);
        }

        Ok(all_embeddings)
    }

    fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        let results = self.call_api(&[query], "query")?;
        results.into_iter().next()
            .ok_or_else(|| LuminaError::EmbeddingError(
                "Empty response from Voyage API".to_string()
            ))
    }

    fn dimensions(&self) -> usize {
        VOYAGE_DIMENSIONS
    }
}

impl VoyageEmbedder {
    fn call_api(&self, texts: &[&str], input_type: &str) -> Result<Vec<Vec<f32>>> {
        let request_body = VoyageRequest {
            model: &self.model,
            input: texts,
            input_type,
        };

        let response = self.client
            .post(VOYAGE_API_URL)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request_body)
            .send()?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();

            if status.as_u16() == 429 {
                return Err(LuminaError::EmbeddingRateLimited {
                    retry_after_secs: 60,
                });
            }

            return Err(LuminaError::EmbeddingError(
                format!("Voyage API returned {}: {}", status, body)
            ));
        }

        let voyage_response: VoyageResponse = response.json()?;

        // Sort by index to ensure order matches input
        let mut data = voyage_response.data;
        data.sort_by_key(|d| d.index);

        Ok(data.into_iter().map(|d| d.embedding).collect())
    }
}

// ── API request/response types ──

#[derive(Serialize)]
struct VoyageRequest<'a> {
    model: &'a str,
    input: &'a [&'a str],
    input_type: &'a str,
}

#[derive(Deserialize)]
struct VoyageResponse {
    data: Vec<VoyageEmbeddingData>,
}

#[derive(Deserialize)]
struct VoyageEmbeddingData {
    embedding: Vec<f32>,
    index: usize,
}
```

---

## `src/store/mod.rs` — Store Traits

```rust
use crate::error::Result;
use crate::types::{Chunk, FileMetadata, SearchResult, SymbolInfo};

/// Trait for vector-based storage and search.
///
/// Implementations store chunks with their embedding vectors and
/// provide approximate nearest neighbor (ANN) search.
pub trait VectorStore: Send + Sync {
    /// Insert or update chunks in the store.
    /// Chunks must have embeddings (embedding field must be Some).
    /// If a chunk with the same ID exists, it's replaced.
    fn upsert_chunks(&self, chunks: &[Chunk]) -> Result<()>;

    /// Search for chunks similar to the given embedding vector.
    /// Returns top-k results ordered by similarity (descending).
    fn vector_search(&self, embedding: &[f32], k: usize) -> Result<Vec<SearchResult>>;

    /// Delete all chunks belonging to a specific file.
    /// Used during re-indexing when a file changes.
    fn delete_by_file(&self, file: &str) -> Result<()>;

    /// Check if a chunk with the given ID exists in the store.
    fn chunk_exists(&self, id: &str) -> Result<bool>;

    /// List metadata for all indexed files.
    fn list_files(&self) -> Result<Vec<FileMetadata>>;

    /// Get total number of chunks in the store.
    fn chunk_count(&self) -> Result<usize>;
}

/// Trait for keyword-based (BM25) storage and search.
///
/// Implementations index chunk text for full-text search and
/// symbol names for exact symbol lookup.
pub trait KeywordStore: Send + Sync {
    /// Insert or update chunks in the keyword index.
    fn upsert_chunks(&self, chunks: &[Chunk]) -> Result<()>;

    /// Search chunk text using BM25 scoring.
    /// Returns top-k results ordered by BM25 score (descending).
    fn keyword_search(&self, query: &str, k: usize) -> Result<Vec<SearchResult>>;

    /// Search for symbols by name (exact or prefix match).
    fn symbol_search(&self, name: &str) -> Result<Vec<SymbolInfo>>;

    /// Delete all chunks belonging to a specific file.
    fn delete_by_file(&self, file: &str) -> Result<()>;
}

pub mod lance;
pub mod tantivy_store;

pub use self::lance::LanceStore;
pub use tantivy_store::TantivyStore;
```

---

## `src/search/mod.rs` — Search Engine

```rust
use crate::config::LuminaConfig;
use crate::embeddings::Embedder;
use crate::error::Result;
use crate::search::reranker::Reranker;
use crate::search::rrf;
use crate::store::{KeywordStore, VectorStore};
use crate::types::{FileMetadata, SearchResult, SymbolInfo};
use std::path::Path;

pub mod reranker;
pub mod rrf;

/// The main search orchestrator.
/// Combines vector search, keyword search, RRF fusion,
/// optional reranking, and token budget enforcement.
pub struct SearchEngine {
    vector_store: Box<dyn VectorStore>,
    keyword_store: Box<dyn KeywordStore>,
    embedder: Box<dyn Embedder>,
    reranker: Box<dyn Reranker>,
    config: LuminaConfig,
}

impl SearchEngine {
    pub fn new(
        vector_store: Box<dyn VectorStore>,
        keyword_store: Box<dyn KeywordStore>,
        embedder: Box<dyn Embedder>,
        reranker: Box<dyn Reranker>,
        config: LuminaConfig,
    ) -> Self;

    /// Full semantic search pipeline.
    ///
    /// 1. Embed the query using Voyage API
    /// 2. Vector search on LanceDB (top search_k_vector candidates)
    /// 3. Keyword search on tantivy (top search_k_keyword candidates)
    /// 4. RRF fusion of both result sets
    /// 5. Optional reranking with cross-encoder
    /// 6. Return top-k results
    pub fn semantic_search(&self, query: &str, k: usize) -> Result<Vec<SearchResult>>;

    /// Find a symbol by name.
    /// Delegates to keyword store's symbol_search.
    pub fn find_symbol(&self, name: &str) -> Result<Vec<SymbolInfo>>;

    /// Read a span of lines from a file.
    /// Reads the actual file from disk (not from the index).
    pub fn get_file_span(
        &self,
        file: &str,
        start_line: u32,
        end_line: u32,
    ) -> Result<String>;

    /// List all indexed files with metadata.
    pub fn list_files(&self) -> Result<Vec<FileMetadata>>;

    /// Format search results as a markdown string within a token budget.
    ///
    /// Format:
    /// ```
    /// ## Search Results for: "query"
    ///
    /// ### 1. src/auth/middleware.rs:15-42 (`authenticate`)
    /// ```rust
    /// pub fn authenticate(...) -> ... {
    ///     ...
    /// }
    /// ```
    ///
    /// ### 2. src/models/user.rs:8-25 (`User`)
    /// ...
    ///
    /// ---
    /// *5 results from 3 files | ~847 tokens*
    /// ```
    pub fn format_results(
        &self,
        query: &str,
        results: &[SearchResult],
        budget: usize,
    ) -> String;
}
```

---

## `src/search/rrf.rs` — Reciprocal Rank Fusion

```rust
use crate::types::{SearchResult, SearchSource};
use std::collections::HashMap;

/// Merge two ranked result lists using Reciprocal Rank Fusion.
///
/// RRF formula: score(d) = Σ 1/(k + rank_i(d))
///
/// Where:
/// - k is a constant (typically 60) that prevents top-ranked items
///   from dominating the score
/// - rank_i(d) is the rank of document d in the i-th result list
///   (1-indexed, so the top result has rank 1)
///
/// Properties:
/// - Documents appearing in both lists get higher scores
/// - Does not require score calibration between lists
/// - The k constant controls how much top ranks are weighted
///
/// # Arguments
/// - `vector_results`: Results from vector search, ordered by similarity
/// - `keyword_results`: Results from keyword search, ordered by BM25 score
/// - `k`: RRF constant (standard: 60)
///
/// # Returns
/// Merged, deduplicated results ordered by RRF score (descending).
/// All results have their source set to SearchSource::Fused.
pub fn rrf_merge(
    vector_results: Vec<SearchResult>,
    keyword_results: Vec<SearchResult>,
    k: u32,
) -> Vec<SearchResult> {
    let mut scores: HashMap<String, f32> = HashMap::new();
    let mut results_by_id: HashMap<String, SearchResult> = HashMap::new();

    // Score vector results
    for (rank, result) in vector_results.into_iter().enumerate() {
        let rrf_score = 1.0 / (k as f32 + (rank + 1) as f32);
        *scores.entry(result.chunk_id.clone()).or_insert(0.0) += rrf_score;
        results_by_id.entry(result.chunk_id.clone()).or_insert(result);
    }

    // Score keyword results
    for (rank, result) in keyword_results.into_iter().enumerate() {
        let rrf_score = 1.0 / (k as f32 + (rank + 1) as f32);
        *scores.entry(result.chunk_id.clone()).or_insert(0.0) += rrf_score;
        results_by_id.entry(result.chunk_id.clone()).or_insert(result);
    }

    // Build final list, sorted by RRF score descending
    let mut merged: Vec<SearchResult> = scores.into_iter()
        .filter_map(|(id, score)| {
            results_by_id.remove(&id).map(|mut r| {
                r.score = score;
                r.source = SearchSource::Fused;
                r
            })
        })
        .collect();

    merged.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    merged
}
```

---

## `src/indexer/hasher.rs` — SHA-256 Hashing & Cache

```rust
use crate::error::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Manages SHA-256 hashes for incremental indexing.
///
/// Two-level caching:
/// 1. File-level: hash of entire file content. If unchanged, skip parsing.
/// 2. Chunk-level: hash of chunk text content (= chunk ID). If a chunk's
///    content hash exists in the store, skip re-embedding.
#[derive(Debug, Serialize, Deserialize)]
pub struct FileHasher {
    /// Maps file path (relative to repo root) → SHA-256 of file content
    file_hashes: HashMap<String, String>,
}

impl FileHasher {
    /// Create a new empty hasher.
    pub fn new() -> Self {
        Self {
            file_hashes: HashMap::new(),
        }
    }

    /// Load hash cache from disk.
    /// Returns empty hasher if file doesn't exist (first run).
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let data = std::fs::read(path)?;
        let hasher: Self = bincode::deserialize(&data)
            .map_err(|e| crate::error::LuminaError::ConfigError(
                format!("Failed to load hash cache: {}", e)
            ))?;
        Ok(hasher)
    }

    /// Save hash cache to disk.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data = bincode::serialize(self)
            .map_err(|e| crate::error::LuminaError::ConfigError(
                format!("Failed to serialize hash cache: {}", e)
            ))?;
        std::fs::write(path, data)?;
        Ok(())
    }

    /// Check if a file has changed since last index.
    pub fn is_changed(&self, file_path: &str, content: &str) -> bool {
        let current_hash = hash_content(content);
        match self.file_hashes.get(file_path) {
            Some(stored_hash) => stored_hash != &current_hash,
            None => true, // New file, not in cache
        }
    }

    /// Update the stored hash for a file.
    pub fn update(&mut self, file_path: String, content: &str) {
        let hash = hash_content(content);
        self.file_hashes.insert(file_path, hash);
    }

    /// Remove a file from the hash cache (file was deleted).
    pub fn remove(&mut self, file_path: &str) {
        self.file_hashes.remove(file_path);
    }

    /// Number of files in the cache.
    pub fn len(&self) -> usize {
        self.file_hashes.len()
    }
}

/// Compute SHA-256 hash of a string, returning hex-encoded result.
pub fn hash_content(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}
```

---

## `src/indexer/mod.rs` — Indexing Pipeline

```rust
use crate::chunker::Chunker;
use crate::config::LuminaConfig;
use crate::embeddings::Embedder;
use crate::error::{LuminaError, Result};
use crate::indexer::hasher::FileHasher;
use crate::store::{KeywordStore, VectorStore};
use crate::types::{Chunk, IndexStats};
use ignore::WalkBuilder;
use rayon::prelude::*;
use std::path::PathBuf;
use std::time::Instant;
use tracing::{debug, info, warn};

pub mod hasher;

pub struct Indexer {
    chunker: Box<dyn Chunker>,
    embedder: Box<dyn Embedder>,
    vector_store: Box<dyn VectorStore>,
    keyword_store: Box<dyn KeywordStore>,
    hasher: FileHasher,
    config: LuminaConfig,
}

impl Indexer {
    pub fn new(
        chunker: Box<dyn Chunker>,
        embedder: Box<dyn Embedder>,
        vector_store: Box<dyn VectorStore>,
        keyword_store: Box<dyn KeywordStore>,
        config: LuminaConfig,
    ) -> Result<Self> {
        let hasher = FileHasher::load(&config.hashes_path())?;
        Ok(Self {
            chunker, embedder, vector_store, keyword_store, hasher, config,
        })
    }

    /// Run the full indexing pipeline.
    ///
    /// Steps:
    /// 1. Walk repo, collect supported files
    /// 2. Filter unchanged files (SHA-256 check)
    /// 3. Parse changed files with tree-sitter (parallel via rayon)
    /// 4. Deduplicate chunks by content hash
    /// 5. Embed new chunks via Voyage API (sequential, batched)
    /// 6. Upsert to vector store + keyword store
    /// 7. Save updated hash cache
    pub fn index(&mut self) -> Result<IndexStats>;

    /// Walk the repository and collect all indexable file paths.
    fn walk_repo(&self) -> Vec<PathBuf>;

    /// Read file content and check if it changed since last index.
    /// Returns (path_string, content) for changed files only.
    fn filter_changed_files(
        &self,
        files: Vec<PathBuf>,
        stats: &mut IndexStats,
    ) -> Vec<(String, String)>;

    /// Parse and chunk files using rayon for parallelism.
    fn parse_files(
        &self,
        files: Vec<(String, String)>,
    ) -> Vec<Chunk>;

    /// Embed chunks that don't exist in the store yet.
    fn embed_new_chunks(
        &self,
        chunks: &mut [Chunk],
        stats: &mut IndexStats,
    ) -> Result<()>;

    /// Store chunks in both vector and keyword stores.
    fn store_chunks(&self, chunks: &[Chunk]) -> Result<()>;
}

// ── File filtering helpers ──

/// Check if a file should be indexed based on extension and content.
fn should_index(path: &Path, config: &LuminaConfig) -> bool;

/// Check if a file appears to be binary (contains null bytes).
fn is_binary(content: &[u8]) -> bool {
    content.iter().take(8192).any(|&b| b == 0)
}

/// Directories to always skip, even if not in .gitignore.
const SKIP_DIRS: &[&str] = &[
    "node_modules", "target", "__pycache__", ".git", ".svn",
    ".hg", "vendor", "dist", "build", ".next", ".nuxt",
    "venv", ".venv", "env", ".tox", "coverage",
];

/// File extensions to always skip (generated, binary, or declaration files).
const SKIP_EXTENSIONS: &[&str] = &[
    "min.js", "min.css", "map", "lock",
    "wasm", "pb.go", "pb.rs",
    "d.ts",  // TypeScript declarations (generated, no logic)
    "pyc", "pyo", "class", "o", "a", "so", "dylib", "dll",
    "jpg", "jpeg", "png", "gif", "svg", "ico", "webp",
    "mp3", "mp4", "wav", "avi", "mov",
    "zip", "tar", "gz", "bz2", "xz", "rar",
    "pdf", "doc", "docx", "xls", "xlsx",
    "exe", "bin", "dat",
];
```

---

## `src/main.rs` — CLI Entry Point

```rust
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "lumina")]
#[command(about = "Semantic code search MCP server for Claude Code")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Index the current repository (or re-index changed files)
    Index {
        /// Path to the repository root (default: current directory)
        #[arg(long, default_value = ".")]
        repo: PathBuf,

        /// Force full re-index (ignore hash cache)
        #[arg(long)]
        force: bool,
    },

    /// Search the index from the command line
    Query {
        /// The search query
        query: String,

        /// Number of results to return
        #[arg(short, long, default_value = "5")]
        k: usize,

        /// Path to the repository root
        #[arg(long, default_value = ".")]
        repo: PathBuf,
    },

    /// Start the MCP server (stdio transport)
    Mcp {
        /// Path to the repository root
        #[arg(long, default_value = ".")]
        repo: PathBuf,
    },

    /// Show index status and statistics
    Status {
        /// Path to the repository root
        #[arg(long, default_value = ".")]
        repo: PathBuf,
    },
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    // Initialize tracing to stderr (stdout reserved for MCP)
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("lumina=info".parse()?)
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Index { repo, force } => { /* create indexer, run index() */ }
        Commands::Query { query, k, repo } => { /* create search engine, run query */ }
        Commands::Mcp { repo } => { /* create MCP server, run loop */ }
        Commands::Status { repo } => { /* read index stats, print */ }
    }

    Ok(())
}
```
