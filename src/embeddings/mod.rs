pub mod voyage;
pub mod openai;
pub mod local;

use crate::config::{EmbeddingProvider, LuminaConfig};
use crate::error::{LuminaError, Result};

/// Trait for text embedding models.
///
/// Implementations may call external APIs (Voyage, OpenAI) or run local models.
/// All methods are synchronous — async is only used where LanceDB forces it.
pub trait Embedder: Send + Sync {
    /// Embed multiple document texts in a single batch.
    ///
    /// Implementations must handle internal batching if the API has
    /// a max batch size (e.g., Voyage limits to 128 texts per call).
    ///
    /// Returns one embedding vector per input text, in the same order.
    fn embed_texts(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;

    /// Embed a single query text.
    ///
    /// Some embedding models use different encoding for queries vs documents
    /// (e.g., Voyage uses input_type "query" vs "document").
    fn embed_query(&self, query: &str) -> Result<Vec<f32>>;

    /// Number of dimensions in the embedding vectors.
    fn dimensions(&self) -> usize;
}

/// A mock embedder for testing — produces deterministic fake embeddings
/// based on SHA-256 of the input text, so the same text always yields
/// the same vector. This allows deterministic integration tests without API calls.
pub struct MockEmbedder {
    dims: usize,
}

impl MockEmbedder {
    pub fn new(dims: usize) -> Self {
        Self { dims }
    }
}

impl Embedder for MockEmbedder {
    fn embed_texts(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|t| deterministic_embedding(t, self.dims)).collect())
    }

    fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        Ok(deterministic_embedding(query, self.dims))
    }

    fn dimensions(&self) -> usize {
        self.dims
    }
}

/// Generate a deterministic embedding from text using SHA-256.
/// The hash bytes are repeated/truncated to fill `dims` dimensions,
/// then normalized to unit length.
fn deterministic_embedding(text: &str, dims: usize) -> Vec<f32> {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(text.as_bytes());
    let hash_bytes = hash.as_slice();

    let mut vec: Vec<f32> = (0..dims)
        .map(|i| {
            let byte = hash_bytes[i % hash_bytes.len()];
            (byte as f32 / 255.0) * 2.0 - 1.0 // normalize to [-1, 1]
        })
        .collect();

    // L2 normalize
    let norm: f32 = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 0.0 {
        for v in &mut vec {
            *v /= norm;
        }
    }

    vec
}

/// Create the appropriate embedder based on config.
pub fn create_embedder(config: &LuminaConfig) -> Result<Box<dyn Embedder>> {
    match config.embedding_provider {
        EmbeddingProvider::Local => {
            Ok(Box::new(local::LocalEmbedder::new(&config.embedding_model)?))
        }
        EmbeddingProvider::Voyage => {
            let api_key = config.voyage_api_key.clone().ok_or_else(|| {
                LuminaError::MissingApiKey {
                    env_var: "VOYAGE_API_KEY".to_string(),
                }
            })?;
            Ok(Box::new(voyage::VoyageEmbedder::new(
                api_key,
                config.embedding_model.clone(),
                config.embedding_batch_size,
            )))
        }
        EmbeddingProvider::OpenAi => {
            let api_key = config.openai_api_key.clone().ok_or_else(|| {
                LuminaError::MissingApiKey {
                    env_var: "OPENAI_API_KEY".to_string(),
                }
            })?;
            Ok(Box::new(openai::OpenAiEmbedder::new(
                api_key,
                config.embedding_model.clone(),
                config.embedding_batch_size,
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_embedder_deterministic() {
        let embedder = MockEmbedder::new(1024);
        let v1 = embedder.embed_query("hello world").unwrap();
        let v2 = embedder.embed_query("hello world").unwrap();
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_mock_embedder_different_inputs() {
        let embedder = MockEmbedder::new(1024);
        let v1 = embedder.embed_query("hello").unwrap();
        let v2 = embedder.embed_query("world").unwrap();
        assert_ne!(v1, v2);
    }

    #[test]
    fn test_mock_embedder_correct_dimensions() {
        let embedder = MockEmbedder::new(1024);
        let v = embedder.embed_query("test").unwrap();
        assert_eq!(v.len(), 1024);
    }

    #[test]
    fn test_mock_embedder_batch() {
        let embedder = MockEmbedder::new(1024);
        let texts = vec!["hello", "world", "test"];
        let vecs = embedder.embed_texts(&texts).unwrap();
        assert_eq!(vecs.len(), 3);
        assert_eq!(vecs[0].len(), 1024);
        // Same text = same embedding
        let single = embedder.embed_query("hello").unwrap();
        assert_eq!(vecs[0], single);
    }

    #[test]
    fn test_mock_embedder_normalized() {
        let embedder = MockEmbedder::new(1024);
        let v = embedder.embed_query("test").unwrap();
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.01, "Expected unit norm, got {}", norm);
    }
}
