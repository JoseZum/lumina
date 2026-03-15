pub mod tantivy_store;
pub mod lance;

use crate::error::Result;
use crate::types::{Chunk, SearchResult};

/// Trait for vector-based similarity search
pub trait VectorStore: Send + Sync {
    /// Insert or update chunks with their embeddings
    fn upsert(&self, chunks: &[Chunk]) -> Result<()>;

    /// Search for similar chunks by embedding vector
    fn search(&self, embedding: &[f32], limit: usize) -> Result<Vec<SearchResult>>;

    /// Search with a file path prefix filter. Default: search then filter.
    fn search_filtered(&self, embedding: &[f32], limit: usize, file_prefix: &str) -> Result<Vec<SearchResult>> {
        let dir_prefix = format!("{}/", file_prefix);
        let results = self.search(embedding, limit * 5)?;
        Ok(results.into_iter()
            .filter(|r| r.file == file_prefix || r.file.starts_with(&dir_prefix))
            .take(limit)
            .collect())
    }

    /// Delete all chunks belonging to a specific file
    fn delete_by_file(&self, file_path: &str) -> Result<()>;

    /// Get the total number of stored chunks
    fn count(&self) -> Result<usize>;
}

/// Trait for keyword-based BM25 search
pub trait KeywordStore: Send + Sync {
    /// Insert or update chunks for keyword search
    fn upsert(&self, chunks: &[Chunk]) -> Result<()>;

    /// Full-text keyword search
    fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>>;

    /// Keyword search with a file path prefix filter. Default: search then filter.
    fn search_filtered(&self, query: &str, limit: usize, file_prefix: &str) -> Result<Vec<SearchResult>> {
        let dir_prefix = format!("{}/", file_prefix);
        let results = self.search(query, limit * 5)?;
        Ok(results.into_iter()
            .filter(|r| r.file == file_prefix || r.file.starts_with(&dir_prefix))
            .take(limit)
            .collect())
    }

    /// Search for symbols by name (exact or prefix match)
    fn search_symbol(&self, symbol_name: &str, limit: usize) -> Result<Vec<SearchResult>>;

    /// Delete all chunks belonging to a specific file
    fn delete_by_file(&self, file_path: &str) -> Result<()>;

    /// List all indexed files
    fn list_files(&self) -> Result<Vec<String>>;

    /// Get the total number of stored chunks
    fn count(&self) -> Result<usize>;
}
