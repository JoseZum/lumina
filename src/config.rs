use crate::error::{LuminaError, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ── Embedding Provider ──

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingProvider {
    Local,
    Voyage,
    OpenAi,
}

impl EmbeddingProvider {
    pub fn default_model(&self) -> &'static str {
        match self {
            Self::Local => "jinaai/jina-embeddings-v2-base-code",
            Self::Voyage => "voyage-code-3",
            Self::OpenAi => "text-embedding-3-small",
        }
    }

    pub fn default_dimensions(&self) -> usize {
        match self {
            Self::Local => 768,
            Self::Voyage => 1024,
            Self::OpenAi => 1536,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Local => "Local (fastembed)",
            Self::Voyage => "Voyage AI",
            Self::OpenAi => "OpenAI",
        }
    }

    pub fn env_var(&self) -> Option<&'static str> {
        match self {
            Self::Local => None,
            Self::Voyage => Some("VOYAGE_API_KEY"),
            Self::OpenAi => Some("OPENAI_API_KEY"),
        }
    }
}

impl Default for EmbeddingProvider {
    fn default() -> Self {
        Self::Local
    }
}

impl std::fmt::Display for EmbeddingProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local => write!(f, "local"),
            Self::Voyage => write!(f, "voyage"),
            Self::OpenAi => write!(f, "openai"),
        }
    }
}

// ── Config ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LuminaConfig {
    /// Root directory of the repository to index
    #[serde(skip)]
    pub repo_root: PathBuf,

    /// Directory for index data (default: {repo_root}/.lumina)
    #[serde(skip)]
    pub data_dir: PathBuf,

    // ── Embedding config ──
    /// Which embedding provider to use
    #[serde(default)]
    pub embedding_provider: EmbeddingProvider,

    /// Embedding model name (provider-specific)
    #[serde(default = "default_embedding_model")]
    pub embedding_model: String,

    /// Embedding vector dimensions
    #[serde(default = "default_embedding_dimensions")]
    pub embedding_dimensions: usize,

    /// Max texts per embedding API call
    #[serde(default = "default_embedding_batch_size")]
    pub embedding_batch_size: usize,

    /// API key for the active provider (resolved from env, not serialized)
    #[serde(skip)]
    pub embedding_api_key: Option<String>,

    /// Voyage AI API key (from VOYAGE_API_KEY env var)
    #[serde(skip)]
    pub voyage_api_key: Option<String>,

    /// OpenAI API key (from OPENAI_API_KEY env var)
    #[serde(skip)]
    pub openai_api_key: Option<String>,

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
fn default_embedding_model() -> String { EmbeddingProvider::Local.default_model().to_string() }
fn default_embedding_dimensions() -> usize { EmbeddingProvider::Local.default_dimensions() }
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

        // Load API keys from environment
        config.voyage_api_key = std::env::var("VOYAGE_API_KEY").ok();
        config.openai_api_key = std::env::var("OPENAI_API_KEY").ok();
        config.reranker_api_key = std::env::var("RERANKER_API_KEY").ok();

        // Resolve the active API key based on provider
        config.embedding_api_key = match config.embedding_provider {
            EmbeddingProvider::Voyage => config.voyage_api_key.clone(),
            EmbeddingProvider::OpenAi => config.openai_api_key.clone(),
            EmbeddingProvider::Local => None,
        };

        Ok(config)
    }

    /// Save the current configuration to .lumina/config.toml
    pub fn save(&self) -> Result<()> {
        std::fs::create_dir_all(&self.data_dir)?;
        let config_path = self.data_dir.join("config.toml");
        let content = toml::to_string_pretty(self)
            .map_err(|e| LuminaError::ConfigError(e.to_string()))?;
        std::fs::write(&config_path, content)?;
        Ok(())
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

    /// Returns path to the provider lock file
    pub fn provider_lock_path(&self) -> PathBuf {
        self.data_dir.join("provider.lock")
    }
}

impl Default for LuminaConfig {
    fn default() -> Self {
        Self {
            repo_root: PathBuf::new(),
            data_dir: PathBuf::new(),
            embedding_provider: EmbeddingProvider::default(),
            embedding_model: default_embedding_model(),
            embedding_dimensions: default_embedding_dimensions(),
            embedding_batch_size: default_embedding_batch_size(),
            embedding_api_key: None,
            voyage_api_key: None,
            openai_api_key: None,
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
