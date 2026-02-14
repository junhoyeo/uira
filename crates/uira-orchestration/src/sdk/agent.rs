//! Agent configuration types for Uira SDK
//!
//! These types are defined in the agents module and re-exported here
//! for backwards compatibility.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub use crate::agents::{AgentConfig, AgentOverrideConfig, AgentOverrides};

/// Agent definitions map (what createSisyphusSession returns)
pub type AgentDefinitions = HashMap<String, AgentDefinitionEntry>;

/// Single agent definition entry (SDK format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinitionEntry {
    /// Agent description
    pub description: String,
    /// System prompt
    pub prompt: String,
    /// Available tools
    pub tools: Vec<String>,
    /// Model to use
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl From<AgentConfig> for AgentDefinitionEntry {
    fn from(config: AgentConfig) -> Self {
        Self {
            description: config.description,
            prompt: config.prompt,
            tools: config.tools,
            model: config.model.map(|m| m.to_string()),
        }
    }
}
