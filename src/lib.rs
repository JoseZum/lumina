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
pub mod init;

use crate::chunker::TreeSitterChunker;
use crate::config::LuminaConfig;
use crate::error::Result;
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

    let embedder = embeddings::create_embedder(config)?;

    let vector_store = Box::new(LanceStore::new(
        &config.lance_path(),
        config.embedding_dimensions,
    )?);
    let keyword_store = Box::new(TantivyStore::new(&config.tantivy_path())?);

    Indexer::new(chunker, embedder, vector_store, keyword_store, config.clone())
}

/// Create a SearchEngine with all components wired up.
pub fn create_search_engine(config: &LuminaConfig) -> Result<SearchEngine> {
    let embedder = embeddings::create_embedder(config)?;

    let vector_store = Box::new(LanceStore::new(
        &config.lance_path(),
        config.embedding_dimensions,
    )?);
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
