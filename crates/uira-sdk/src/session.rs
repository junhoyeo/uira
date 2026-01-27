//! Session management for Uira SDK
//!
//! Provides the main entry point for creating orchestration sessions.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::agent::{AgentDefinitionEntry, AgentDefinitions};
use crate::config::PluginConfig;
use crate::mcp::McpServerConfig;
use crate::types::{AgentState, BackgroundTask};

/// Options for creating an Uira session
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

/// Result of creating an Uira session
#[derive(Debug, Clone)]
pub struct UiraSession {
    /// Query options to pass to Claude Agent SDK
    pub query_options: QueryOptions,
    /// Session state
    pub state: SessionState,
    /// Loaded configuration
    pub config: PluginConfig,
}

impl UiraSession {
    /// Create a new Uira session
    ///
    /// This prepares all the configuration and options needed
    /// to run a query with the Claude Agent SDK.
    pub fn new(options: SessionOptions) -> Self {
        // Determine working directory
        let working_dir = options
            .working_directory
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        // Load configuration
        let config = if options.skip_config_load {
            options.config.unwrap_or_default()
        } else {
            Self::load_and_merge_config(&working_dir, options.config)
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

        // Initialize session state with context files
        let context_files = if options.skip_context_injection {
            Vec::new()
        } else {
            Self::find_context_files(&working_dir)
                .iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect()
        };

        let state = SessionState {
            session_id: None,
            active_agents: HashMap::new(),
            background_tasks: Vec::new(),
            context_files,
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
        r#"You are Uira, a multi-agent orchestration system.

You are the orchestrator that coordinates specialized agents to accomplish complex tasks.
You have access to specialized agents that you can delegate tasks to using the Task tool.

## Available Agents

- **explore**: Fast codebase search using grep, glob, and LSP
- **architect**: Architecture and debugging advisor
- **executor**: Focused task executor for implementing changes

## Your Role

1. Analyze user requests and break them into subtasks
2. Delegate subtasks to appropriate specialized agents
3. Coordinate agent outputs to produce final results
4. Ensure all work is verified and complete before finishing

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

    /// Load and merge configuration from files
    fn load_and_merge_config(
        working_dir: &Path,
        options_config: Option<PluginConfig>,
    ) -> PluginConfig {
        // Try to load config from standard locations
        let loaded_config = Self::find_and_load_config(working_dir);

        // Merge loaded config with options (options take precedence)
        match (loaded_config, options_config) {
            (Some(loaded), Some(opts)) => Self::merge_configs(loaded, opts),
            (Some(loaded), None) => loaded,
            (None, Some(opts)) => opts,
            (None, None) => PluginConfig::default(),
        }
    }

    /// Find and load configuration from standard locations
    fn find_and_load_config(working_dir: &Path) -> Option<PluginConfig> {
        let config_paths = Self::find_config_files(working_dir);

        for path in config_paths {
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    // Try YAML first
                    if let Ok(config) = serde_yaml::from_str::<PluginConfig>(&content) {
                        return Some(config);
                    }
                    // Try JSON
                    if let Ok(config) = serde_json::from_str::<PluginConfig>(&content) {
                        return Some(config);
                    }

                    // Try loading as UiraConfig and convert
                    if let Ok(uira_config) =
                        serde_yaml::from_str::<uira_config::UiraConfig>(&content)
                    {
                        return Some(Self::convert_uira_to_plugin_config(&uira_config));
                    }
                    if let Ok(uira_config) =
                        serde_json::from_str::<uira_config::UiraConfig>(&content)
                    {
                        return Some(Self::convert_uira_to_plugin_config(&uira_config));
                    }
                }
            }
        }

        None
    }

    /// Convert UiraConfig to PluginConfig
    fn convert_uira_to_plugin_config(uira: &uira_config::UiraConfig) -> PluginConfig {
        use crate::config::*;

        // Convert agent settings
        let agents = if !uira.agents.agents.is_empty() {
            Some(AgentsConfig::default())
        } else {
            None
        };

        // Convert MCP servers
        let mcp_servers = if !uira.mcp.servers.is_empty() {
            let mut config = McpServersConfig::default();

            // Check for specific known servers
            if let Some(exa) = uira.mcp.servers.get("exa") {
                config.exa = Some(ExaConfig {
                    enabled: Some(true),
                    api_key: exa.env.get("EXA_API_KEY").cloned(),
                });
            }

            if uira.mcp.servers.contains_key("context7") {
                config.context7 = Some(Context7Config {
                    enabled: Some(true),
                });
            }

            Some(config)
        } else {
            None
        };

        PluginConfig {
            agents,
            features: Some(FeaturesConfig {
                parallel_execution: Some(true),
                lsp_tools: Some(false),
                ast_tools: Some(false),
                continuation_enforcement: Some(true),
                auto_context_injection: Some(true),
            }),
            mcp_servers,
            permissions: Some(PermissionsConfig {
                allow_bash: Some(true),
                allow_edit: Some(true),
                allow_write: Some(true),
                max_background_tasks: Some(5),
            }),
            magic_keywords: None,
            routing: None,
        }
    }

    /// Find configuration files in standard locations
    fn find_config_files(working_dir: &Path) -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // Local config files (in working directory)
        let local_candidates = vec![
            "uira.yaml",
            "uira.yml",
            "uira.json",
            ".uira.yaml",
            ".uira.yml",
        ];

        for candidate in &local_candidates {
            let path = working_dir.join(candidate);
            if path.exists() {
                paths.push(path);
            }
        }

        // User config files (in home directory)
        if let Ok(home) = std::env::var("HOME") {
            let home_path = PathBuf::from(home);

            // ~/.uira/config.yaml
            let uira_config = home_path.join(".uira").join("config.yaml");
            if uira_config.exists() {
                paths.push(uira_config);
            }

            // ~/.config/uira/config.yaml
            let xdg_config = home_path
                .join(".config")
                .join("uira")
                .join("config.yaml");
            if xdg_config.exists() {
                paths.push(xdg_config);
            }
        }

        paths
    }

