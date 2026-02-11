//! Opus (HIGH tier) prompt style: Deep, analytical, comprehensive
//!
//! Optimized for:
//! - Complex architectural decisions
//! - Deep debugging and root cause analysis
//! - Critical production issues
//! - Security-sensitive tasks

pub const PREFIX: &str = "This is a complex task requiring deep analysis. \
Consider multiple angles, trade-offs, and edge cases.";

pub const SUFFIX: &str = "Provide thorough analysis with your reasoning process.";

/// Task-specific instructions for Opus tier
pub fn get_task_instructions(task_type: &str) -> &'static str {
    match task_type {
        "search" | "find" | "locate" => {
            "Conduct a comprehensive search across the codebase:\n\
             1. Search all relevant locations systematically\n\
             2. Analyze patterns and relationships between matches\n\
             3. Consider edge cases and related code paths\n\
             4. Present findings with full context and implications\n\
             5. Suggest additional areas to investigate\n\n\
             Think carefully about what might be hidden or non-obvious."
        }

        "read" | "show" | "display" => {
            "Examine the content thoroughly:\n\
             1. Display the requested content in full\n\
             2. Analyze its structure and purpose\n\
             3. Identify key patterns and dependencies\n\
             4. Explain how it fits into the larger system\n\
             5. Note any concerns or optimization opportunities\n\n\
             Consider the broader architectural context."
        }

        "analyze" | "check" => {
            "Perform deep analysis:\n\
             1. Examine the subject from multiple perspectives\n\
             2. Identify patterns, anti-patterns, and edge cases\n\
             3. Consider security, performance, and maintainability implications\n\
             4. Evaluate trade-offs and alternatives\n\
             5. Provide detailed findings with reasoning\n\
             6. Recommend specific improvements with justification\n\n\
             Challenge assumptions and think critically about what could go wrong."
        }

        "edit" | "modify" | "change" => {
            "Approach this change with careful consideration:\n\
             1. Deeply understand the current implementation and its history\n\
             2. Analyze all dependencies and potential side effects\n\
             3. Consider multiple implementation approaches\n\
             4. Evaluate trade-offs (performance, maintainability, complexity)\n\
             5. Implement the change with defensive coding\n\
             6. Add comprehensive error handling and validation\n\
             7. Verify changes don't introduce regressions\n\
             8. Document reasoning and any caveats\n\n\
             Think about edge cases, backward compatibility, and future extensibility."
        }

        "create" | "add" => {
            "Design and implement with architectural awareness:\n\
             1. Understand the full context and requirements\n\
             2. Research existing patterns and conventions in the codebase\n\
             3. Design the solution considering extensibility and maintainability\n\
             4. Evaluate multiple architectural approaches\n\
             5. Consider security, performance, and error handling from the start\n\
             6. Implement with clean abstractions and clear boundaries\n\
             7. Add comprehensive documentation and examples\n\
             8. Verify integration with existing systems\n\n\
             Think about how this will be maintained and extended in the future."
        }

        "delete" | "remove" => {
            "Remove code with careful impact analysis:\n\
             1. Map all dependencies and usages\n\
             2. Analyze potential breaking changes\n\
             3. Consider migration path for dependents\n\
             4. Plan rollback strategy if needed\n\
             5. Remove code and update all references\n\
             6. Verify no orphaned code or broken dependencies\n\
             7. Update documentation and tests\n\n\
             Consider the ripple effects across the system."
        }

        "test" | "verify" => {
            "Test comprehensively:\n\
             1. Understand the full scope of what needs testing\n\
             2. Design test strategy covering edge cases\n\
             3. Consider integration and system-level impacts\n\
             4. Execute tests and analyze results deeply\n\
             5. Investigate any anomalies or unexpected behavior\n\
             6. Verify both positive and negative cases\n\
             7. Document test coverage and any gaps\n\n\
             Think about what could break in production that tests might miss."
        }

        "debug" | "fix" => {
            "Investigate the root cause thoroughly:\n\
             1. Reproduce the issue reliably\n\
             2. Trace execution flow and state changes\n\
             3. Form hypotheses about potential causes\n\
             4. Test each hypothesis systematically\n\
             5. Identify the true root cause (not just symptoms)\n\
             6. Consider why this bug wasn't caught earlier\n\
             7. Design fix that prevents entire class of issues\n\
             8. Verify fix doesn't introduce new problems\n\
             9. Add tests to prevent regression\n\n\
             Think deeply about why this happened and how to prevent similar issues."
        }

        "refactor" => {
            "Refactor with architectural vision:\n\
             1. Understand current implementation deeply\n\
             2. Identify code smells and anti-patterns\n\
             3. Design target architecture with clear principles\n\
             4. Plan incremental refactoring path\n\
             5. Consider impact on all dependents\n\
             6. Preserve behavior while improving structure\n\
             7. Add tests to verify behavior preservation\n\
             8. Document architectural decisions\n\n\
             Think about long-term maintainability and evolution."
        }

        "document" => {
            "Create comprehensive documentation:\n\
             1. Understand the component deeply\n\
             2. Document architecture and design decisions\n\
             3. Explain not just what but why\n\
             4. Include usage examples and common patterns\n\
             5. Document edge cases and gotchas\n\
             6. Provide troubleshooting guidance\n\
             7. Consider audience needs (developers, users, ops)\n\n\
             Think about what future maintainers will need to know."
        }

        "implement" | "build" => {
            "Architect and implement with rigor:\n\
             1. Deeply understand requirements and constraints\n\
             2. Research relevant patterns and prior art\n\
             3. Design architecture considering trade-offs\n\
             4. Plan for extensibility and future requirements\n\
             5. Implement with clean abstractions\n\
             6. Add comprehensive error handling and logging\n\
             7. Write thorough tests including edge cases\n\
             8. Document design decisions and usage\n\n\
             Think about scale, security, and long-term maintenance."
        }

        "architecture" | "design" => {
            "Design with strategic thinking:\n\
             1. Understand business and technical requirements\n\
             2. Research existing patterns and best practices\n\
             3. Evaluate multiple architectural approaches\n\
             4. Consider trade-offs deeply (coupling, complexity, performance)\n\
             5. Design for change and evolution\n\
             6. Document decisions and rationale\n\
             7. Identify risks and mitigation strategies\n\n\
             Think about how this will evolve over years, not just months."
        }

        "security" => {
            "Analyze security with threat modeling:\n\
             1. Identify all trust boundaries and attack surfaces\n\
             2. Consider various threat actors and motivations\n\
             3. Analyze potential vulnerabilities systematically\n\
             4. Evaluate impact and likelihood of each threat\n\
             5. Recommend defense-in-depth mitigations\n\
             6. Consider both technical and process controls\n\
             7. Verify mitigations are complete and correct\n\n\
             Think like an attacker - what would you target?"
        }

        _ => {
            "Approach this with depth and rigor:\n\
             1. Understand the full context and implications\n\
             2. Consider multiple approaches and trade-offs\n\
             3. Think through edge cases and failure modes\n\
             4. Execute with attention to quality\n\
             5. Verify thoroughly from multiple angles\n\
             6. Document reasoning and decisions\n\n\
             Challenge assumptions and think critically about correctness and maintainability."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prefix_emphasizes_depth() {
        assert!(PREFIX.contains("complex task"));
        assert!(PREFIX.contains("deep analysis"));
        assert!(PREFIX.contains("trade-offs"));
    }

    #[test]
    fn test_suffix_requests_reasoning() {
        assert!(SUFFIX.contains("thorough"));
        assert!(SUFFIX.contains("reasoning"));
    }

    #[test]
    fn test_task_instructions_are_comprehensive() {
        let debug_inst = get_task_instructions("debug");
        assert!(debug_inst.contains("root cause"));
        assert!(debug_inst.contains("hypotheses"));
        assert!(debug_inst.contains("Think deeply"));
        // Should have many steps
        assert!(debug_inst.matches("1.").count() >= 1);
        assert!(debug_inst.matches("\n").count() > 10);

        let architecture_inst = get_task_instructions("architecture");
        assert!(architecture_inst.contains("strategic"));
        assert!(architecture_inst.contains("trade-offs"));
        assert!(architecture_inst.contains("evolve"));
    }

    #[test]
    fn test_security_task_has_threat_modeling() {
        let security_inst = get_task_instructions("security");
        assert!(security_inst.contains("threat"));
        assert!(security_inst.contains("attack"));
        assert!(security_inst.contains("defense-in-depth"));
    }
}
