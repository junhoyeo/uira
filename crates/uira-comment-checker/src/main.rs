use std::collections::HashSet;
use std::io::{self, Read};
use std::path::Path;
use std::process::ExitCode;

use clap::Parser;
use serde::Deserialize;

use uira_comment_checker::{
    format_hook_message, CommentDetector, CommentInfo, FilterChain, LanguageRegistry,
};

const EXIT_PASS: u8 = 0;
const EXIT_BLOCK: u8 = 2;

#[derive(Parser)]
#[command(name = "comment-checker")]
#[command(about = "Check for problematic comments in source code")]
#[command(
    long_about = "A hook for Claude Code that detects and warns about comments and docstrings in source code."
)]
struct Cli {
    #[arg(
        long,
        help = "Custom prompt to replace the default warning message. Use {{comments}} placeholder for detected comments XML."
    )]
    prompt: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ToolInput {
    file_path: Option<String>,
    content: Option<String>,
    new_string: Option<String>,
    old_string: Option<String>,
    edits: Option<Vec<Edit>>,
}

#[derive(Debug, Deserialize)]
struct Edit {
    old_string: Option<String>,
    new_string: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct HookInput {
    #[serde(default)]
    session_id: Option<String>,
    tool_name: Option<String>,
    #[serde(default)]
    transcript_path: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    hook_event_name: Option<String>,
    tool_input: Option<ToolInput>,
    #[serde(default)]
    tool_response: Option<serde_json::Value>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let mut input = String::new();
    if io::stdin().read_to_string(&mut input).is_err() {
        eprintln!("[check-comments] Skipping: Failed to read stdin");
        return ExitCode::from(EXIT_PASS);
    }

    if input.is_empty() {
        eprintln!("[check-comments] Skipping: No input provided");
        return ExitCode::from(EXIT_PASS);
    }

    let hook_input: HookInput = match serde_json::from_str(&input) {
        Ok(h) => h,
        Err(_) => {
            eprintln!("[check-comments] Skipping: Invalid input format");
            return ExitCode::from(EXIT_PASS);
        }
    };

    let tool_input = match hook_input.tool_input {
        Some(ti) => ti,
        None => {
            eprintln!("[check-comments] Skipping: No tool input");
            return ExitCode::from(EXIT_PASS);
        }
    };

    let file_path = match &tool_input.file_path {
        Some(fp) if !fp.is_empty() => fp.clone(),
        _ => {
            eprintln!("[check-comments] Skipping: No file path provided");
            return ExitCode::from(EXIT_PASS);
        }
    };

    let ext = get_extension(&file_path);
    let registry = LanguageRegistry::new();
    if !registry.is_supported(&ext) {
        eprintln!("[check-comments] Skipping: Non-code file");
        return ExitCode::from(EXIT_PASS);
    }

    let detector = CommentDetector::new();
    let tool_name = hook_input.tool_name.as_deref().unwrap_or("");

    let comments = match tool_name {
        "Edit" => {
            let new_string = match &tool_input.new_string {
                Some(s) if !s.is_empty() => s,
                _ => {
                    eprintln!("[check-comments] Skipping: No content to check");
                    return ExitCode::from(EXIT_PASS);
                }
            };
            detect_new_comments_for_edit(
                &detector,
                tool_input.old_string.as_deref().unwrap_or(""),
                new_string,
                &file_path,
            )
        }
        "MultiEdit" => {
            let edits = match &tool_input.edits {
                Some(e) if !e.is_empty() => e,
                _ => {
                    eprintln!("[check-comments] Skipping: No content to check");
                    return ExitCode::from(EXIT_PASS);
                }
            };
            let mut all_comments = Vec::new();
            for edit in edits {
                let new_string = match &edit.new_string {
                    Some(s) if !s.is_empty() => s,
                    _ => continue,
                };
                let edit_comments = detect_new_comments_for_edit(
                    &detector,
                    edit.old_string.as_deref().unwrap_or(""),
                    new_string,
                    &file_path,
                );
                all_comments.extend(edit_comments);
            }
            all_comments
        }
        _ => {
            let content = get_content_to_check(&tool_input, tool_name);
            if content.is_empty() {
                eprintln!("[check-comments] Skipping: No content to check");
                return ExitCode::from(EXIT_PASS);
            }
            detector.detect(&content, &file_path, true)
        }
    };

    if comments.is_empty() {
        eprintln!("[check-comments] Success: No problematic comments/docstrings found");
        return ExitCode::from(EXIT_PASS);
    }

    let filter_chain = FilterChain::new();
    let filtered: Vec<CommentInfo> = comments
        .into_iter()
        .filter(|c| !filter_chain.should_skip(c))
        .collect();

    if filtered.is_empty() {
        eprintln!("[check-comments] Success: No problematic comments/docstrings found");
        return ExitCode::from(EXIT_PASS);
    }

    let message = format_hook_message(&filtered, cli.prompt.as_deref());
    eprint!("{}", message);
    ExitCode::from(EXIT_BLOCK)
}

fn get_extension(file_path: &str) -> String {
    let path = Path::new(file_path);
    match path.extension() {
        Some(ext) => ext.to_string_lossy().to_lowercase(),
        None => path
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default(),
    }
}

fn get_content_to_check(input: &ToolInput, tool_name: &str) -> String {
    match tool_name {
        "Write" => input.content.clone().unwrap_or_default(),
        "Edit" => input.new_string.clone().unwrap_or_default(),
        "MultiEdit" => input
            .edits
            .as_ref()
            .map(|edits| {
                edits
                    .iter()
                    .filter_map(|e| e.new_string.as_ref())
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default(),
        _ => {
            if let Some(content) = &input.content {
                if !content.is_empty() {
                    return content.clone();
                }
            }
            input.new_string.clone().unwrap_or_default()
        }
    }
}

fn build_comment_text_set(comments: &[CommentInfo]) -> HashSet<String> {
    comments.iter().map(|c| c.normalized_text()).collect()
}

fn filter_new_comments(
    old_comments: &[CommentInfo],
    new_comments: Vec<CommentInfo>,
) -> Vec<CommentInfo> {
    if old_comments.is_empty() {
        return new_comments;
    }

    let old_set = build_comment_text_set(old_comments);
    new_comments
        .into_iter()
        .filter(|c| !old_set.contains(&c.normalized_text()))
        .collect()
}

fn detect_new_comments_for_edit(
    detector: &CommentDetector,
    old_string: &str,
    new_string: &str,
    file_path: &str,
) -> Vec<CommentInfo> {
    let old_comments = detector.detect(old_string, file_path, true);
    let new_comments = detector.detect(new_string, file_path, true);
    filter_new_comments(&old_comments, new_comments)
}
