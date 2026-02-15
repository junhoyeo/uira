//! Recursive agent executor for subagent delegation

use async_trait::async_trait;
use std::sync::Arc;
use uira_orchestration::AgentExecutor;
use uira_providers::{ModelClient, ModelClientBuilder, ProviderConfig};
use uira_types::Provider;

use crate::{Agent, AgentConfig};

const DEFAULT_MAX_DEPTH: usize = 3;

#[derive(Clone)]
pub struct ExecutorConfig {
    pub provider_config: ProviderConfig,
    pub agent_config: AgentConfig,
    pub max_depth: usize,
}

impl ExecutorConfig {
    pub fn new(provider_config: ProviderConfig, agent_config: AgentConfig) -> Self {
        Self {
            provider_config,
            agent_config,
            max_depth: DEFAULT_MAX_DEPTH,
        }
    }

    pub fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = max_depth;
        self
    }
}

pub struct RecursiveAgentExecutor {
    config: ExecutorConfig,
    current_depth: usize,
}

impl RecursiveAgentExecutor {
    pub fn new(config: ExecutorConfig) -> Self {
        Self {
            config,
            current_depth: 0,
        }
    }

    fn child_executor(&self) -> Self {
        Self {
            config: self.config.clone(),
            current_depth: self.current_depth + 1,
        }
    }

    fn create_client(&self, model: &str) -> Result<Arc<dyn ModelClient>, String> {
        let (provider, model_name) = parse_model_string(model);

        let mut config = self.config.provider_config.clone();
        config.provider = provider;
        config.model = model_name.clone();

        tracing::debug!(
            "Creating subagent client: provider={:?}, model={}, original_provider={:?}",
            provider,
            model_name,
            self.config.provider_config.provider
        );

        ModelClientBuilder::new()
            .with_config(config)
            .build()
            .map_err(|e| format!("Failed to create model client: {}", e))
    }
}

#[async_trait]
impl AgentExecutor for RecursiveAgentExecutor {
    async fn execute(
        &self,
        prompt: &str,
        model: &str,
        _allowed_tools: Option<Vec<String>>,
        max_turns: Option<usize>,
    ) -> Result<String, String> {
        if self.current_depth >= self.config.max_depth {
            return Err(format!(
                "Maximum delegation depth ({}) exceeded. Current depth: {}",
                self.config.max_depth, self.current_depth
            ));
        }

        let client = self.create_client(model)?;

        let mut agent_config = self.config.agent_config.clone();
        agent_config.require_approval_for_writes = false;
        agent_config.require_approval_for_commands = false;

        if let Some(turns) = max_turns {
            agent_config.max_turns = turns;
        }

        let child_executor = Arc::new(self.child_executor());
        let mut agent = Agent::new_with_executor(agent_config, client, Some(child_executor));

        let result = agent
            .run(prompt)
            .await
            .map_err(|e| format!("Subagent execution failed: {}", e))?;

        if result.success {
            Ok(if result.output.is_empty() {
                "[Subagent completed with no output]\n\n\
                 The delegated task finished successfully but produced no text output.\n\
                 This is the FINAL result - do NOT retry or call delegate_task again.\n\
                 Report to the user that the task completed."
                    .to_string()
            } else {
                result.output
            })
        } else {
            Err(format!(
                "Subagent failed: {}",
                result
                    .error
                    .map(|e| e.to_string())
                    .unwrap_or_else(|| "Unknown error".to_string())
            ))
        }
    }
}

fn parse_model_string(model: &str) -> (Provider, String) {
    if let Some((provider_str, model_name)) = model.split_once('/') {
        let provider = match provider_str.to_lowercase().as_str() {
            "anthropic" => Provider::Anthropic,
            "openai" => Provider::OpenAI,
            "google" => Provider::Google,
            "ollama" => Provider::Ollama,
            "opencode" => Provider::OpenCode,
            "openrouter" => Provider::OpenRouter,
            _ => Provider::Custom,
        };
        (provider, model_name.to_string())
    } else {
        let provider = if model.starts_with("claude") {
            Provider::Anthropic
        } else if model.starts_with("gpt") || model.starts_with("o1") {
            Provider::OpenAI
        } else if model.starts_with("gemini") {
            Provider::Google
        } else {
            Provider::Anthropic
        };
        (provider, model.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_model_string() {
        let (provider, model) = parse_model_string("anthropic/claude-sonnet-4-20250514");
        assert_eq!(provider, Provider::Anthropic);
        assert_eq!(model, "claude-sonnet-4-20250514");

        let (provider, model) = parse_model_string("openai/gpt-4o");
        assert_eq!(provider, Provider::OpenAI);
        assert_eq!(model, "gpt-4o");

        let (provider, model) = parse_model_string("claude-sonnet-4-20250514");
        assert_eq!(provider, Provider::Anthropic);
        assert_eq!(model, "claude-sonnet-4-20250514");

        let (provider, model) = parse_model_string("gpt-4o");
        assert_eq!(provider, Provider::OpenAI);
        assert_eq!(model, "gpt-4o");
    }
}
