//! Session management for Astrape SDK
//!
//! Provides the main entry point for creating orchestration sessions,
//! mirroring oh-my-claudecode's createSisyphusSession function.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::agent::{AgentDefinitionEntry, AgentDefinitions};
use crate::bridge::{
    AgentDef, BridgeQueryOptions, McpServerDef, QueryParams, SdkBridge, StreamMessage,
};
use crate::config::PluginConfig;
use crate::error::SdkResult;
use crate::mcp::McpServerConfig;
use crate::types::{AgentState, BackgroundTask};

/// Options for creating an Astrape session
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionOptions {
    /// Custom configuration (merged with loaded config)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<PluginConfig>,
    /// Working directory (default: current directory)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    /// Skip loading config files
    #[serde(default)]
    pub skip_config_load: bool,
    /// Skip context file injection
    #[serde(default)]
    pub skip_context_injection: bool,
    /// Custom system prompt addition
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_system_prompt: Option<String>,
    /// API key (default: from ANTHROPIC_API_KEY env)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

/// Session state tracking
#[derive(Debug, Clone, Default)]
pub struct SessionState {
    /// Session identifier
    pub session_id: Option<String>,
    /// Active agents and their states
    pub active_agents: HashMap<String, AgentState>,
    /// Background tasks
    pub background_tasks: Vec<BackgroundTask>,
    /// Context files found in working directory
    pub context_files: Vec<String>,
}

/// Query options to pass to Claude Agent SDK
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryOptions {
    /// System prompt for the orchestrator
    pub system_prompt: String,
    /// Agent definitions
    pub agents: AgentDefinitions,
    /// MCP server configurations
    pub mcp_servers: HashMap<String, McpServerConfig>,
    /// Allowed tools
    pub allowed_tools: Vec<String>,
    /// Permission mode
    pub permission_mode: String,
}

/// Result of creating an Astrape session
/// Mirrors oh-my-claudecode's SisyphusSession
#[derive(Debug, Clone)]
pub struct AstrapeSession {
    /// Query options to pass to Claude Agent SDK
    pub query_options: QueryOptions,
    /// Session state
    pub state: SessionState,
    /// Loaded configuration
    pub config: PluginConfig,
}

impl AstrapeSession {
    /// Create a new Astrape session
    ///
    /// This prepares all the configuration and options needed
    /// to run a query with the Claude Agent SDK.
    pub fn new(options: SessionOptions) -> Self {
        // Load configuration
        let config = if options.skip_config_load {
            options.config.unwrap_or_default()
        } else {
            // TODO: Load from files and merge with options.config
            options.config.unwrap_or_default()
        };

        // Build system prompt
        let mut system_prompt = Self::get_default_system_prompt();

        // Add continuation enforcement
        if config
            .features
            .as_ref()
            .and_then(|f| f.continuation_enforcement)
            .unwrap_or(true)
        {
            system_prompt.push_str(&Self::get_continuation_prompt());
        }

        // Add custom system prompt
        if let Some(custom) = options.custom_system_prompt {
            system_prompt.push_str("\n\n## Custom Instructions\n\n");
            system_prompt.push_str(&custom);
        }

        // Get agent definitions
        let agents = Self::get_default_agent_definitions();

        // Build MCP servers configuration
        let mcp_servers = Self::get_default_mcp_servers(&config);

        // Build allowed tools list
        let mut allowed_tools = vec![
            "Read".to_string(),
            "Glob".to_string(),
            "Grep".to_string(),
            "WebSearch".to_string(),
            "WebFetch".to_string(),
            "Task".to_string(),
            "TodoWrite".to_string(),
        ];

        if config
            .permissions
            .as_ref()
            .and_then(|p| p.allow_bash)
            .unwrap_or(true)
        {
            allowed_tools.push("Bash".to_string());
        }

        if config
            .permissions
            .as_ref()
            .and_then(|p| p.allow_edit)
            .unwrap_or(true)
        {
            allowed_tools.push("Edit".to_string());
        }

        if config
            .permissions
            .as_ref()
            .and_then(|p| p.allow_write)
            .unwrap_or(true)
        {
            allowed_tools.push("Write".to_string());
        }

        // Add MCP tool names
        for server_name in mcp_servers.keys() {
            allowed_tools.push(format!("mcp__{}__*", server_name));
        }

        // Initialize session state
        let state = SessionState {
            session_id: None,
            active_agents: HashMap::new(),
            background_tasks: Vec::new(),
            context_files: Vec::new(), // TODO: Find context files
        };

        let query_options = QueryOptions {
            system_prompt,
            agents,
            mcp_servers,
            allowed_tools,
            permission_mode: "acceptEdits".to_string(),
        };

        Self {
            query_options,
            state,
            config,
        }
    }

    /// Get the default system prompt
    fn get_default_system_prompt() -> String {
        // TODO: Load from embedded file
        r#"You are Astrape, a multi-agent orchestration system.

You have access to specialized agents that you can delegate tasks to.
Use the Task tool to invoke agents for specific purposes."#
            .to_string()
    }

    /// Get the continuation enforcement prompt addition
    fn get_continuation_prompt() -> String {
        r#"

## Continuation Enforcement

You MUST complete all tasks before stopping. If you have pending todos or incomplete work,
continue working until everything is done."#
            .to_string()
    }

    /// Get default agent definitions
    fn get_default_agent_definitions() -> AgentDefinitions {
        let mut agents = HashMap::new();

        // Explore agent
        agents.insert(
            "explore".to_string(),
            AgentDefinitionEntry {
                description: "Fast codebase search using grep, glob, and LSP".to_string(),
                prompt: "You are an explore agent. Search the codebase efficiently.".to_string(),
                tools: vec!["Read".to_string(), "Glob".to_string(), "Grep".to_string()],
                model: Some("haiku".to_string()),
            },
        );

        // Architect agent
        agents.insert(
            "architect".to_string(),
            AgentDefinitionEntry {
                description: "Architecture and debugging advisor".to_string(),
                prompt: "You are an architect agent. Provide strategic guidance.".to_string(),
                tools: vec![
                    "Read".to_string(),
                    "Glob".to_string(),
                    "Grep".to_string(),
                    "WebSearch".to_string(),
                ],
                model: Some("opus".to_string()),
            },
        );

        // Executor agent
        agents.insert(
            "executor".to_string(),
            AgentDefinitionEntry {
                description: "Focused task executor".to_string(),
                prompt: "You are an executor agent. Implement changes directly.".to_string(),
                tools: vec![
                    "Read".to_string(),
                    "Glob".to_string(),
                    "Grep".to_string(),
                    "Edit".to_string(),
                    "Write".to_string(),
                    "Bash".to_string(),
                ],
                model: Some("sonnet".to_string()),
            },
        );

        agents
    }

    /// Get default MCP servers configuration
    fn get_default_mcp_servers(config: &PluginConfig) -> HashMap<String, McpServerConfig> {
        let mut servers = HashMap::new();

        // Context7 (always enabled by default)
        if config
            .mcp_servers
            .as_ref()
            .and_then(|m| m.context7.as_ref())
            .and_then(|c| c.enabled)
            .unwrap_or(true)
        {
            servers.insert(
                "context7".to_string(),
                McpServerConfig {
                    command: "npx".to_string(),
                    args: vec!["-y".to_string(), "@upstash/context7-mcp".to_string()],
                    env: None,
                },
            );
        }

        // Exa (if API key provided)
        if let Some(api_key) = config
            .mcp_servers
            .as_ref()
            .and_then(|m| m.exa.as_ref())
            .and_then(|e| e.api_key.clone())
        {
            let mut env = HashMap::new();
            env.insert("EXA_API_KEY".to_string(), api_key);

            servers.insert(
                "exa".to_string(),
                McpServerConfig {
                    command: "npx".to_string(),
                    args: vec!["-y".to_string(), "exa-mcp-server".to_string()],
                    env: Some(env),
                },
            );
        }

        servers
    }

    /// Process a prompt (applies magic keywords)
    pub fn process_prompt(&self, prompt: &str) -> String {
        // TODO: Implement magic keyword processing
        prompt.to_string()
    }

    /// Detect magic keywords in a prompt
    pub fn detect_keywords(&self, prompt: &str) -> Vec<String> {
        let mut keywords = Vec::new();

        let prompt_lower = prompt.to_lowercase();
        if prompt_lower.contains("ultrawork") || prompt_lower.contains("ulw") {
            keywords.push("ultrawork".to_string());
        }
        if prompt_lower.contains("ralph") {
            keywords.push("ralph".to_string());
        }
        if prompt_lower.contains("plan") {
            keywords.push("plan".to_string());
        }

        keywords
    }

    /// Convert session QueryOptions to BridgeQueryOptions
    pub fn to_bridge_options(&self) -> BridgeQueryOptions {
        let agents: HashMap<String, AgentDef> = self
            .query_options
            .agents
            .iter()
            .map(|(name, entry)| {
                (
                    name.clone(),
                    AgentDef {
                        description: entry.description.clone(),
                        prompt: entry.prompt.clone(),
                        tools: Some(entry.tools.clone()),
                        model: entry.model.clone(),
                    },
                )
            })
            .collect();

        let mcp_servers: HashMap<String, McpServerDef> = self
            .query_options
            .mcp_servers
            .iter()
            .map(|(name, config)| {
                (
                    name.clone(),
                    McpServerDef {
                        command: config.command.clone(),
                        args: config.args.clone(),
                        env: config.env.clone(),
                    },
                )
            })
            .collect();

        BridgeQueryOptions {
            system_prompt: Some(self.query_options.system_prompt.clone()),
            agents: Some(agents),
            mcp_servers: Some(mcp_servers),
            allowed_tools: Some(self.query_options.allowed_tools.clone()),
            permission_mode: Some(self.query_options.permission_mode.clone()),
        }
    }

    /// Create query params for a prompt
    pub fn create_query_params(&self, prompt: &str) -> QueryParams {
        let processed = self.process_prompt(prompt);
        QueryParams {
            prompt: processed,
            options: Some(self.to_bridge_options()),
        }
    }

    /// Start a query using the TypeScript bridge
    pub fn query(
        &self,
        bridge: &mut SdkBridge,
        prompt: &str,
    ) -> SdkResult<tokio::sync::mpsc::Receiver<SdkResult<StreamMessage>>> {
        let params = self.create_query_params(prompt);
        bridge.query(params)
    }
}

/// Create a new Astrape session (convenience function)
pub fn create_astrape_session(options: Option<SessionOptions>) -> AstrapeSession {
    AstrapeSession::new(options.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_session_default() {
        let session = create_astrape_session(None);
        assert!(!session.query_options.system_prompt.is_empty());
        assert!(!session.query_options.agents.is_empty());
        assert!(session
            .query_options
            .allowed_tools
            .contains(&"Read".to_string()));
    }

    #[test]
    fn test_session_with_custom_prompt() {
        let options = SessionOptions {
            custom_system_prompt: Some("Always be helpful.".to_string()),
            ..Default::default()
        };
        let session = create_astrape_session(Some(options));
        assert!(session
            .query_options
            .system_prompt
            .contains("Always be helpful"));
    }

    #[test]
    fn test_detect_keywords() {
        let session = create_astrape_session(None);
        let keywords = session.detect_keywords("ultrawork: fix the bug");
        assert!(keywords.contains(&"ultrawork".to_string()));
    }
}
