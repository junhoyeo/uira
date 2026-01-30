use std::collections::HashMap;

use crate::types::{AgentConfig, AgentOverrides, ModelType};
use crate::config::apply_overrides;
use crate::prompt_loader::PromptLoader;
use crate::tool_restrictions::ToolRestrictionsRegistry;

/// Model configuration for agents (maps agent name to model ID string)
pub type AgentModelConfig = HashMap<String, String>;

/// Returns all agent definitions with their configurations.
///
/// This builds the full agent config map using embedded prompts by default.
pub fn get_agent_definitions(overrides: Option<&AgentOverrides>) -> HashMap<String, AgentConfig> {
    let loader = PromptLoader::from_embedded_map(crate::prompts::EMBEDDED_PROMPTS);
    get_agent_definitions_with_loader(&loader, overrides, None)
}

/// Returns agent definitions with model config for dynamic model display.
///
/// When `model_config` is provided, the actual configured model will be
/// appended to each agent's description.
pub fn get_agent_definitions_with_config(
    overrides: Option<&AgentOverrides>,
    model_config: Option<&AgentModelConfig>,
) -> HashMap<String, AgentConfig> {
    let loader = PromptLoader::from_embedded_map(crate::prompts::EMBEDDED_PROMPTS);
    get_agent_definitions_with_loader(&loader, overrides, model_config)
}

pub fn get_agent_definitions_with_loader(
    prompt_loader: &PromptLoader,
    overrides: Option<&AgentOverrides>,
    model_config: Option<&AgentModelConfig>,
) -> HashMap<String, AgentConfig> {
    let tools = ToolRestrictionsRegistry::with_default_allowlists();
    let mut agents = HashMap::<String, AgentConfig>::new();

    // Primary agents (name, base_description, default_model)
    let primary_agents: Vec<(&str, &str, ModelType)> = vec![
        (
            "architect",
            "Architecture & Debugging Advisor. Use for complex problems.",
            ModelType::Opus,
        ),
        (
            "librarian",
            "Open-source codebase understanding agent for multi-repository analysis, searching remote codebases, and retrieving official documentation.",
            ModelType::Sonnet,
        ),
        (
            "explore",
            "Fast codebase pattern matching.",
            ModelType::Haiku,
        ),
        ("designer", "UI/UX specialist.", ModelType::Sonnet),
        ("writer", "Technical writing specialist.", ModelType::Haiku),
        ("vision", "Visual analysis specialist.", ModelType::Sonnet),
        ("critic", "Plan/work reviewer.", ModelType::Opus),
        ("analyst", "Pre-planning consultant.", ModelType::Sonnet),
        (
            "executor",
            "Focused executor for implementation tasks.",
            ModelType::Sonnet,
        ),
        (
            "planner",
            "Strategic planner for comprehensive implementation plans.",
            ModelType::Opus,
        ),
        ("qa-tester", "CLI testing specialist.", ModelType::Opus),
        ("scientist", "Data/ML specialist.", ModelType::Sonnet),
    ];

    for (name, base_desc, default_model) in primary_agents {
        let description =
            build_description_with_model(name, base_desc, default_model, model_config);
        insert(
            &mut agents,
            prompt_loader,
            &tools,
            name,
            &description,
            Some(default_model),
        );
    }

    // Tiered variants
    let tiered_variants: Vec<(&str, ModelType, &str)> = vec![
        (
            "architect-medium",
            ModelType::Sonnet,
            "Architecture & Debugging Advisor - Medium complexity. Use for moderate analysis.",
        ),
        (
            "architect-low",
            ModelType::Haiku,
            "Quick code questions & simple lookups. Use for simple questions that need fast answers.",
        ),
        (
            "executor-high",
            ModelType::Opus,
            "Complex task executor for multi-file changes. Use for tasks requiring deep reasoning.",
        ),
        (
            "executor-low",
            ModelType::Haiku,
            "Simple single-file task executor. Use for trivial tasks.",
        ),
        (
            "designer-low",
            ModelType::Haiku,
            "Simple styling and minor UI tweaks. Use for trivial frontend work.",
        ),
        (
            "designer-high",
            ModelType::Opus,
            "Complex UI architecture and design systems. Use for sophisticated frontend work.",
        ),
        (
            "qa-tester-high",
            ModelType::Opus,
            "Comprehensive production-ready QA testing.",
        ),
        (
            "scientist-low",
            ModelType::Haiku,
            "Quick data inspection and simple statistics. Use for fast, simple queries.",
        ),
        (
            "scientist-high",
            ModelType::Opus,
            "Complex research, hypothesis testing, and ML specialist. Use for deep analysis.",
        ),
    ];

    for (name, default_model, base_desc) in tiered_variants {
        let description =
            build_description_with_model(name, base_desc, default_model, model_config);
        insert(
            &mut agents,
            prompt_loader,
            &tools,
            name,
            &description,
            Some(default_model),
        );
    }

    // Specialized agents
    let specialized_agents: Vec<(&str, ModelType, &str)> = vec![
        (
            "security-reviewer",
            ModelType::Opus,
            "Security vulnerability detection specialist. Use for security audits and code review.",
        ),
        (
            "security-reviewer-low",
            ModelType::Haiku,
            "Quick security scan specialist. Use for fast security checks on small code changes.",
        ),
        (
            "build-fixer",
            ModelType::Sonnet,
            "Build and TypeScript error resolution specialist. Use for fixing build errors.",
        ),
        (
            "build-fixer-low",
            ModelType::Haiku,
            "Simple build error fixer. Use for trivial type errors and single-line fixes.",
        ),
        (
            "tdd-guide",
            ModelType::Sonnet,
            "Test-Driven Development specialist. Use for TDD workflows and test coverage.",
        ),
        (
            "tdd-guide-low",
            ModelType::Haiku,
            "Quick test suggestion specialist. Use for simple test case ideas.",
        ),
        (
            "code-reviewer",
            ModelType::Opus,
            "Expert code review specialist. Use for comprehensive code quality review.",
        ),
        (
            "code-reviewer-low",
            ModelType::Haiku,
            "Quick code quality checker. Use for fast review of small changes.",
        ),
    ];

    for (name, default_model, base_desc) in specialized_agents {
        let description =
            build_description_with_model(name, base_desc, default_model, model_config);
        insert(
            &mut agents,
            prompt_loader,
            &tools,
            name,
            &description,
            Some(default_model),
        );
    }

    apply_overrides(agents, overrides)
}

