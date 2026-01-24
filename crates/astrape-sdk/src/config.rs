//! Plugin configuration types for Astrape SDK
//!
//! Mirrors oh-my-claudecode/src/shared/types.ts PluginConfig

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::types::RoutingTier;

/// Main plugin configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginConfig {
    /// Agent model overrides
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agents: Option<AgentsConfig>,

    /// Feature toggles
    #[serde(skip_serializing_if = "Option::is_none")]
    pub features: Option<FeaturesConfig>,

    /// MCP server configurations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<McpServersConfig>,

    /// Permission settings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<PermissionsConfig>,

    /// Magic keyword customization
    #[serde(skip_serializing_if = "Option::is_none")]
    pub magic_keywords: Option<MagicKeywordsConfig>,

    /// Intelligent model routing configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub routing: Option<RoutingConfig>,
}

/// Agent model overrides
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub omc: Option<AgentModelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub architect: Option<AgentModelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub researcher: Option<AgentModelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explore: Option<AgentModelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frontend_engineer: Option<AgentModelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_writer: Option<AgentModelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multimodal_looker: Option<AgentModelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub critic: Option<AgentModelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analyst: Option<AgentModelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orchestrator_sisyphus: Option<AgentModelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sisyphus_junior: Option<AgentModelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub planner: Option<AgentModelConfig>,
}

/// Single agent model configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentModelConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// Feature toggles
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FeaturesConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_execution: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lsp_tools: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ast_tools: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub continuation_enforcement: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_context_injection: Option<bool>,
}

/// MCP server configurations
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpServersConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exa: Option<ExaConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context7: Option<Context7Config>,
}

/// Exa search configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExaConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

/// Context7 configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Context7Config {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// Permission settings
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PermissionsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_bash: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_edit: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_write: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_background_tasks: Option<u32>,
}

/// Magic keyword customization
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MagicKeywordsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ultrawork: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analyze: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ultrathink: Option<Vec<String>>,
}

/// Intelligent model routing configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RoutingConfig {
    /// Enable intelligent model routing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Default tier when no rules match
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_tier: Option<RoutingTier>,
    /// Enable automatic escalation on failure
    #[serde(skip_serializing_if = "Option::is_none")]
    pub escalation_enabled: Option<bool>,
    /// Maximum escalation attempts
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_escalations: Option<u32>,
    /// Model mapping per tier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier_models: Option<TierModelsConfig>,
    /// Agent-specific tier overrides
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_overrides: Option<HashMap<String, AgentTierOverride>>,
    /// Keywords that force escalation to higher tier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub escalation_keywords: Option<Vec<String>>,
    /// Keywords that suggest lower tier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub simplification_keywords: Option<Vec<String>>,
}

/// Model mapping per tier
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TierModelsConfig {
    #[serde(rename = "LOW", skip_serializing_if = "Option::is_none")]
    pub low: Option<String>,
    #[serde(rename = "MEDIUM", skip_serializing_if = "Option::is_none")]
    pub medium: Option<String>,
    #[serde(rename = "HIGH", skip_serializing_if = "Option::is_none")]
    pub high: Option<String>,
}

/// Agent-specific tier override
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTierOverride {
    pub tier: RoutingTier,
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_config_default() {
        let config = PluginConfig::default();
        assert!(config.agents.is_none());
        assert!(config.features.is_none());
    }

    #[test]
    fn test_plugin_config_serialize() {
        let config = PluginConfig {
            features: Some(FeaturesConfig {
                parallel_execution: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        };

        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("parallel_execution"));
    }
}
