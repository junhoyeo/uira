//! Dynamic Prompt Builder
//!
//! Generates orchestrator prompt sections dynamically from agent metadata.
//! When agents are added/removed, the orchestrator's delegation table,
//! key triggers, and tool selection guide auto-update.
//!
//! Inspired by oh-my-opencode's `dynamic-agent-prompt-builder.ts`.

use crate::agents::types::{AgentCategory, AgentCost, AgentPromptMetadata};
use std::collections::HashMap;

/// Available agent info for prompt generation
#[derive(Debug, Clone)]
pub struct AvailableAgent {
    pub name: String,
    pub description: String,
    pub metadata: AgentPromptMetadata,
}

/// Available skill info for prompt generation
#[derive(Debug, Clone)]
pub struct AvailableSkill {
    pub name: String,
    pub description: String,
}

/// Available delegation category for prompt generation
#[derive(Debug, Clone)]
pub struct AvailableDelegationCategory {
    pub name: String,
    pub description: String,
}

/// Build the complete dynamic orchestrator prompt from available agents, skills, and categories.
pub fn build_dynamic_orchestrator_prompt(
    agents: &[AvailableAgent],
    skills: &[AvailableSkill],
    categories: &[AvailableDelegationCategory],
) -> String {
    let mut sections = Vec::new();

    // Key triggers section
    let triggers = build_key_triggers_section(agents);
    if !triggers.is_empty() {
        sections.push(triggers);
    }

    // Delegation table
    let delegation = build_delegation_table(agents);
    if !delegation.is_empty() {
        sections.push(delegation);
    }

    // Tool selection guide
    let tools = build_tool_selection_table(agents);
    if !tools.is_empty() {
        sections.push(tools);
    }

    // Category-skills delegation guide
    if !categories.is_empty() || !skills.is_empty() {
        sections.push(build_category_skills_guide(categories, skills));
    }

    // Dedicated agent sections
    for agent in agents {
        if let Some(ref desc) = agent.metadata.prompt_description {
            sections.push(format!(
                "## {} Agent\n\n{}",
                capitalize(&agent.name),
                desc
            ));
        }
    }

    // Anti-patterns
    sections.push(build_anti_patterns_section());

    sections.join("\n\n")
}

/// Build the key triggers section from agent metadata.
fn build_key_triggers_section(agents: &[AvailableAgent]) -> String {
    let mut lines = vec!["## Key Triggers\n".to_string()];
    lines.push("| Trigger | Agent | Cost |".to_string());
    lines.push("|---------|-------|------|".to_string());

    let mut has_triggers = false;
    for agent in agents {
        for trigger in &agent.metadata.triggers {
            has_triggers = true;
            let cost = match agent.metadata.cost {
                AgentCost::Free => "FREE",
                AgentCost::Cheap => "CHEAP",
                AgentCost::Expensive => "EXPENSIVE",
            };
            lines.push(format!(
                "| {} | {} | {} |",
                trigger.trigger, agent.name, cost
            ));
        }
    }

    if !has_triggers {
        return String::new();
    }

    lines.join("\n")
}

/// Build the delegation routing table.
fn build_delegation_table(agents: &[AvailableAgent]) -> String {
    let mut lines = vec!["## Delegation Table\n".to_string()];
    lines.push("| Agent | Category | Cost | When to Use |".to_string());
    lines.push("|-------|----------|------|-------------|".to_string());

    // Group by category
    let mut by_category: HashMap<AgentCategory, Vec<&AvailableAgent>> = HashMap::new();
    for agent in agents {
        by_category
            .entry(agent.metadata.category)
            .or_default()
            .push(agent);
    }

    let category_order = [
        AgentCategory::Exploration,
        AgentCategory::Specialist,
        AgentCategory::Advisor,
        AgentCategory::Utility,
        AgentCategory::Planner,
        AgentCategory::Reviewer,
        AgentCategory::Orchestration,
    ];

    for category in &category_order {
        if let Some(agents_in_cat) = by_category.get(category) {
            for agent in agents_in_cat {
                let cost = match agent.metadata.cost {
                    AgentCost::Free => "FREE",
                    AgentCost::Cheap => "CHEAP",
                    AgentCost::Expensive => "EXPENSIVE",
                };
                let use_when = if agent.metadata.use_when.is_empty() {
                    agent.description.clone()
                } else {
                    agent.metadata.use_when.join("; ")
                };
                lines.push(format!(
                    "| {} | {:?} | {} | {} |",
                    agent.name, agent.metadata.category, cost, use_when
                ));
            }
        }
    }

    if lines.len() <= 3 {
        return String::new();
    }

    lines.join("\n")
}

