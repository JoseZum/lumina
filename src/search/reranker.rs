use crate::error::{LuminaError, Result};
use crate::types::{SearchResult, SearchSource};
use serde_json::Value;

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

/// Jina AI reranker — calls the Jina Reranker API.
pub struct JinaReranker {
    client: reqwest::blocking::Client,
    api_key: String,
    model: String,
}

impl JinaReranker {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            client: reqwest::blocking::Client::new(),
            api_key,
            model,
        }
    }
}

impl Reranker for JinaReranker {
    fn rerank(&self, query: &str, results: Vec<SearchResult>, top_k: usize) -> Result<Vec<SearchResult>> {
        if results.is_empty() {
            return Ok(results);
        }

        let documents: Vec<String> = results.iter()
            .map(|r| format!("{}:{} {}\n{}", r.file, r.start_line, r.symbol, r.text))
            .collect();

        let body = serde_json::json!({
            "model": self.model,
            "query": query,
            "documents": documents,
            "top_n": top_k.min(results.len()),
        });

        let response = self.client.post("https://api.jina.ai/v1/rerank")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .map_err(|e| LuminaError::EmbeddingError(format!("Reranker request failed: {}", e)))?;

        let json: Value = response.json()
            .map_err(|e| LuminaError::EmbeddingError(format!("Reranker parse failed: {}", e)))?;

        let reranked = json["results"].as_array()
            .ok_or_else(|| LuminaError::EmbeddingError("Invalid reranker response".into()))?;

        let mut scored: Vec<(usize, f32)> = reranked.iter().filter_map(|r| {
            let idx = r["index"].as_u64()? as usize;
            let score = r["relevance_score"].as_f64()? as f32;
            Some((idx, score))
        }).collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        Ok(scored.into_iter().filter_map(|(idx, score)| {
            results.get(idx).map(|r| {
                let mut r = r.clone();
                r.score = score;
                r.source = SearchSource::Reranked;
                r
            })
        }).take(top_k).collect())
    }
}