/// Build description with actual model as prefix (so it doesn't get truncated in UI).
/// If model_config has an override for this agent, use that; otherwise use the default model.
fn build_description_with_model(
    agent_name: &str,
    base_description: &str,
    default_model: ModelType,
    model_config: Option<&AgentModelConfig>,
) -> String {
    let model_str = match model_config.and_then(|cfg| cfg.get(agent_name)) {
        Some(configured_model) => configured_model.clone(),
        None => default_model.as_str().to_string(),
    };
    format!("[{}] {}", model_str, base_description)
}

fn insert(
    agents: &mut HashMap<String, AgentConfig>,
    loader: &PromptLoader,
    tool_registry: &ToolRestrictionsRegistry,
    name: &str,
    description: &str,
    model: Option<ModelType>,
) {
    let mut cfg = AgentConfig {
        name: name.to_string(),
        description: description.to_string(),
        prompt: loader.load(name),
        tools: tool_registry
            .get(name)
            .map(|r| {
                // We don't store allowlists separately; convert it back to tool names
                // by intersecting with what the registry would allow.
                // Since ToolRestrictions is internal, we just use the known defaults
                // by applying it onto the broadest tool list.
                let tools = vec![
                    "Read".to_string(),
                    "Glob".to_string(),
                    "Grep".to_string(),
                    "Edit".to_string(),
                    "Write".to_string(),
                    "Bash".to_string(),
                    "TodoWrite".to_string(),
                    "WebSearch".to_string(),
                    "WebFetch".to_string(),
                    "python_repl".to_string(),
                ];
                let mut tmp = AgentConfig {
                    name: name.to_string(),
                    description: String::new(),
                    prompt: String::new(),
                    tools,
                    model: None,
                    default_model: None,
                    metadata: None,
                };
                r.apply_to_config(&mut tmp);
                tmp.tools
            })
            .unwrap_or_default(),
        model,
        default_model: model,
        metadata: None,
    };

    tool_registry.apply(&mut cfg);
    agents.insert(name.to_string(), cfg);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompt_loader::PromptLoader;
    use tempfile::tempdir;

    #[test]
    fn returns_all_known_agents() {
        let tmp = tempdir().unwrap();
        let loader = PromptLoader::from_fs(tmp.path());
        let defs = get_agent_definitions_with_loader(&loader, None, None);

        // Spot-check a few keys.
        assert!(defs.contains_key("architect"));
        assert!(defs.contains_key("executor"));
        assert!(defs.contains_key("executor-high"));
        assert!(defs.contains_key("code-reviewer-low"));
    }

    #[test]
    fn model_config_overrides_description() {
        let tmp = tempdir().unwrap();
        let loader = PromptLoader::from_fs(tmp.path());

        let mut model_config = AgentModelConfig::new();
        model_config.insert("explore".to_string(), "opencode/gpt-5-nano".to_string());
        model_config.insert("librarian".to_string(), "opencode/big-pickle".to_string());

        let defs = get_agent_definitions_with_loader(&loader, None, Some(&model_config));

        // explore should show the configured model
        let explore = defs.get("explore").unwrap();
        assert!(explore.description.contains("opencode/gpt-5-nano"));

        // librarian should show its configured model
        let librarian = defs.get("librarian").unwrap();
        assert!(librarian.description.contains("opencode/big-pickle"));

        // architect has no override, should show default (opus)
        let architect = defs.get("architect").unwrap();
        assert!(architect.description.contains("opus"));
    }
}