/// Build the tool selection table.
fn build_tool_selection_table(agents: &[AvailableAgent]) -> String {
    let mut lines = vec!["## Agent Tool Guide\n".to_string()];
    lines.push("| Agent | Primary Tools | Avoid |".to_string());
    lines.push("|-------|---------------|-------|".to_string());

    let mut has_entries = false;
    for agent in agents {
        if !agent.metadata.tools.is_empty() || !agent.metadata.avoid_when.is_empty() {
            has_entries = true;
            let tools_str = if agent.metadata.tools.is_empty() {
                "—".to_string()
            } else {
                agent.metadata.tools.join(", ")
            };
            let avoid_str = if agent.metadata.avoid_when.is_empty() {
                "—".to_string()
            } else {
                agent.metadata.avoid_when.join("; ")
            };
            lines.push(format!("| {} | {} | {} |", agent.name, tools_str, avoid_str));
        }
    }

    if !has_entries {
        return String::new();
    }

    lines.join("\n")
}

/// Build the category-skills delegation guide.
fn build_category_skills_guide(
    categories: &[AvailableDelegationCategory],
    skills: &[AvailableSkill],
) -> String {
    let mut lines = vec!["## Categories & Skills\n".to_string()];

    if !categories.is_empty() {
        lines.push("### Delegation Categories\n".to_string());
        lines.push("| Category | Description |".to_string());
        lines.push("|----------|-------------|".to_string());
        for cat in categories {
            lines.push(format!("| {} | {} |", cat.name, cat.description));
        }
        lines.push(String::new());
    }

    if !skills.is_empty() {
        lines.push("### Available Skills\n".to_string());
        lines.push("Skills can be injected into delegated agent prompts via `load_skills`:\n".to_string());
        for skill in skills {
            lines.push(format!("- **{}**: {}", skill.name, skill.description));
        }
    }

    lines.join("\n")
}

/// Build static anti-patterns section.
fn build_anti_patterns_section() -> String {
    r#"## Anti-Patterns (NEVER do these)

- **Infinite delegation**: Do NOT delegate a task to an agent that delegates back to you
- **Skipping verification**: ALWAYS verify delegated work before reporting success
- **Over-delegation**: Simple tasks (< 5 lines of code) should be done directly
- **Canceling background tasks**: NEVER cancel a running explore or architect task
- **Ignoring failures**: If a task fails, investigate root cause before retrying
- **Guessing file paths**: Use explore agent to find files, don't guess"#
        .to_string()
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().to_string() + c.as_str(),
    }
}

