use crate::types::{AgentCategory, AgentConfig};
use std::collections::HashMap;

/// Build the header section with core orchestrator identity
pub fn build_header() -> String {
    r#"You are the relentless orchestrator of a multi-agent development system.

## RELENTLESS EXECUTION

You are BOUND to your task list. You do not stop. You do not quit. You do not take breaks. Work continues until EVERY task is COMPLETE.

## Your Core Duty
You coordinate specialized subagents to accomplish complex software engineering tasks. Abandoning work mid-task is not an option. If you stop without completing ALL tasks, you have failed."#.to_string()
}

/// Build the agent registry section with descriptions
pub fn build_agent_registry(agents: &[AgentConfig]) -> String {
    let mut lines = vec!["## Available Subagents".to_string(), String::new()];

    // Group agents by tier (base vs variants)
    let base_agents: Vec<_> = agents.iter().filter(|a| !a.name.contains('-')).collect();
    let tiered_agents: Vec<_> = agents.iter().filter(|a| a.name.contains('-')).collect();

    // Base agents
    if !base_agents.is_empty() {
        lines.push("### Primary Agents".to_string());
        for agent in base_agents {
            let model_info = agent
                .model
                .as_ref()
                .map(|m| format!(" ({})", m.as_str()))
                .unwrap_or_default();
            lines.push(format!(
                "- **{}**{}: {}",
                agent.name, model_info, agent.description
            ));
        }
        lines.push(String::new());
    }

    // Tiered variants
    if !tiered_agents.is_empty() {
        lines.push("### Tiered Variants".to_string());
        lines.push(
            "Use tiered variants for smart model routing based on task complexity:".to_string(),
        );
        lines.push("- **HIGH tier (opus)**: Complex analysis, architecture, debugging".to_string());
        lines.push("- **MEDIUM tier (sonnet)**: Standard tasks, moderate complexity".to_string());
        lines.push("- **LOW tier (haiku)**: Simple lookups, trivial operations".to_string());
        lines.push(String::new());

        for agent in tiered_agents {
            let model_info = agent
                .model
                .as_ref()
                .map(|m| format!(" ({})", m.as_str()))
                .unwrap_or_default();
            lines.push(format!(
                "- **{}**{}: {}",
                agent.name, model_info, agent.description
            ));
        }
        lines.push(String::new());
    }

    lines.join("\n")
}

/// Build the trigger table showing when to use each agent
pub fn build_trigger_table(agents: &[AgentConfig]) -> Option<String> {
    // Filter agents with metadata triggers
    let agents_with_triggers: Vec<_> = agents
        .iter()
        .filter(|a| {
            a.metadata
                .as_ref()
                .and_then(|m| m.triggers.as_ref())
                .map(|t| !t.is_empty())
                .unwrap_or(false)
        })
        .collect();

    if agents_with_triggers.is_empty() {
        return None;
    }

    let mut lines = vec![
        "## Key Triggers".to_string(),
        String::new(),
        "| Agent | Domain | Trigger Condition |".to_string(),
        "|-------|--------|------------------|".to_string(),
    ];

    for agent in agents_with_triggers {
        if let Some(metadata) = &agent.metadata {
            if let Some(triggers) = &metadata.triggers {
                for (i, trigger) in triggers.iter().enumerate() {
                    let agent_name = if i == 0 {
                        format!("**{}**", agent.name)
                    } else {
                        String::new()
                    };
                    lines.push(format!(
                        "| {} | {} | {} |",
                        agent_name, trigger.domain, trigger.trigger
                    ));
                }
            }
        }
    }

    lines.push(String::new());
    Some(lines.join("\n"))
}

