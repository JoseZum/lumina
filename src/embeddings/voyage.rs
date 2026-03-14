use crate::embeddings::Embedder;
use crate::error::{LuminaError, Result};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

const VOYAGE_API_URL: &str = "https://api.voyageai.com/v1/embeddings";
const VOYAGE_DIMENSIONS: usize = 1024;

pub struct VoyageEmbedder {
    client: Client,
    api_key: String,
    model: String,
    batch_size: usize,
}

impl VoyageEmbedder {
    pub fn new(api_key: String, model: String, batch_size: usize) -> Self {
        Self {
            client: Client::new(),
            api_key,
            model,
            batch_size,
        }
    }

    fn call_api(&self, texts: &[&str], input_type: &str) -> Result<Vec<Vec<f32>>> {
        let request_body = VoyageRequest {
            model: &self.model,
            input: texts,
            input_type,
        };

        let response = self
            .client
            .post(VOYAGE_API_URL)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request_body)
            .send()?;

        let status = response.status();

        if !status.is_success() {
            let body = response.text().unwrap_or_default();

            if status.as_u16() == 429 {
                warn!("Voyage API rate limited");
                return Err(LuminaError::EmbeddingRateLimited {
                    retry_after_secs: 60,
                });
            }

            return Err(LuminaError::EmbeddingError(format!(
                "Voyage API returned {}: {}",
                status, body
            )));
        }

        let voyage_response: VoyageResponse = response.json()?;

        // Sort by index to ensure order matches input
        let mut data = voyage_response.data;
        data.sort_by_key(|d| d.index);

        Ok(data.into_iter().map(|d| d.embedding).collect())
    }
}

impl Embedder for VoyageEmbedder {
    fn embed_texts(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let mut all_embeddings = Vec::with_capacity(texts.len());

        for batch in texts.chunks(self.batch_size) {
            debug!("Embedding batch of {} texts", batch.len());
            let embeddings = self.call_api(batch, "document")?;
            all_embeddings.extend(embeddings);
        }

        Ok(all_embeddings)
    }

    fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        let results = self.call_api(&[query], "query")?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| LuminaError::EmbeddingError("Empty response from Voyage API".into()))
    }

    fn dimensions(&self) -> usize {
        VOYAGE_DIMENSIONS
    }
}

// ── API request/response types ──

#[derive(Serialize)]
struct VoyageRequest<'a> {
    model: &'a str,
    input: &'a [&'a str],
    input_type: &'a str,
}

#[derive(Deserialize)]
struct VoyageResponse {
    data: Vec<VoyageEmbeddingData>,
}

#[derive(Deserialize)]
struct VoyageEmbeddingData {
    embedding: Vec<f32>,
    index: usize,
}