/// Convenience: build metadata for known builtin agents.
/// Returns a map of agent name -> AgentPromptMetadata for all agents
/// that have defined metadata.
pub fn builtin_agent_metadata() -> HashMap<String, AgentPromptMetadata> {
    use crate::agents::types::{DelegationTrigger};

    let mut map = HashMap::new();

    map.insert(
        "explore".to_string(),
        AgentPromptMetadata {
            category: AgentCategory::Exploration,
            cost: AgentCost::Free,
            prompt_alias: Some("explore".to_string()),
            triggers: vec![DelegationTrigger {
                domain: "codebase".to_string(),
                trigger: "2+ modules involved -> fire explore background".to_string(),
            }],
            use_when: vec![
                "Need to find files or code patterns".to_string(),
                "Task touches multiple modules".to_string(),
            ],
            avoid_when: vec!["Already know exact file paths".to_string()],
            prompt_description: Some(
                "Read-only codebase search agent. Use for finding files, patterns, and code structure. Outputs structured results with absolute file paths.".to_string(),
            ),
            tools: vec![
                "Read".to_string(),
                "Glob".to_string(),
                "Grep".to_string(),
            ],
        },
    );

    map.insert(
        "librarian".to_string(),
        AgentPromptMetadata {
            category: AgentCategory::Exploration,
            cost: AgentCost::Cheap,
            prompt_alias: Some("librarian".to_string()),
            triggers: vec![DelegationTrigger {
                domain: "external".to_string(),
                trigger: "Need external docs or multi-repo analysis".to_string(),
            }],
            use_when: vec![
                "Need official documentation".to_string(),
                "Multi-repository analysis".to_string(),
                "Finding implementation examples in open source".to_string(),
            ],
            avoid_when: vec!["Question is about local codebase only".to_string()],
            prompt_description: Some(
                "External documentation and multi-repo search agent. Uses WebSearch, WebFetch, and GitHub CLI.".to_string(),
            ),
            tools: vec![
                "WebSearch".to_string(),
                "WebFetch".to_string(),
                "Bash".to_string(),
            ],
        },
    );

    map.insert(
        "architect".to_string(),
        AgentPromptMetadata {
            category: AgentCategory::Advisor,
            cost: AgentCost::Expensive,
            prompt_alias: Some("architect".to_string()),
            triggers: vec![DelegationTrigger {
                domain: "debugging".to_string(),
                trigger: "Complex debugging, race conditions, architecture decisions".to_string(),
            }],
            use_when: vec![
                "Deep debugging needed".to_string(),
                "Architecture decisions".to_string(),
                "Complex system analysis".to_string(),
            ],
            avoid_when: vec![
                "Simple questions".to_string(),
                "Straightforward implementation".to_string(),
            ],
            prompt_description: Some(
                "High-IQ advisor for complex debugging, architecture analysis, and strategic decisions. Read-only consultant — does not write code.".to_string(),
            ),
            tools: vec![
                "Read".to_string(),
                "Glob".to_string(),
                "Grep".to_string(),
                "WebSearch".to_string(),
                "WebFetch".to_string(),
            ],
        },
    );

    map.insert(
        "executor".to_string(),
        AgentPromptMetadata {
            category: AgentCategory::Specialist,
            cost: AgentCost::Cheap,
            prompt_alias: Some("executor".to_string()),
            triggers: vec![DelegationTrigger {
                domain: "implementation".to_string(),
                trigger: "Feature implementation, code changes".to_string(),
            }],
            use_when: vec![
                "Implementation tasks".to_string(),
                "Code changes needed".to_string(),
            ],
            avoid_when: vec![
                "Complex architectural decisions".to_string(),
                "Read-only analysis".to_string(),
            ],
            prompt_description: None,
            tools: vec![
                "Read".to_string(),
                "Edit".to_string(),
                "Write".to_string(),
                "Bash".to_string(),
            ],
        },
    );

    map.insert(
        "critic".to_string(),
        AgentPromptMetadata {
            category: AgentCategory::Reviewer,
            cost: AgentCost::Expensive,
            prompt_alias: Some("critic".to_string()),
            triggers: vec![DelegationTrigger {
                domain: "review".to_string(),
                trigger: "Plan review, code review, quality check".to_string(),
            }],
            use_when: vec![
                "Reviewing plans before execution".to_string(),
                "Code quality review".to_string(),
            ],
            avoid_when: vec!["Trivial changes".to_string()],
            prompt_description: Some(
                "Critical reviewer with approval bias. Returns [OKAY] or [REJECT] with max 3 blocking issues. When in doubt, APPROVE.".to_string(),
            ),
            tools: vec!["Read".to_string(), "Glob".to_string(), "Grep".to_string()],
        },
    );

    map.insert(
        "designer".to_string(),
        AgentPromptMetadata {
            category: AgentCategory::Specialist,
            cost: AgentCost::Cheap,
            prompt_alias: Some("designer".to_string()),
            triggers: vec![DelegationTrigger {
                domain: "frontend".to_string(),
                trigger: "UI/UX implementation, design systems, frontend work".to_string(),
            }],
            use_when: vec!["UI component work".to_string(), "Frontend tasks".to_string()],
            avoid_when: vec!["Backend-only tasks".to_string()],
            prompt_description: None,
            tools: vec![
                "Read".to_string(),
                "Edit".to_string(),
                "Write".to_string(),
                "Bash".to_string(),
            ],
        },
    );

    map.insert(
        "planner".to_string(),
        AgentPromptMetadata {
            category: AgentCategory::Planner,
            cost: AgentCost::Expensive,
            prompt_alias: Some("planner".to_string()),
            triggers: vec![DelegationTrigger {
                domain: "planning".to_string(),
                trigger: "Complex task requiring strategic planning".to_string(),
            }],
            use_when: vec![
                "Multi-step implementation".to_string(),
                "Complex refactoring".to_string(),
            ],
            avoid_when: vec!["Simple, single-step tasks".to_string()],
            prompt_description: Some(
                "Strategic planner. Creates detailed implementation plans with dependency mapping, risk assessment, and verification criteria. Never writes code.".to_string(),
            ),
            tools: vec![
                "Read".to_string(),
                "Glob".to_string(),
                "Grep".to_string(),
                "Write".to_string(),
            ],
        },
    );

    map.insert(
        "vision".to_string(),
        AgentPromptMetadata {
            category: AgentCategory::Utility,
            cost: AgentCost::Cheap,
            prompt_alias: Some("vision".to_string()),
            triggers: vec![DelegationTrigger {
                domain: "media".to_string(),
                trigger: "Image, PDF, or diagram analysis needed".to_string(),
            }],
            use_when: vec![
                "Analyzing screenshots or mockups".to_string(),
                "Extracting info from images".to_string(),
            ],
            avoid_when: vec!["No visual content involved".to_string()],
            prompt_description: None,
            tools: vec!["Read".to_string()],
        },
    );

    map
}

