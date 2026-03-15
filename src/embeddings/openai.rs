use crate::embeddings::Embedder;
use crate::error::{LuminaError, Result};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

const OPENAI_API_URL: &str = "https://api.openai.com/v1/embeddings";
const OPENAI_DIMENSIONS: usize = 1536;

pub struct OpenAiEmbedder {
    client: Client,
    api_key: String,
    model: String,
    batch_size: usize,
}

impl OpenAiEmbedder {
    pub fn new(api_key: String, model: String, batch_size: usize) -> Self {
        Self {
            client: Client::new(),
            api_key,
            model,
            batch_size,
        }
    }

    fn call_api(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let request_body = OpenAiRequest {
            model: &self.model,
            input: texts,
        };

        let response = self
            .client
            .post(OPENAI_API_URL)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request_body)
            .send()?;

        let status = response.status();

        if !status.is_success() {
            let body = response.text().unwrap_or_default();

            if status.as_u16() == 429 {
                warn!("OpenAI API rate limited");
                return Err(LuminaError::EmbeddingRateLimited {
                    retry_after_secs: 60,
                });
            }

            return Err(LuminaError::EmbeddingError(format!(
                "OpenAI API returned {}: {}",
                status, body
            )));
        }

        let openai_response: OpenAiResponse = response.json()?;

        // Sort by index to ensure order matches input
        let mut data = openai_response.data;
        data.sort_by_key(|d| d.index);

        Ok(data.into_iter().map(|d| d.embedding).collect())
    }
}

impl Embedder for OpenAiEmbedder {
    fn embed_texts(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let mut all_embeddings = Vec::with_capacity(texts.len());

        for batch in texts.chunks(self.batch_size) {
            debug!("Embedding batch of {} texts via OpenAI", batch.len());
            let embeddings = self.call_api(batch)?;
            all_embeddings.extend(embeddings);
        }

        Ok(all_embeddings)
    }

    fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        let results = self.call_api(&[query])?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| LuminaError::EmbeddingError("Empty response from OpenAI API".into()))
    }

    fn dimensions(&self) -> usize {
        OPENAI_DIMENSIONS
    }
}

// ── API request/response types ──

#[derive(Serialize)]
struct OpenAiRequest<'a> {
    model: &'a str,
    input: &'a [&'a str],
}

#[derive(Deserialize)]
struct OpenAiResponse {
    data: Vec<OpenAiEmbeddingData>,
}

#[derive(Deserialize)]
struct OpenAiEmbeddingData {
    embedding: Vec<f32>,
    index: usize,
}
