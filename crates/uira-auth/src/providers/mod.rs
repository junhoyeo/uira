//! OAuth provider implementations for UIRA authentication
//!
//! This module provides native OAuth provider implementations for various services.
//! Currently supports:
//! - Anthropic OAuth
//! - OpenAI OAuth and API Key
//! - Google OAuth for Gemini API

pub mod anthropic;
pub mod google;
pub mod openai;

pub use anthropic::AnthropicAuth;
pub use google::GoogleAuth;
pub use openai::OpenAIAuth;
