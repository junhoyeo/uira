use std::collections::HashMap;

use astrape_sdk::{AgentConfig, AgentOverrides, ModelType};

use crate::config::apply_overrides;
use crate::prompt_loader::{default_agents_dir, PromptLoader};
use crate::tool_restrictions::ToolRestrictionsRegistry;

/// Returns all agent definitions with their configurations.
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
        "Architecture & Debugging Advisor. Use for complex problems.",
        Some(ModelType::Opus),
    );
    insert(
        &mut agents,
        prompt_loader,
        &tools,
        "librarian",
        "Open-source codebase understanding agent for multi-repository analysis, searching remote codebases, and retrieving official documentation. Model: opencode/big-pickle",
        Some(ModelType::Sonnet),
    );
    insert(
        &mut agents,
        prompt_loader,
        &tools,
        "explore",
        "Fast codebase pattern matching. Model: Haiku",
        Some(ModelType::Haiku),
    );
    insert(
        &mut agents,
        prompt_loader,
        &tools,
        "designer",
        "UI/UX specialist.",
        Some(ModelType::Sonnet),
    );
    insert(
        &mut agents,
        prompt_loader,
        &tools,
        "writer",
        "Technical writing specialist.",
        Some(ModelType::Haiku),
    );
    insert(
        &mut agents,
        prompt_loader,
        &tools,
        "vision",
        "Visual analysis specialist.",
        Some(ModelType::Sonnet),
    );
    insert(
        &mut agents,
        prompt_loader,
        &tools,
        "critic",
        "Plan/work reviewer.",
        Some(ModelType::Opus),
    );
    insert(
        &mut agents,
        prompt_loader,
        &tools,
        "analyst",
        "Pre-planning consultant.",
        Some(ModelType::Sonnet),
    );
    insert(
        &mut agents,
        prompt_loader,
        &tools,
        "executor",
        "Focused executor for implementation tasks.",
        Some(ModelType::Sonnet),
    );
    insert(
        &mut agents,
        prompt_loader,
        &tools,
        "planner",
        "Strategic planner for comprehensive implementation plans.",
        Some(ModelType::Opus),
    );
    insert(
        &mut agents,
        prompt_loader,
        &tools,
        "qa-tester",
        "CLI testing specialist.",
        Some(ModelType::Opus),
    );
    insert(
        &mut agents,
        prompt_loader,
        &tools,
        "scientist",
        "Data/ML specialist.",
        Some(ModelType::Sonnet),
    );

    // Tiered variants (in TS these are separate .md prompts)
    for (name, model, desc) in [
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
    ] {
        insert(&mut agents, prompt_loader, &tools, name, desc, Some(model));
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
