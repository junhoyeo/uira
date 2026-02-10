//! Uira Providers - Model client implementations
//!
//! This crate provides a unified interface for interacting with various LLM providers:
//! - Anthropic (Claude)
//! - OpenAI (GPT)
//! - Google (Gemini)
//! - Ollama (local models)

mod anthropic;
mod beta_features;
mod client;
mod config;
mod error;
mod error_classify;
mod gemini;
mod image;
mod ollama;
mod openai;
mod opencode;
mod payload_log;
mod response_handling;
mod retry;
mod traits;
mod turn_validation;

pub use anthropic::AnthropicClient;
pub use beta_features::BetaFeatures;
pub use client::ModelClientBuilder;
pub use config::ProviderConfig;
pub use error::ProviderError;
pub use error_classify::classify_error;
pub use gemini::GeminiClient;
pub use ollama::OllamaClient;
pub use openai::OpenAIClient;
pub use opencode::OpenCodeClient;
pub use payload_log::{PayloadLogEvent, PayloadLogger};
pub use retry::{with_retry, RetryConfig};
pub use secrecy::SecretString;
pub use traits::{ModelClient, ModelResult, ResponseStream};
pub use turn_validation::validate_anthropic_turns;
