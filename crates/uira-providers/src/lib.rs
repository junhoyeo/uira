//! Uira Providers - Model client implementations
//!
//! This crate provides a unified interface for interacting with various LLM providers:
//! - Anthropic (Claude)
//! - OpenAI (GPT)
//! - Google (Gemini)
//! - Ollama (local models)

#![allow(hidden_glob_reexports)]

mod anthropic;
pub mod auth;
mod client;
mod config;
mod error;
mod gemini;
mod image;
mod ollama;
mod openai;
mod opencode;
mod traits;

pub use anthropic::classify_error;
pub use anthropic::validate_anthropic_turns;
pub use anthropic::AnthropicClient;
pub use anthropic::BetaFeatures;
pub use anthropic::{with_retry, PayloadLogEvent, PayloadLogger, RetryConfig};
pub use auth::*;
pub use client::ModelClientBuilder;
pub use config::ProviderConfig;
pub use error::ProviderError;
pub use gemini::GeminiClient;
pub use ollama::OllamaClient;
pub use openai::classify_error as classify_openai_error;
pub use openai::OpenAIClient;
pub use opencode::OpenCodeClient;
pub use secrecy::SecretString;
pub use traits::{ModelClient, ModelResult, ResponseStream};
