//! # astrape-mcp
//!
//! MCP (Model Context Protocol) types and integrations for Astrape.
//!
//! This crate provides core types and traits for working with MCP servers,
//! including server configuration, tools, resources, and prompts.
//!
//! ## Features
//!
//! - **Server Configuration**: Define and manage MCP server configurations
//! - **Tool Support**: Define and call MCP tools with JSON schemas
//! - **Resource Management**: Access and manage MCP resources
//! - **Prompt Templates**: Define and execute prompt templates
//! - **Server Registry**: Manage multiple MCP servers
//! - **Client Trait**: Extensible trait for MCP client implementations
//!
//! ## Example
//!
//! ```rust,no_run
//! use astrape_mcp::types::{McpServerConfig, McpTool};
//! use astrape_mcp::registry::McpServerRegistry;
//!
//! let mut registry = McpServerRegistry::new();
//!
//! let config = McpServerConfig {
//!     name: "my-server".to_string(),
//!     command: "mcp-server".to_string(),
//!     args: vec!["--port".to_string(), "8080".to_string()],
//!     env: None,
//! };
//!
//! registry.register(config);
//! ```

pub mod client;
pub mod registry;
pub mod types;

pub use client::{McpClient, McpClientError, McpResult};
pub use registry::McpServerRegistry;
pub use types::{
    McpCapabilities, McpPrompt, McpResource, McpServerConfig, McpTool, McpToolResponse,
    PromptArgument,
};
