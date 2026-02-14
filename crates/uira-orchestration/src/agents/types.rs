//! Agent infrastructure types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Model type for Claude models
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ModelType {
    #[default]
    Sonnet,
    Opus,
    Haiku,
    /// Inherit from parent/orchestrator
    Inherit,
}

impl ModelType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ModelType::Sonnet => "sonnet",
            ModelType::Opus => "opus",
            ModelType::Haiku => "haiku",
            ModelType::Inherit => "inherit",
        }
    }
}

impl std::fmt::Display for ModelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Cost tier for agent usage
/// Used to guide when to invoke expensive vs cheap agents
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum AgentCost {
    Free,
    Cheap,
    Expensive,
}

/// Agent category for routing and grouping
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentCategory {
    /// Code search and discovery
    Exploration,
    /// Domain-specific implementation
    Specialist,
    /// Strategic consultation (read-only)
    Advisor,
    /// General purpose helpers
    Utility,
    /// Multi-agent coordination
    Orchestration,
    /// Strategic planning
    Planner,
    /// Plan/work review
    Reviewer,
}

impl AgentCategory {
    /// Get the default model for this category
    pub fn default_model(&self) -> ModelType {
        match self {
            AgentCategory::Exploration => ModelType::Haiku, // Fast, cheap
            AgentCategory::Specialist => ModelType::Sonnet, // Balanced
            AgentCategory::Advisor => ModelType::Opus,      // High quality reasoning
            AgentCategory::Utility => ModelType::Haiku,     // Fast, cheap
            AgentCategory::Orchestration => ModelType::Sonnet, // Balanced
            AgentCategory::Planner => ModelType::Opus,      // Strategic thinking
            AgentCategory::Reviewer => ModelType::Opus,     // Critical analysis
        }
    }
}

/// Trigger condition for delegation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationTrigger {
    /// Domain or area this trigger applies to
    pub domain: String,
    /// Condition that triggers delegation
    pub trigger: String,
}

/// Metadata about an agent for dynamic prompt generation
/// This enables Uira to build delegation tables automatically
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPromptMetadata {
    /// Agent category
    pub category: AgentCategory,
    /// Cost tier
    pub cost: AgentCost,
    /// Short alias for prompts
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_alias: Option<String>,
    /// Conditions that trigger delegation to this agent
    #[serde(default)]
    pub triggers: Vec<DelegationTrigger>,
    /// When to use this agent
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub use_when: Vec<String>,
    /// When NOT to use this agent
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub avoid_when: Vec<String>,
    /// Description for dynamic prompt building
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_description: Option<String>,
    /// Tools this agent uses (for tool selection guidance)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
}

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

/// Routing tier for model selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum RoutingTier {
    Low,
    Medium,
    High,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_type_serialize() {
        let model = ModelType::Sonnet;
        let json = serde_json::to_string(&model).unwrap();
        assert_eq!(json, r#""sonnet""#);
    }

    #[test]
    fn test_agent_category_default_model() {
        assert_eq!(AgentCategory::Exploration.default_model(), ModelType::Haiku);
        assert_eq!(AgentCategory::Advisor.default_model(), ModelType::Opus);
        assert_eq!(AgentCategory::Specialist.default_model(), ModelType::Sonnet);
    }

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
}
