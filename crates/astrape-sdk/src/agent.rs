//! Agent configuration types for Astrape SDK

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::types::{AgentPromptMetadata, ModelType};

/// Base agent configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Agent name/identifier
    pub name: String,
    /// Short description for agent selection
    pub description: String,
    /// System prompt for the agent
    pub prompt: String,
    /// Tools the agent can use
    pub tools: Vec<String>,
    /// Model to use (defaults to sonnet)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelType>,
    /// Default model for this agent (explicit tier mapping)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_model: Option<ModelType>,
    /// Optional metadata for dynamic prompt generation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<AgentPromptMetadata>,
}

impl AgentConfig {
    /// Create a new agent config
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        prompt: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            prompt: prompt.into(),
            tools: vec![],
            model: None,
            default_model: None,
            metadata: None,
        }
    }

    /// Set tools for this agent
    pub fn with_tools(mut self, tools: Vec<String>) -> Self {
        self.tools = tools;
        self
    }

    /// Set model for this agent
    pub fn with_model(mut self, model: ModelType) -> Self {
        self.model = Some(model);
        self
    }

    /// Set metadata for this agent
    pub fn with_metadata(mut self, metadata: AgentPromptMetadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Get the effective model (model > default_model > Sonnet)
    pub fn effective_model(&self) -> ModelType {
        self.model
            .or(self.default_model)
            .unwrap_or(ModelType::Sonnet)
    }
}

/// Extended agent config with all optional fields
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullAgentConfig {
    /// Base configuration
    #[serde(flatten)]
    pub base: AgentConfig,
    /// Temperature setting
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Max tokens
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Thinking configuration (for Claude models)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,
    /// Tool restrictions
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_restrictions: Vec<String>,
}

/// Thinking mode configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingConfig {
    /// Whether thinking is enabled
    #[serde(rename = "type")]
    pub thinking_type: ThinkingType,
    /// Budget tokens for thinking
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget_tokens: Option<u32>,
}

/// Thinking type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThinkingType {
    Enabled,
    Disabled,
}

/// Agent override configuration for customization
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentOverrideConfig {
    /// Override model
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Enable/disable agent
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Append to prompt
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_append: Option<String>,
    /// Override temperature
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
}

/// Map of agent overrides
pub type AgentOverrides = HashMap<String, AgentOverrideConfig>;

/// Available agent descriptor for prompt building
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailableAgent {
    /// Agent name
    pub name: String,
    /// Agent description
    pub description: String,
    /// Agent metadata
    pub metadata: AgentPromptMetadata,
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_config_builder() {
        let agent = AgentConfig::new(
            "explore",
            "Fast codebase search",
            "You are an explore agent...",
        )
        .with_tools(vec![
            "Read".to_string(),
            "Glob".to_string(),
            "Grep".to_string(),
        ])
        .with_model(ModelType::Haiku);

        assert_eq!(agent.name, "explore");
        assert_eq!(agent.effective_model(), ModelType::Haiku);
        assert_eq!(agent.tools.len(), 3);
    }

    #[test]
    fn test_agent_definition_entry_from_config() {
        let config =
            AgentConfig::new("test", "Test agent", "Test prompt").with_model(ModelType::Sonnet);

        let entry: AgentDefinitionEntry = config.into();
        assert_eq!(entry.description, "Test agent");
        assert_eq!(entry.model, Some("sonnet".to_string()));
    }
}
