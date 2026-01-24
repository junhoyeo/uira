//! Feature modules for Astrape
//!
//! This crate provides advanced feature implementations for the Astrape framework:
//! - State management and session lifecycle
//! - Workflow orchestration
//! - Advanced execution patterns

pub mod state_manager;

pub use state_manager::{SessionState, StateManager};
