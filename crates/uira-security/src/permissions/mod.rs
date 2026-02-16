//! Pattern-based permission evaluation for Uira
//!
//! This crate provides a flexible permission system based on glob patterns.
//! Permissions are evaluated against a set of rules, with the last matching
//! rule taking precedence. The default action (when no rules match) is Allow.
//!
//! # Architecture
//!
//! ```text
//! Tool Request
//!      │
//!      ▼
//! ┌─────────────────────┐
//! │ PermissionEvaluator │
//! │                     │
//! │  Rules (in order):  │
//! │  1. file:read → **  │ → Allow
//! │  2. file:write → ** │ → Ask
//! │  3. file:write →    │
//! │     src/**          │ → Allow (overrides #2)
//! └─────────────────────┘
//!      │
//!      ▼
//! EvaluationResult { action: Allow/Deny/Ask }
//! ```
//!
//! # Example
//!
//! ```rust
//! use uira_security::permissions::{PermissionEvaluator, PermissionRule, Action, EvaluatorBuilder};
//!
//! // Using the builder
//! let evaluator = EvaluatorBuilder::new()
//!     .allow_reads()
//!     .ask_for_writes()
//!     .allow_workspace("./src")
//!     .build()
//!     .unwrap();
//!
//! // Evaluate a permission
//! let result = evaluator.evaluate("file:write", "./src/main.rs");
//! assert!(result.is_allowed());
//!
//! // Writes outside workspace need approval
//! let result = evaluator.evaluate("file:write", "./config/secret.yml");
//! assert!(result.needs_approval());
//! ```
//!
//! # Configuration
//!
//! Permissions can be configured in `uira.yml`:
//!
//! ```yaml
//! permissions:
//!   rules:
//!     - permission: "file:read"
//!       pattern: "**"
//!       action: allow
//!
//!     - permission: "file:write"
//!       pattern: "**"
//!       action: ask
//!
//!     - permission: "file:write"
//!       pattern: "src/**"
//!       action: allow
//!       name: "allow-src-writes"
//!
//!     - permission: "shell:execute"
//!       pattern: "**"
//!       action: ask
//! ```

pub mod config;
pub mod evaluator;
pub mod pattern;
pub mod rule;
pub mod types;

pub use config::{build_evaluator_from_rules, ConfigAction, ConfigRule};
pub use evaluator::{EvaluationResult, EvaluatorBuilder, PermissionEvaluator};
pub use pattern::{expand_path, normalize_path, Pattern, PatternError};
pub use rule::{CompiledRule, PermissionRule};
pub use types::{Action, Permission};

/// Prelude for common imports
pub mod prelude {
    pub use super::evaluator::{EvaluationResult, EvaluatorBuilder, PermissionEvaluator};
    pub use super::rule::PermissionRule;
    pub use super::types::{Action, Permission};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_full_workflow() {
        // Create rules like a typical configuration
        let rules = vec![
            // Allow all reads
            PermissionRule::new("file:read", "**", Action::Allow),
            // Ask for writes by default
            PermissionRule::new("file:write", "**", Action::Ask),
            // But allow writes to src/
            PermissionRule::new("file:write", "src/**", Action::Allow),
            // Deny writes to sensitive files
            PermissionRule::new("file:write", "**/.env*", Action::Deny),
            // Ask for shell commands
            PermissionRule::new("shell:execute", "**", Action::Ask),
        ];

        let evaluator = PermissionEvaluator::with_rules(rules).unwrap();

        // Test various scenarios
        assert!(evaluator.evaluate("file:read", "any/file.txt").is_allowed());
        assert!(evaluator.evaluate("file:write", "src/main.rs").is_allowed());
        assert!(evaluator
            .evaluate("file:write", "tests/test.rs")
            .needs_approval());
        assert!(evaluator.evaluate("file:write", ".env.local").is_denied());
        assert!(evaluator
            .evaluate("shell:execute", "ls -la")
            .needs_approval());

        // Unknown permission defaults to Allow
        assert!(evaluator
            .evaluate("unknown:permission", "any/path")
            .is_allowed());
    }

    #[test]
    fn test_integration_with_tools() {
        let evaluator = EvaluatorBuilder::new()
            .allow_reads()
            .ask_for_writes()
            .allow_workspace("./workspace")
            .ask_for_shell()
            .build()
            .unwrap();

        // Simulate tool calls
        let write_input = serde_json::json!({
            "file_path": "./workspace/src/lib.rs",
            "content": "// new content"
        });
        let result = evaluator.evaluate_tool("write", &write_input);
        assert!(result.is_allowed());

        let bash_input = serde_json::json!({
            "command": "cargo build"
        });
        let result = evaluator.evaluate_tool("bash", &bash_input);
        assert!(result.needs_approval());
    }
}
