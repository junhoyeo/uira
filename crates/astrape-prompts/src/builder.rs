use crate::sections::*;
use crate::types::{AgentConfig, GeneratorOptions};

/// Generate complete orchestrator prompt from agent definitions
///
/// # Example
/// ```
/// use astrape_prompts::{AgentConfig, ModelType, generate_orchestrator_prompt};
///
/// let agents = vec![
///     AgentConfig::new("explorer", "Searches codebase")
///         .with_model(ModelType::Haiku),
///     AgentConfig::new("implementer", "Writes code")
///         .with_model(ModelType::Sonnet),
/// ];
///
/// let prompt = generate_orchestrator_prompt(&agents, None);
/// assert!(prompt.contains("Available Subagents"));
/// ```
pub fn generate_orchestrator_prompt(
    agents: &[AgentConfig],
    options: Option<GeneratorOptions>,
) -> String {
    let opts = options.unwrap_or_default();
    let mut sections = Vec::new();

    // Always include header
    sections.push(build_header());
    sections.push(String::new());

    // Agent registry
    if opts.include_agents {
        sections.push(build_agent_registry(agents));
    }

    // Orchestration principles
    if opts.include_principles {
        sections.push(build_orchestration_principles());
        sections.push(String::new());
    }

    // Trigger table
    if opts.include_triggers {
        if let Some(trigger_section) = build_trigger_table(agents) {
            sections.push(trigger_section);
        }
    }

    // Tool selection guidance
    if opts.include_tools {
        sections.push(build_tool_selection_section(agents));
    }

    // Delegation matrix
    if opts.include_delegation_table {
        sections.push(build_delegation_matrix(agents));
    }

    // Workflow
    if opts.include_workflow {
        sections.push(build_workflow());
        sections.push(String::new());
    }

    // Critical rules
    if opts.include_rules {
        sections.push(build_critical_rules());
        sections.push(String::new());
    }

    // Completion checklist
    if opts.include_checklist {
        sections.push(build_completion_checklist());
    }

    sections.join("\n")
}

/// Build agent section only (for embedding in other prompts)
pub fn build_agent_section(agents: &[AgentConfig]) -> String {
    build_agent_registry(agents)
}

/// Build triggers section only
pub fn build_triggers_section(agents: &[AgentConfig]) -> String {
    build_trigger_table(agents).unwrap_or_default()
}

/// Build delegation table section only
pub fn build_delegation_table_section(agents: &[AgentConfig]) -> String {
    build_delegation_matrix(agents)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AgentCategory, ModelType};

    fn create_test_agents() -> Vec<AgentConfig> {
        vec![
            AgentConfig::new("explorer", "Searches codebase for patterns")
                .with_model(ModelType::Haiku)
                .with_category(AgentCategory::Exploration)
                .with_tools(vec!["grep".to_string(), "ast-grep".to_string()]),
            AgentConfig::new("implementer", "Writes production code")
                .with_model(ModelType::Sonnet)
                .with_category(AgentCategory::Implementation)
                .with_tools(vec!["edit".to_string(), "write".to_string()]),
            AgentConfig::new("oracle", "Provides architectural guidance")
                .with_model(ModelType::Opus)
                .with_category(AgentCategory::Analysis)
                .with_tools(vec!["read".to_string()]),
        ]
    }

    #[test]
    fn test_generate_orchestrator_prompt_default() {
        let agents = create_test_agents();
        let prompt = generate_orchestrator_prompt(&agents, None);

        // Should contain header
        assert!(prompt.contains("relentless orchestrator"));

        // Should contain agent registry
        assert!(prompt.contains("Available Subagents"));
        assert!(prompt.contains("explorer"));
        assert!(prompt.contains("implementer"));
        assert!(prompt.contains("oracle"));

        // Should contain principles
        assert!(prompt.contains("Orchestration Principles"));

        // Should contain workflow
        assert!(prompt.contains("Workflow"));

        // Should contain critical rules
        assert!(prompt.contains("CRITICAL RULES"));

        // Should contain checklist
        assert!(prompt.contains("Completion Checklist"));
    }

    #[test]
    fn test_generate_orchestrator_prompt_minimal() {
        let agents = create_test_agents();
        let prompt = generate_orchestrator_prompt(&agents, Some(GeneratorOptions::minimal()));

        // Should contain header (always)
        assert!(prompt.contains("relentless orchestrator"));

        // Should contain agent registry (minimal includes this)
        assert!(prompt.contains("Available Subagents"));

        // Should NOT contain other sections
        assert!(!prompt.contains("Orchestration Principles"));
        assert!(!prompt.contains("Workflow"));
        assert!(!prompt.contains("CRITICAL RULES"));
    }

    #[test]
    fn test_build_agent_section() {
        let agents = create_test_agents();
        let section = build_agent_section(&agents);

        assert!(section.contains("Available Subagents"));
        assert!(section.contains("explorer"));
        assert!(section.contains("haiku"));
    }

    #[test]
    fn test_build_delegation_table_section() {
        let agents = create_test_agents();
        let section = build_delegation_table_section(&agents);

        assert!(section.contains("Delegation Guide"));
        assert!(section.contains("explorer"));
        assert!(section.contains("implementer"));
        assert!(section.contains("oracle"));
    }
}
