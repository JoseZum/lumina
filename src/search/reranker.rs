use crate::error::Result;
use crate::types::SearchResult;

/// Trait for reranking search results using a cross-encoder model.
pub trait Reranker: Send + Sync {
    /// Rerank results by relevance to the query.
    /// Returns results in new order with updated scores.
    fn rerank(&self, query: &str, results: Vec<SearchResult>, top_k: usize) -> Result<Vec<SearchResult>>;
}

/// Pass-through reranker — returns results unchanged.
/// Used when no reranker API key is configured.
pub struct NoopReranker;

impl Reranker for NoopReranker {
    fn rerank(&self, _query: &str, mut results: Vec<SearchResult>, top_k: usize) -> Result<Vec<SearchResult>> {
        results.truncate(top_k);
        Ok(results)
    }
}