/// Build tool selection guidance section
pub fn build_tool_selection_section(agents: &[AgentConfig]) -> String {
    let mut lines = vec!["## Tool Selection Guidance".to_string(), String::new()];

    // Group by category
    let mut categorized_agents: HashMap<AgentCategory, Vec<&AgentConfig>> = HashMap::new();
    for agent in agents {
        let category = agent
            .metadata
            .as_ref()
            .and_then(|m| m.category)
            .unwrap_or(AgentCategory::Utility);
        categorized_agents.entry(category).or_default().push(agent);
    }

    // Sort categories for consistent output
    let mut categories: Vec<_> = categorized_agents.keys().collect();
    categories.sort_by_key(|c| c.as_str());

    for category in categories {
        let category_agents = &categorized_agents[category];
        lines.push(format!("### {} Agents", category.capitalize()));

        for agent in category_agents {
            let model = agent.model.as_ref().map(|m| m.as_str()).unwrap_or("sonnet");
            lines.push(format!("**{}** ({}):", agent.name, model));
            lines.push(format!("- Tools: {}", agent.tools.join(", ")));

            if let Some(metadata) = &agent.metadata {
                if let Some(use_when) = &metadata.use_when {
                    if !use_when.is_empty() {
                        lines.push(format!("- Use when: {}", use_when.join("; ")));
                    }
                }

                if let Some(avoid_when) = &metadata.avoid_when {
                    if !avoid_when.is_empty() {
                        lines.push(format!("- Avoid when: {}", avoid_when.join("; ")));
                    }
                }
            }

            lines.push(String::new());
        }
    }

    lines.join("\n")
}

/// Build delegation matrix/guide table
pub fn build_delegation_matrix(agents: &[AgentConfig]) -> String {
    let mut lines = vec!["## Delegation Guide".to_string(), String::new()];

    // Group by category
    let mut categorized_agents: HashMap<AgentCategory, Vec<&AgentConfig>> = HashMap::new();
    for agent in agents {
        let category = agent
            .metadata
            .as_ref()
            .and_then(|m| m.category)
            .unwrap_or(AgentCategory::Utility);
        categorized_agents.entry(category).or_default().push(agent);
    }

    lines.push("| Category | Agent | Model | Use Case |".to_string());
    lines.push("|----------|-------|-------|----------|".to_string());

    // Sort categories for consistent output
    let mut categories: Vec<_> = categorized_agents.keys().collect();
    categories.sort_by_key(|c| c.as_str());

    for category in categories {
        let category_agents = &categorized_agents[category];
        let category_name = category.capitalize();

        for (i, agent) in category_agents.iter().enumerate() {
            let cat_display = if i == 0 { category_name } else { "" };
            let model = agent.model.as_ref().map(|m| m.as_str()).unwrap_or("sonnet");

            let use_case = agent
                .metadata
                .as_ref()
                .and_then(|m| m.use_when.as_ref())
                .and_then(|u| u.first())
                .map(|s| s.as_str())
                .unwrap_or(&agent.description);

            lines.push(format!(
                "| {} | **{}** | {} | {} |",
                cat_display, agent.name, model, use_case
            ));
        }
    }

    lines.push(String::new());
    lines.join("\n")
}

/// Build orchestration principles section
pub fn build_orchestration_principles() -> String {
    r#"## Orchestration Principles
1. **Delegate Aggressively**: Fire off subagents for specialized tasks - don't do everything yourself
2. **Parallelize Ruthlessly**: Launch multiple subagents concurrently whenever tasks are independent
3. **PERSIST RELENTLESSLY**: Continue until ALL tasks are VERIFIED complete - check your todo list BEFORE stopping
4. **Communicate Progress**: Keep the user informed but DON'T STOP to explain when you should be working
5. **Verify Thoroughly**: Test, check, verify - then verify again"#.to_string()
}

/// Build workflow section
pub fn build_workflow() -> String {
    r#"## Workflow
1. Analyze the user's request and break it into tasks using TodoWrite
2. Mark the first task in_progress and BEGIN WORKING
3. Delegate to appropriate subagents based on task type
4. Coordinate results and handle any issues WITHOUT STOPPING
5. Mark tasks complete ONLY when verified
6. LOOP back to step 2 until ALL tasks show 'completed'
7. Final verification: Re-read todo list, confirm 100% completion
8. Only THEN may you rest"#
        .to_string()
}

/// Build critical rules section
pub fn build_critical_rules() -> String {
    r#"## CRITICAL RULES - VIOLATION IS FAILURE

1. **NEVER STOP WITH INCOMPLETE WORK** - If your todo list has pending/in_progress items, YOU ARE NOT DONE
2. **ALWAYS VERIFY** - Check your todo list before ANY attempt to conclude
3. **NO PREMATURE CONCLUSIONS** - Saying "I've completed the task" without verification is a LIE
4. **PARALLEL EXECUTION** - Use it whenever possible for speed
5. **CONTINUOUS PROGRESS** - Report progress but keep working
6. **WHEN BLOCKED, UNBLOCK** - Don't stop because something is hard; find another way
7. **ASK ONLY WHEN NECESSARY** - Clarifying questions are for ambiguity, not for avoiding work"#.to_string()
}

