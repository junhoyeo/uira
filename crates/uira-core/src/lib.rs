pub mod config;
pub mod events;

pub const UIRA_DIR: &str = ".uira";
pub const DEFAULT_ANTHROPIC_MODEL: &str = "claude-sonnet-4-20250514";
pub const DEFAULT_OPENAI_MODEL: &str = "gpt-4o";
pub const ENV_ANTHROPIC_API_KEY: &str = "ANTHROPIC_API_KEY";
pub const ENV_OPENAI_API_KEY: &str = "OPENAI_API_KEY";
pub const ENV_GEMINI_API_KEY: &str = "GEMINI_API_KEY";
pub const ENV_GOOGLE_API_KEY: &str = "GOOGLE_API_KEY";

pub use config::*;
pub use events::*;
