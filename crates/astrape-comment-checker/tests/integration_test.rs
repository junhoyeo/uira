use std::io::Write;
use std::process::{Command, Stdio};

fn get_binary_path() -> String {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    format!("{}/../../target/debug/comment-checker", manifest_dir)
}

fn run_cli(input: &str) -> (String, i32) {
    let binary_path = get_binary_path();
    let mut child = Command::new(&binary_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn comment-checker");

    {
        let stdin = child.stdin.as_mut().expect("Failed to open stdin");
        stdin
            .write_all(input.as_bytes())
            .expect("Failed to write to stdin");
    }

    let output = child.wait_with_output().expect("Failed to read output");
    let exit_code = output.status.code().unwrap_or(-1);

    let mut combined = String::from_utf8_lossy(&output.stdout).to_string();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));

    (combined, exit_code)
}

#[test]
fn test_cli_no_comment_exit_zero() {
    let input =
        r##"{"tool_name":"Write","tool_input":{"file_path":"test.py","content":"print(1)"}}"##;
    let (output, exit_code) = run_cli(input);

    assert_eq!(exit_code, 0, "Expected exit 0 for no comments");
    assert!(
        output.contains("Success"),
        "Output should contain 'Success': {}",
        output
    );
}

#[test]
fn test_cli_with_comment_exit_two() {
    let input = r##"{"tool_name":"Write","tool_input":{"file_path":"test.py","content":"# comment\nprint(1)"}}"##;
    let (_, exit_code) = run_cli(input);

    assert_eq!(exit_code, 2, "Expected exit code 2 for comment detected");
}

#[test]
fn test_cli_non_code_file_exit_zero_skip() {
    // Use .txt which is truly unsupported (not .json which now has tree-sitter support)
    let input = r##"{"tool_name":"Write","tool_input":{"file_path":"test.txt","content":"hello"}}"##;
    let (output, exit_code) = run_cli(input);

    assert_eq!(exit_code, 0, "Expected exit 0 for non-code file");
    assert!(
        output.contains("Skipping"),
        "Output should contain 'Skipping': {}",
        output
    );
}

#[test]
fn test_cli_invalid_json_exit_zero_skip() {
    let input = "invalid json";
    let (output, exit_code) = run_cli(input);

    assert_eq!(exit_code, 0, "Expected exit 0 for invalid JSON");
    assert!(
        output.contains("Skipping"),
        "Output should contain 'Skipping': {}",
        output
    );
}

#[test]
fn test_cli_edit_tool_with_comment_exit_two() {
    let input = r##"{"tool_name":"Edit","tool_input":{"file_path":"test.py","old_string":"x","new_string":"# comment\ny"}}"##;
    let (_, exit_code) = run_cli(input);

    assert_eq!(exit_code, 2, "Expected exit code 2 for comment in Edit");
}

#[test]
fn test_cli_multi_edit_with_comment_exit_two() {
    let input = r##"{"tool_name":"MultiEdit","tool_input":{"file_path":"test.py","edits":[{"old_string":"a","new_string":"# comment"}]}}"##;
    let (_, exit_code) = run_cli(input);

    assert_eq!(
        exit_code, 2,
        "Expected exit code 2 for comment in MultiEdit"
    );
}

#[test]
fn test_cli_bdd_comment_exit_zero() {
    let input = r##"{"tool_name":"Write","tool_input":{"file_path":"test.py","content":"# given\nprint(1)"}}"##;
    let (output, exit_code) = run_cli(input);

    assert_eq!(exit_code, 0, "Expected exit 0 for BDD comment");
    assert!(
        output.contains("Success"),
        "Output should contain 'Success': {}",
        output
    );
}

#[test]
fn test_cli_directive_comment_exit_zero() {
    let input = r##"{"tool_name":"Write","tool_input":{"file_path":"test.py","content":"# noqa: E501\nprint(1)"}}"##;
    let (output, exit_code) = run_cli(input);

    assert_eq!(exit_code, 0, "Expected exit 0 for directive comment");
    assert!(
        output.contains("Success"),
        "Output should contain 'Success': {}",
        output
    );
}

#[test]
fn test_cli_shebang_exit_zero() {
    let input = r##"{"tool_name":"Write","tool_input":{"file_path":"test.py","content":"#!/usr/bin/env python\nprint(1)"}}"##;
    let (output, exit_code) = run_cli(input);

    assert_eq!(exit_code, 0, "Expected exit 0 for shebang");
    assert!(
        output.contains("Success"),
        "Output should contain 'Success': {}",
        output
    );
}

#[test]
fn test_cli_agent_memo_shows_warning() {
    let input = r##"{"tool_name":"Write","tool_input":{"file_path":"test.py","content":"# Changed from old to new\nprint(1)"}}"##;
    let (output, exit_code) = run_cli(input);

    assert_eq!(exit_code, 2, "Expected exit code 2 for agent memo");
    assert!(
        output.contains("AGENT MEMO"),
        "Output should contain 'AGENT MEMO': {}",
        output
    );
    assert!(
        output.contains("CODE SMELL"),
        "Output should contain 'CODE SMELL': {}",
        output
    );
}

#[test]
fn test_cli_custom_prompt() {
    let input = r##"{"tool_name":"Write","tool_input":{"file_path":"test.py","content":"# comment\nprint(1)"}}"##;

    let binary_path = get_binary_path();
    let mut child = Command::new(&binary_path)
        .arg("--prompt")
        .arg("Custom: {{comments}}")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn comment-checker");

    {
        let stdin = child.stdin.as_mut().expect("Failed to open stdin");
        stdin
            .write_all(input.as_bytes())
            .expect("Failed to write to stdin");
    }

    let output = child.wait_with_output().expect("Failed to read output");
    let combined = String::from_utf8_lossy(&output.stderr);

    assert!(
        combined.contains("Custom:"),
        "Output should contain custom prompt: {}",
        combined
    );
}
