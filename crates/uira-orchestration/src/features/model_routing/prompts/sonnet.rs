//! Sonnet (MEDIUM tier) prompt style: Balanced, structured, clear
//!
//! Optimized for:
//! - Standard implementation tasks
//! - Feature development
//! - Moderate complexity debugging
//! - Clear documentation

pub const PREFIX: &str = "Execute this task efficiently while maintaining quality.";

pub const SUFFIX: &str = "Provide clear, structured output.";

/// Task-specific instructions for Sonnet tier
pub fn get_task_instructions(task_type: &str) -> &'static str {
    match task_type {
        "search" | "find" | "locate" => {
            "Search thoroughly and present results in a structured format. \
             Include file paths, line numbers, and relevant context."
        }

        "read" | "show" | "display" => {
            "Display the content with clear formatting. \
             Highlight important sections and provide brief context."
        }

        "analyze" | "check" => {
            "Analyze systematically and present findings in organized sections. \
             Include what you found, implications, and recommendations."
        }

        "edit" | "modify" | "change" => {
            "Make the requested changes following these steps:\n\
             1. Read and understand current code\n\
             2. Apply changes carefully\n\
             3. Verify the changes work\n\
             4. Report what was changed and why"
        }

        "create" | "add" => {
            "Create the requested component following best practices:\n\
             1. Follow existing code patterns\n\
             2. Add appropriate error handling\n\
             3. Include inline documentation\n\
             4. Verify it integrates properly"
        }

        "delete" | "remove" => {
            "Remove the specified code:\n\
             1. Verify what will be removed\n\
             2. Check for dependencies\n\
             3. Remove cleanly\n\
             4. Confirm successful removal"
        }

        "test" | "verify" => {
            "Test thoroughly:\n\
             1. Understand test requirements\n\
             2. Execute relevant tests\n\
             3. Report results with details\n\
             4. Suggest fixes if failures occur"
        }

        "debug" | "fix" => {
            "Debug systematically:\n\
             1. Reproduce the issue\n\
             2. Identify root cause\n\
             3. Implement fix\n\
             4. Verify resolution"
        }

        "refactor" => {
            "Refactor carefully:\n\
             1. Understand current implementation\n\
             2. Plan refactoring approach\n\
             3. Make changes incrementally\n\
             4. Verify functionality preserved"
        }

        "document" => {
            "Create clear documentation:\n\
             1. Understand the component\n\
             2. Document purpose and usage\n\
             3. Include examples\n\
             4. Note any gotchas"
        }

        "implement" | "build" => {
            "Implement the feature following this workflow:\n\
             1. Understand requirements\n\
             2. Plan implementation approach\n\
             3. Write clean, maintainable code\n\
             4. Test and verify"
        }

        _ => {
            "Complete the task following these principles:\n\
             1. Understand the requirements\n\
             2. Execute systematically\n\
             3. Maintain code quality\n\
             4. Verify results"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prefix_mentions_quality() {
        assert!(PREFIX.contains("quality"));
        assert!(PREFIX.contains("efficiently"));
    }

    #[test]
    fn test_suffix_mentions_structure() {
        assert!(SUFFIX.contains("structured"));
        assert!(SUFFIX.contains("clear"));
    }

    #[test]
    fn test_task_instructions_have_structure() {
        let edit_inst = get_task_instructions("edit");
        assert!(edit_inst.contains("1."));
        assert!(edit_inst.contains("2."));
        assert!(edit_inst.contains("3."));
        assert!(edit_inst.contains("4."));

        let search_inst = get_task_instructions("search");
        assert!(search_inst.contains("structured format"));
        assert!(search_inst.contains("context"));
    }
}
