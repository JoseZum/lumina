use serde::{Deserialize, Serialize};

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
