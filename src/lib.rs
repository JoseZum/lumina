#![allow(warnings)]

pub mod error;
pub mod types;
pub mod config;
pub mod chunker;
pub mod embeddings;
pub mod store;
pub mod search;
pub mod indexer;
pub mod mcp;

use crate::chunker::TreeSitterChunker;
use crate::config::LuminaConfig;
use crate::embeddings::voyage::VoyageEmbedder;
use crate::embeddings::MockEmbedder;
use crate::error::{LuminaError, Result};
use crate::indexer::Indexer;
use crate::search::reranker::NoopReranker;
use crate::search::SearchEngine;
use crate::store::lance::LanceStore;
use crate::store::tantivy_store::TantivyStore;

/// Create an Indexer with all components wired up.
pub fn create_indexer(config: &LuminaConfig) -> Result<Indexer> {
    let chunker = Box::new(TreeSitterChunker::new(
        config.max_chunk_tokens,
        config.min_chunk_tokens,
    ));

    let embedder: Box<dyn embeddings::Embedder> = if let Some(ref key) = config.voyage_api_key {
        Box::new(VoyageEmbedder::new(
            key.clone(),
            config.voyage_model.clone(),
            config.embedding_batch_size,
        ))
    } else {
        eprintln!("Warning: No VOYAGE_API_KEY set. Using mock embedder (results will be random).");
        Box::new(MockEmbedder::new(1024))
    };

    let vector_store = Box::new(LanceStore::new(&config.lance_path())?);
    let keyword_store = Box::new(TantivyStore::new(&config.tantivy_path())?);

    Indexer::new(chunker, embedder, vector_store, keyword_store, config.clone())
}

/// Create a SearchEngine with all components wired up.
pub fn create_search_engine(config: &LuminaConfig) -> Result<SearchEngine> {
    let embedder: Box<dyn embeddings::Embedder> = if let Some(ref key) = config.voyage_api_key {
        Box::new(VoyageEmbedder::new(
            key.clone(),
            config.voyage_model.clone(),
            config.embedding_batch_size,
        ))
    } else {
        eprintln!("Warning: No VOYAGE_API_KEY set. Using mock embedder (results will be random).");
        Box::new(MockEmbedder::new(1024))
    };

    let vector_store = Box::new(LanceStore::new(&config.lance_path())?);
    let keyword_store = Box::new(TantivyStore::new(&config.tantivy_path())?);
    let reranker = Box::new(NoopReranker);

    Ok(SearchEngine::new(
        vector_store,
        keyword_store,
        embedder,
        reranker,
        config.clone(),
    ))
}
