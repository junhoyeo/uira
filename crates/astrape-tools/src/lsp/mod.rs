//! LSP client infrastructure for Astrape.
//!
//! This module provides a real LSP client implementation that can communicate
//! with language servers. Tool definitions are provided by astrape-mcp-server.

pub mod client;
pub mod servers;
pub mod utils;

pub use client::{LspClient, LspClientImpl};
pub use servers::{get_server_config, known_servers, LspServerConfig};
