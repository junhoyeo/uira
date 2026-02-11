use std::collections::{HashMap, HashSet};

use super::types::AgentConfig;

/// Tool restrictions expressed as an allowlist.
#[derive(Debug, Clone, Default)]
pub struct ToolRestrictions {
    allowed: HashSet<String>,
}

impl ToolRestrictions {
    pub fn from_allowlist<I, S>(tools: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let allowed = tools
            .into_iter()
            .map(|t| t.into())
            .map(|t| t.to_lowercase())
            .collect();
        Self { allowed }
    }

    pub fn allows(&self, tool: &str) -> bool {
        self.allowed.contains(&tool.to_lowercase())
    }

    pub fn apply_to_config(&self, config: &mut AgentConfig) {
        config
            .tools
            .retain(|t| self.allowed.contains(&t.to_lowercase()));
    }
}

/// Registry of per-agent tool allowlists.
#[derive(Debug, Clone, Default)]
pub struct ToolRestrictionsRegistry {
    by_agent: HashMap<String, ToolRestrictions>,
}

impl ToolRestrictionsRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, agent_name: impl Into<String>, restrictions: ToolRestrictions) {
        self.by_agent.insert(agent_name.into(), restrictions);
    }

    pub fn get(&self, agent_name: &str) -> Option<&ToolRestrictions> {
        self.by_agent.get(agent_name)
    }

    pub fn apply(&self, config: &mut AgentConfig) {
        if let Some(r) = self.get(&config.name) {
            r.apply_to_config(config);
        }
    }

    pub fn with_default_allowlists() -> Self {
        let mut reg = Self::new();

        // Base agents
        reg.register(
            "architect",
            allow(&["Read", "Glob", "Grep", "WebSearch", "WebFetch"]),
        );
        reg.register(
            "librarian",
            allow(&["Read", "Glob", "Grep", "WebSearch", "WebFetch", "Bash"]),
        );
        reg.register("explore", allow(&["Read", "Glob", "Grep"]));
        reg.register(
            "executor",
            allow(&["Read", "Glob", "Grep", "Edit", "Write", "Bash", "TodoWrite"]),
        );
        reg.register(
            "designer",
            allow(&["Read", "Glob", "Grep", "Edit", "Write", "Bash"]),
        );
        reg.register("writer", allow(&["Read", "Glob", "Grep", "Write"]));
        reg.register("vision", allow(&["Read", "Glob", "Grep"]));
        reg.register("critic", allow(&["Read", "Glob", "Grep"]));
        reg.register("analyst", allow(&["Read", "Glob", "Grep"]));
        reg.register("planner", allow(&["Read", "Glob", "Grep", "Write"]));
        reg.register(
            "qa-tester",
            allow(&["Bash", "Read", "Grep", "Glob", "TodoWrite"]),
        );
        reg.register(
            "scientist",
            allow(&["Read", "Glob", "Grep", "Bash", "python_repl"]),
        );

        // Tiered variants (same tools as their base in the TS source)
        for name in [
            "architect-low",
            "architect-medium",
            "executor-low",
            "executor-high",
            "designer-low",
            "designer-high",
            "qa-tester-high",
            "scientist-low",
            "scientist-high",
        ] {
            // Inherit from base by taking prefix before '-'.
            let base = name.split('-').next().unwrap_or(name);
            if let Some(r) = reg.get(base).cloned() {
                reg.register(name, r);
            }
        }

        // Specialized
        reg.register(
            "security-reviewer",
            allow(&["Read", "Grep", "Glob", "Bash"]),
        );
        reg.register(
            "security-reviewer-low",
            allow(&["Read", "Grep", "Glob", "Bash"]),
        );
        reg.register(
            "build-fixer",
            allow(&["Read", "Grep", "Glob", "Edit", "Write", "Bash"]),
        );
        reg.register(
            "build-fixer-low",
            allow(&["Read", "Grep", "Glob", "Edit", "Write", "Bash"]),
        );
        reg.register(
            "tdd-guide",
            allow(&["Read", "Grep", "Glob", "Edit", "Write", "Bash"]),
        );
        reg.register("tdd-guide-low", allow(&["Read", "Grep", "Glob", "Bash"]));
        reg.register("code-reviewer", allow(&["Read", "Grep", "Glob", "Bash"]));
        reg.register(
            "code-reviewer-low",
            allow(&["Read", "Grep", "Glob", "Bash"]),
        );

        reg
    }
}

fn allow(tools: &[&str]) -> ToolRestrictions {
    ToolRestrictions::from_allowlist(tools.iter().copied())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_filters_tool_list_case_insensitively() {
        let mut cfg = AgentConfig {
            name: "explore".to_string(),
            description: "".to_string(),
            prompt: "".to_string(),
            tools: vec!["Read".to_string(), "Bash".to_string(), "Grep".to_string()],
            model: None,
            default_model: None,
            metadata: None,
        };

        let reg = ToolRestrictionsRegistry::with_default_allowlists();
        reg.apply(&mut cfg);
        assert_eq!(cfg.tools, vec!["Read".to_string(), "Grep".to_string()]);
    }
}
