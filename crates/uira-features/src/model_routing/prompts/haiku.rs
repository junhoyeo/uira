//! Haiku (LOW tier) prompt style: Minimal, direct, no elaboration
//!
//! Optimized for:
//! - Quick lookups and searches
//! - Simple code changes
//! - Fast iteration
//! - Token efficiency

pub const PREFIX: &str = "Execute quickly and concisely.";

pub const SUFFIX: &str = "Be brief. Return result.";

/// Task-specific instructions for Haiku tier
pub fn get_task_instructions(task_type: &str) -> &'static str {
    match task_type {
        "search" | "find" | "locate" => "Find and list matches. No explanation needed.",

        "read" | "show" | "display" => "Show the content directly.",

        "analyze" | "check" => "State findings briefly.",

        "edit" | "modify" | "change" => "Make the change. Confirm when done.",

        "create" | "add" => "Create it. Report completion.",

        "delete" | "remove" => "Remove it. Confirm removal.",

        "test" | "verify" => "Run test. Report pass/fail.",

        "debug" | "fix" => "Identify issue. Fix it.",

        "refactor" => "Refactor as requested.",

        "document" => "Add brief documentation.",

        _ => "Complete the task efficiently.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prefix_is_concise() {
        assert!(PREFIX.len() < 50);
        assert!(PREFIX.contains("quickly"));
    }

    #[test]
    fn test_suffix_is_minimal() {
        assert!(SUFFIX.len() < 30);
        assert!(SUFFIX.contains("brief"));
    }

    #[test]
    fn test_task_instructions_are_direct() {
        let search_inst = get_task_instructions("search");
        assert!(search_inst.contains("Find"));
        assert!(search_inst.contains("No explanation"));

        let edit_inst = get_task_instructions("edit");
        assert!(edit_inst.contains("Make the change"));
        assert!(edit_inst.len() < 50);
    }
}
