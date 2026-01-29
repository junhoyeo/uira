//! Uira Providers - Model client implementations
//!
//! This crate provides a unified interface for interacting with various LLM providers:
//! - Anthropic (Claude)
//! - OpenAI (GPT)
//! - Google (Gemini)
//! - Ollama (local models)

mod anthropic;
mod client;
mod config;
mod error;
mod gemini;
mod ollama;
mod openai;
mod traits;

pub use anthropic::AnthropicClient;
pub use client::ModelClientBuilder;
pub use config::ProviderConfig;
pub use error::ProviderError;
pub use gemini::GeminiClient;
pub use ollama::OllamaClient;
pub use openai::OpenAIClient;
pub use traits::{ModelClient, ModelResult, ResponseStream};
