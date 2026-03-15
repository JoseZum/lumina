use crate::embeddings::Embedder;
use crate::error::{LuminaError, Result};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use std::sync::Mutex;
use tracing::{debug, info};

pub struct LocalEmbedder {
    model: Mutex<TextEmbedding>,
    dims: usize,
}

impl LocalEmbedder {
    pub fn new(model_name: &str) -> Result<Self> {
        let (embedding_model, dims) = match model_name {
            "jinaai/jina-embeddings-v2-base-code" => {
                (EmbeddingModel::JinaEmbeddingsV2BaseCode, 768)
            }
            "all-MiniLM-L6-v2" | "sentence-transformers/all-MiniLM-L6-v2" => {
                (EmbeddingModel::AllMiniLML6V2, 384)
            }
            _ => {
                return Err(LuminaError::EmbeddingError(format!(
                    "Unknown local model: '{}'. Supported: 'jinaai/jina-embeddings-v2-base-code', 'all-MiniLM-L6-v2'",
                    model_name
                )));
            }
        };

        info!("Loading local embedding model: {} ({} dims)", model_name, dims);

        let model = TextEmbedding::try_new(
            InitOptions::new(embedding_model).with_show_download_progress(true),
        )
        .map_err(|e| {
            LuminaError::EmbeddingError(format!(
                "Failed to load local model '{}': {}",
                model_name, e
            ))
        })?;

        Ok(Self {
            model: Mutex::new(model),
            dims,
        })
    }
}

impl Embedder for LocalEmbedder {
    fn embed_texts(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        debug!("Embedding {} texts locally", texts.len());

        let owned: Vec<String> = texts.iter().map(|t| t.to_string()).collect();
        let mut model = self.model.lock().map_err(|e| {
            LuminaError::EmbeddingError(format!("Lock poisoned: {}", e))
        })?;
        let embeddings = model.embed(owned, None).map_err(|e| {
            LuminaError::EmbeddingError(format!("Local embedding failed: {}", e))
        })?;

        Ok(embeddings)
    }

    fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        let results = self.embed_texts(&[query])?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| LuminaError::EmbeddingError("Empty local embedding result".into()))
    }

    fn dimensions(&self) -> usize {
        self.dims
    }
}
