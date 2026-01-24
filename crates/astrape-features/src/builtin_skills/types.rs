use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for MCP server integration with a skill
pub type SkillMcpConfig = HashMap<String, McpServerConfig>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
}

/// A builtin skill definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuiltinSkill {
    /// Unique skill name
    pub name: String,
    /// Short description of the skill
    pub description: String,
    /// Full template content for the skill
    pub template: String,
    /// License information (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// Compatibility notes (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compatibility: Option<String>,
    /// Additional metadata (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    /// Allowed tools for this skill (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,
    /// Agent to use with this skill (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// Model to use with this skill (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Whether this is a subtask skill (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtask: Option<bool>,
    /// Hint for arguments (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub argument_hint: Option<String>,
    /// MCP server configuration (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_config: Option<SkillMcpConfig>,
}

/// Skill registry for runtime access
pub trait SkillRegistry {
    /// Get all registered skills
    fn get_all(&self) -> Vec<BuiltinSkill>;
    /// Get a skill by name
    fn get(&self, name: &str) -> Option<BuiltinSkill>;
    /// Register a new skill
    fn register(&mut self, skill: BuiltinSkill);
    /// Check if a skill exists
    fn has(&self, name: &str) -> bool;
}
