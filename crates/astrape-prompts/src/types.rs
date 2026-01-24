use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Agent category for grouping and organization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentCategory {
    Exploration,
    Implementation,
    Analysis,
    Planning,
    Quality,
    Utility,
}

impl AgentCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Exploration => "exploration",
            Self::Implementation => "implementation",
            Self::Analysis => "analysis",
            Self::Planning => "planning",
            Self::Quality => "quality",
            Self::Utility => "utility",
        }
    }

    pub fn capitalize(&self) -> &'static str {
        match self {
            Self::Exploration => "Exploration",
            Self::Implementation => "Implementation",
            Self::Analysis => "Analysis",
            Self::Planning => "Planning",
            Self::Quality => "Quality",
            Self::Utility => "Utility",
        }
    }
}

/// Model type for agent execution
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelType {
    Opus,
    Sonnet,
    Haiku,
    Inherit,
}

impl ModelType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Opus => "opus",
            Self::Sonnet => "sonnet",
            Self::Haiku => "haiku",
            Self::Inherit => "inherit",
        }
    }
}

/// Trigger condition for when to use an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTrigger {
    pub domain: String,
    pub trigger: String,
}

/// Metadata about an agent's usage patterns
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPromptMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<AgentCategory>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub triggers: Option<Vec<AgentTrigger>>,

    #[serde(skip_serializing_if = "Option::is_none", rename = "useWhen")]
    pub use_when: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none", rename = "avoidWhen")]
    pub avoid_when: Option<Vec<String>>,

    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Configuration for an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub name: String,
    pub description: String,
    pub prompt: String,
    pub tools: Vec<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelType>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<AgentPromptMetadata>,
}

impl AgentConfig {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            prompt: String::new(),
            tools: Vec::new(),
            model: None,
            metadata: None,
        }
    }

    pub fn with_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.prompt = prompt.into();
        self
    }

    pub fn with_tools(mut self, tools: Vec<String>) -> Self {
        self.tools = tools;
        self
    }

    pub fn with_model(mut self, model: ModelType) -> Self {
        self.model = Some(model);
        self
    }

    pub fn with_metadata(mut self, metadata: AgentPromptMetadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    pub fn with_category(mut self, category: AgentCategory) -> Self {
        let mut metadata = self.metadata.unwrap_or_else(|| AgentPromptMetadata {
            category: None,
            triggers: None,
            use_when: None,
            avoid_when: None,
            extra: HashMap::new(),
        });
        metadata.category = Some(category);
        self.metadata = Some(metadata);
        self
    }
}

/// Prompt section type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptSection {
    Header,
    AgentRegistry,
    Triggers,
    ToolSelection,
    DelegationMatrix,
    Principles,
    Workflow,
    CriticalRules,
    CompletionChecklist,
}

impl PromptSection {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Header => "header",
            Self::AgentRegistry => "agent_registry",
            Self::Triggers => "triggers",
            Self::ToolSelection => "tool_selection",
            Self::DelegationMatrix => "delegation_matrix",
            Self::Principles => "principles",
            Self::Workflow => "workflow",
            Self::CriticalRules => "critical_rules",
            Self::CompletionChecklist => "completion_checklist",
        }
    }
}

/// Options for controlling what sections are included in generated prompt
#[derive(Debug, Clone)]
pub struct GeneratorOptions {
    pub include_agents: bool,
    pub include_triggers: bool,
    pub include_tools: bool,
    pub include_delegation_table: bool,
    pub include_principles: bool,
    pub include_workflow: bool,
    pub include_rules: bool,
    pub include_checklist: bool,
}

impl Default for GeneratorOptions {
    fn default() -> Self {
        Self {
            include_agents: true,
            include_triggers: true,
            include_tools: true,
            include_delegation_table: true,
            include_principles: true,
            include_workflow: true,
            include_rules: true,
            include_checklist: true,
        }
    }
}

impl GeneratorOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn all_enabled() -> Self {
        Self::default()
    }

    pub fn minimal() -> Self {
        Self {
            include_agents: true,
            include_triggers: false,
            include_tools: false,
            include_delegation_table: false,
            include_principles: false,
            include_workflow: false,
            include_rules: false,
            include_checklist: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_category_as_str() {
        assert_eq!(AgentCategory::Exploration.as_str(), "exploration");
        assert_eq!(AgentCategory::Implementation.as_str(), "implementation");
    }

    #[test]
    fn test_agent_category_capitalize() {
        assert_eq!(AgentCategory::Exploration.capitalize(), "Exploration");
        assert_eq!(AgentCategory::Quality.capitalize(), "Quality");
    }

    #[test]
    fn test_model_type_as_str() {
        assert_eq!(ModelType::Opus.as_str(), "opus");
        assert_eq!(ModelType::Sonnet.as_str(), "sonnet");
        assert_eq!(ModelType::Haiku.as_str(), "haiku");
        assert_eq!(ModelType::Inherit.as_str(), "inherit");
    }

    #[test]
    fn test_agent_config_builder() {
        let agent = AgentConfig::new("test-agent", "A test agent")
            .with_prompt("Test prompt")
            .with_tools(vec!["tool1".to_string(), "tool2".to_string()])
            .with_model(ModelType::Sonnet)
            .with_category(AgentCategory::Implementation);

        assert_eq!(agent.name, "test-agent");
        assert_eq!(agent.description, "A test agent");
        assert_eq!(agent.prompt, "Test prompt");
        assert_eq!(agent.tools.len(), 2);
        assert_eq!(agent.model, Some(ModelType::Sonnet));
        assert!(agent.metadata.is_some());
        assert_eq!(
            agent.metadata.unwrap().category,
            Some(AgentCategory::Implementation)
        );
    }

    #[test]
    fn test_generator_options_default() {
        let opts = GeneratorOptions::default();
        assert!(opts.include_agents);
        assert!(opts.include_triggers);
        assert!(opts.include_tools);
    }

    #[test]
    fn test_generator_options_minimal() {
        let opts = GeneratorOptions::minimal();
        assert!(opts.include_agents);
        assert!(!opts.include_triggers);
        assert!(!opts.include_tools);
    }

    #[test]
    fn test_prompt_section_as_str() {
        assert_eq!(PromptSection::Header.as_str(), "header");
        assert_eq!(PromptSection::AgentRegistry.as_str(), "agent_registry");
        assert_eq!(PromptSection::Workflow.as_str(), "workflow");
    }
}
