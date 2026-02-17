//! Feature modules for Uira
//!
//! This crate provides advanced feature implementations for the Uira framework:
//! - State management and session lifecycle
//! - Model routing and smart selection
//! - Workflow orchestration
//! - Advanced execution patterns
//! - Analytics and metrics collection
//! - Keyword detection for mode activation

pub mod analytics;
pub mod background_agent;
pub mod builtin_skills;
pub mod context_injector;
pub mod delegation_categories;
pub mod dynamic_prompt_builder;
pub mod keywords;
pub mod model_routing;
pub mod notepad_wisdom;
pub mod rate_limit_wait;
pub mod state_manager;
pub mod task_decomposer;
pub mod uira_state;
pub mod verification;

pub use context_injector::{build_environment_context, register_environment_context};
pub use keywords::{KeywordDetector, KeywordPattern};
pub use state_manager::{SessionState, StateManager};
