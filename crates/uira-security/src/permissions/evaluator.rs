//! Permission evaluator
//!
//! Evaluates permissions against a set of rules.
//! Default action is Allow (per user preference in plan).

use super::pattern::PatternError;
use super::rule::{CompiledRule, PermissionRule};
use super::types::{Action, Permission};

/// Result of permission evaluation
#[derive(Debug, Clone)]
pub struct EvaluationResult {
    /// The action to take
    pub action: Action,
    /// The rule that matched (if any)
    pub matched_rule: Option<String>,
    /// The permission that was evaluated
    pub permission: String,
    /// The path that was evaluated
    pub path: String,
}

impl EvaluationResult {
    /// Create a result with no matching rule (uses default action)
    pub fn default_action(permission: String, path: String) -> Self {
        Self {
            action: Action::Allow, // Default is Allow
            matched_rule: None,
            permission,
            path,
        }
    }

    /// Create a result with a matching rule
    pub fn from_rule(permission: String, path: String, action: Action, rule_name: String) -> Self {
        Self {
            action,
            matched_rule: Some(rule_name),
            permission,
            path,
        }
    }

    /// Check if the action allows the operation
    pub fn is_allowed(&self) -> bool {
        self.action.is_allow()
    }

    /// Check if the action denies the operation
    pub fn is_denied(&self) -> bool {
        self.action.is_deny()
    }

    /// Check if the action requires asking the user
    pub fn needs_approval(&self) -> bool {
        self.action.is_ask()
    }
}

/// Permission evaluator that checks permissions against rules
///
/// Rules are evaluated in order, with later rules overriding earlier ones.
/// If no rule matches, the default action is Allow.
#[derive(Debug, Default)]
pub struct PermissionEvaluator {
    /// Compiled rules for efficient matching
    rules: Vec<CompiledRule>,
}

impl PermissionEvaluator {
    /// Create a new empty evaluator
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// Create an evaluator with the given rules
    pub fn with_rules(rules: Vec<PermissionRule>) -> Result<Self, PatternError> {
        let compiled = rules
            .into_iter()
            .map(CompiledRule::compile)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self { rules: compiled })
    }

    /// Add a rule to the evaluator
    pub fn add_rule(&mut self, rule: PermissionRule) -> Result<(), PatternError> {
        self.rules.push(CompiledRule::compile(rule)?);
        Ok(())
    }

    /// Evaluate a permission for a given path
    ///
    /// Returns the action to take based on the rules.
    /// Later rules override earlier ones (last match wins).
    /// Default action is Allow if no rules match.
    pub fn evaluate(&self, permission: &str, path: &str) -> EvaluationResult {
        // Find the last matching rule (later rules override earlier)
        let matched = self
            .rules
            .iter()
            .rev()
            .find(|r| r.matches(permission, path));

        match matched {
            Some(rule) => {
                let rule_name = rule.rule().name.clone().unwrap_or_else(|| {
                    format!("{}:{}", rule.rule().permission, rule.rule().pattern)
                });

                EvaluationResult::from_rule(
                    permission.to_string(),
                    path.to_string(),
                    rule.action(),
                    rule_name,
                )
            }
            None => EvaluationResult::default_action(permission.to_string(), path.to_string()),
        }
    }

    /// Evaluate a permission for a tool and input
    ///
    /// Extracts the path from common tool input formats.
    pub fn evaluate_tool(&self, tool_name: &str, input: &serde_json::Value) -> EvaluationResult {
        let permission = Permission::from_tool_name(tool_name);
        let path = extract_path_from_input(input);

        self.evaluate(permission.as_str(), &path)
    }

    /// Get the number of rules
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// Check if evaluator has any rules
    pub fn has_rules(&self) -> bool {
        !self.rules.is_empty()
    }
}

/// Extract a path from tool input
///
/// Looks for common path field names in tool inputs.
fn extract_path_from_input(input: &serde_json::Value) -> String {
    // Common path field names in order of priority
    let path_fields = [
        "path",
        "file_path",
        "filePath",
        "file",
        "url",
        "uri",
        "query",
        "target",
        "directory",
        "dir",
    ];

    for field in path_fields {
        if let Some(path) = input.get(field).and_then(|v| v.as_str()) {
            return path.to_string();
        }
    }

    // For bash/shell commands, use the command itself
    if let Some(command) = input.get("command").and_then(|v| v.as_str()) {
        return command.to_string();
    }

    // Fallback: use the whole input as a string representation
    input.to_string()
}

/// Builder for creating an evaluator with a fluent API
pub struct EvaluatorBuilder {
    rules: Vec<PermissionRule>,
}

