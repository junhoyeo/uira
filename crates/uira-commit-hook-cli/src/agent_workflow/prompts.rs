use super::{TaskOptions, WorkflowTask};

pub fn build_system_prompt(task: WorkflowTask, options: &TaskOptions) -> String {
    let base = format!(
        r#"You are a code quality fixer. Issues have been pre-detected for you.

## Your Mission
{mission}

## Available Tools
- **Read**: View file contents (for context only)
- **Edit**: Modify files (use oldString/newString for precise edits)

## Your Role
- You will receive a list of pre-detected issues
- For each issue: FIX it with Edit, or SKIP if intentional
- Do NOT run detection tools - issues are already detected
- Do NOT scan the codebase - work only with provided issues

## Completion Protocol
When ALL issues are handled, output:
- `<DONE/>`
- Or with summary: `<DONE>Fixed N issues, skipped M</DONE>`

DO NOT output <DONE/> until you are certain all issues are resolved.
If you encounter an error you cannot fix, explain the issue instead.

## Important Rules
- Make minimal, targeted changes
- Preserve existing code style
- Do not introduce new issues while fixing old ones
- If unsure about a fix, skip it rather than break code
"#,
        mission = task_mission(task, options),
    );

    base
}

fn task_mission(task: WorkflowTask, options: &TaskOptions) -> String {
    match task {
        WorkflowTask::Typos => r#"
Fix or ignore the typos that have been pre-detected for you.

For each typo:
1. If it's a genuine mistake → use Edit to fix it
2. If it's intentional (variable name, domain term) → skip it

Do NOT run the `typos` CLI or scan for issues - detection is already done.
Use Read only if you need to see surrounding code context.

When all typos have been handled (fixed or intentionally skipped), output <DONE/>.
"#
        .to_string(),

        WorkflowTask::Diagnostics => {
            let severity = options.severity.as_deref().unwrap_or("error");
            format!(
                r#"
Fix code diagnostics (errors and warnings). Focus on:
- Severity: {severity} (and above)
- Languages: {languages}

Use `lsp_diagnostics` to find issues, then fix them with Edit.
Common fixes:
- Add missing imports
- Fix type errors
- Remove unused variables
- Add missing return statements
"#,
                severity = severity,
                languages = if options.languages.is_empty() {
                    "all".to_string()
                } else {
                    options.languages.join(", ")
                },
            )
        }

        WorkflowTask::Comments => {
            let pragma = options.pragma_format.as_deref().unwrap_or("@uira-allow");
            format!(
                r#"
Review and clean up comments in the codebase. Remove:
- Outdated TODO comments that are already done
- Commented-out code blocks
- Obvious/redundant comments that don't add value
- Debug comments left in production code

Keep:
- Comments marked with `{pragma}`
- Meaningful documentation comments
- Complex logic explanations
- License headers
{docstring_note}
"#,
                pragma = pragma,
                docstring_note = if options.include_docstrings {
                    "- Review docstrings too"
                } else {
                    "- Skip docstrings (they are documentation)"
                },
            )
        }
    }
}
