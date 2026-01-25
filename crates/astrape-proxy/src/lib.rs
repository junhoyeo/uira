//! Astrape Proxy - HTTP proxy for using non-Anthropic models with Claude Code.
//!
//! Claude Code speaks Anthropic's `/v1/messages` API. This crate exposes a
//! compatible HTTP surface, but translates requests/responses to the OpenAI-ish
//! format used by LiteLLM (`/v1/chat/completions`).
//!
//! Design goals:
//! - Accept Claude Code traffic (Anthropic wire format).
//! - Forward to a LiteLLM-compatible backend (OpenAI-style `chat/completions`).
//! - Translate responses back to Anthropic semantics (including SSE streaming).
//! - Use OpenCode's local auth store (`auth.json`) to obtain provider tokens.

pub mod auth;
pub mod config;
pub mod server;
pub mod streaming;
pub mod translation;
pub mod types;

pub use config::ProxyConfig;
pub use server::serve;
