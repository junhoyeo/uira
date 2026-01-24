//! Configuration loading for the Astrape system
//!
//! This crate provides configuration management for Astrape, supporting:
//! - YAML and JSON configuration files
//! - Environment variable expansion
//! - Sensible defaults for all settings
//! - Type-safe configuration structures

pub mod loader;
pub mod schema;

pub use loader::{load_config, load_config_from_file};
pub use schema::{
    AgentConfig, AgentSettings, AiHookCommand, AiHooksConfig, AiSettings, AstrapeConfig,
    HookCommand, HookConfig, HooksConfig, McpServerConfig, McpSettings,
};