    /// Find context files (CLAUDE.md, AGENTS.md, etc.)
    fn find_context_files(working_dir: &Path) -> Vec<PathBuf> {
        let mut context_files = Vec::new();

        let candidates = vec![
            "CLAUDE.md",
            "AGENTS.md",
            ".claude/CLAUDE.md",
            ".claude/AGENTS.md",
        ];

        for candidate in &candidates {
            let path = working_dir.join(candidate);
            if path.exists() && path.is_file() {
                context_files.push(path);
            }
        }

        context_files
    }

    /// Merge two plugin configurations (opts takes precedence)
    fn merge_configs(base: PluginConfig, opts: PluginConfig) -> PluginConfig {
        PluginConfig {
            agents: opts.agents.or(base.agents),
            features: opts.features.or(base.features),
            mcp_servers: opts.mcp_servers.or(base.mcp_servers),
            permissions: opts.permissions.or(base.permissions),
            magic_keywords: opts.magic_keywords.or(base.magic_keywords),
            routing: opts.routing.or(base.routing),
        }
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
        let lowercase = prompt.to_lowercase();

        // Define magic keywords and their activation messages
        let keywords = &[
            ("autopilot", "[AUTOPILOT MODE ACTIVATED]"),
            ("ultrawork", "[ULTRAWORK MODE ACTIVATED]"),
            ("ulw", "[ULTRAWORK MODE ACTIVATED]"),
            ("ralph", "[RALPH MODE ACTIVATED]"),
            ("plan", "[PLANNING MODE]"),
            ("ecomode", "[ECOMODE ACTIVATED]"),
            ("eco", "[ECOMODE ACTIVATED]"),
            ("ultrapilot", "[ULTRAPILOT MODE ACTIVATED]"),
            ("ralplan", "[RALPLAN MODE ACTIVATED]"),
        ];

        // Check for magic keywords in the prompt
        for (keyword, prefix) in keywords {
            if lowercase.contains(keyword) {
                // Check config to see if this keyword is enabled
                let is_enabled = match *keyword {
                    "ultrawork" | "ulw" => {
                        self.config
                            .magic_keywords
                            .as_ref()
                            .and_then(|k| k.ultrawork.as_ref())
                            .map(|keywords| {
                                keywords
                                    .iter()
                                    .any(|k| lowercase.contains(&k.to_lowercase()))
                            })
                            .unwrap_or(true) // Default enabled
                    }
                    "plan" => {
                        self.config
                            .magic_keywords
                            .as_ref()
                            .and_then(|k| k.search.as_ref())
                            .map(|keywords| {
                                keywords
                                    .iter()
                                    .any(|k| lowercase.contains(&k.to_lowercase()))
                            })
                            .unwrap_or(true) // Default enabled
                    }
                    "analyze" => {
                        self.config
                            .magic_keywords
                            .as_ref()
                            .and_then(|k| k.analyze.as_ref())
                            .map(|keywords| {
                                keywords
                                    .iter()
                                    .any(|k| lowercase.contains(&k.to_lowercase()))
                            })
                            .unwrap_or(true) // Default enabled
                    }
                    _ => true, // Other keywords always enabled
                };

                if is_enabled {
                    return format!("{}\n\n{}", prefix, prompt);
                }
            }
        }

        // No magic keywords detected, return original prompt
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
}

/// Create a new Uira session (convenience function)
pub fn create_uira_session(options: Option<SessionOptions>) -> UiraSession {
    UiraSession::new(options.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_session_default() {
        let session = create_uira_session(None);
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
        let session = create_uira_session(Some(options));
        assert!(session
            .query_options
            .system_prompt
            .contains("Always be helpful"));
    }

    #[test]
    fn test_detect_keywords() {
        let session = create_uira_session(None);
        let keywords = session.detect_keywords("ultrawork: fix the bug");
        assert!(keywords.contains(&"ultrawork".to_string()));
    }
}
