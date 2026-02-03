//! Permission rule definitions
//!
//! Rules define what action to take for specific permission/path combinations.

use serde::{Deserialize, Serialize};

use crate::pattern::{Pattern, PatternError};
use crate::types::Action;

/// A permission rule that matches a permission and path pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRule {
    /// Human-readable name/description for this rule
    #[serde(default)]
    pub name: Option<String>,

    /// Permission pattern to match (e.g., "file:*", "shell:execute")
    pub permission: String,

    /// Path/pattern to match (e.g., "src/**", "~/.config/**")
    pub pattern: String,

    /// Action to take when this rule matches
    pub action: Action,

    /// Optional comment explaining this rule
    #[serde(default)]
    pub comment: Option<String>,
}

impl PermissionRule {
    /// Create a new permission rule
    pub fn new(permission: impl Into<String>, pattern: impl Into<String>, action: Action) -> Self {
        Self {
            name: None,
            permission: permission.into(),
            pattern: pattern.into(),
            action,
            comment: None,
        }
    }

    /// Add a name to this rule
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Add a comment to this rule
    pub fn with_comment(mut self, comment: impl Into<String>) -> Self {
        self.comment = Some(comment.into());
        self
    }
}

/// A compiled permission rule with pre-compiled patterns for efficient matching
#[derive(Debug)]
pub struct CompiledRule {
    /// Original rule
    rule: PermissionRule,
    /// Compiled permission pattern
    permission_pattern: Pattern,
    /// Compiled path pattern
    path_pattern: Pattern,
}

impl CompiledRule {
    /// Compile a permission rule
    pub fn compile(rule: PermissionRule) -> Result<Self, PatternError> {
        let permission_pattern = Pattern::new(&rule.permission)?;
        let path_pattern = Pattern::new(&rule.pattern)?;

        Ok(Self {
            rule,
            permission_pattern,
            path_pattern,
        })
    }

    /// Check if this rule matches the given permission and path
    pub fn matches(&self, permission: &str, path: &str) -> bool {
        self.permission_pattern.matches(permission) && self.path_pattern.matches_expanded(path)
    }

    /// Get the action for this rule
    pub fn action(&self) -> Action {
        self.rule.action
    }

    /// Get the original rule
    pub fn rule(&self) -> &PermissionRule {
        &self.rule
    }
}

/// A set of common default rules
impl PermissionRule {
    /// Create a rule that allows all file reads
    pub fn allow_all_reads() -> Self {
        Self::new("file:read", "**", Action::Allow).with_name("allow-all-reads")
    }

    /// Create a rule that requires approval for file writes
    pub fn ask_for_writes() -> Self {
        Self::new("file:write", "**", Action::Ask).with_name("ask-for-writes")
    }

    /// Create a rule that denies writes to home config
    pub fn deny_home_config() -> Self {
        Self::new("file:write", "~/.config/**", Action::Deny).with_name("deny-home-config")
    }

    /// Create a rule that allows writes to workspace
    pub fn allow_workspace_writes(workspace: &str) -> Self {
        Self::new("file:write", format!("{}/**", workspace), Action::Allow)
            .with_name("allow-workspace-writes")
    }

    /// Create a rule that asks for shell commands
    pub fn ask_for_shell() -> Self {
        Self::new("shell:execute", "**", Action::Ask).with_name("ask-for-shell")
    }

    /// Create a rule that allows network access
    pub fn allow_network() -> Self {
        Self::new("network:access", "**", Action::Allow).with_name("allow-network")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rule_creation() {
        let rule = PermissionRule::new("file:write", "src/**", Action::Allow)
            .with_name("allow-src-writes")
            .with_comment("Allow writes to src directory");

        assert_eq!(rule.permission, "file:write");
        assert_eq!(rule.pattern, "src/**");
        assert_eq!(rule.action, Action::Allow);
        assert_eq!(rule.name, Some("allow-src-writes".to_string()));
        assert!(rule.comment.is_some());
    }

    #[test]
    fn test_compiled_rule_matches() {
        let rule = PermissionRule::new("file:write", "src/**", Action::Allow);
        let compiled = CompiledRule::compile(rule).unwrap();

        assert!(compiled.matches("file:write", "src/main.rs"));
        assert!(compiled.matches("file:write", "src/lib/mod.rs"));
        assert!(!compiled.matches("file:read", "src/main.rs"));
        assert!(!compiled.matches("file:write", "tests/test.rs"));
    }

    #[test]
    fn test_wildcard_permission() {
        let rule = PermissionRule::new("file:*", "**", Action::Ask);
        let compiled = CompiledRule::compile(rule).unwrap();

        assert!(compiled.matches("file:read", "any/path"));
        assert!(compiled.matches("file:write", "any/path"));
        assert!(compiled.matches("file:delete", "any/path"));
        assert!(!compiled.matches("shell:execute", "any/path"));
    }

    #[test]
    fn test_default_rules() {
        let rule = PermissionRule::allow_all_reads();
        assert_eq!(rule.permission, "file:read");
        assert_eq!(rule.action, Action::Allow);

        let rule = PermissionRule::ask_for_shell();
        assert_eq!(rule.permission, "shell:execute");
        assert_eq!(rule.action, Action::Ask);
    }

    #[test]
    fn test_rule_serialization() {
        let rule = PermissionRule::new("file:write", "src/**", Action::Deny);
        let json = serde_json::to_string(&rule).unwrap();
        assert!(json.contains("\"permission\":\"file:write\""));
        assert!(json.contains("\"action\":\"deny\""));

        let parsed: PermissionRule = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.permission, rule.permission);
        assert_eq!(parsed.action, rule.action);
    }
}
