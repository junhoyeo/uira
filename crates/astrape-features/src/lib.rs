//! Feature modules for Astrape
//!
//! This crate provides advanced feature implementations for the Astrape framework:
//! - State management and session lifecycle
//! - Model routing and smart selection
//! - Workflow orchestration
//! - Advanced execution patterns

pub mod astrape_state;
pub mod background_agent;
pub mod builtin_skills;
pub mod context_injector;
pub mod delegation_categories;
pub mod model_routing;
pub mod notepad_wisdom;
pub mod state_manager;
pub mod task_decomposer;
pub mod verification;

pub use state_manager::{SessionState, StateManager};
