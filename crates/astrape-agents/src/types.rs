//! Agent infrastructure types.
//!
//! These mirror the TypeScript types in `oh-my-claudecode/src/agents/types.ts`.
//!
//! Note: Astrape already defines these in `astrape-sdk`; this crate re-exports
//! them to keep the agent system cohesive.

pub use astrape_sdk::{
    AgentCategory, AgentConfig, AgentCost, AgentPromptMetadata, DelegationTrigger, ModelType,
};
