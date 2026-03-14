use crate::error::{LuminaError, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
