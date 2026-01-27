//! Formatter for hook messages.

use std::collections::HashMap;

use crate::filters::AgentMemoFilter;
use crate::models::CommentInfo;
use crate::output::build_comments_xml;

/// Formats comment detection results for Claude Code hooks.
/// Groups comments by file path and builds complete error message with
/// instructions and XML blocks for each file.
/// If custom_prompt is provided, it replaces the default message template.
/// Use `{{comments}}` placeholder in custom_prompt to insert detected comments XML.
/// Returns formatted hook error message, or empty string if no comments provided.
pub fn format_hook_message(comments: &[CommentInfo], custom_prompt: Option<&str>) -> String {
    if comments.is_empty() {
        return String::new();
    }

    // Group comments by file path
    let mut by_file: HashMap<String, Vec<&CommentInfo>> = HashMap::new();
    let mut file_order: Vec<String> = Vec::new();

    for comment in comments {
        if !by_file.contains_key(&comment.file_path) {
            file_order.push(comment.file_path.clone());
        }
        by_file
            .entry(comment.file_path.clone())
            .or_default()
            .push(comment);
    }

    // Build comments XML
    let mut comments_xml = String::new();
    for file_path in &file_order {
        let file_comments: Vec<CommentInfo> =
            by_file[file_path].iter().map(|c| (*c).clone()).collect();
        comments_xml.push_str(&build_comments_xml(&file_comments, file_path));
        comments_xml.push('\n');
    }

    // If custom prompt is provided, use it with {{comments}} replacement
    if let Some(prompt) = custom_prompt {
        return prompt.replace("{{comments}}", &comments_xml);
    }

    // Default message template
    // Detect agent memo comments
    let agent_memo_filter = AgentMemoFilter::new();
    let agent_memo_comments: Vec<&CommentInfo> = comments
        .iter()
        .filter(|c| agent_memo_filter.is_agent_memo(c))
        .collect();
    let has_agent_memo = !agent_memo_comments.is_empty();

    let mut sb = String::new();

    // Header
    if has_agent_memo {
        sb.push_str("🚨 AGENT MEMO COMMENT DETECTED - CODE SMELL ALERT 🚨\n\n");
    } else {
        sb.push_str("COMMENT/DOCSTRING DETECTED - IMMEDIATE ACTION REQUIRED\n\n");
    }

    // Agent memo warning (if detected)
    if has_agent_memo {
        sb.push_str("⚠️  AGENT MEMO COMMENTS DETECTED - THIS IS A CODE SMELL  ⚠️\n\n");
        sb.push_str("You left \"memo-style\" comments that describe WHAT you changed or HOW you implemented something.\n");
        sb.push_str(
            "These are typically signs of an AI agent leaving notes for itself or the user.\n\n",
        );
        sb.push_str("Examples of agent memo patterns detected:\n");
        sb.push_str("  - \"Changed from X to Y\", \"Modified to...\", \"Updated from...\"\n");
        sb.push_str("  - \"Added new...\", \"Removed...\", \"Refactored...\"\n");
        sb.push_str("  - \"This implements...\", \"Here we...\", \"Now this...\"\n");
        sb.push_str("  - \"Note:\", \"Implementation of...\"\n");
        sb.push_str("  - Korean: \"여기서 변경됨\", \"구현함\", \"추가함\", \"수정됨\"\n\n");
        sb.push_str("WHY THIS IS BAD:\n");
        sb.push_str("  1. Code should be self-documenting - if you need to explain what changed, the code isn't clear enough\n");
        sb.push_str("  2. These comments become outdated instantly and mislead future readers\n");
        sb.push_str(
            "  3. Git history already tracks what changed - comments duplicate this poorly\n",
        );
        sb.push_str("  4. It's a sign of rushing without proper refactoring\n\n");
        sb.push_str("ACTION REQUIRED:\n");
        sb.push_str("  -> REMOVE these memo comments immediately\n");
        sb.push_str("  -> If the code needs explanation, refactor it to be clearer instead\n");
        sb.push_str("  -> Use meaningful variable/function names that explain intent\n");
        sb.push_str("  -> Let git commit messages document the \"what\" and \"why\"\n\n");
        sb.push_str("Detected agent memo comments:\n");
        for memo in &agent_memo_comments {
            sb.push_str(&format!(
                "  - Line {}: {}\n",
                memo.line_number,
                memo.text.trim()
            ));
        }
        sb.push_str("\n---\n\n");
    }

    // Guidelines
    sb.push_str("Your recent changes contain comments or docstrings, which triggered this hook.\n");
    sb.push_str("You need to take immediate action. You must follow the conditions below.\n");
    sb.push_str(
        "(Listed in priority order - you must always act according to this priority order)\n\n",
    );

    sb.push_str("CRITICAL WARNING: This hook message MUST NEVER be ignored, even if you receive it multiple times.\n");
    sb.push_str("You MUST provide corresponding explanation or action for EACH occurrence of this message.\n");
    sb.push_str(
        "Ignoring this message or failing to respond appropriately is strictly prohibited.\n\n",
    );

    sb.push_str("PRIORITY-BASED ACTION GUIDELINES:\n\n");

    sb.push_str("1. This is a comment/docstring that already existed before\n");
    sb.push_str("\t-> Explain to the user that this is an existing comment/docstring and proceed (justify it)\n\n");

    sb.push_str("2. This is a newly written comment: but it's in given, when, then format\n");
    sb.push_str("\t-> Tell the user it's a BDD comment and proceed (justify it)\n");
    sb.push_str("\t-> Note: This applies to comments only, not docstrings\n\n");

    sb.push_str(
        "3. This is a newly written comment/docstring: but it's a necessary comment/docstring\n",
    );
    sb.push_str("\t-> Tell the user why this comment/docstring is absolutely necessary and proceed (justify it)\n");
    sb.push_str("\t-> Examples of necessary comments: complex algorithms, security-related, performance optimization, regex, mathematical formulas\n");
    sb.push_str("\t-> Examples of necessary docstrings: public API documentation, complex module/class interfaces\n");
    sb.push_str("\t-> IMPORTANT: Most docstrings are unnecessary if the code is self-explanatory. Only keep truly essential ones.\n\n");

    sb.push_str(
        "4. This is a newly written comment/docstring: but it's an unnecessary comment/docstring\n",
    );
    sb.push_str("\t-> Apologize to the user and remove the comment/docstring.\n");
    sb.push_str(
        "\t-> Make the code itself clearer so it can be understood without comments/docstrings.\n",
    );
    sb.push_str("\t-> For verbose docstrings: refactor code to be self-documenting instead of adding lengthy explanations.\n\n");

    sb.push_str("MANDATORY REQUIREMENT: You must acknowledge this hook message and take one of the above actions.\n");
    sb.push_str("Review in the above priority order and take the corresponding action EVERY TIME this appears.\n\n");

    sb.push_str("REMINDER: These rules apply to ALL your future code, not just this specific edit. Always be deliberate and cautious when writing comments - only add them when absolutely necessary.\n\n");

    sb.push_str("Detected comments/docstrings:\n");
    sb.push_str(&comments_xml);

    sb
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CommentType;

    fn make_comment(text: &str, line: usize, file: &str) -> CommentInfo {
        CommentInfo::new(text.to_string(), line, file.to_string(), CommentType::Line)
    }

    #[test]
    fn test_empty_comments() {
        let result = format_hook_message(&[], None);
        assert!(result.is_empty());
    }

    #[test]
    fn test_regular_comment_output() {
        let comments = vec![make_comment("# Test comment", 5, "test.py")];
        let result = format_hook_message(&comments, None);

        assert!(result.contains("COMMENT/DOCSTRING DETECTED"));
        assert!(result.contains("test.py"));
        assert!(result.contains("Test comment"));
    }

    #[test]
    fn test_agent_memo_warning() {
        let comments = vec![make_comment("# Changed from old to new", 5, "test.py")];
        let result = format_hook_message(&comments, None);

        assert!(result.contains("AGENT MEMO"));
        assert!(result.contains("CODE SMELL"));
    }

    #[test]
    fn test_custom_prompt() {
        let comments = vec![make_comment("# Test", 1, "test.py")];
        let result = format_hook_message(&comments, Some("Custom: {{comments}}"));

        assert!(result.starts_with("Custom: "));
        assert!(result.contains("<comments"));
    }
}
