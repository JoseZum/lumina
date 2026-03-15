pub mod reranker;
pub mod rrf;

use crate::config::LuminaConfig;
use crate::embeddings::Embedder;
use crate::error::{LuminaError, Result};
use crate::search::reranker::Reranker;
use crate::store::{KeywordStore, VectorStore};
use crate::types::SearchResult;
use std::path::Path;

/// The main search orchestrator.
/// Combines vector search, keyword search, RRF fusion,
/// optional reranking, and token budget enforcement.
pub struct SearchEngine {
    vector_store: Box<dyn VectorStore>,
    keyword_store: Box<dyn KeywordStore>,
    embedder: Box<dyn Embedder>,
    reranker: Box<dyn Reranker>,
    config: LuminaConfig,
}

impl SearchEngine {
    pub fn new(
        vector_store: Box<dyn VectorStore>,
        keyword_store: Box<dyn KeywordStore>,
        embedder: Box<dyn Embedder>,
        reranker: Box<dyn Reranker>,
        config: LuminaConfig,
    ) -> Self {
        Self {
            vector_store,
            keyword_store,
            embedder,
            reranker,
            config,
        }
    }

    /// Full semantic search pipeline:
    /// 1. Embed query → 2. Vector search → 3. Keyword search → 4. RRF → 5. Rerank
    pub fn semantic_search(&self, query: &str, k: usize) -> Result<Vec<SearchResult>> {
        // 1. Embed the query
        let query_embedding = self.embedder.embed_query(query)?;

        // 2. Vector search
        let vector_results = self
            .vector_store
            .search(&query_embedding, self.config.search_k_vector)?;

        // 3. Keyword search
        let keyword_results = self
            .keyword_store
            .search(query, self.config.search_k_keyword)?;

        // 4. RRF fusion
        let fused = rrf::rrf_merge(vector_results, keyword_results, self.config.rrf_k);

        // 5. Rerank (or noop)
        let results = self.reranker.rerank(query, fused, k)?;

        Ok(results)
    }

    /// Semantic search scoped to a specific directory using native store filters.
    pub fn search_in_directory(&self, query: &str, directory: &str, k: usize) -> Result<Vec<SearchResult>> {
        let query_embedding = self.embedder.embed_query(query)?;
        let vector_results = self.vector_store
            .search_filtered(&query_embedding, self.config.search_k_vector, directory)?;
        let keyword_results = self.keyword_store
            .search_filtered(query, self.config.search_k_keyword, directory)?;
        let fused = rrf::rrf_merge(vector_results, keyword_results, self.config.rrf_k);
        self.reranker.rerank(query, fused, k)
    }

    /// Find symbols by name via keyword store.
    pub fn find_symbol(&self, name: &str, limit: usize) -> Result<Vec<SearchResult>> {
        self.keyword_store.search_symbol(name, limit)
    }

    /// Read a span of lines from a file on disk.
    pub fn get_file_span(
        &self,
        file: &str,
        start_line: u32,
        end_line: u32,
    ) -> Result<String> {
        let full_path = self.config.repo_root.join(file);

        if !full_path.exists() {
            return Err(LuminaError::FileNotFound(full_path));
        }

        let content = std::fs::read_to_string(&full_path)?;
        let lines: Vec<&str> = content.lines().collect();

        let start = (start_line.saturating_sub(1)) as usize;
        let end = (end_line as usize).min(lines.len());

        if start >= lines.len() {
            return Ok(String::new());
        }

        Ok(lines[start..end].join("\n"))
    }

    /// List all indexed files.
    pub fn list_files(&self) -> Result<Vec<String>> {
        self.keyword_store.list_files()
    }

    /// Format search results as markdown within a token budget.
    pub fn format_results(
        &self,
        query: &str,
        results: &[SearchResult],
        budget: usize,
    ) -> String {
        let mut output = format!("## Search Results for: \"{}\"\n\n", query);
        let mut tokens_used = estimate_tokens(&output);
        let mut files_seen = std::collections::HashSet::new();

        for (i, result) in results.iter().enumerate() {
            files_seen.insert(&result.file);

            // Format this result
            let header = format!(
                "### {}. {}:{}-{} (`{}`)\n",
                i + 1,
                result.file,
                result.start_line,
                result.end_line,
                result.symbol
            );
            let code_block = format!("```{}\n{}\n```\n\n", result.language, result.text);
            let entry = format!("{}{}", header, code_block);
            let entry_tokens = estimate_tokens(&entry);

            // Check budget
            if tokens_used + entry_tokens > budget {
                // Try metadata-only (no code)
                let meta_only = format!(
                    "### {}. {}:{}-{} (`{}`) — *truncated*\n\n",
                    i + 1,
                    result.file,
                    result.start_line,
                    result.end_line,
                    result.symbol
                );
                let meta_tokens = estimate_tokens(&meta_only);
                if tokens_used + meta_tokens <= budget {
                    output.push_str(&meta_only);
                    tokens_used += meta_tokens;
                } else {
                    break;
                }
            } else {
                output.push_str(&entry);
                tokens_used += entry_tokens;
            }
        }

        // Footer
        let footer = format!(
            "---\n*{} results from {} files | ~{} tokens*\n",
            results.len().min(output.matches("### ").count()),
            files_seen.len(),
            tokens_used
        );
        output.push_str(&footer);

        output
    }
}

/// Estimate token count. ~4 chars per token for code.
fn estimate_tokens(text: &str) -> usize {
    (text.len() + 3) / 4
}