/// Convenience: build the full dynamic orchestrator prompt using all builtin agents.
/// This is the main entry point for callers who just want the complete prompt section.
pub fn build_default_orchestrator_prompt() -> String {
    let metadata = builtin_agent_metadata();
    let agents: Vec<AvailableAgent> = metadata
        .into_iter()
        .map(|(name, meta)| AvailableAgent {
            description: meta.prompt_description.clone().unwrap_or_default(),
            name,
            metadata: meta,
        })
        .collect();
    build_dynamic_orchestrator_prompt(&agents, &[], &[])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_agents() -> Vec<AvailableAgent> {
        let metadata = builtin_agent_metadata();
        metadata
            .into_iter()
            .map(|(name, meta)| AvailableAgent {
                description: format!("{} agent", name),
                name,
                metadata: meta,
            })
            .collect()
    }

    #[test]
    fn test_build_dynamic_prompt_not_empty() {
        let agents = sample_agents();
        let prompt = build_dynamic_orchestrator_prompt(&agents, &[], &[]);
        assert!(!prompt.is_empty());
        assert!(prompt.contains("Delegation Table"));
        assert!(prompt.contains("Anti-Patterns"));
    }

    #[test]
    fn test_delegation_table_contains_all_agents() {
        let agents = sample_agents();
        let table = build_delegation_table(&agents);
        for agent in &agents {
            assert!(
                table.contains(&agent.name),
                "Agent {} not found in delegation table",
                agent.name
            );
        }
    }

    #[test]
    fn test_key_triggers_generated() {
        let agents = sample_agents();
        let triggers = build_key_triggers_section(&agents);
        assert!(triggers.contains("Key Triggers"));
        assert!(triggers.contains("explore"));
    }

    #[test]
    fn test_empty_agents_produces_minimal_prompt() {
        let prompt = build_dynamic_orchestrator_prompt(&[], &[], &[]);
        assert!(prompt.contains("Anti-Patterns"));
        // Should not have empty tables
        assert!(!prompt.contains("Delegation Table"));
        assert!(!prompt.contains("Key Triggers"));
    }

    #[test]
    fn test_skills_section() {
        let skills = vec![AvailableSkill {
            name: "frontend-ui-ux".to_string(),
            description: "Frontend UI/UX expertise".to_string(),
        }];
        let prompt = build_dynamic_orchestrator_prompt(&[], &skills, &[]);
        assert!(prompt.contains("frontend-ui-ux"));
    }

    #[test]
    fn test_builtin_metadata_covers_key_agents() {
        let meta = builtin_agent_metadata();
        assert!(meta.contains_key("explore"));
        assert!(meta.contains_key("librarian"));
        assert!(meta.contains_key("architect"));
        assert!(meta.contains_key("executor"));
        assert!(meta.contains_key("critic"));
        assert!(meta.contains_key("planner"));
    }
}
