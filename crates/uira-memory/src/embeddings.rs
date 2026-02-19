use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::config::MemoryConfig;

#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
    fn dimension(&self) -> usize;
    fn model_name(&self) -> &str;
}

pub fn content_hash(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub struct OpenAIEmbeddingProvider {
    client: reqwest::Client,
    api_key: String,
    api_base: String,
    model: String,
    dimension: usize,
}

impl OpenAIEmbeddingProvider {
    pub fn new(config: &MemoryConfig) -> Result<Self> {
        let api_key = std::env::var(&config.embedding_api_key_env).with_context(|| {
            format!(
                "Missing API key env var '{}' for embedding provider",
                config.embedding_api_key_env
            )
        })?;

        Ok(Self {
            client: reqwest::Client::new(),
            api_key,
            api_base: config.embedding_api_base.clone(),
            model: config.embedding_model.clone(),
            dimension: config.embedding_dimension,
        })
    }

    pub fn new_with_key(api_key: String, config: &MemoryConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            api_base: config.embedding_api_base.clone(),
            model: config.embedding_model.clone(),
            dimension: config.embedding_dimension,
        }
    }
}

#[derive(Serialize)]
struct EmbeddingRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

#[async_trait]
impl EmbeddingProvider for OpenAIEmbeddingProvider {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let request = EmbeddingRequest {
            model: self.model.clone(),
            input: texts.to_vec(),
        };

        let response = self
            .client
            .post(format!("{}/embeddings", self.api_base))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send embedding request")?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Embedding API returned {status}: {body}");
        }

        let body: EmbeddingResponse = response
            .json()
            .await
            .context("Failed to parse embedding response")?;

        Ok(body.data.into_iter().map(|d| d.embedding).collect())
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}

pub struct MockEmbeddingProvider {
    dimension: usize,
}

impl MockEmbeddingProvider {
    pub fn new(dimension: usize) -> Self {
        Self { dimension }
    }
}

#[async_trait]
impl EmbeddingProvider for MockEmbeddingProvider {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        Ok(texts
            .iter()
            .map(|text| {
                let hash = content_hash(text);
                let bytes = hash.as_bytes();
                (0..self.dimension)
                    .map(|i| {
                        let byte = bytes[i % bytes.len()] as f32;
                        (byte / 255.0) * 2.0 - 1.0
                    })
                    .collect()
            })
            .collect())
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn model_name(&self) -> &str {
        "mock"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_hash_is_deterministic() {
        let h1 = content_hash("hello world");
        let h2 = content_hash("hello world");
        assert_eq!(h1, h2);
    }

    #[test]
    fn content_hash_differs_for_different_input() {
        let h1 = content_hash("hello");
        let h2 = content_hash("world");
        assert_ne!(h1, h2);
    }

    #[tokio::test]
    async fn mock_provider_produces_correct_dimensions() {
        let provider = MockEmbeddingProvider::new(128);
        let embeddings = provider.embed(&["test".to_string()]).await.unwrap();
        assert_eq!(embeddings.len(), 1);
        assert_eq!(embeddings[0].len(), 128);
    }

    #[tokio::test]
    async fn mock_provider_empty_input() {
        let provider = MockEmbeddingProvider::new(128);
        let embeddings = provider.embed(&[]).await.unwrap();
        assert!(embeddings.is_empty());
    }

    #[tokio::test]
    async fn mock_provider_deterministic() {
        let provider = MockEmbeddingProvider::new(64);
        let e1 = provider.embed(&["hello".to_string()]).await.unwrap();
        let e2 = provider.embed(&["hello".to_string()]).await.unwrap();
        assert_eq!(e1, e2);
    }
}
