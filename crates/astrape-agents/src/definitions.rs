use std::collections::HashMap;

use astrape_sdk::{AgentConfig, AgentOverrides, ModelType};

use crate::config::apply_overrides;
use crate::prompt_loader::{default_agents_dir, PromptLoader};
use crate::tool_restrictions::ToolRestrictionsRegistry;

/// Equivalent to oh-my-claudecode's `getAgentDefinitions()`.
///
/// This builds the full agent config map. Prompts are loaded from
/// `packages/claude-plugin/agents/{name}.md` by default.
pub fn get_agent_definitions(overrides: Option<&AgentOverrides>) -> HashMap<String, AgentConfig> {
    let loader = PromptLoader::from_fs(default_agents_dir());
    get_agent_definitions_with_loader(&loader, overrides)
}

pub fn get_agent_definitions_with_loader(
    prompt_loader: &PromptLoader,
    overrides: Option<&AgentOverrides>,
) -> HashMap<String, AgentConfig> {
    let tools = ToolRestrictionsRegistry::with_default_allowlists();
    let mut agents = HashMap::<String, AgentConfig>::new();

    // Primary agents
    insert(
        &mut agents,
        prompt_loader,
        &tools,
        "architect",
        "Architecture & Debugging Advisor (Opus). Use for complex problems.",
        Some(ModelType::Opus),
    );
    insert(
        &mut agents,
        prompt_loader,
        &tools,
        "researcher",
        "Documentation and external reference finder (Sonnet).",
        Some(ModelType::Sonnet),
    );
    insert(
        &mut agents,
        prompt_loader,
        &tools,
        "explore",
        "Fast codebase pattern matching (Haiku).",
        Some(ModelType::Haiku),
    );
    insert(
        &mut agents,
        prompt_loader,
        &tools,
        "designer",
        "UI/UX specialist (Sonnet).",
        Some(ModelType::Sonnet),
    );
    insert(
        &mut agents,
        prompt_loader,
        &tools,
        "writer",
        "Technical writing specialist (Haiku).",
        Some(ModelType::Haiku),
    );
    insert(
        &mut agents,
        prompt_loader,
        &tools,
        "vision",
        "Visual analysis specialist (Sonnet).",
        Some(ModelType::Sonnet),
    );
    insert(
        &mut agents,
        prompt_loader,
        &tools,
        "critic",
        "Plan/work reviewer (Opus).",
        Some(ModelType::Opus),
    );
    insert(
        &mut agents,
        prompt_loader,
        &tools,
        "analyst",
        "Pre-planning consultant (Sonnet).",
        Some(ModelType::Sonnet),
    );
    insert(
        &mut agents,
        prompt_loader,
        &tools,
        "executor",
        "Focused executor for implementation tasks (Sonnet).",
        Some(ModelType::Sonnet),
    );
    insert(
        &mut agents,
        prompt_loader,
        &tools,
        "planner",
        "Strategic planner for comprehensive implementation plans (Opus).",
        Some(ModelType::Opus),
    );
    insert(
        &mut agents,
        prompt_loader,
        &tools,
        "qa-tester",
        "CLI testing specialist (Opus).",
        Some(ModelType::Opus),
    );
    insert(
        &mut agents,
        prompt_loader,
        &tools,
        "scientist",
        "Data/ML specialist (Sonnet).",
        Some(ModelType::Sonnet),
    );

    // Tiered variants (in TS these are separate .md prompts)
    for (name, model, desc) in [
        (
            "architect-medium",
            ModelType::Sonnet,
            "Architecture & Debugging Advisor - Medium complexity (Sonnet). Use for moderate analysis.",
        ),
        (
            "architect-low",
            ModelType::Haiku,
            "Quick code questions & simple lookups (Haiku). Use for simple questions that need fast answers.",
        ),
        (
            "executor-high",
            ModelType::Opus,
            "Complex task executor for multi-file changes (Opus). Use for tasks requiring deep reasoning.",
        ),
        (
            "executor-low",
            ModelType::Haiku,
            "Simple single-file task executor (Haiku). Use for trivial tasks.",
        ),
        (
            "researcher-low",
            ModelType::Haiku,
            "Quick documentation lookups (Haiku). Use for simple documentation queries.",
        ),
        (
            "designer-low",
            ModelType::Haiku,
            "Simple styling and minor UI tweaks (Haiku). Use for trivial frontend work.",
        ),
        (
            "designer-high",
            ModelType::Opus,
            "Complex UI architecture and design systems (Opus). Use for sophisticated frontend work.",
        ),
        (
            "qa-tester-high",
            ModelType::Opus,
            "Comprehensive production-ready QA testing with Opus.",
        ),
        (
            "scientist-low",
            ModelType::Haiku,
            "Quick data inspection and simple statistics (Haiku). Use for fast, simple queries.",
        ),
        (
            "scientist-high",
            ModelType::Opus,
            "Complex research, hypothesis testing, and ML specialist (Opus). Use for deep analysis.",
        ),
    ] {
        insert(
            &mut agents,
            prompt_loader,
            &tools,
            name,
            desc,
            Some(model),
        );
    }

    // Specialized agents
    for (name, model, desc) in [
        (
            "security-reviewer",
            ModelType::Opus,
            "Security vulnerability detection specialist (Opus). Use for security audits and code review.",
        ),
        (
            "security-reviewer-low",
            ModelType::Haiku,
            "Quick security scan specialist (Haiku). Use for fast security checks on small code changes.",
        ),
        (
            "build-fixer",
            ModelType::Sonnet,
            "Build and TypeScript error resolution specialist (Sonnet). Use for fixing build errors.",
        ),
        (
            "build-fixer-low",
            ModelType::Haiku,
            "Simple build error fixer (Haiku). Use for trivial type errors and single-line fixes.",
        ),
        (
            "tdd-guide",
            ModelType::Sonnet,
            "Test-Driven Development specialist (Sonnet). Use for TDD workflows and test coverage.",
        ),
        (
            "tdd-guide-low",
            ModelType::Haiku,
            "Quick test suggestion specialist (Haiku). Use for simple test case ideas.",
        ),
        (
            "code-reviewer",
            ModelType::Opus,
            "Expert code review specialist (Opus). Use for comprehensive code quality review.",
        ),
        (
            "code-reviewer-low",
            ModelType::Haiku,
            "Quick code quality checker (Haiku). Use for fast review of small changes.",
        ),
    ] {
        insert(
            &mut agents,
            prompt_loader,
            &tools,
            name,
            desc,
            Some(model),
        );
    }

    apply_overrides(agents, overrides)
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
        let defs = get_agent_definitions_with_loader(&loader, None);

        // Spot-check a few keys.
        assert!(defs.contains_key("architect"));
        assert!(defs.contains_key("executor"));
        assert!(defs.contains_key("executor-high"));
        assert!(defs.contains_key("code-reviewer-low"));
    }
}
