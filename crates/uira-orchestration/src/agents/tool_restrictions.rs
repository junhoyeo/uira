use std::collections::{HashMap, HashSet};

use super::types::AgentConfig;

/// Tool restrictions expressed as an allowlist.
#[derive(Debug, Clone, Default, PartialEq)]
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

    #[test]
    fn test_tiered_variants_inherit_base_restrictions() {
        let reg = ToolRestrictionsRegistry::with_default_allowlists();

        // Test architect variants
        let architect_base = reg.get("architect");
        let architect_low = reg.get("architect-low");
        let architect_medium = reg.get("architect-medium");
        assert!(architect_base.is_some());
        assert_eq!(architect_base, architect_low);
        assert_eq!(architect_base, architect_medium);

        // Test executor variants
        let executor_base = reg.get("executor");
        let executor_low = reg.get("executor-low");
        let executor_high = reg.get("executor-high");
        assert!(executor_base.is_some());
        assert_eq!(executor_base, executor_low);
        assert_eq!(executor_base, executor_high);

        // Test designer variants
        let designer_base = reg.get("designer");
        let designer_low = reg.get("designer-low");
        let designer_high = reg.get("designer-high");
        assert!(designer_base.is_some());
        assert_eq!(designer_base, designer_low);
        assert_eq!(designer_base, designer_high);

        // Test scientist variants
        let scientist_base = reg.get("scientist");
        let scientist_low = reg.get("scientist-low");
        let scientist_high = reg.get("scientist-high");
        assert!(scientist_base.is_some());
        assert_eq!(scientist_base, scientist_low);
        assert_eq!(scientist_base, scientist_high);

        // Test that non-existent agents return None
        assert!(reg.get("nonexistent-agent").is_none());
        assert!(reg.get("architect-ultra").is_none());
    }
}
