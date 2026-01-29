mod haiku;
mod opus;
mod sonnet;

use crate::model_routing::types::ModelTier;

/// Delegation context for creating comprehensive prompts
#[derive(Debug, Clone, Default)]
pub struct DelegationContext {
    pub task_type: Option<String>,
    pub file_paths: Vec<String>,
    pub dependencies: Vec<String>,
    pub constraints: Vec<String>,
    pub previous_attempts: Option<u32>,
    pub expected_output: Option<String>,
}

/// Adapt a prompt for a specific tier by adding tier-appropriate framing
pub fn adapt_prompt_for_tier(prompt: &str, tier: ModelTier) -> String {
    let prefix = get_prompt_prefix(tier);
    let suffix = get_prompt_suffix(tier);

    format!("{}\n\n{}\n\n{}", prefix, prompt.trim(), suffix)
}

/// Get the tier-specific prompt prefix
pub fn get_prompt_prefix(tier: ModelTier) -> &'static str {
    match tier {
        ModelTier::Low => haiku::PREFIX,
        ModelTier::Medium => sonnet::PREFIX,
        ModelTier::High => opus::PREFIX,
    }
}

/// Get the tier-specific prompt suffix
pub fn get_prompt_suffix(tier: ModelTier) -> &'static str {
    match tier {
        ModelTier::Low => haiku::SUFFIX,
        ModelTier::Medium => sonnet::SUFFIX,
        ModelTier::High => opus::SUFFIX,
    }
}

/// Create a comprehensive delegation prompt with context
pub fn create_delegation_prompt(
    tier: ModelTier,
    task: &str,
    context: &DelegationContext,
) -> String {
    let prefix = get_prompt_prefix(tier);
    let suffix = get_prompt_suffix(tier);

    let mut parts = vec![prefix.to_string()];

    // Add task description
    parts.push(format!("\n## Task\n{}", task.trim()));

    // Add context sections based on tier
    match tier {
        ModelTier::Low => {
            // Minimal context for Haiku
            if !context.file_paths.is_empty() {
                parts.push(format!("\nFiles: {}", context.file_paths.join(", ")));
            }
        }
        ModelTier::Medium => {
            // Structured context for Sonnet
            if let Some(ref task_type) = context.task_type {
                parts.push(format!("\n## Task Type\n{}", task_type));
            }
            if !context.file_paths.is_empty() {
                parts.push(format!("\n## Files\n- {}", context.file_paths.join("\n- ")));
            }
            if !context.constraints.is_empty() {
                parts.push(format!(
                    "\n## Constraints\n- {}",
                    context.constraints.join("\n- ")
                ));
            }
        }
        ModelTier::High => {
            // Comprehensive context for Opus
            if let Some(ref task_type) = context.task_type {
                parts.push(format!("\n## Task Type\n{}", task_type));
            }
            if !context.file_paths.is_empty() {
                parts.push(format!(
                    "\n## Target Files\n- {}",
                    context.file_paths.join("\n- ")
                ));
            }
            if !context.dependencies.is_empty() {
                parts.push(format!(
                    "\n## Dependencies\n- {}",
                    context.dependencies.join("\n- ")
                ));
            }
            if !context.constraints.is_empty() {
                parts.push(format!(
                    "\n## Constraints\n- {}",
                    context.constraints.join("\n- ")
                ));
            }
            if let Some(attempts) = context.previous_attempts {
                parts.push(format!(
                    "\n## Previous Attempts\nThis task has been attempted {} time(s) before. Consider alternative approaches.",
                    attempts
                ));
            }
            if let Some(ref expected) = context.expected_output {
                parts.push(format!("\n## Expected Output\n{}", expected));
            }
        }
    }

    parts.push(format!("\n{}", suffix));

    parts.join("\n")
}

/// Get tier-specific task instructions
pub fn get_task_instructions(tier: ModelTier, task_type: &str) -> &'static str {
    match tier {
        ModelTier::Low => haiku::get_task_instructions(task_type),
        ModelTier::Medium => sonnet::get_task_instructions(task_type),
        ModelTier::High => opus::get_task_instructions(task_type),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adapt_prompt_for_tier() {
        let prompt = "Fix the bug in auth.ts";

        let haiku_prompt = adapt_prompt_for_tier(prompt, ModelTier::Low);
        assert!(haiku_prompt.contains("Execute quickly"));
        assert!(haiku_prompt.contains("Be brief"));

        let sonnet_prompt = adapt_prompt_for_tier(prompt, ModelTier::Medium);
        assert!(sonnet_prompt.contains("Execute this task efficiently"));
        assert!(sonnet_prompt.contains("structured output"));

        let opus_prompt = adapt_prompt_for_tier(prompt, ModelTier::High);
        assert!(opus_prompt.contains("complex task"));
        assert!(opus_prompt.contains("thorough analysis"));
    }

    #[test]
    fn test_delegation_prompt_with_context() {
        let context = DelegationContext {
            task_type: Some("debugging".to_string()),
            file_paths: vec!["src/auth.ts".to_string()],
            constraints: vec!["No breaking changes".to_string()],
            previous_attempts: Some(2),
            ..Default::default()
        };

        let prompt =
            create_delegation_prompt(ModelTier::High, "Debug authentication failure", &context);

        assert!(prompt.contains("Task Type"));
        assert!(prompt.contains("Target Files"));
        assert!(prompt.contains("Constraints"));
        assert!(prompt.contains("Previous Attempts"));
        assert!(prompt.contains("attempted 2 time(s)"));
    }

    #[test]
    fn test_minimal_context_for_haiku() {
        let context = DelegationContext {
            file_paths: vec!["src/auth.ts".to_string()],
            ..Default::default()
        };

        let prompt = create_delegation_prompt(ModelTier::Low, "Find the bug", &context);

        // Haiku should have minimal formatting
        assert!(!prompt.contains("## Task Type"));
        assert!(prompt.contains("Files:"));
        assert!(prompt.contains("Be brief"));
    }

    #[test]
    fn test_structured_context_for_sonnet() {
        let context = DelegationContext {
            task_type: Some("implementation".to_string()),
            file_paths: vec!["src/feature.ts".to_string()],
            constraints: vec!["Follow existing patterns".to_string()],
            ..Default::default()
        };

        let prompt = create_delegation_prompt(ModelTier::Medium, "Implement feature", &context);

        assert!(prompt.contains("## Task Type"));
        assert!(prompt.contains("## Files"));
        assert!(prompt.contains("## Constraints"));
        assert!(prompt.contains("structured output"));
    }
}
