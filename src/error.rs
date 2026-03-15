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

    #[error("No API key set. Set {env_var} environment variable.")]
    MissingApiKey { env_var: String },

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
