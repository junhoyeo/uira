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
mod monitor;
mod truncate;

pub use compact::{CompactionConfig, CompactionResult, CompactionStrategy, PruningStrategy};
pub use error::ContextError;
pub use history::MessageHistory;
pub use manager::ContextManager;
pub use monitor::{TokenMonitor, TokenMonitorSnapshot};
pub use truncate::TruncationPolicy;
