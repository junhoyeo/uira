use once_cell::sync::Lazy;
use regex::Regex;

pub static ALLOWED_PATH_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        Regex::new(r"^\.omc/").unwrap(),
        Regex::new(r"^\.claude/").unwrap(),
        Regex::new(r"^~?/\.claude/").unwrap(),
        Regex::new(r"/\.claude/").unwrap(),
        Regex::new(r"CLAUDE\.md$").unwrap(),
        Regex::new(r"AGENTS\.md$").unwrap(),
    ]
});

pub const WARNED_EXTENSIONS: &[&str] = &[
    ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".py", ".pyw", ".go", ".rs", ".java", ".kt",
    ".scala", ".c", ".cpp", ".cc", ".h", ".hpp", ".rb", ".php", ".svelte", ".vue", ".graphql",
    ".gql", ".sh", ".bash", ".zsh",
];

pub const WRITE_EDIT_TOOLS: &[&str] = &["Write", "Edit", "write", "edit"];

pub const DIRECT_WORK_REMINDER: &str = r#"
---

[SYSTEM REMINDER - DELEGATION REQUIRED]

You just performed direct file modifications outside `.omc/`.

**You are an ORCHESTRATOR, not an IMPLEMENTER.**

As an orchestrator, you should:
- **DELEGATE** implementation work to subagents via the Task tool
- **VERIFY** the work done by subagents
- **COORDINATE** multiple tasks and ensure completion

You should NOT:
- Write code directly (except for `.omc/` files like plans and notepads)
- Make direct file edits outside `.omc/`
- Implement features yourself

**If you need to make changes:**
1. Use the Task tool to delegate to an appropriate subagent
2. Provide clear instructions in the prompt
3. Verify the subagent's work after completion

---
"#;

pub fn orchestrator_delegation_required(file_path: &str) -> String {
    format!(
        r#"
---

[CRITICAL SYSTEM DIRECTIVE - DELEGATION REQUIRED]

**STOP. YOU ARE VIOLATING ORCHESTRATOR PROTOCOL.**

You (coordinator) are attempting to directly modify a file outside `.omc/`.

**Path attempted:** {file_path}

---

**THIS IS FORBIDDEN** (except for VERIFICATION purposes)

As an ORCHESTRATOR, you MUST:
1. **DELEGATE** all implementation work via the Task tool
2. **VERIFY** the work done by subagents (reading files is OK)
3. **COORDINATE** - you orchestrate, you don't implement

**ALLOWED direct file operations:**
- Files inside `.omc/` (plans, notepads, drafts)
- Files inside `~/.claude/` (global config)
- `CLAUDE.md` and `AGENTS.md` files
- Reading files for verification
- Running diagnostics/tests

**FORBIDDEN direct file operations:**
- Writing/editing source code
- Creating new files outside `.omc/`
- Any implementation work

---

**IF THIS IS FOR VERIFICATION:**
Proceed if you are verifying subagent work by making a small fix.
But for any substantial changes, USE the Task tool.

**CORRECT APPROACH:**
```
Task tool with subagent_type="executor"
prompt="[specific single task with clear acceptance criteria]"
```

DELEGATE. DON'T IMPLEMENT.

---
"#
    )
}

pub fn boulder_continuation_prompt(plan_name: &str) -> String {
    format!(
        r#"[SYSTEM REMINDER - BOULDER CONTINUATION]

You have an active work plan with incomplete tasks. Continue working.

RULES:
- Proceed without asking for permission
- Mark each checkbox [x] in the plan file when done
- Use the notepad at .omc/notepads/{plan_name}/ to record learnings
- Do not stop until all tasks are complete
- If blocked, document the blocker and move to the next task"#
    )
}

pub const VERIFICATION_REMINDER: &str = r#"**MANDATORY VERIFICATION - SUBAGENTS LIE**

Subagents FREQUENTLY claim completion when:
- Tests are actually FAILING
- Code has type/lint ERRORS
- Implementation is INCOMPLETE
- Patterns were NOT followed

**YOU MUST VERIFY EVERYTHING YOURSELF:**

1. Run tests yourself - Must PASS (not "agent said it passed")
2. Read the actual code - Must match requirements
3. Check build/typecheck - Must succeed

DO NOT TRUST THE AGENT'S SELF-REPORT.
VERIFY EACH CLAIM WITH YOUR OWN TOOL CALLS."#;

pub const SINGLE_TASK_DIRECTIVE: &str = r#"
[SYSTEM DIRECTIVE - SINGLE TASK ONLY]

**STOP. READ THIS BEFORE PROCEEDING.**

If you were NOT given **exactly ONE atomic task**, you MUST:
1. **IMMEDIATELY REFUSE** this request
2. **DEMAND** the orchestrator provide a single, specific task

**Your response if multiple tasks detected:**
> "I refuse to proceed. You provided multiple tasks. An orchestrator's impatience destroys work quality.
>
> PROVIDE EXACTLY ONE TASK. One file. One change. One verification.
>
> Your rushing will cause: incomplete work, missed edge cases, broken tests, wasted context."

**WARNING TO ORCHESTRATOR:**
- Your hasty batching RUINS deliverables
- Each task needs FULL attention and PROPER verification
- Batch delegation = sloppy work = rework = wasted tokens

**REFUSE multi-task requests. DEMAND single-task clarity.**
"#;

pub fn is_allowed_path(path: &str) -> bool {
    ALLOWED_PATH_PATTERNS.iter().any(|re| re.is_match(path))
}

pub fn is_source_file(path: &str) -> bool {
    WARNED_EXTENSIONS.iter().any(|ext| path.ends_with(ext))
}

pub fn is_write_edit_tool(tool_name: &str) -> bool {
    WRITE_EDIT_TOOLS.contains(&tool_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allowed_paths() {
        assert!(is_allowed_path(".omc/plans/test.md"));
        assert!(is_allowed_path(".claude/config.json"));
        assert!(is_allowed_path("~/. claude/settings.json"));
        assert!(is_allowed_path("CLAUDE.md"));
        assert!(is_allowed_path("docs/AGENTS.md"));
        assert!(!is_allowed_path("src/main.rs"));
        assert!(!is_allowed_path("package.json"));
    }

    #[test]
    fn test_source_files() {
        assert!(is_source_file("src/main.rs"));
        assert!(is_source_file("app/page.tsx"));
        assert!(is_source_file("script.py"));
        assert!(!is_source_file("README.md"));
        assert!(!is_source_file("package.json"));
    }

    #[test]
    fn test_write_edit_tools() {
        assert!(is_write_edit_tool("Write"));
        assert!(is_write_edit_tool("Edit"));
        assert!(is_write_edit_tool("write"));
        assert!(is_write_edit_tool("edit"));
        assert!(!is_write_edit_tool("Read"));
        assert!(!is_write_edit_tool("Bash"));
    }
}
