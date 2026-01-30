//! Uira Context - Context window management
//!
//! This crate handles context window management for the agent:
//! - Message history storage
//! - Token estimation and tracking
//! - FIFO trimming when context is exceeded
//! - Compaction (summarization) of old context

mod compact;
mod error;
mod history;
mod manager;
mod truncate;

pub use compact::CompactionStrategy;
pub use error::ContextError;
pub use history::MessageHistory;
pub use manager::ContextManager;
pub use truncate::TruncationPolicy;
