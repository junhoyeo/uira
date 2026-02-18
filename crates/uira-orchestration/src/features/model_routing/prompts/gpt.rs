//! GPT/OpenAI-specific prompt style adaptations
//!
//! GPT models respond differently to prompts than Claude. Key differences:
//! - GPT models prefer more structured, explicit instructions
//! - They benefit from numbered steps and clear formatting
//! - They handle JSON output schemas better than XML
//! - Extended thinking uses `reasoningEffort` instead of `thinking.budgetTokens`
//!
//! This module provides GPT-optimized prefixes, suffixes, and task instructions
//! that can be used when the detected provider is OpenAI/GPT.

/// GPT-optimized prefix for low-tier tasks
pub const LOW_PREFIX: &str = "TASK: Execute the following. Be concise. Return only the result.";

/// GPT-optimized suffix for low-tier tasks
pub const LOW_SUFFIX: &str = "Output only the result. No explanation.";

/// GPT-optimized prefix for medium-tier tasks
pub const MEDIUM_PREFIX: &str = "\
You are a skilled software engineer. Follow these instructions precisely.\n\
- Read all relevant code before making changes\n\
- Follow existing patterns in the codebase\n\
- Verify your changes compile and work correctly";

/// GPT-optimized suffix for medium-tier tasks
pub const MEDIUM_SUFFIX: &str = "\
Respond with:\n\
1. What you found/changed\n\
2. How you verified it works\n\
3. Any concerns or follow-ups";

/// GPT-optimized prefix for high-tier tasks
pub const HIGH_PREFIX: &str = "\
You are a senior software architect. This task requires careful analysis.\n\n\
IMPORTANT INSTRUCTIONS:\n\
- Consider all edge cases and failure modes\n\
- Evaluate multiple approaches before choosing one\n\
- Provide evidence for your conclusions (file:line references)\n\
- Think step by step through complex problems\n\
- Do NOT skip verification steps";

/// GPT-optimized suffix for high-tier tasks
pub const HIGH_SUFFIX: &str = "\
Structure your response as:\n\
## Analysis\n\
[Your analysis with specific file:line references]\n\
## Approach\n\
[Chosen approach with rationale]\n\
## Changes\n\
[What was changed and why]\n\
## Verification\n\
[How you verified the changes work]";

/// Detect if a model string is a GPT/OpenAI model
pub fn is_gpt_model(model: &str) -> bool {
    let lower = model.to_lowercase();
    let model_part = lower.rsplit('/').next().unwrap_or(lower.as_str());

    if is_gpt_family_model(model_part) || lower.contains("openai/") || lower.contains("openai-") {
        return true;
    }

    false
}

fn is_gpt_family_model(model_part: &str) -> bool {
    model_part.starts_with("gpt-")
        || model_part.starts_with("gpt4")
        || matches!(
            (model_part.chars().next(), model_part.chars().nth(1)),
            (Some('o'), Some(digit)) if digit.is_ascii_digit()
        )
}

/// Get GPT-optimized task instructions
pub fn get_task_instructions(task_type: &str) -> &'static str {
    match task_type {
        "search" | "find" | "locate" => {
            "Search the codebase for the specified pattern.\n\
             Return results as a numbered list:\n\
             1. file_path:line_number - brief description\n\
             2. file_path:line_number - brief description\n\
             Include at most 10 results. Prioritize most relevant."
        }

        "edit" | "modify" | "change" => {
            "Make the requested changes:\n\
             Step 1: Read the target file(s)\n\
             Step 2: Understand the current implementation\n\
             Step 3: Make minimal, precise changes\n\
             Step 4: Verify changes with diagnostics\n\
             Step 5: Report what changed (old -> new)"
        }

        "debug" | "fix" => {
            "Debug the issue:\n\
             Step 1: Reproduce (find the failing case)\n\
             Step 2: Trace (follow the execution path)\n\
             Step 3: Hypothesize (what could cause this?)\n\
             Step 4: Test (verify each hypothesis)\n\
             Step 5: Fix (minimal change to fix root cause)\n\
             Step 6: Verify (prove the fix works)"
        }

        "analyze" | "check" => {
            "Analyze systematically:\n\
             1. Identify what needs to be analyzed\n\
             2. Examine from multiple angles\n\
             3. Note findings with evidence (file:line)\n\
             4. Prioritize findings by severity\n\
             5. Recommend specific actions"
        }

        _ => {
            "Complete the task:\n\
             1. Understand what's needed\n\
             2. Plan your approach\n\
             3. Execute the changes\n\
             4. Verify the result\n\
             5. Report what was done"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_gpt_model() {
        assert!(is_gpt_model("gpt-4o"));
        assert!(is_gpt_model("gpt-4-turbo"));
        assert!(is_gpt_model("o1-preview"));
        assert!(is_gpt_model("o3-mini"));
        assert!(is_gpt_model("openai/gpt-4"));
        assert!(is_gpt_model("opencode/gpt-5-nano"));
        assert!(is_gpt_model("azure/gpt-4-turbo"));
        assert!(is_gpt_model("custom-provider/o3-mini"));
        assert!(is_gpt_model("custom-provider/o9-preview"));
        assert!(!is_gpt_model("claude-sonnet-4"));
        assert!(!is_gpt_model("claude-opus-4"));
        assert!(!is_gpt_model("custom-provider/claude-o3"));
        assert!(!is_gpt_model("llama-3.1"));
    }

    #[test]
    fn test_gpt_prefixes_are_structured() {
        // GPT prompts should use numbered steps or bullet points
        assert!(MEDIUM_PREFIX.contains('-'));
        assert!(HIGH_PREFIX.contains("IMPORTANT"));
        assert!(HIGH_SUFFIX.contains("##"));
    }

    #[test]
    fn test_gpt_task_instructions() {
        let search = get_task_instructions("search");
        assert!(search.contains("numbered list"));

        let debug = get_task_instructions("debug");
        assert!(debug.contains("Step 1"));
        assert!(debug.contains("Step 6"));
    }
}
