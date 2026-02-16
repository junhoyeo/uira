pub mod config;
pub mod events;

pub const UIRA_DIR: &str = ".uira";
pub const DEFAULT_ANTHROPIC_MODEL: &str = "claude-sonnet-4-20250514";
pub const DEFAULT_OPENAI_MODEL: &str = "gpt-4o";

pub use config::*;
pub use events::*;