impl EvaluatorBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// Add a rule
    pub fn rule(mut self, rule: PermissionRule) -> Self {
        self.rules.push(rule);
        self
    }

    /// Allow all reads
    pub fn allow_reads(self) -> Self {
        self.rule(PermissionRule::allow_all_reads())
    }

    /// Ask for all writes
    pub fn ask_for_writes(self) -> Self {
        self.rule(PermissionRule::ask_for_writes())
    }

    /// Allow workspace writes
    pub fn allow_workspace(self, workspace: &str) -> Self {
        self.rule(PermissionRule::allow_workspace_writes(workspace))
    }

    /// Ask for shell commands
    pub fn ask_for_shell(self) -> Self {
        self.rule(PermissionRule::ask_for_shell())
    }

    /// Build the evaluator
    pub fn build(self) -> Result<PermissionEvaluator, PatternError> {
        PermissionEvaluator::with_rules(self.rules)
    }
}

impl Default for EvaluatorBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_evaluator_allows_all() {
        let evaluator = PermissionEvaluator::new();

        let result = evaluator.evaluate("file:write", "any/path");
        assert!(result.is_allowed());
        assert!(result.matched_rule.is_none());
    }

    #[test]
    fn test_single_rule() {
        let evaluator = PermissionEvaluator::with_rules(vec![PermissionRule::new(
            "file:write",
            "src/**",
            Action::Allow,
        )])
        .unwrap();

        let result = evaluator.evaluate("file:write", "src/main.rs");
        assert!(result.is_allowed());
        assert!(result.matched_rule.is_some());
    }

    #[test]
    fn test_last_rule_wins() {
        let evaluator = PermissionEvaluator::with_rules(vec![
            PermissionRule::new("file:write", "**", Action::Deny),
            PermissionRule::new("file:write", "src/**", Action::Allow),
        ])
        .unwrap();

        // src/ matches both rules, but last (Allow) wins
        let result = evaluator.evaluate("file:write", "src/main.rs");
        assert!(result.is_allowed());

        // tests/ only matches first rule (Deny)
        let result = evaluator.evaluate("file:write", "tests/test.rs");
        assert!(result.is_denied());
    }

    #[test]
    fn test_evaluate_tool() {
        let evaluator = PermissionEvaluator::with_rules(vec![PermissionRule::new(
            "file:write",
            "src/**",
            Action::Ask,
        )])
        .unwrap();

        let input = serde_json::json!({
            "file_path": "src/main.rs",
            "content": "fn main() {}"
        });

        let result = evaluator.evaluate_tool("write", &input);
        assert!(result.needs_approval());
    }

    #[test]
    fn test_builder() {
        let evaluator = EvaluatorBuilder::new()
            .allow_reads()
            .ask_for_writes()
            .allow_workspace("/project/src")
            .build()
            .unwrap();

        assert_eq!(evaluator.rule_count(), 3);

        // Reads are allowed
        let result = evaluator.evaluate("file:read", "/any/path");
        assert!(result.is_allowed());

        // Writes to workspace are allowed (overrides ask_for_writes)
        let result = evaluator.evaluate("file:write", "/project/src/main.rs");
        assert!(result.is_allowed());

        // Writes outside workspace need approval
        let result = evaluator.evaluate("file:write", "/other/path");
        assert!(result.needs_approval());
    }

    #[test]
    fn test_extract_path_from_input() {
        let input = serde_json::json!({"file_path": "/src/main.rs"});
        assert_eq!(extract_path_from_input(&input), "/src/main.rs");

        let input = serde_json::json!({"path": "/different/path"});
        assert_eq!(extract_path_from_input(&input), "/different/path");

        let input = serde_json::json!({"url": "https://example.com/docs"});
        assert_eq!(extract_path_from_input(&input), "https://example.com/docs");

        let input = serde_json::json!({"query": "rust serde"});
        assert_eq!(extract_path_from_input(&input), "rust serde");

        let input = serde_json::json!({"command": "ls -la"});
        assert_eq!(extract_path_from_input(&input), "ls -la");

        let input = serde_json::json!({"unknown": "field"});
        assert!(extract_path_from_input(&input).contains("unknown"));
    }

    #[test]
    fn test_evaluation_result_methods() {
        let result = EvaluationResult::default_action("file:read".into(), "/path".into());
        assert!(result.is_allowed());
        assert!(!result.is_denied());
        assert!(!result.needs_approval());

        let result = EvaluationResult::from_rule(
            "file:write".into(),
            "/path".into(),
            Action::Ask,
            "rule1".into(),
        );
        assert!(!result.is_allowed());
        assert!(!result.is_denied());
        assert!(result.needs_approval());
    }
}