/// Build completion checklist section
pub fn build_completion_checklist() -> String {
    r#"## Completion Checklist
Before concluding, you MUST verify:
- [ ] Every todo item is marked 'completed'
- [ ] All requested functionality is implemented
- [ ] Tests pass (if applicable)
- [ ] No errors remain unaddressed
- [ ] The user's original request is FULLY satisfied

If ANY checkbox is unchecked, YOU ARE NOT DONE. Continue working."#
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AgentPromptMetadata, AgentTrigger, ModelType};

    #[test]
    fn test_build_header() {
        let header = build_header();
        assert!(header.contains("relentless orchestrator"));
        assert!(header.contains("RELENTLESS EXECUTION"));
    }

    #[test]
    fn test_build_agent_registry() {
        let agents = vec![
            AgentConfig::new("explorer", "Searches codebase").with_model(ModelType::Haiku),
            AgentConfig::new("implementer", "Writes code").with_model(ModelType::Sonnet),
        ];

        let registry = build_agent_registry(&agents);
        assert!(registry.contains("Available Subagents"));
        assert!(registry.contains("explorer"));
        assert!(registry.contains("haiku"));
        assert!(registry.contains("implementer"));
        assert!(registry.contains("sonnet"));
    }

    #[test]
    fn test_build_trigger_table_with_triggers() {
        let metadata = AgentPromptMetadata {
            category: Some(AgentCategory::Exploration),
            triggers: Some(vec![AgentTrigger {
                domain: "Search".to_string(),
                trigger: "User asks to find code".to_string(),
            }]),
            use_when: None,
            avoid_when: None,
            extra: HashMap::new(),
        };

        let agents =
            vec![AgentConfig::new("explorer", "Searches codebase").with_metadata(metadata)];

        let table = build_trigger_table(&agents);
        assert!(table.is_some());
        let table = table.unwrap();
        assert!(table.contains("Key Triggers"));
        assert!(table.contains("explorer"));
        assert!(table.contains("Search"));
    }

    #[test]
    fn test_build_trigger_table_without_triggers() {
        let agents = vec![AgentConfig::new("explorer", "Searches codebase")];
        let table = build_trigger_table(&agents);
        assert!(table.is_none());
    }

    #[test]
    fn test_build_tool_selection_section() {
        let agents = vec![AgentConfig::new("explorer", "Searches codebase")
            .with_tools(vec!["grep".to_string(), "ast-grep".to_string()])
            .with_category(AgentCategory::Exploration)];

        let section = build_tool_selection_section(&agents);
        assert!(section.contains("Tool Selection Guidance"));
        assert!(section.contains("Exploration Agents"));
        assert!(section.contains("grep, ast-grep"));
    }

    #[test]
    fn test_build_delegation_matrix() {
        let agents = vec![
            AgentConfig::new("explorer", "Searches codebase")
                .with_model(ModelType::Haiku)
                .with_category(AgentCategory::Exploration),
            AgentConfig::new("implementer", "Writes code")
                .with_model(ModelType::Sonnet)
                .with_category(AgentCategory::Implementation),
        ];

        let matrix = build_delegation_matrix(&agents);
        assert!(matrix.contains("Delegation Guide"));
        assert!(matrix.contains("explorer"));
        assert!(matrix.contains("implementer"));
        assert!(matrix.contains("haiku"));
        assert!(matrix.contains("sonnet"));
    }

    #[test]
    fn test_build_orchestration_principles() {
        let principles = build_orchestration_principles();
        assert!(principles.contains("Orchestration Principles"));
        assert!(principles.contains("Delegate Aggressively"));
        assert!(principles.contains("PERSIST RELENTLESSLY"));
    }

    #[test]
    fn test_build_workflow() {
        let workflow = build_workflow();
        assert!(workflow.contains("Workflow"));
        assert!(workflow.contains("TodoWrite"));
        assert!(workflow.contains("BEGIN WORKING"));
    }

    #[test]
    fn test_build_critical_rules() {
        let rules = build_critical_rules();
        assert!(rules.contains("CRITICAL RULES"));
        assert!(rules.contains("NEVER STOP WITH INCOMPLETE WORK"));
    }

    #[test]
    fn test_build_completion_checklist() {
        let checklist = build_completion_checklist();
        assert!(checklist.contains("Completion Checklist"));
        assert!(checklist.contains("Every todo item is marked 'completed'"));
    }
}
