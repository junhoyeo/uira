//! Main TUI application

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};
use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind,
};
use futures::StreamExt;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, ListState, Paragraph, Wrap},
    Terminal,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Stdout, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::process::Command as TokioCommand;
use tokio::sync::mpsc;
use uira_agent::{Agent, AgentCommand, AgentConfig, ApprovalReceiver, BranchInfo, CommandSender};
use uira_providers::{
    AnthropicClient, GeminiClient, ModelClient, OllamaClient, OpenAIClient, OpenCodeClient,
    ProviderConfig, SecretString,
};
use uira_types::Provider;
use uira_types::{
    AgentState, ContentBlock, ImageSource, Item, Message, MessageContent, Role, ThreadEvent,
    TodoItem, TodoPriority, TodoStatus,
};

use crate::keybinds::KeybindConfig;
use crate::views::{
    ApprovalOverlay, ApprovalRequest, ChatView, CommandPalette, ModelSelector, PaletteAction,
    ToastManager, ToastVariant, INLINE_APPROVAL_HEIGHT, MODEL_GROUPS,
};
use crate::views::session_nav::{self, SessionStack, SessionView};
use crate::widgets::hud::{self, BackgroundTaskRegistry};
use crate::widgets::ChatMessage;
use crate::{AppEvent, Theme, ThemeOverrides};

/// Maximum size for the streaming buffer (1MB)
const MAX_STREAMING_BUFFER_SIZE: usize = 1024 * 1024;
/// Maximum image size for prompt attachments (10MB)
const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;
/// Terminal width threshold for narrow layout (<80 cols)
const NARROW_THRESHOLD: u16 = 80;
/// Terminal width threshold for wide layout (>120 cols)
const WIDE_THRESHOLD: u16 = 120;
/// Sidebar width for wide terminals
const SIDEBAR_WIDTH_WIDE: u16 = 40;
/// Sidebar width for standard terminals (when shown)
const SIDEBAR_WIDTH_STANDARD: u16 = 30;

#[derive(Clone, Debug)]
struct PendingImage {
    label: String,
    media_type: String,
    data: String,
    size_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QueuedMessagePriority {
    Normal,
    Interrupt,
}

#[derive(Debug, Clone)]
struct QueuedMessage {
    content: String,
    images: Vec<PendingImage>,
    priority: QueuedMessagePriority,
    queued_at: SystemTime,
}

#[derive(Debug, Default)]
struct MessageQueue {
    messages: VecDeque<QueuedMessage>,
}

impl MessageQueue {
    fn enqueue(&mut self, message: QueuedMessage) {
        match message.priority {
            QueuedMessagePriority::Normal => self.messages.push_back(message),
            QueuedMessagePriority::Interrupt => self.messages.push_front(message),
        }
    }

    fn dequeue(&mut self) -> Option<QueuedMessage> {
        self.messages.pop_front()
    }

    fn len(&self) -> usize {
        self.messages.len()
    }

    fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    fn clear(&mut self) {
        self.messages.clear();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyAction {
    None,
    OpenExternalEditor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ReviewTarget {
    Staged,
    File(String),
    Revision(String),
}

impl ReviewTarget {
    fn description(&self) -> String {
        match self {
            Self::Staged => "staged changes".to_string(),
            Self::File(path) => format!("changes in `{}`", path),
            Self::Revision(revision) => format!("commit `{}`", revision),
        }
    }
}

fn parse_review_target(arguments: &[&str]) -> Result<ReviewTarget, String> {
    if arguments.is_empty() {
        return Ok(ReviewTarget::Staged);
    }
    let target = arguments.join(" ");
    if is_commit_reference(&target) {
        if is_valid_commit_reference(&target) {
            Ok(ReviewTarget::Revision(target))
        } else {
            Err(format!(
                "Invalid revision '{}': not a valid commit in this repository",
                target
            ))
        }
    } else {
        Ok(ReviewTarget::File(target))
    }
}

fn parse_review_target_from_command(raw_command: &str) -> Result<ReviewTarget, String> {
    let remainder = raw_command
        .trim()
        .strip_prefix("/review")
        .unwrap_or("")
        .trim();

    if remainder.is_empty() {
        return Ok(ReviewTarget::Staged);
    }
    let target = if remainder.len() >= 2 {
        let bytes = remainder.as_bytes();
        if (bytes[0] == b'"' && bytes[remainder.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[remainder.len() - 1] == b'\'')
        {
            remainder[1..remainder.len() - 1].to_string()
        } else {
            remainder.to_string()
        }
    } else {
        remainder.to_string()
    };

    if Path::new(&target).exists() {
        return Ok(ReviewTarget::File(target));
    }

    parse_review_target(&[target.as_str()])
}

fn is_commit_reference(target: &str) -> bool {
    target == "HEAD"
        || target.starts_with("HEAD~")
        || target.starts_with("HEAD^")
        || (target.len() >= 7
            && target.len() <= 40
            && target.chars().all(|ch| ch.is_ascii_hexdigit()))
}

fn is_valid_commit_reference(target: &str) -> bool {
    Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", target])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn run_git_command(args: &[&str], working_directory: &str) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-c")
        .arg("core.quotePath=false")
        .args(args)
        .current_dir(working_directory)
        .output()
        .map_err(|err| {
            format!(
                "Failed to run `git {}` in `{}`: {}",
                args.join(" "),
                working_directory,
                err
            )
        })?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout)
            .trim_end()
            .to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            Err(format!(
                "`git {}` failed with status {}",
                args.join(" "),
                output.status
            ))
        } else {
            Err(stderr)
        }
    }
}

fn parse_binary_paths(numstat: &str) -> HashSet<String> {
    numstat
        .lines()
        .filter_map(|line| {
            let mut parts = line.split('\t');
            let added = parts.next()?;
            let removed = parts.next()?;
            let path = parts.collect::<Vec<_>>().join("\t");

            if added == "-" || removed == "-" {
                Some(normalize_numstat_path(&path))
            } else {
                None
            }
        })
        .collect()
}

fn normalize_numstat_path(path: &str) -> String {
    let path = path.trim();
    if let Some((prefix, rest)) = path.split_once('{') {
        if let Some((inner, suffix)) = rest.split_once('}') {
            if let Some((_, to)) = inner.split_once(" => ") {
                return format!("{}{}{}", prefix, to.trim(), suffix);
            }
        }
    }
    if let Some((_, to)) = path.rsplit_once(" => ") {
        return to.trim().to_string();
    }
    path.to_string()
}

fn take_diff_token(input: &str) -> Option<(String, &str)> {
    let trimmed = input.trim_start();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(rest) = trimmed.strip_prefix('"') {
        let mut escaped = false;
        let mut token = String::new();
        for (idx, ch) in rest.char_indices() {
            if escaped {
                token.push(ch);
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == '"' {
                let tail = &rest[idx + ch.len_utf8()..];
                return Some((token, tail));
            }
            token.push(ch);
        }
        return None;
    }

    let end = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
    Some((trimmed[..end].to_string(), &trimmed[end..]))
}

fn parse_diff_paths(header: &str) -> Option<(String, String)> {
    let rest = header.strip_prefix("diff --git ")?;
    let (left, rest) = take_diff_token(rest)?;
    let (right, _) = take_diff_token(rest)?;
    Some((
        left.trim_start_matches("a/").to_string(),
        right.trim_start_matches("b/").to_string(),
    ))
}

fn filter_binary_sections(diff: &str, binary_paths: &HashSet<String>) -> String {
    if binary_paths.is_empty() {
        return diff.to_string();
    }

    let mut out = String::new();
    let mut section = String::new();
    let mut section_paths: Option<(String, String)> = None;
    let mut in_section = false;

    for line in diff.lines() {
        if line.starts_with("diff --git ") {
            if in_section {
                let is_binary = section_paths.as_ref().is_some_and(|(left, right)| {
                    binary_paths.contains(left) || binary_paths.contains(right)
                });
                if !is_binary {
                    out.push_str(&section);
                }
                section.clear();
            }
            in_section = true;
            section_paths = parse_diff_paths(line);
        }

        if in_section {
            section.push_str(line);
            section.push('\n');
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }

    if in_section {
        let is_binary = section_paths.as_ref().is_some_and(|(left, right)| {
            binary_paths.contains(left) || binary_paths.contains(right)
        });
        if !is_binary {
            out.push_str(&section);
        }
    }

    out.trim_end().to_string()
}

fn collect_review_content(
    target: &ReviewTarget,
    working_directory: &str,
) -> Result<(String, Vec<String>), String> {
    match target {
        ReviewTarget::Staged => {
            let diff = run_git_command(&["diff", "--staged", "--no-color"], working_directory)?;
            let numstat = run_git_command(&["diff", "--staged", "--numstat"], working_directory)?;
            let binary_paths = parse_binary_paths(&numstat);
            let filtered = filter_binary_sections(&diff, &binary_paths);
            let mut skipped: Vec<String> = binary_paths.into_iter().collect();
            skipped.sort();
            Ok((filtered, skipped))
        }
        ReviewTarget::File(path) => {
            let staged = run_git_command(
                &["diff", "--staged", "--no-color", "--", path],
                working_directory,
            )?;
            let unstaged = run_git_command(&["diff", "--no-color", "--", path], working_directory)?;
            let staged_numstat = run_git_command(
                &["diff", "--staged", "--numstat", "--", path],
                working_directory,
            )?;
            let unstaged_numstat =
                run_git_command(&["diff", "--numstat", "--", path], working_directory)?;

            let mut chunks = Vec::new();
            if !staged.is_empty() {
                chunks.push(format!("### Staged changes\n{}", staged));
            }
            if !unstaged.is_empty() {
                chunks.push(format!("### Unstaged changes\n{}", unstaged));
            }

            let combined = chunks.join("\n\n");
            let mut binary_paths = parse_binary_paths(&staged_numstat);
            binary_paths.extend(parse_binary_paths(&unstaged_numstat));
            let filtered = filter_binary_sections(&combined, &binary_paths);
            let mut skipped: Vec<String> = binary_paths.into_iter().collect();
            skipped.sort();
            Ok((filtered, skipped))
        }
        ReviewTarget::Revision(revision) => {
            let diff = run_git_command(
                &["show", "--no-color", "--patch", revision],
                working_directory,
            )?;
            let numstat = run_git_command(
                &["show", "--numstat", "--format=", revision],
                working_directory,
            )?;
            let binary_paths = parse_binary_paths(&numstat);
            let filtered = filter_binary_sections(&diff, &binary_paths);
            let mut skipped: Vec<String> = binary_paths.into_iter().collect();
            skipped.sort();
            Ok((filtered, skipped))
        }
    }
}

fn build_review_prompt(target: &ReviewTarget, content: &str) -> String {
    format!(
        "You are reviewing {}.\n\
\n\
Provide structured feedback using this exact format:\n\
## Issues\n\
- ...\n\
\n\
## Suggestions\n\
- ...\n\
\n\
## Praise\n\
- ...\n\
\n\
Rules:\n\
- Be specific and reference concrete diff lines when possible.\n\
- Focus on correctness, security, performance, and maintainability.\n\
- If a section has no points, write `- None.`.\n\
\n\
Diff/context:\n\
```diff\n\
{}\n\
```",
        target.description(),
        content
    )
}

#[derive(Debug, Clone)]
struct ShareCommandOptions {
    public: bool,
    description: Option<String>,
}

fn parse_share_command(parts: &[&str]) -> Result<ShareCommandOptions, String> {
    let mut options = ShareCommandOptions {
        public: false,
        description: None,
    };

    let mut i = 1;
    while i < parts.len() {
        match parts[i] {
            "--public" => {
                options.public = true;
                i += 1;
            }
            "--description" | "-d" => {
                if i + 1 >= parts.len() {
                    return Err("Missing value for --description".to_string());
                }
                let raw_description = parts[i + 1..].join(" ");
                let description = raw_description
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string();
                if description.is_empty() {
                    return Err("Description cannot be empty".to_string());
                }
                options.description = Some(description);
                break;
            }
            unknown => {
                return Err(format!(
                    "Unknown option: {}. Usage: /share [--public] [--description <text>]",
                    unknown
                ));
            }
        }
    }

    Ok(options)
}

fn sanitize_filename_part(input: &str) -> String {
    input
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect::<String>()
}

fn role_title(role: &str) -> &str {
    match role {
        "user" => "User",
        "assistant" => "Assistant",
        "system" => "System",
        "tool" => "Tool",
        "error" => "Error",
        "thinking" => "Thinking",
        _ => "Message",
    }
}

fn render_session_markdown(
    messages: &[ChatMessage],
    session_id: Option<&str>,
    model: Option<&str>,
) -> String {
    let generated_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut markdown = String::new();
    markdown.push_str("# Uira Session\n\n");
    if let Some(session_id) = session_id {
        markdown.push_str(&format!("- Session ID: `{}`\n", session_id));
    }
    if let Some(model) = model {
        markdown.push_str(&format!("- Model: `{}`\n", model));
    }
    markdown.push_str(&format!("- Messages: {}\n", messages.len()));
    markdown.push_str(&format!("- Generated (unix): `{}`\n\n", generated_at));
    markdown.push_str("---\n\n");
    markdown.push_str("## Conversation\n\n");

    if messages.is_empty() {
        markdown.push_str("_(No messages in this session yet.)_\n");
        return markdown;
    }

    for (idx, message) in messages.iter().enumerate() {
        markdown.push_str(&format!(
            "### {}. {}\n\n",
            idx + 1,
            role_title(&message.role)
        ));
        markdown.push_str("```text\n");
        if message.content.trim().is_empty() {
            markdown.push_str("(empty)\n");
        } else {
            markdown.push_str(&message.content);
            if !message.content.ends_with('\n') {
                markdown.push('\n');
            }
        }
        markdown.push_str("```\n\n");
    }

    markdown.push_str("---\n\n");
    markdown.push_str("Shared via [Uira](https://github.com/junhoyeo/uira)\n");
    markdown
}

fn format_gh_error(stderr: &str) -> String {
    let lowered = stderr.to_lowercase();
    if lowered.contains("gh: command not found") || lowered.contains("not found") {
        return "GitHub CLI (`gh`) is not installed. Install it from https://cli.github.com/"
            .to_string();
    }
    if lowered.contains("not logged into")
        || lowered.contains("authentication")
        || lowered.contains("gh auth login")
    {
        return "GitHub authentication required. Run `gh auth login` and try `/share` again."
            .to_string();
    }

    if lowered.contains("api rate limit") || lowered.contains("rate limit") {
        return "GitHub API rate limit reached. Please wait and retry.".to_string();
    }

    if stderr.trim().is_empty() {
        "Failed to create GitHub Gist.".to_string()
    } else {
        format!("Failed to create GitHub Gist: {}", stderr.trim())
    }
}

fn extract_task_id(output: &str) -> Option<String> {
    let json = serde_json::from_str::<serde_json::Value>(output).ok()?;
    for key in ["task_id", "taskId", "id"] {
        if let Some(value) = json.get(key).and_then(|value| value.as_str()) {
            return Some(value.to_string());
        }
    }
    None
}

async fn create_gist_from_markdown(
    markdown: String,
    options: ShareCommandOptions,
    session_id: Option<String>,
) -> Result<String, String> {
    let session_slug = session_id
        .as_deref()
        .map(sanitize_filename_part)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "session".to_string());

    let mut temp_file = tempfile::Builder::new()
        .prefix("uira-share-")
        .suffix(".md")
        .tempfile()
        .map_err(|e| format!("Failed to create temporary gist file: {}", e))?;
    temp_file
        .write_all(markdown.as_bytes())
        .map_err(|e| format!("Failed to write temporary gist file: {}", e))?;

    let default_desc = format!("Uira session {}", session_slug);
    let description = options.description.unwrap_or(default_desc);
    let file_path_str = temp_file.path().to_string_lossy().to_string();

    let mut cmd = TokioCommand::new("gh");
    cmd.arg("gist")
        .arg("create")
        .arg(&file_path_str)
        .arg("--desc")
        .arg(description);

    if options.public {
        cmd.arg("--public");
    }

    let output = cmd.output().await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "GitHub CLI (`gh`) is not installed. Install it from https://cli.github.com/"
                .to_string()
        } else {
            format!("Failed to run `gh gist create`: {}", e)
        }
    })?;

    if !output.status.success() {
        return Err(format_gh_error(&String::from_utf8_lossy(&output.stderr)));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let url = stdout
        .split_whitespace()
        .find(|part| part.starts_with("https://"))
        .or_else(|| {
            stdout
                .lines()
                .find(|line| line.trim().starts_with("https://"))
        })
        .map(|s| s.trim().to_string())
        .ok_or_else(|| "Gist created, but no URL was returned by gh CLI.".to_string())?;

    if !url.contains("gist.github.com/") {
        return Err(format!("Unexpected gist URL returned by gh CLI: {}", url));
    }

    Ok(url)
}

fn format_branch_list(branches: &[BranchInfo]) -> String {
    if branches.is_empty() {
        return "No branches available.".to_string();
    }

    let mut lines = Vec::with_capacity(branches.len() + 1);
    lines.push("Branches:".to_string());

    for branch in branches {
        let marker = if branch.is_current { "*" } else { " " };
        let parent = branch
            .parent
            .as_deref()
            .map(|p| format!(" (from {})", p))
            .unwrap_or_default();
        lines.push(format!(
            "{} {} -> {}{}",
            marker, branch.name, branch.session_id, parent
        ));
    }

    lines.join("\n")
}

/// Create a model client from a "provider/model" string (e.g., "anthropic/claude-sonnet-4")
fn create_client_for_model(model_str: &str) -> Result<Arc<dyn ModelClient>, String> {
    let (provider, model) = model_str
        .split_once('/')
        .unwrap_or(("anthropic", model_str));

    match provider {
        "anthropic" => {
            let api_key = std::env::var("ANTHROPIC_API_KEY")
                .ok()
                .map(SecretString::from);

            let config = ProviderConfig {
                provider: Provider::Anthropic,
                api_key,
                model: model.to_string(),
                ..Default::default()
            };

            AnthropicClient::new(config)
                .map(|c| Arc::new(c) as Arc<dyn ModelClient>)
                .map_err(|e| e.to_string())
        }
        "openai" => {
            let api_key = std::env::var("OPENAI_API_KEY").ok().map(SecretString::from);

            let config = ProviderConfig {
                provider: Provider::OpenAI,
                api_key,
                model: model.to_string(),
                ..Default::default()
            };

            OpenAIClient::new(config)
                .map(|c| Arc::new(c) as Arc<dyn ModelClient>)
                .map_err(|e| e.to_string())
        }
        "google" | "gemini" => {
            let api_key = std::env::var("GEMINI_API_KEY")
                .or_else(|_| std::env::var("GOOGLE_API_KEY"))
                .ok()
                .map(SecretString::from);

            let config = ProviderConfig {
                provider: Provider::Google,
                api_key,
                model: model.to_string(),
                ..Default::default()
            };

            GeminiClient::new(config)
                .map(|c| Arc::new(c) as Arc<dyn ModelClient>)
                .map_err(|e| e.to_string())
        }
        "ollama" => {
            let config = ProviderConfig {
                provider: Provider::Ollama,
                api_key: None,
                model: model.to_string(),
                base_url: Some(
                    std::env::var("OLLAMA_HOST")
                        .unwrap_or_else(|_| "http://localhost:11434".to_string()),
                ),
                ..Default::default()
            };

            OllamaClient::new(config)
                .map(|c| Arc::new(c) as Arc<dyn ModelClient>)
                .map_err(|e| e.to_string())
        }
        "opencode" => {
            let api_key = std::env::var("OPENCODE_API_KEY")
                .ok()
                .map(SecretString::from);

            let config = ProviderConfig {
                provider: Provider::OpenCode,
                api_key,
                model: model.to_string(),
                ..Default::default()
            };

            OpenCodeClient::new(config)
                .map(|c| Arc::new(c) as Arc<dyn ModelClient>)
                .map_err(|e| e.to_string())
        }
        _ => Err(format!("Unknown provider: {}", provider)),
    }
}

/// Spawn a task that handles approval requests from the agent
fn spawn_approval_handler(mut approval_rx: ApprovalReceiver, event_tx: mpsc::Sender<AppEvent>) {
    tokio::spawn(async move {
        while let Some(pending) = approval_rx.recv().await {
            // Convert agent's ApprovalPending to TUI's ApprovalRequest
            let request = ApprovalRequest {
                id: pending.id,
                tool_name: pending.tool_name,
                input: pending.input,
                reason: pending.reason,
                response_tx: pending.response_tx,
            };

            // Send to app event loop
            if event_tx
                .send(AppEvent::ApprovalRequest(request))
                .await
                .is_err()
            {
                tracing::warn!("App event channel closed");
                break;
            }
        }
    });
}

fn spawn_tracing_log_handler(
    mut tracing_rx: mpsc::UnboundedReceiver<String>,
    event_tx: mpsc::Sender<AppEvent>,
) {
    tokio::spawn(async move {
        while let Some(message) = tracing_rx.recv().await {
            if event_tx.send(AppEvent::TracingLog(message)).await.is_err() {
                break;
            }
        }
    });
}

pub struct App {
    should_quit: bool,
    event_tx: mpsc::Sender<AppEvent>,
    event_rx: mpsc::Receiver<AppEvent>,
    chat_view: ChatView,
    #[cfg(test)]
    messages: Vec<ChatMessage>,
    input: String,
    cursor_pos: usize,
    agent_state: AgentState,
    status: String,
    input_focused: bool,
    approval_overlay: ApprovalOverlay,
    model_selector: ModelSelector,
    command_palette: CommandPalette,
    agent_input_tx: Option<mpsc::Sender<Message>>,
    agent_command_tx: Option<CommandSender>,
    current_model: Option<String>,
    session_id: Option<String>,
    working_directory: String,
    current_branch: String,
    pub session_stack: SessionStack,
    pub task_registry: BackgroundTaskRegistry,
    todos: Vec<TodoItem>,
    show_todo_sidebar: bool,
    todo_list_state: ListState,
    /// Collapse state for sidebar sections: [Context, MCP, Todos, Files]
    sidebar_sections: [bool; 4],
    /// Files modified by tool calls (Edit, Write)
    modified_files: HashSet<String>,
    /// Pending tool call paths: tool_call_id -> file_path
    pending_tool_paths: HashMap<String, String>,
    subagent_task_sessions: HashMap<String, String>,
    theme: Theme,
    theme_overrides: ThemeOverrides,
    pending_images: Vec<PendingImage>,
    message_queue: MessageQueue,
    #[allow(dead_code)]
    toast_manager: ToastManager,
    keybinds: KeybindConfig,
    /// Prompt history for Up/Down navigation
    prompt_history: Vec<String>,
    /// Current position in history (None = not browsing, Some(idx) = browsing)
    history_index: Option<usize>,
    /// Saved input when entering history mode
    history_stash: String,
}

impl App {
    pub fn new() -> Self {
        let (event_tx, event_rx) = mpsc::channel(100);
        let theme = Theme::default();
        let mut approval_overlay = ApprovalOverlay::new();
        approval_overlay.set_theme(theme.clone());
        let mut model_selector = ModelSelector::new();
        model_selector.set_theme(theme.clone());
        let mut command_palette = CommandPalette::new();
        command_palette.set_theme(theme.clone());
        Self {
            should_quit: false,
            event_tx,
            event_rx,
            chat_view: ChatView::new(theme.clone()),
            #[cfg(test)]
            messages: Vec::new(),
            input: String::new(),
            cursor_pos: 0,
            agent_state: AgentState::Idle,
            status: "Ready".to_string(),
            input_focused: true,
            approval_overlay,
            model_selector,
            command_palette,
            agent_input_tx: None,
            agent_command_tx: None,
            current_model: None,
            session_id: None,
            working_directory: std::env::current_dir()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            current_branch: "main".to_string(),
            session_stack: SessionStack::new(),
            task_registry: BackgroundTaskRegistry::new(),
            todos: Vec::new(),
            show_todo_sidebar: true,
            todo_list_state: ListState::default(),
            sidebar_sections: [true; 4],
            modified_files: HashSet::new(),
            pending_tool_paths: HashMap::new(),
            subagent_task_sessions: HashMap::new(),
            theme,
            theme_overrides: ThemeOverrides::default(),
            pending_images: Vec::new(),
            message_queue: MessageQueue::default(),
            toast_manager: ToastManager::new(),
            keybinds: KeybindConfig::default(),
            prompt_history: Self::load_prompt_history().unwrap_or_default(),
            history_index: None,
            history_stash: String::new(),
        }
    }

    pub fn configure_theme(
        &mut self,
        theme_name: &str,
        overrides: ThemeOverrides,
    ) -> Result<(), String> {
        self.theme_overrides = overrides;
        self.set_theme_by_name(theme_name)
    }

    fn set_theme_by_name(&mut self, theme_name: &str) -> Result<(), String> {
        let theme = Theme::from_name_with_overrides(theme_name, &self.theme_overrides)?;
        self.theme = theme;
        self.approval_overlay.set_theme(self.theme.clone());
        self.model_selector.set_theme(self.theme.clone());
        self.command_palette.set_theme(self.theme.clone());
        self.chat_view.set_theme(self.theme.clone());
        self.toast_manager
            .show(format!("Theme: {}", theme_name), ToastVariant::Info, 2000);
        Ok(())
    }

    pub fn configure_keybinds(&mut self, keybinds: KeybindConfig) {
        self.keybinds = keybinds;
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.current_model = Some(model.to_string());
        self
    }

    /// Run the TUI application
    pub async fn run(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> std::io::Result<()> {
        loop {
            // Draw UI
            terminal.draw(|frame| {
                self.render(frame);
            })?;

            // Handle events
            if event::poll(std::time::Duration::from_millis(50))? {
                match event::read()? {
                    Event::Key(key) => {
                        if self.handle_key_event(key) == KeyAction::OpenExternalEditor {
                            self.open_external_editor();
                        }
                    }
                    Event::Mouse(mouse) => match mouse.kind {
                        MouseEventKind::ScrollUp => self.chat_view.scroll_up(),
                        MouseEventKind::ScrollDown => self.chat_view.scroll_down(),
                        MouseEventKind::Down(MouseButton::Left) => {
                            self.handle_mouse_click_event(mouse.column, mouse.row);
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }

            // Check internal events
            while let Ok(event) = self.event_rx.try_recv() {
                self.handle_app_event(event);
            }

            if self.should_quit {
                // Deny any pending approvals before quitting
                self.approval_overlay.deny_all();
                break;
            }
        }

        Ok(())
    }

    pub async fn run_with_agent(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
        config: AgentConfig,
        client: Arc<dyn ModelClient>,
        tracing_rx: Option<mpsc::UnboundedReceiver<String>>,
    ) -> std::io::Result<()> {
        let working_directory = config
            .working_directory
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| {
                std::env::current_dir()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string()
            });
        self.working_directory = working_directory.clone();

        let mut event_system = uira_agent::create_event_system(working_directory);
        event_system.start();

        let (agent, event_stream) = Agent::new(config, client)
            .with_event_system(&event_system)
            .with_event_stream();
        let (mut agent, input_tx, approval_rx, command_tx) = agent.with_interactive();

        self.agent_input_tx = Some(input_tx);
        self.agent_command_tx = Some(command_tx);

        let event_tx = self.event_tx.clone();
        tokio::spawn(async move {
            let mut stream = event_stream;
            while let Some(event) = stream.next().await {
                if let Err(e) = event_tx.send(AppEvent::Agent(event)).await {
                    tracing::warn!("Failed to send agent event to TUI: {}", e);
                }
            }
        });

        spawn_approval_handler(approval_rx, self.event_tx.clone());

        if let Some(rx) = tracing_rx {
            spawn_tracing_log_handler(rx, self.event_tx.clone());
        }

        tokio::spawn(async move {
            if let Err(e) = agent.run_interactive().await {
                tracing::error!("Agent error: {}", e);
            }
        });

        self.run(terminal).await
    }

    fn render(&mut self, frame: &mut ratatui::Frame) {
        let area = frame.area();
        let hud_height = if self.task_registry.has_running_tasks() {
            1
        } else {
            0
        };
        let session_header_height = if self.session_stack.is_in_child() { 1 } else { 0 };

        let has_sidebar_content = !self.todos.is_empty()
            || !self.modified_files.is_empty()
            || self.current_model.is_some();

        // Responsive sidebar logic based on terminal width
        let show_sidebar = match area.width {
            // Narrow (<80): sidebar always hidden
            w if w < NARROW_THRESHOLD => false,
            // Standard (80-120): sidebar shown only if toggled AND has content
            w if w < WIDE_THRESHOLD => self.show_todo_sidebar && has_sidebar_content,
            // Wide (>120): sidebar shown by default when content exists
            _ => self.show_todo_sidebar && has_sidebar_content,
        };

        let main_area = if show_sidebar {
            let sidebar_width = match area.width {
                w if w < NARROW_THRESHOLD => SIDEBAR_WIDTH_STANDARD,
                w if w < WIDE_THRESHOLD => SIDEBAR_WIDTH_STANDARD,
                _ => SIDEBAR_WIDTH_WIDE,
            };
            let h_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(40), Constraint::Length(sidebar_width)])
                .split(area);
            self.render_sidebar(frame, h_chunks[1]);
            h_chunks[0]
        } else {
            area
        };

        if self.approval_overlay.is_active() {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(3),
                    Constraint::Length(INLINE_APPROVAL_HEIGHT),
                    Constraint::Length(session_header_height),
                    Constraint::Length(hud_height),
                    Constraint::Length(1),
                    Constraint::Length(3),
                ])
                .split(main_area);

            self.chat_view.render_chat(frame, chunks[0]);
            self.approval_overlay.render(frame, chunks[1]);
            self.render_session_header(frame, chunks[2]);
            self.render_hud(frame, chunks[3]);
            self.render_status(frame, chunks[4]);
            self.render_input(frame, chunks[5]);
        } else {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(3),
                    Constraint::Length(session_header_height),
                    Constraint::Length(hud_height),
                    Constraint::Length(1),
                    Constraint::Length(3),
                ])
                .split(main_area);

            self.chat_view.render_chat(frame, chunks[0]);
            self.render_session_header(frame, chunks[1]);
            self.render_hud(frame, chunks[2]);
            self.render_status(frame, chunks[3]);
            self.render_input(frame, chunks[4]);
        }

        self.toast_manager.tick();
        self.toast_manager.render(frame, area, &self.theme);

        if self.model_selector.is_active() {
            self.model_selector.render(frame, area);
        }

        if self.command_palette.is_active() {
            self.command_palette.render(frame, area);
        }
    }

    fn render_status(&self, frame: &mut ratatui::Frame, area: Rect) {
        let state_str = match self.agent_state {
            AgentState::Idle => ("Idle", self.theme.borders),
            AgentState::Thinking => ("Thinking...", self.theme.warning),
            AgentState::ExecutingTool => ("Executing tool...", self.theme.accent),
            AgentState::WaitingForApproval => ("Awaiting approval", self.theme.error),
            AgentState::WaitingForUser => ("Waiting for input", self.theme.accent),
            AgentState::Complete => ("Complete", self.theme.success),
            AgentState::Cancelled => ("Cancelled", self.theme.error),
            AgentState::Failed => ("Failed", self.theme.error),
        };

        let is_narrow = area.width < NARROW_THRESHOLD;

        let mut spans = vec![
            Span::styled(
                format!(" {} ", state_str.0),
                Style::default()
                    .fg(Theme::contrast_text(state_str.1))
                    .bg(state_str.1),
            ),
            Span::raw(" "),
            Span::styled(&self.status, Style::default().fg(self.theme.fg)),
            Span::raw(" | "),
        ];

        let branch_display = if is_narrow {
            self.current_branch.chars().take(10).collect::<String>()
        } else {
            self.current_branch.clone()
        };
        spans.push(Span::styled(
            format!("branch: {}", branch_display),
            Style::default().fg(self.theme.accent),
        ));

        if !is_narrow {
            let pending = self.approval_overlay.pending_count();
            if pending > 0 {
                spans.push(Span::raw(" | "));
                spans.push(Span::styled(
                    format!("{} pending approval(s)", pending),
                    Style::default().fg(self.theme.warning),
                ));
            }

            let queued = self.message_queue.len();
            if queued > 0 {
                spans.push(Span::raw(" | "));
                spans.push(Span::styled(
                    format!("({} queued)", queued),
                    Style::default().fg(self.theme.warning),
                ));
            }

            if self.chat_view.user_scrolled {
                spans.push(Span::raw(" | "));
                spans.push(Span::styled(
                    "[↓ Scroll to bottom]",
                    Style::default().fg(self.theme.warning),
                ));
            }
        }

        let status = Paragraph::new(Line::from(spans))
            .style(Style::default().bg(self.theme.bg).fg(self.theme.fg));

        frame.render_widget(status, area);
    }

    fn render_hud(&self, frame: &mut ratatui::Frame, area: Rect) {
        if !self.task_registry.has_running_tasks() {
            return;
        }

        let hud_line = hud::render_hud_line(&self.task_registry);
        let hud_paragraph = Paragraph::new(hud_line).style(Style::default().bg(self.theme.bg));
        frame.render_widget(hud_paragraph, area);
    }

    fn render_session_header(&self, frame: &mut ratatui::Frame, area: Rect) {
        if !self.session_stack.is_in_child() {
            return;
        }

        if let Some(header_line) =
            session_nav::render_session_header(&self.session_stack, self.theme.accent)
        {
            let header_paragraph =
                Paragraph::new(header_line).style(Style::default().bg(self.theme.bg));
            frame.render_widget(header_paragraph, area);
        }
    }

    fn render_input(&self, frame: &mut ratatui::Frame, area: Rect) {
        let is_narrow = area.width < NARROW_THRESHOLD;

        let pending_label = if self.pending_images.is_empty() {
            String::new()
        } else {
            format!(" | {} image(s) attached", self.pending_images.len())
        };

        let model_prefix = if is_narrow {
            String::new()
        } else {
            self.current_model
                .as_ref()
                .map(|model| format!("model: {} | ", model))
                .unwrap_or_default()
        };

        let title = if is_narrow {
            " Input ".to_string()
        } else if self.approval_overlay.is_active() {
            format!(
                " Input ({}approval overlay active{}) ",
                model_prefix, pending_label
            )
        } else if self.is_agent_busy() {
            format!(
                " Input ({}Enter to queue, Alt+Enter to interrupt, Ctrl+G external editor, Ctrl+C to quit{}) ",
                model_prefix, pending_label
            )
        } else {
            format!(
                " Input ({}Enter to send, Ctrl+G external editor, Ctrl+C to quit{}) ",
                model_prefix, pending_label
            )
        };

        let block = Block::default().title(title).borders(Borders::ALL).style(
            if self.input_focused && !self.approval_overlay.is_active() {
                Style::default().fg(self.theme.accent)
            } else {
                Style::default().fg(self.theme.borders)
            },
        );

        let inner = block.inner(area);

        // Display input with cursor (use char boundary for UTF-8 safety)
        let char_count = self.input.chars().count();
        let display_input = if self.cursor_pos >= char_count {
            format!("{}_", self.input)
        } else {
            let byte_pos = self
                .input
                .char_indices()
                .nth(self.cursor_pos)
                .map(|(i, _)| i)
                .unwrap_or(self.input.len());
            let (before, after) = self.input.split_at(byte_pos);
            format!("{}|{}", before, after)
        };

        let input_paragraph = Paragraph::new(display_input).wrap(Wrap { trim: false });

        frame.render_widget(block, area);
        frame.render_widget(input_paragraph, inner);
    }

    fn render_sidebar(&self, frame: &mut ratatui::Frame, area: Rect) {
        let outer_block = Block::default()
            .title(" Info ")
            .borders(Borders::ALL)
            .style(Style::default().fg(self.theme.borders));
        let inner = outer_block.inner(area);
        frame.render_widget(outer_block, area);

        let mut lines: Vec<Line> = Vec::new();
        let max_width = inner.width as usize;

        // Section 1: Context
        let ctx_arrow = if self.sidebar_sections[0] {
            "▼"
        } else {
            "▶"
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("{} Context", ctx_arrow),
                Style::default().fg(self.theme.accent),
            ),
            Span::styled(" [1]", Style::default().fg(self.theme.text_muted)),
        ]));
        if self.sidebar_sections[0] {
            let model = self.current_model.as_deref().unwrap_or("not connected");
            lines.push(Line::from(Span::styled(
                format!("  Model: {}", model),
                Style::default().fg(self.theme.fg),
            )));
            lines.push(Line::from(Span::styled(
                format!("  Branch: {}", self.current_branch),
                Style::default().fg(self.theme.fg),
            )));
            lines.push(Line::from(Span::styled(
                format!("  CWD: {}", self.working_directory),
                Style::default().fg(self.theme.fg),
            )));
            if let Some(ref session_id) = self.session_id {
                lines.push(Line::from(Span::styled(
                    format!("  Session: {}", session_id),
                    Style::default().fg(self.theme.fg),
                )));
            }
            let token_info = self.status.clone();
            if token_info.contains("tokens") {
                lines.push(Line::from(Span::styled(
                    format!("  {}", token_info),
                    Style::default().fg(self.theme.text_muted),
                )));
            }
            lines.push(Line::from(""));
        }

        // Section 2: MCP
        let mcp_arrow = if self.sidebar_sections[1] {
            "▼"
        } else {
            "▶"
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("{} MCP", mcp_arrow),
                Style::default().fg(self.theme.accent),
            ),
            Span::styled(" [2]", Style::default().fg(self.theme.text_muted)),
        ]));
        if self.sidebar_sections[1] {
            lines.push(Line::from(Span::styled(
                "  ○ No MCP servers",
                Style::default().fg(self.theme.text_muted),
            )));
            lines.push(Line::from(""));
        }

        // Section 3: Todos
        let completed = self
            .todos
            .iter()
            .filter(|t| t.status == TodoStatus::Completed)
            .count();
        let total = self.todos.len();
        let todo_arrow = if self.sidebar_sections[2] {
            "▼"
        } else {
            "▶"
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("{} Todos ({}/{})", todo_arrow, completed, total),
                Style::default().fg(self.theme.accent),
            ),
            Span::styled(" [3]", Style::default().fg(self.theme.text_muted)),
        ]));
        if self.sidebar_sections[2] {
            if self.todos.is_empty() {
                lines.push(Line::from(Span::styled(
                    "  No todos",
                    Style::default().fg(self.theme.text_muted),
                )));
            } else {
                for todo in &self.todos {
                    let (indicator, status_color) = match todo.status {
                        TodoStatus::Completed => ("✓", self.theme.success),
                        TodoStatus::InProgress => ("•", self.theme.warning),
                        TodoStatus::Cancelled => ("✗", self.theme.borders),
                        TodoStatus::Pending => (" ", self.theme.borders),
                    };

                    let (priority_marker, color) = match (todo.status, todo.priority) {
                        (TodoStatus::Completed, _) | (TodoStatus::Cancelled, _) => {
                            ("", status_color)
                        }
                        (_, TodoPriority::High) => ("⚡", self.theme.error),
                        (_, TodoPriority::Medium) => ("•", self.theme.warning),
                        (_, TodoPriority::Low) => ("", status_color),
                    };

                    let prefix = format!("  [{}] {}", indicator, priority_marker);
                    let prefix_len = prefix.chars().count();
                    let avail = max_width.saturating_sub(prefix_len);
                    let content = if todo.content.chars().count() > avail && avail > 3 {
                        let trunc: String = todo.content.chars().take(avail - 3).collect();
                        format!("{}...", trunc)
                    } else {
                        todo.content.clone()
                    };

                    lines.push(Line::from(vec![
                        Span::styled(prefix, Style::default().fg(color)),
                        Span::styled(content, Style::default().fg(color)),
                    ]));
                }
            }
            lines.push(Line::from(""));
        }

        // Section 4: Files
        let file_count = self.modified_files.len();
        let files_arrow = if self.sidebar_sections[3] {
            "▼"
        } else {
            "▶"
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("{} Files ({})", files_arrow, file_count),
                Style::default().fg(self.theme.accent),
            ),
            Span::styled(" [4]", Style::default().fg(self.theme.text_muted)),
        ]));
        if self.sidebar_sections[3] {
            if self.modified_files.is_empty() {
                lines.push(Line::from(Span::styled(
                    "  No modified files",
                    Style::default().fg(self.theme.text_muted),
                )));
            } else {
                let mut sorted: Vec<&String> = self.modified_files.iter().collect();
                sorted.sort();
                for path in sorted {
                    let display = path.rsplit('/').next().unwrap_or(path);
                    let avail = max_width.saturating_sub(4);
                    let display = if display.chars().count() > avail && avail > 3 {
                        let trunc: String = display.chars().take(avail - 3).collect();
                        format!("{}...", trunc)
                    } else {
                        display.to_string()
                    };
                    lines.push(Line::from(Span::styled(
                        format!("  • {}", display),
                        Style::default().fg(self.theme.text_muted),
                    )));
                }
            }
        }

        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        frame.render_widget(paragraph, inner);
    }

    fn ensure_todo_selection(&mut self) {
        if self.todos.is_empty() {
            self.todo_list_state.select(None);
            return;
        }

        if let Some(selected) = self.todo_list_state.selected() {
            if selected < self.todos.len() {
                let status = self.todos[selected].status;
                if status != TodoStatus::Completed && status != TodoStatus::Cancelled {
                    return;
                }
            }
        }

        let selected = self
            .todos
            .iter()
            .position(|todo| {
                todo.status != TodoStatus::Completed && todo.status != TodoStatus::Cancelled
            })
            .unwrap_or(0);
        self.todo_list_state.select(Some(selected));
    }

    fn toggle_todo_sidebar(&mut self) {
        self.show_todo_sidebar = !self.show_todo_sidebar;
        if self.show_todo_sidebar {
            self.ensure_todo_selection();
            self.status = "TODO sidebar shown".to_string();
        } else {
            self.status = "TODO sidebar hidden".to_string();
        }
    }

    fn next_open_todo_index(&self, after: usize) -> Option<usize> {
        if self.todos.is_empty() {
            return None;
        }

        for offset in 1..=self.todos.len() {
            let index = (after + offset) % self.todos.len();
            let status = self.todos[index].status;
            if status != TodoStatus::Completed && status != TodoStatus::Cancelled {
                return Some(index);
            }
        }

        None
    }

    fn mark_selected_todo_done(&mut self) {
        if !self.show_todo_sidebar {
            self.status = "TODO sidebar is hidden".to_string();
            return;
        }

        self.ensure_todo_selection();

        let Some(selected) = self.todo_list_state.selected() else {
            self.status = "No TODO selected".to_string();
            return;
        };

        if selected >= self.todos.len() {
            self.todo_list_state.select(None);
            self.status = "No TODO selected".to_string();
            return;
        }

        let status = self.todos[selected].status;
        if status == TodoStatus::Completed || status == TodoStatus::Cancelled {
            self.status = "Selected TODO is already closed".to_string();
            return;
        }

        let content = self.todos[selected].content.clone();
        self.todos[selected].status = TodoStatus::Completed;
        self.status = format!("Marked TODO done: {}", content);

        if let Some(next) = self.next_open_todo_index(selected) {
            self.todo_list_state.select(Some(next));
        } else {
            self.todo_list_state.select(Some(selected));
        }
    }

    fn load_prompt_history() -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let history_path = dirs::home_dir()
            .ok_or("Could not determine home directory")?
            .join(".uira")
            .join("prompt_history.jsonl");

        if !history_path.exists() {
            return Ok(Vec::new());
        }

        let content = std::fs::read_to_string(&history_path)?;
        let mut history = Vec::new();

        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                if let Some(prompt) = json.get("prompt").and_then(|v| v.as_str()) {
                    history.push(prompt.to_string());
                }
            }
        }

        Ok(history)
    }

    fn save_prompt_history(&self) -> Result<(), Box<dyn std::error::Error>> {
        let home = dirs::home_dir().ok_or("Could not determine home directory")?;
        let uira_dir = home.join(".uira");
        std::fs::create_dir_all(&uira_dir)?;

        let history_path = uira_dir.join("prompt_history.jsonl");

        // Keep only the last 1000 entries
        let to_save: Vec<_> = self
            .prompt_history
            .iter()
            .rev()
            .take(1000)
            .rev()
            .cloned()
            .collect();

        let mut file = std::fs::File::create(&history_path)?;
        for prompt in to_save {
            let json = serde_json::json!({ "prompt": prompt });
            writeln!(file, "{}", json)?;
        }

        Ok(())
    }

    fn is_agent_busy(&self) -> bool {
        matches!(
            self.agent_state,
            AgentState::Thinking | AgentState::ExecutingTool | AgentState::WaitingForApproval
        )
    }

    fn queue_message(&mut self, content: String, priority: QueuedMessagePriority) {
        let images = std::mem::take(&mut self.pending_images);
        self.message_queue.enqueue(QueuedMessage {
            content,
            images,
            priority,
            queued_at: SystemTime::now(),
        });

        let mode = match priority {
            QueuedMessagePriority::Normal => "Queued message",
            QueuedMessagePriority::Interrupt => "Queued interrupt message",
        };
        self.status = format!("{} ({} pending)", mode, self.message_queue.len());
    }

    fn request_interrupt(&mut self) {
        if let Some(ref tx) = self.agent_command_tx {
            let tx = tx.clone();
            tokio::spawn(async move {
                if tx.send(AgentCommand::Interrupt).await.is_err() {
                    tracing::warn!("Failed to send interrupt command");
                }
            });
            self.status = "Interrupt requested...".to_string();
        } else {
            self.status = "Unable to interrupt: no agent command channel".to_string();
        }
    }

    fn send_next_queued_message(&mut self) -> bool {
        let Some(queued) = self.message_queue.dequeue() else {
            return false;
        };

        let age_secs = queued
            .queued_at
            .elapsed()
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        let mode = match queued.priority {
            QueuedMessagePriority::Normal => "queued",
            QueuedMessagePriority::Interrupt => "interrupt",
        };
        self.status = format!("Sending {} message (queued {}s ago)...", mode, age_secs);
        self.pending_images = queued.images;
        self.submit_input(queued.content);
        true
    }

    fn process_queued_messages(&mut self) {
        if self.message_queue.is_empty() || self.is_agent_busy() {
            return;
        }

        while !self.is_agent_busy() {
            if !self.send_next_queued_message() {
                break;
            }
            if self.message_queue.is_empty() {
                break;
            }
        }
    }

    fn handle_mouse_click_event(&mut self, column: u16, row: u16) {
        let terminal_size = crossterm::terminal::size().unwrap_or((80, 24));
        let area = Rect::new(0, 0, terminal_size.0, terminal_size.1);
        let hud_height = if self.task_registry.has_running_tasks() {
            1
        } else {
            0
        };
        let session_header_height = if self.session_stack.is_in_child() { 1 } else { 0 };
        
        let has_sidebar_content = !self.todos.is_empty()
            || !self.modified_files.is_empty()
            || self.current_model.is_some();
        let show_sidebar = if area.width > 120 {
            self.show_todo_sidebar
        } else {
            self.show_todo_sidebar && has_sidebar_content
        };

        let (main_area, sidebar_area) = if show_sidebar {
            let h_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(40), Constraint::Length(40)])
                .split(area);
            (h_chunks[0], Some(h_chunks[1]))
        } else {
            (area, None)
        };

        let chat_area = if self.approval_overlay.is_active() {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(3),
                    Constraint::Length(INLINE_APPROVAL_HEIGHT),
                    Constraint::Length(session_header_height),
                    Constraint::Length(hud_height),
                    Constraint::Length(1),
                    Constraint::Length(3),
                ])
                .split(main_area);
            chunks[0]
        } else {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(3),
                    Constraint::Length(session_header_height),
                    Constraint::Length(hud_height),
                    Constraint::Length(1),
                    Constraint::Length(3),
                ])
                .split(main_area);
            chunks[0]
        };

        self.handle_mouse_click(column, row, chat_area, sidebar_area);
    }

    fn handle_mouse_click(&mut self, column: u16, row: u16, main_area: Rect, sidebar_area: Option<Rect>) {
        // Check if click is in sidebar
        if let Some(sidebar) = sidebar_area {
            if column >= sidebar.x && column < sidebar.x + sidebar.width
                && row >= sidebar.y && row < sidebar.y + sidebar.height
            {
                self.handle_sidebar_click(row, sidebar);
                return;
            }
        }

        // Check if click is in chat area
        if column >= main_area.x && column < main_area.x + main_area.width
            && row >= main_area.y && row < main_area.y + main_area.height
        {
            self.handle_chat_click(row, main_area);
        }
    }

    fn handle_chat_click(&mut self, row: u16, chat_area: Rect) {
        // Chat area has borders, so inner area starts at y+1
        let inner_y = chat_area.y + 1;
        let inner_height = chat_area.height.saturating_sub(2);
        
        if row < inner_y || row >= inner_y + inner_height {
            return;
        }

        // Calculate relative row within the chat viewport
        let relative_row = (row - inner_y) as usize;
        
        // Calculate absolute line in the rendered content
        let absolute_line = self.chat_view.scroll_offset + relative_row;
        
        if let Some(message_index) = self.chat_view.get_message_index_at_line(absolute_line) {
            let has_tool_output = self.chat_view.messages.get(message_index)
                .map(|msg| msg.tool_output.is_some())
                .unwrap_or(false);
            
            if has_tool_output {
                if let Some(msg) = self.chat_view.messages.get_mut(message_index) {
                    if let Some(tool_output) = msg.tool_output.as_mut() {
                        tool_output.collapsed = !tool_output.collapsed;
                        let action = if tool_output.collapsed { "Collapsed" } else { "Expanded" };
                        let tool_name = tool_output.tool_name.clone();
                        self.chat_view.invalidate_render_cache();
                        self.status = format!("{} {} output", action, tool_name);
                    }
                }
            }
        }
    }

    fn handle_sidebar_click(&mut self, row: u16, sidebar_area: Rect) {
        // Sidebar has borders, inner area starts at y+1
        let inner_y = sidebar_area.y + 1;
        
        if row < inner_y {
            return;
        }

        let relative_row = (row - inner_y) as usize;
        
        // Calculate which section header was clicked based on row position
        // We need to track the current line as we build the sidebar
        let mut current_line = 0;
        
        // Section 1: Context header at line 0
        if relative_row == current_line {
            self.sidebar_sections[0] = !self.sidebar_sections[0];
            let state = if self.sidebar_sections[0] { "expanded" } else { "collapsed" };
            self.status = format!("Context section {}", state);
            return;
        }
        current_line += 1;
        
        // Skip Context content if expanded
        if self.sidebar_sections[0] {
            let context_lines = 3;
            let session_lines = usize::from(self.session_id.is_some());
            let token_lines = usize::from(self.status.contains("tokens"));
            current_line += context_lines + session_lines + token_lines + 1; // +1 for blank line
        }
        
        // Section 2: MCP header
        if relative_row == current_line {
            self.sidebar_sections[1] = !self.sidebar_sections[1];
            let state = if self.sidebar_sections[1] { "expanded" } else { "collapsed" };
            self.status = format!("MCP section {}", state);
            return;
        }
        current_line += 1;
        
        // Skip MCP content if expanded
        if self.sidebar_sections[1] {
            current_line += 2; // "No MCP servers" + blank line
        }
        
        // Section 3: Todos header
        if relative_row == current_line {
            self.sidebar_sections[2] = !self.sidebar_sections[2];
            let state = if self.sidebar_sections[2] { "expanded" } else { "collapsed" };
            self.status = format!("Todos section {}", state);
            return;
        }
        current_line += 1;
        
        // Skip Todos content if expanded
        if self.sidebar_sections[2] {
            let todo_lines = if self.todos.is_empty() { 1 } else { self.todos.len() };
            current_line += todo_lines + 1; // +1 for blank line
        }
        
        // Section 4: Files header
        if relative_row == current_line {
            self.sidebar_sections[3] = !self.sidebar_sections[3];
            let state = if self.sidebar_sections[3] { "expanded" } else { "collapsed" };
            self.status = format!("Files section {}", state);
        }
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> KeyAction {
        if self.command_palette.is_active() {
            match self.command_palette.handle_key(key.code) {
                PaletteAction::Execute(id) => {
                    self.execute_palette_command(&id);
                }
                PaletteAction::Close | PaletteAction::None => {}
            }
            return KeyAction::None;
        }

        if self.model_selector.is_active() {
            if let Some(selected_model) = self.model_selector.handle_key(key.code) {
                self.switch_model(&selected_model);
            }
            return KeyAction::None;
        }

        // Approval overlay takes priority for key handling
        if self.approval_overlay.handle_key(key.code) {
            if !self.approval_overlay.is_active() {
                self.set_agent_state(AgentState::Thinking);
            }
            return KeyAction::None;
        }

        // Global shortcuts
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') | KeyCode::Char('q') => {
                    self.should_quit = true;
                }
                KeyCode::Char('l') => {
                    self.status = "Screen refresh requested".to_string();
                }
                KeyCode::Char('o') | KeyCode::Char('O') => {
                    if KeybindConfig::matches_any(
                        &self.keybinds.expand_tools,
                        key.code,
                        key.modifiers,
                    ) {
                        self.status = self.chat_view.expand_all_tool_outputs();
                    } else if KeybindConfig::matches_any(
                        &self.keybinds.collapse_tools,
                        key.code,
                        key.modifiers,
                    ) {
                        self.status = self.chat_view.collapse_all_tool_outputs();
                    }
                }
                _ if KeybindConfig::matches_any(
                    &self.keybinds.command_palette,
                    key.code,
                    key.modifiers,
                ) =>
                {
                    self.command_palette.open();
                    return KeyAction::None;
                }
                KeyCode::Char('g') => {
                    if self.approval_overlay.is_active() {
                        self.status = "Finish approval first before opening editor".to_string();
                        return KeyAction::None;
                    }
                    return KeyAction::OpenExternalEditor;
                }
                KeyCode::Up => {
                    self.chat_view.scroll_to_prev_user_message();
                }
                KeyCode::Down => {
                    self.chat_view.scroll_to_next_user_message();
                }
                _ => {}
            }
            return KeyAction::None;
        }

        if key.modifiers.is_empty() && self.input.is_empty() {
            match key.code {
                KeyCode::Char(c) if c.eq_ignore_ascii_case(&'t') => {
                    self.toggle_todo_sidebar();
                    return KeyAction::None;
                }
                KeyCode::Char(c) if c.eq_ignore_ascii_case(&'d') => {
                    self.mark_selected_todo_done();
                    return KeyAction::None;
                }
                KeyCode::Char(c @ '1'..='4') => {
                    let idx = (c as usize) - ('1' as usize);
                    self.sidebar_sections[idx] = !self.sidebar_sections[idx];
                    let names = ["Context", "MCP", "Todos", "Files"];
                    let state = if self.sidebar_sections[idx] {
                        "expanded"
                    } else {
                        "collapsed"
                    };
                    self.status = format!("{} section {}", names[idx], state);
                    return KeyAction::None;
                }
                KeyCode::Char('[') => {
                    if self.session_stack.is_in_child() {
                        self.session_stack.prev_sibling();
                        return KeyAction::None;
                    }
                }
                KeyCode::Char(']') => {
                    if self.session_stack.is_in_child() {
                        self.session_stack.next_sibling();
                        return KeyAction::None;
                    }
                }
                _ => {}
            }
        }

        // Input handling (cursor_pos is char index, not byte index for UTF-8 safety)
        if self.input_focused {
            let char_count = self.input.chars().count();
            match key.code {
                KeyCode::Char(c) => {
                    // Exit history mode when typing
                    if self.history_index.is_some() {
                        self.history_stash = self.input.clone();
                        self.history_index = None;
                    }
                    let byte_pos = self
                        .input
                        .char_indices()
                        .nth(self.cursor_pos)
                        .map(|(i, _)| i)
                        .unwrap_or(self.input.len());
                    self.input.insert(byte_pos, c);
                    self.cursor_pos += 1;
                }
                KeyCode::Backspace => {
                    if self.cursor_pos > 0 {
                        self.cursor_pos -= 1;
                        let byte_pos = self
                            .input
                            .char_indices()
                            .nth(self.cursor_pos)
                            .map(|(i, _)| i)
                            .unwrap_or(0);
                        self.input.remove(byte_pos);
                    }
                }
                KeyCode::Delete => {
                    if self.cursor_pos < char_count {
                        let byte_pos = self
                            .input
                            .char_indices()
                            .nth(self.cursor_pos)
                            .map(|(i, _)| i)
                            .unwrap_or(self.input.len());
                        self.input.remove(byte_pos);
                    }
                }
                KeyCode::Left => {
                    if self.cursor_pos > 0 {
                        self.cursor_pos -= 1;
                    }
                }
                KeyCode::Right => {
                    if self.cursor_pos < char_count {
                        self.cursor_pos += 1;
                    }
                }
                KeyCode::Home => {
                    self.chat_view.scroll_to_top();
                }
                KeyCode::End => {
                    self.chat_view.scroll_to_bottom();
                }
                KeyCode::Enter => {
                    if self.input.is_empty() && self.pending_images.is_empty() {
                        let selected_task_id = self
                            .chat_view
                            .selected_message_index()
                            .and_then(|idx| self.chat_view.messages.get(idx))
                            .and_then(|msg| {
                                let tool_name = msg.tool_output.as_ref()?.tool_name.as_str();
                                if matches!(tool_name, "delegate_task" | "Task") {
                                    msg.message_id.clone()
                                } else {
                                    None
                                }
                            });
                        if let Some(status) = self.chat_view.toggle_selected_tool_output() {
                            self.status = status;
                            if let Some(task_id) = selected_task_id {
                                if let Some(session_id) = self.subagent_task_sessions.get(&task_id) {
                                    if self.session_stack.push_session(session_id) {
                                        self.status = format!("Entered subagent session {}", session_id);
                                    }
                                }
                            }
                            return KeyAction::None;
                        }
                    }

                    if !self.input.trim().is_empty() || !self.pending_images.is_empty() {
                        let input = std::mem::take(&mut self.input);
                        self.cursor_pos = 0;

                        if self.is_agent_busy() {
                            if key.modifiers.contains(KeyModifiers::ALT) {
                                self.queue_message(input, QueuedMessagePriority::Interrupt);
                                self.request_interrupt();
                                let _ = self.send_next_queued_message();
                            } else {
                                self.queue_message(input, QueuedMessagePriority::Normal);
                            }
                        } else {
                            self.submit_input(input);
                        }
                    }
                }
                KeyCode::Tab => {
                    if let Some(status) = self.chat_view.toggle_selected_tool_output() {
                        self.status = status;
                    }
                }
                KeyCode::Esc => {
                    if self.session_stack.is_in_child() {
                        self.session_stack.pop_session();
                    } else if self.history_index.is_some() {
                        self.history_index = None;
                        self.input = self.history_stash.clone();
                        self.cursor_pos = self.input.chars().count();
                    } else {
                        self.should_quit = true;
                    }
                }
                KeyCode::Up => {
                    // History navigation when input is empty
                    if self.input.is_empty() && self.history_index.is_none() {
                        // Enter history mode
                        if !self.prompt_history.is_empty() {
                            self.history_stash = String::new();
                            let idx = self.prompt_history.len() - 1;
                            self.history_index = Some(idx);
                            self.input = self.prompt_history[idx].clone();
                            self.cursor_pos = self.input.chars().count();
                        }
                    } else if let Some(idx) = self.history_index {
                        // Navigate backward in history
                        if idx > 0 {
                            self.history_index = Some(idx - 1);
                            self.input = self.prompt_history[idx - 1].clone();
                            self.cursor_pos = self.input.chars().count();
                        }
                    } else {
                        // Not in history mode, scroll chat
                        self.chat_view.scroll_up();
                    }
                }
                KeyCode::Down => {
                    // History navigation
                    if let Some(idx) = self.history_index {
                        if idx < self.prompt_history.len() - 1 {
                            // Navigate forward in history
                            self.history_index = Some(idx + 1);
                            self.input = self.prompt_history[idx + 1].clone();
                            self.cursor_pos = self.input.chars().count();
                        } else {
                            // Exit history mode, restore stashed input
                            self.history_index = None;
                            self.input = self.history_stash.clone();
                            self.cursor_pos = self.input.chars().count();
                        }
                    } else {
                        // Not in history mode, scroll chat
                        self.chat_view.scroll_down();
                    }
                }
                _ if KeybindConfig::matches_any(
                    &self.keybinds.scroll_up,
                    key.code,
                    key.modifiers,
                ) =>
                {
                    self.chat_view.scroll_up();
                }
                _ if KeybindConfig::matches_any(
                    &self.keybinds.scroll_down,
                    key.code,
                    key.modifiers,
                ) =>
                {
                    self.chat_view.scroll_down();
                }
                _ if KeybindConfig::matches_any(&self.keybinds.page_up, key.code, key.modifiers) => {
                    self.chat_view.page_up();
                }
                _ if KeybindConfig::matches_any(
                    &self.keybinds.page_down,
                    key.code,
                    key.modifiers,
                ) =>
                {
                    self.chat_view.page_down();
                }
                _ => {}
            }
        }

        KeyAction::None
    }

    fn command_exists(command: &str) -> bool {
        if command.contains(std::path::MAIN_SEPARATOR) {
            return Path::new(command).is_file();
        }

        let Some(paths) = std::env::var_os("PATH") else {
            return false;
        };

        std::env::split_paths(&paths).any(|path| path.join(command).is_file())
    }

    fn resolve_external_editor() -> Option<String> {
        std::env::var("VISUAL")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .or_else(|| {
                std::env::var("EDITOR")
                    .ok()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
            })
            .or_else(|| {
                ["vim", "nano"]
                    .into_iter()
                    .find(|command| Self::command_exists(command))
                    .map(str::to_string)
            })
    }

    fn open_external_editor(&mut self) {
        let original_input = self.input.clone();

        let mut temp_file = match tempfile::NamedTempFile::new() {
            Ok(file) => file,
            Err(error) => {
                self.status = format!("Failed to create temp file: {}", error);
                return;
            }
        };

        if let Err(error) = temp_file.write_all(self.input.as_bytes()) {
            self.status = format!("Failed to prepare editor content: {}", error);
            return;
        }

        let temp_path = temp_file.into_temp_path();

        let editor = match Self::resolve_external_editor() {
            Some(editor) => editor,
            None => {
                self.status =
                    "No external editor found. Set $EDITOR/$VISUAL or install vim/nano".to_string();
                return;
            }
        };

        if let Err(error) = Self::suspend_terminal_for_external_editor() {
            self.status = format!("Failed to suspend terminal for editor: {}", error);
            return;
        }

        let editor_result = Self::run_editor_command(&editor, &temp_path);
        let restore_result = Self::restore_terminal_after_external_editor();

        if let Err(error) = restore_result {
            self.status = format!("Failed to restore terminal after editor: {}", error);
            self.should_quit = true;
            return;
        }

        if let Err(error) = editor_result {
            self.input = original_input;
            self.cursor_pos = self.input.chars().count();
            self.status = match error.kind() {
                std::io::ErrorKind::Interrupted => "External editor cancelled".to_string(),
                _ => format!("External editor failed: {}", error),
            };
            return;
        }

        match std::fs::read_to_string(&temp_path) {
            Ok(content) => {
                self.input = content;
                self.cursor_pos = self.input.chars().count();
                self.status = "Input updated from external editor".to_string();
            }
            Err(error) => {
                self.input = original_input;
                self.cursor_pos = self.input.chars().count();
                self.status = format!("Failed to read editor output: {}", error);
            }
        }
    }

    fn suspend_terminal_for_external_editor() -> std::io::Result<()> {
        crossterm::terminal::disable_raw_mode()?;

        let mut stdout = std::io::stdout();
        if let Err(error) = crossterm::execute!(
            stdout,
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::event::DisableMouseCapture
        ) {
            let _ = crossterm::terminal::enable_raw_mode();
            return Err(error);
        }

        stdout.flush()?;
        Ok(())
    }

    fn restore_terminal_after_external_editor() -> std::io::Result<()> {
        crossterm::terminal::enable_raw_mode()?;

        let mut stdout = std::io::stdout();
        if let Err(error) = crossterm::execute!(
            stdout,
            crossterm::terminal::EnterAlternateScreen,
            crossterm::event::EnableMouseCapture
        ) {
            let _ = crossterm::terminal::disable_raw_mode();
            return Err(error);
        }

        stdout.flush()?;
        Ok(())
    }

    fn run_editor_command(editor: &str, path: &Path) -> std::io::Result<()> {
        let mut parts = Vec::new();
        let mut current = String::new();
        let mut in_single = false;
        let mut in_double = false;
        let mut escaped = false;

        for ch in editor.chars() {
            if escaped {
                current.push(ch);
                escaped = false;
                continue;
            }

            match ch {
                '\\' if !in_single => escaped = true,
                '\'' if !in_double => in_single = !in_single,
                '"' if !in_single => in_double = !in_double,
                c if c.is_whitespace() && !in_single && !in_double => {
                    if !current.is_empty() {
                        parts.push(std::mem::take(&mut current));
                    }
                }
                _ => current.push(ch),
            }
        }
        if !current.is_empty() {
            parts.push(current);
        }

        let Some(program) = parts.first() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Editor command is empty",
            ));
        };

        let mut command = Command::new(program);
        command.args(parts.iter().skip(1)).arg(path);
        let status = command.status()?;

        if status.success() {
            Ok(())
        } else if status.code() == Some(130) {
            Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "Editor cancelled",
            ))
        } else {
            Err(std::io::Error::other(format!(
                "Editor command exited with status {}",
                status
            )))
        }
    }

    fn submit_input(&mut self, input: String) {
        if input.starts_with('/') {
            self.handle_slash_command(&input);
            return;
        }

        // Add to prompt history and reset history navigation
        if !input.trim().is_empty() {
            self.prompt_history.push(input.clone());
            let _ = self.save_prompt_history();
        }
        self.history_index = None;
        self.history_stash.clear();

        let pending_images = std::mem::take(&mut self.pending_images);
        let has_images = !pending_images.is_empty();

        let display_message = Self::format_user_display(&input, &pending_images);
        self.chat_view.push_message("user", display_message, None);

        let message = if has_images {
            let mut blocks = match MessageContent::from_prompt(&input) {
                MessageContent::Text(text) => {
                    if text.trim().is_empty() {
                        Vec::new()
                    } else {
                        vec![ContentBlock::text(text)]
                    }
                }
                MessageContent::Blocks(blocks) => blocks,
                MessageContent::ToolCalls(_) => Vec::new(),
            };

            for image in &pending_images {
                blocks.push(ContentBlock::Image {
                    source: ImageSource::Base64 {
                        media_type: image.media_type.clone(),
                        data: image.data.clone(),
                    },
                });
            }
            Message::with_blocks(Role::User, blocks)
        } else {
            Message::user(input.clone())
        };

        if let Some(ref tx) = self.agent_input_tx {
            let tx = tx.clone();
            tokio::spawn(async move {
                if tx.send(message).await.is_err() {
                    tracing::warn!("Agent input channel closed");
                }
            });
            self.status = if has_images {
                "Processing with image attachment(s)...".to_string()
            } else {
                "Processing...".to_string()
            };
            self.set_agent_state(AgentState::Thinking);
        } else {
            self.status = "No agent connected".to_string();
        }
    }

    fn format_user_display(input: &str, pending_images: &[PendingImage]) -> String {
        let parsed_input = MessageContent::from_prompt(input);
        let parsed_input = Self::format_content_for_display(&parsed_input);

        if pending_images.is_empty() {
            return parsed_input;
        }

        let mut lines = Vec::new();
        if !parsed_input.trim().is_empty() {
            lines.push(parsed_input);
        }

        for image in pending_images {
            lines.push(format!(
                "[image] {} ({})",
                image.label,
                Self::format_size(image.size_bytes)
            ));
        }

        lines.join("\n")
    }

    fn format_content_for_display(content: &MessageContent) -> String {
        match content {
            MessageContent::Text(text) => text.clone(),
            MessageContent::Blocks(blocks) => {
                let mut lines = Vec::new();

                for block in blocks {
                    match block {
                        ContentBlock::Text { text } => {
                            if !text.is_empty() {
                                lines.push(text.clone());
                            }
                        }
                        ContentBlock::Image { source } => lines.push(match source {
                            ImageSource::FilePath { path } => format!("[image] {}", path),
                            ImageSource::Url { url } => format!("[image] {}", url),
                            ImageSource::Base64 { media_type, .. } => {
                                format!("[image] embedded ({})", media_type)
                            }
                        }),
                        ContentBlock::ToolResult { content, .. } => {
                            if !content.is_empty() {
                                lines.push(content.clone());
                            }
                        }
                        _ => {}
                    }
                }

                lines.join("\n")
            }
            MessageContent::ToolCalls(calls) => calls
                .iter()
                .map(|call| format!("tool: {} ({})", call.name, call.id))
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }

    fn strip_optional_quotes(input: &str) -> &str {
        if input.len() >= 2 {
            let bytes = input.as_bytes();
            if (bytes[0] == b'"' && bytes[input.len() - 1] == b'"')
                || (bytes[0] == b'\'' && bytes[input.len() - 1] == b'\'')
            {
                return &input[1..input.len() - 1];
            }
        }
        input
    }

    fn format_size(size_bytes: usize) -> String {
        if size_bytes >= 1024 * 1024 {
            format!("{:.1}MB", size_bytes as f64 / (1024.0 * 1024.0))
        } else {
            format!("{:.1}KB", size_bytes as f64 / 1024.0)
        }
    }

    fn resolve_image_path(raw_path: &str, working_directory: &str) -> Result<PathBuf, String> {
        let path = raw_path.trim();
        if path.is_empty() {
            return Err("empty path".to_string());
        }

        let expanded = if path == "~" {
            std::env::var("HOME")
                .map(PathBuf::from)
                .map_err(|_| "HOME environment variable is not set".to_string())?
        } else if let Some(relative) = path.strip_prefix("~/") {
            PathBuf::from(
                std::env::var("HOME")
                    .map_err(|_| "HOME environment variable is not set".to_string())?,
            )
            .join(relative)
        } else {
            PathBuf::from(path)
        };

        let absolute = if expanded.is_absolute() {
            expanded
        } else {
            PathBuf::from(working_directory).join(expanded)
        };

        if !absolute.exists() {
            return Err(format!("file does not exist: {}", absolute.display()));
        }

        if !absolute.is_file() {
            return Err(format!("not a file: {}", absolute.display()));
        }

        Ok(absolute)
    }

    fn media_type_for_path(path: &Path) -> Option<&'static str> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        match ext.as_str() {
            "png" => Some("image/png"),
            "jpg" | "jpeg" => Some("image/jpeg"),
            "gif" => Some("image/gif"),
            "webp" => Some("image/webp"),
            "bmp" => Some("image/bmp"),
            _ => None,
        }
    }

    fn load_pending_image(path: &Path) -> Result<PendingImage, String> {
        let media_type = Self::media_type_for_path(path).ok_or_else(|| {
            format!(
                "unsupported image format for '{}'; supported: png, jpg, jpeg, gif, webp, bmp",
                path.display()
            )
        })?;

        let bytes = std::fs::read(path)
            .map_err(|e| format!("failed to read image '{}': {}", path.display(), e))?;

        if bytes.is_empty() {
            return Err(format!("image is empty: {}", path.display()));
        }

        if bytes.len() > MAX_IMAGE_BYTES {
            return Err(format!(
                "image is too large ({} > {}): {}",
                Self::format_size(bytes.len()),
                Self::format_size(MAX_IMAGE_BYTES),
                path.display()
            ));
        }

        let label = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| path.display().to_string());

        Ok(PendingImage {
            label,
            media_type: media_type.to_string(),
            data: BASE64_STANDARD.encode(bytes.as_slice()),
            size_bytes: bytes.len(),
        })
    }

    fn attach_image_from_path(&self, raw_path: &str) -> Result<PendingImage, String> {
        let path = Self::resolve_image_path(raw_path, &self.working_directory)?;
        Self::load_pending_image(&path)
    }

    fn capture_and_attach_screenshot(&self) -> Result<PendingImage, String> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| format!("system clock error: {}", e))?
            .as_millis();
        let path = std::env::temp_dir().join(format!("uira-screenshot-{}.png", timestamp));

        Self::capture_screenshot_to(&path)?;

        let image_result = Self::load_pending_image(&path);
        if let Err(error) = std::fs::remove_file(&path) {
            tracing::debug!(
                "Failed to remove temporary screenshot '{}': {}",
                path.display(),
                error
            );
        }
        image_result
    }

    fn capture_screenshot_to(path: &Path) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        {
            let status = Command::new("screencapture")
                .arg("-x")
                .arg(path)
                .status()
                .map_err(|e| format!("failed to run screencapture: {}", e))?;

            if status.success() {
                Ok(())
            } else {
                Err(format!("screencapture exited with status {}", status))
            }
        }

        #[cfg(target_os = "linux")]
        {
            if let Ok(status) = Command::new("grim").arg(path).status() {
                if status.success() {
                    return Ok(());
                }
            }

            let status = Command::new("gnome-screenshot")
                .arg("-f")
                .arg(path)
                .status()
                .map_err(|e| format!("failed to run screenshot command: {}", e))?;

            if status.success() {
                Ok(())
            } else {
                Err(format!("screenshot command exited with status {}", status))
            }
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            let _ = path;
            Err("/screenshot is not supported on this platform".to_string())
        }
    }

    fn run_review_command(&mut self, raw_command: &str) {
        let target = match parse_review_target_from_command(raw_command) {
            Ok(target) => target,
            Err(err) => {
                self.chat_view.push_message("error", err, None);
                return;
            }
        };
        let target_description = target.description();

        let (content, skipped_binary) = match collect_review_content(&target, &self.working_directory) {
            Ok(content) => content,
            Err(err) => {
                self.chat_view
                    .push_message("error", format!("Failed to gather review input: {}", err), None);
                return;
            }
        };

        if content.is_empty() {
            let suffix = if skipped_binary.is_empty() {
                String::new()
            } else {
                format!(" Skipped binary files: {}.", skipped_binary.join(", "))
            };
                self.chat_view.push_message(
                    "system",
                    format!("No diff found for {}.{}", target_description, suffix),
                    None,
                );
            return;
        }

        self.chat_view
            .push_message("user", raw_command.to_string(), None);
        self.chat_view.push_message(
            "system",
            format!(
                "Starting review for {}. Output will be grouped into issues, suggestions, and praise.",
                target_description
            ),
            None,
        );

        let mut review_prompt = build_review_prompt(&target, &content);
        if !skipped_binary.is_empty() {
            review_prompt.push_str("\n\nSkipped binary files: ");
            review_prompt.push_str(&skipped_binary.join(", "));
            review_prompt.push('.');
        }
        if let Some(ref tx) = self.agent_input_tx {
            let tx = tx.clone();
            tokio::spawn(async move {
                if tx.send(Message::user(review_prompt)).await.is_err() {
                    tracing::warn!("Agent input channel closed");
                }
            });
            self.status = "Processing review...".to_string();
            self.set_agent_state(AgentState::Thinking);
        } else {
            self.status = "No agent connected".to_string();
        }
    }

    fn handle_slash_command(&mut self, input: &str) {
        let parts: Vec<&str> = input.split_whitespace().collect();
        let command = parts.first().copied().unwrap_or("");

        match command {
            "/exit" | "/quit" | "/q" => {
                self.should_quit = true;
            }
            "/help" | "/h" | "/?" => {
                self.chat_view.messages.push(ChatMessage::new(
                    "system",
                    "Available commands:\n  /help, /h, /?       - Show this help\n  /exit, /quit, /q    - Exit the application\n  /auth, /status      - Show current status\n  /models             - List available models\n  /model <name>       - Switch to a different model\n  /theme              - List available themes\n  /theme <name>       - Switch theme\n  /image <path>       - Attach image for next prompt\n  /screenshot         - Capture and attach screenshot\n  /fork [name|count]  - Fork session (optional branch name or keep first N messages)\n  /switch <branch>    - Switch to branch\n  /branches           - List branches\n  /tree               - Show branch tree\n  /review             - Review staged changes\n  /review <file>      - Review changes for a specific file\n  /review HEAD~1      - Review a specific commit\n  /share              - Share session to GitHub Gist\n  /clear              - Clear chat history and pending attachments",
                ));
            }
            "/auth" | "/status" => {
                let status_msg = if self.agent_input_tx.is_some() {
                    "Agent connected"
                } else {
                    "No agent connected"
                };
                self.chat_view.push_message(
                    "system",
                    format!("Status: {}\nState: {:?}", status_msg, self.agent_state),
                    None,
                );
            }
            "/clear" => {
                self.chat_view.messages.clear();
                self.pending_images.clear();
                self.message_queue.clear();
                self.status = "Chat cleared".to_string();
            }
            "/image" => {
                let path_arg = input[command.len()..].trim();
                if path_arg.is_empty() {
                    self.chat_view
                        .messages
                        .push(ChatMessage::new("system", "Usage: /image <path>"));
                    self.chat_view.auto_scroll_to_bottom();
                    return;
                }

                let parsed_path = Self::strip_optional_quotes(path_arg);
                match self.attach_image_from_path(parsed_path) {
                    Ok(image) => {
                        let image_label = image.label.clone();
                        let image_size = Self::format_size(image.size_bytes);
                        self.pending_images.push(image);
                        self.status = format!("Attached {} image(s)", self.pending_images.len());
                        self.chat_view.messages.push(ChatMessage::new(
                            "system",
                            format!(
                                "Attached image: {} ({})\nIt will be sent with your next prompt.",
                                image_label, image_size
                            ),
                        ));
                    }
                    Err(error) => {
                        self.chat_view.messages.push(ChatMessage::new(
                            "error",
                            format!("Failed to attach image: {}", error),
                        ));
                    }
                }
            }
            "/screenshot" => match self.capture_and_attach_screenshot() {
                Ok(image) => {
                    let image_label = image.label.clone();
                    let image_size = Self::format_size(image.size_bytes);
                    self.pending_images.push(image);
                    self.status = format!("Attached {} image(s)", self.pending_images.len());
                    self.chat_view.messages.push(ChatMessage::new(
                        "system",
                        format!(
                            "Captured screenshot: {} ({})\nIt will be sent with your next prompt.",
                            image_label, image_size
                        ),
                    ));
                }
                Err(error) => {
                    self.chat_view.messages.push(ChatMessage::new(
                        "error",
                        format!("Failed to capture screenshot: {}", error),
                    ));
                }
            },
            "/review" => {
                self.run_review_command(input);
            }
            "/fork" => {
                let (branch_name, message_count) = match parts.get(1).copied() {
                    Some(arg) => match arg.parse::<usize>() {
                        Ok(count) => (None, Some(count)),
                        Err(_) => (Some(arg.to_string()), None),
                    },
                    None => (None, None),
                };
                self.fork_session(branch_name, message_count);
            }
            "/switch" => {
                if let Some(branch_name) = parts.get(1) {
                    self.switch_branch(branch_name);
                } else {
                    self.chat_view
                        .messages
                        .push(ChatMessage::new("system", "Usage: /switch <branch>"));
                }
            }
            "/branches" => {
                self.list_branches();
            }
            "/tree" => {
                self.show_branch_tree();
            }
            "/share" => match parse_share_command(&parts) {
                Ok(options) => self.share_session(options),
                Err(err) => {
                    self.chat_view.messages.push(ChatMessage::new(
                        "error",
                        format!(
                            "{}\nUsage: /share [--public] [--description <text>]",
                            err
                        ),
                    ));
                }
            },
            "/models" => {
                self.model_selector.open(self.current_model.clone());
            }
            "/model" => {
                if let Some(model_name) = parts.get(1) {
                    let full_model = if model_name.contains('/') {
                        (*model_name).to_string()
                    } else {
                        MODEL_GROUPS
                            .iter()
                            .find(|group| group.models.contains(model_name))
                            .map(|group| format!("{}/{}", group.provider, model_name))
                            .unwrap_or_else(|| (*model_name).to_string())
                    };

                    let is_known = MODEL_GROUPS
                        .iter()
                        .any(|group| group.models.iter().any(|m| full_model.ends_with(m)));

                    if is_known || model_name.contains('/') {
                        self.switch_model(&full_model);
                    } else {
                        self.chat_view.push_message(
                            "system",
                            format!(
                                "Unknown model: {}. Use /models to see available options.",
                                model_name
                            ),
                            None,
                        );
                    }
                } else {
                    let current = self
                        .current_model
                        .as_deref()
                        .unwrap_or("(default from config)");
                    self.chat_view.push_message(
                        "system",
                        format!("Current model: {}\nUsage: /model <name>", current),
                        None,
                    );
                }
            }
            "/theme" => {
                if let Some(theme_name) = parts.get(1) {
                    match self.set_theme_by_name(theme_name) {
                        Ok(()) => {
                            self.status = format!("Theme changed to {}", self.theme.name);
                            self.chat_view.messages.push(ChatMessage::new(
                                "system",
                                format!("Theme set to {}", self.theme.name),
                            ));
                        }
                        Err(err) => {
                            self.chat_view.messages.push(ChatMessage::new("system", err));
                        }
                    }
                } else {
                    self.chat_view.messages.push(ChatMessage::new(
                        "system",
                        format!(
                            "Current theme: {}\nAvailable themes: {}\nUsage: /theme <name>",
                            self.theme.name,
                            Theme::available_names().join(", ")
                        ),
                    ));
                }
            }
            _ => {
                self.chat_view.push_message(
                    "system",
                    format!(
                        "Unknown command: {}. Type /help for available commands.",
                        command
                    ),
                    None,
                );
            }
        }

        self.chat_view.auto_scroll_to_bottom();
    }

    fn execute_palette_command(&mut self, id: &str) {
        match id {
            "help" => self.handle_slash_command("/help"),
            "status" => self.handle_slash_command("/status"),
            "clear" => self.handle_slash_command("/clear"),
            "exit" => self.handle_slash_command("/exit"),
            "models" => self.handle_slash_command("/models"),
            "model" => self.handle_slash_command("/model"),
            "theme_list" => self.handle_slash_command("/theme"),
            "fork" => self.handle_slash_command("/fork"),
            "switch" => self.handle_slash_command("/switch"),
            "branches" => self.handle_slash_command("/branches"),
            "tree" => self.handle_slash_command("/tree"),
            "share" => self.handle_slash_command("/share"),
            "review" => self.handle_slash_command("/review"),
            "image" => self.handle_slash_command("/image"),
            "screenshot" => self.handle_slash_command("/screenshot"),
            "collapse_tools" => {
                self.status = self.chat_view.collapse_all_tool_outputs();
            }
            "expand_tools" => {
                self.status = self.chat_view.expand_all_tool_outputs();
            }
            "toggle_sidebar" => {
                self.toggle_todo_sidebar();
            }
            _ => {}
        }
    }

    fn handle_app_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Quit => self.should_quit = true,
            AppEvent::Agent(thread_event) => self.handle_agent_event(thread_event),
            AppEvent::TracingLog(msg) => {
                self.chat_view
                    .push_message("error", format!("log: {}", msg), None);
            }
            AppEvent::UserInput(_) => {
                // Already handled in submit_input
            }
            AppEvent::ApprovalRequest(request) => {
                // Enqueue approval and update state
                self.approval_overlay.enqueue(request);
                self.set_agent_state(AgentState::WaitingForApproval);
            }
            AppEvent::TodoUpdated(todos) => {
                self.todos = todos;
            }
            AppEvent::Info(message) => {
                self.status = message.clone();
                self.chat_view.push_message("system", message, None);
            }
            AppEvent::BranchChanged(branch_name) => {
                self.current_branch = branch_name;
            }
            AppEvent::SessionChanged(session_id) => {
                self.session_id = Some(session_id);
            }
            AppEvent::Redraw => {}
            AppEvent::Error(msg) => {
                self.status = format!("Error: {}", msg);
                self.chat_view
                    .push_message("system", format!("Error: {}", msg), None);
            }
        }
    }

    fn handle_agent_event(&mut self, event: ThreadEvent) {
        match event {
            ThreadEvent::ThreadStarted { thread_id } => {
                self.session_id = Some(thread_id);
                self.set_agent_state(AgentState::Thinking);
                self.status = "Agent started".to_string();
            }
            ThreadEvent::TurnStarted { turn_number } => {
                self.status = format!("Turn {}", turn_number);
            }
            ThreadEvent::TurnCompleted { turn_number, usage } => {
                self.status = format!(
                    "Turn {} complete ({} in / {} out tokens)",
                    turn_number, usage.input_tokens, usage.output_tokens
                );
            }
            ThreadEvent::WaitingForInput { prompt } => {
                self.set_agent_state(AgentState::WaitingForUser);
                self.status = prompt;
                self.process_queued_messages();
            }
            ThreadEvent::ContentDelta { delta } => {
                self.chat_view
                    .append_streaming_delta(&delta, MAX_STREAMING_BUFFER_SIZE);
            }
            ThreadEvent::ThinkingDelta { thinking } => {
                self.chat_view
                    .append_thinking_delta(&thinking, MAX_STREAMING_BUFFER_SIZE);
            }
            ThreadEvent::ItemStarted { item } => match item {
                Item::ToolCall {
                    id,
                    name,
                    ref input,
                } => {
                    if matches!(name.as_str(), "Edit" | "Write") {
                        if let Some(path) = input
                            .get("file_path")
                            .or_else(|| input.get("filePath"))
                            .or_else(|| input.get("path"))
                            .and_then(|v| v.as_str())
                        {
                            self.pending_tool_paths.insert(id.clone(), path.to_string());
                        }
                    }
                    self.chat_view.tool_call_names.insert(id, name.clone());
                    self.set_agent_state(AgentState::ExecutingTool);
                    self.status = format!("Executing: {}", name);
                }
                Item::AgentMessage { content, .. } => {
                    // Start streaming
                    self.chat_view.set_streaming_buffer(content);
                }
                Item::ApprovalRequest {
                    id,
                    tool_name,
                    input,
                    reason,
                } => {
                    // Note: Actual approval requests come via the approval channel with oneshot sender
                    // This event is just for logging/display purposes
                    self.status = format!("Approval needed: {} - {}", tool_name, reason);
                    self.set_agent_state(AgentState::WaitingForApproval);
                    tracing::debug!(
                        "Approval request: {} for {} with input {:?}",
                        id,
                        tool_name,
                        input
                    );
                }
                _ => {}
            },
            ThreadEvent::ItemCompleted { item } => match item {
                Item::ToolResult {
                    tool_call_id,
                    output,
                    is_error,
                } => {
                    if !is_error {
                        if let Some(path) = self.pending_tool_paths.remove(&tool_call_id) {
                            self.modified_files.insert(path);
                        }
                    } else {
                        self.pending_tool_paths.remove(&tool_call_id);
                    }
                    let tool_name = self
                        .chat_view
                        .tool_call_names
                        .remove(&tool_call_id)
                        .unwrap_or_else(|| "tool".to_string());
                    let subagent_task_id = if matches!(tool_name.as_str(), "delegate_task" | "Task") {
                        extract_task_id(&output)
                    } else {
                        None
                    };
                    let role = if is_error { "error" } else { "tool" };
                    self.chat_view
                        .push_tool_message(role, tool_name, output, false, None);
                    if let Some(task_id) = subagent_task_id {
                        if let Some(last) = self.chat_view.messages.last_mut() {
                            last.message_id = Some(task_id);
                        }
                    }
                    self.set_agent_state(AgentState::Thinking);
                }
                Item::AgentMessage { content, name } => {
                    self.chat_view.clear_streaming_buffer();
                    if let Some(thinking) = self.chat_view.take_thinking_buffer() {
                        if !thinking.is_empty() {
                            self.chat_view.push_message("thinking", thinking, None);
                        }
                    }
                    self.chat_view.push_message("assistant", content, name);
                }
                Item::ApprovalDecision {
                    request_id,
                    approved,
                } => {
                    self.status = format!(
                        "Approval {}: {}",
                        request_id,
                        if approved { "granted" } else { "denied" }
                    );
                    if approved {
                        self.set_agent_state(AgentState::ExecutingTool);
                    } else {
                        self.set_agent_state(AgentState::Thinking);
                    }
                }
                _ => {}
            },
            ThreadEvent::ThreadCompleted { usage } => {
                self.set_agent_state(AgentState::Complete);
                self.status = format!(
                    "Complete (total: {} in / {} out tokens)",
                    usage.input_tokens, usage.output_tokens
                );

                if let Some(thinking) = self.chat_view.take_thinking_buffer() {
                    if !thinking.is_empty() {
                        self.chat_view.push_message("thinking", thinking, None);
                    }
                }

                if let Some(buffer) = self.chat_view.clear_streaming_buffer() {
                    if !buffer.is_empty() {
                        self.chat_view.push_message("assistant", buffer, None);
                    }
                }
            }
            ThreadEvent::ThreadCancelled => {
                self.set_agent_state(AgentState::Cancelled);
                self.status = "Cancelled".to_string();
            }
            ThreadEvent::Error { message, .. } => {
                self.set_agent_state(AgentState::Failed);
                self.status = format!("Error: {}", message);
                self.chat_view.push_message("error", message, None);
            }
            // Goal Verification Events
            ThreadEvent::GoalVerificationStarted { goals, .. } => {
                self.status = format!("Verifying {} goals...", goals.len());
            }
            ThreadEvent::GoalVerificationResult {
                goal,
                passed,
                score,
                target,
                ..
            } => {
                let icon = if passed { "[PASS]" } else { "[FAIL]" };
                self.chat_view.push_message(
                    "system",
                    format!(
                        "{} Goal '{}': {:.1}% (target: {:.1}%)",
                        icon, goal, score, target
                    ),
                    None,
                );
            }
            ThreadEvent::GoalVerificationCompleted {
                all_passed,
                passed_count,
                total_count,
            } => {
                if all_passed {
                    self.status = format!("All {}/{} goals passed", passed_count, total_count);
                } else {
                    self.status = format!("Goals: {}/{} passed", passed_count, total_count);
                }
            }
            // Ralph Mode Events
            ThreadEvent::RalphIterationStarted {
                iteration,
                max_iterations,
                ..
            } => {
                self.status = format!("Ralph iteration {}/{}", iteration, max_iterations);
            }
            ThreadEvent::RalphContinuation {
                reason, confidence, ..
            } => {
                self.chat_view.push_message(
                    "system",
                    format!("Ralph continuing: {} (confidence: {}%)", reason, confidence),
                    None,
                );
            }
            ThreadEvent::RalphCircuitBreak { reason, iteration } => {
                self.chat_view.push_message(
                    "system",
                    format!("Ralph stopped at iteration {}: {}", iteration, reason),
                    None,
                );
                self.set_agent_state(AgentState::Complete);
            }
            ThreadEvent::BackgroundTaskSpawned {
                task_id,
                description,
                agent: ref agent_name,
            } => {
                self.task_registry
                    .on_spawned(task_id.clone(), description.clone(), agent_name.clone());
                self.chat_view.push_message(
                    "system",
                    format!("Background task started: {} ({})", description, task_id),
                    Some(agent_name.clone()),
                );
            }
            ThreadEvent::BackgroundTaskProgress {
                task_id,
                status,
                message,
            } => {
                self.task_registry.on_progress(&task_id, &status);
                let msg = message.map(|m| format!(": {}", m)).unwrap_or_default();
                self.chat_view.push_message(
                    "system",
                    format!("Background task {} - {}{}", task_id, status, msg),
                    None,
                );
            }
            ThreadEvent::BackgroundTaskCompleted {
                task_id, success, ..
            } => {
                self.task_registry.on_completed(&task_id, success);
                let status = if success { "completed" } else { "failed" };
                self.chat_view
                    .push_message("system", format!("Background task {}: {}", task_id, status), None);
            }
            ThreadEvent::SubagentStarted {
                task_id,
                agent_name,
                model,
                session_id,
            } => {
                self.subagent_task_sessions
                    .insert(task_id.clone(), session_id.clone());
                let parent_id = self.session_stack.current_id().to_string();
                let child = SessionView::child(
                    session_id.clone(),
                    agent_name.clone(),
                    Some(model.clone()),
                    parent_id,
                );
                self.session_stack.register_child(child);
                self.chat_view.push_message(
                    "system",
                    format!(
                        "Subagent started: {} ({}, task: {}, session: {})",
                        agent_name, model, task_id, session_id
                    ),
                    Some(agent_name),
                );
            }
            ThreadEvent::SubagentCompleted {
                task_id,
                session_id,
                success,
                duration_secs,
            } => {
                self.session_stack.mark_completed(&session_id, success);
                let status = if success { "completed" } else { "failed" };
                self.chat_view.push_message(
                    "system",
                    format!(
                        "Subagent {}: {} (task: {}, {:.1}s)",
                        session_id, status, task_id, duration_secs
                    ),
                    None,
                );
            }
            ThreadEvent::ModelSwitched { model, provider } => {
                self.status = format!("Model: {}/{}", provider, model);
                self.chat_view
                    .push_message("system", format!("Switched to {}/{}", provider, model), None);
            }
            ThreadEvent::TodoUpdated { todos } => {
                let pending = todos
                    .iter()
                    .filter(|t| {
                        t.status != TodoStatus::Completed && t.status != TodoStatus::Cancelled
                    })
                    .count();
                self.status = format!("{} todos ({} remaining)", todos.len(), pending);
                self.todos = todos;
            }
            _ => {
                tracing::debug!("Unhandled ThreadEvent variant");
            }
        }
    }

    /// Get the event sender for external communication
    pub fn event_sender(&self) -> mpsc::Sender<AppEvent> {
        self.event_tx.clone()
    }

    pub fn add_message(&mut self, role: &str, content: &str) {
        self.chat_view.push_message(role, content.to_string(), None);
        #[cfg(test)]
        {
            self.messages = self.chat_view.messages.clone();
        }
    }

    /// Set the status message
    pub fn set_status(&mut self, status: &str) {
        self.status = status.to_string();
    }

    /// Set the agent state
    pub fn set_agent_state(&mut self, state: AgentState) {
        self.agent_state = state;
        self.chat_view.agent_state = state;
    }

    fn switch_model(&mut self, model_str: &str) {
        self.current_model = Some(model_str.to_string());
        self.status = format!("Switching to {}...", model_str);

        match create_client_for_model(model_str) {
            Ok(new_client) => {
                self.toast_manager.show(
                    format!("Switched to {}", model_str),
                    ToastVariant::Success,
                    3000,
                );
                if let Some(ref tx) = self.agent_command_tx {
                    let tx = tx.clone();
                    let model_str = model_str.to_string();
                    tokio::spawn(async move {
                        if tx
                            .send(AgentCommand::SwitchClient(new_client))
                            .await
                            .is_err()
                        {
                            tracing::warn!("Failed to send model switch command for {}", model_str);
                        }
                    });
                } else {
                    self.chat_view.push_message(
                        "system",
                        format!(
                            "Model set to: {}\nNote: No agent connected, change will apply when agent starts.",
                            model_str
                        ),
                        None,
                    );
                }
            }
            Err(e) => {
                self.status = "Model switch failed".to_string();
                self.toast_manager.show(
                    format!("Failed to switch model: {}", e),
                    ToastVariant::Error,
                    5000,
                );
                self.chat_view.push_message(
                    "error",
                    format!("Failed to create client for {}: {}", model_str, e),
                    None,
                );
            }
        }
    }

    fn share_session(&mut self, options: ShareCommandOptions) {
        if self.chat_view.messages.is_empty() {
            self.chat_view.messages.push(ChatMessage::new(
                "system",
                "No messages to share yet. Start a conversation first.",
            ));
            return;
        }

        let messages = self.chat_view.messages.clone();
        let session_id = self.session_id.clone();
        let model = self.current_model.clone();
        let event_tx = self.event_tx.clone();
        self.status = "Sharing session to GitHub Gist...".to_string();
        self.chat_view.push_message(
            "system",
            "Warning: sharing may include sensitive content from system/tool outputs. Review before sharing.\nUse /share --public only when intentional."
                .to_string(),
            None,
        );

        tokio::spawn(async move {
            let markdown =
                render_session_markdown(&messages, session_id.as_deref(), model.as_deref());
            match create_gist_from_markdown(markdown, options.clone(), session_id).await {
                Ok(url) => {
                    let visibility = if options.public { "public" } else { "secret" };
                    let _ = event_tx
                        .send(AppEvent::Info(format!(
                            "Session shared to GitHub Gist ({})\nURL: {}",
                            visibility, url
                        )))
                        .await;
                }
                Err(err) => {
                    let _ = event_tx
                        .send(AppEvent::Error(format!("Share failed: {}", err)))
                        .await;
                }
            }
        });
    }

    fn fork_session(&mut self, branch_name: Option<String>, message_count: Option<usize>) {
        let target = branch_name
            .clone()
            .unwrap_or_else(|| "auto-generated branch".to_string());
        let scope = message_count
            .map(|count| format!(" from first {} message(s)", count))
            .unwrap_or_default();
        self.status = format!("Forking into '{}'{}...", target, scope);

        if let Some(ref tx) = self.agent_command_tx {
            let tx = tx.clone();
            let event_tx = self.event_tx.clone();
            tokio::spawn(async move {
                let (response_tx, response_rx) = tokio::sync::oneshot::channel();
                if tx
                    .send(AgentCommand::Fork {
                        branch_name,
                        message_count,
                        response_tx,
                    })
                    .await
                    .is_err()
                {
                    tracing::warn!("Failed to send fork command");
                    return;
                }

                match response_rx.await {
                    Ok(Ok(result)) => {
                        let _ = event_tx
                            .send(AppEvent::Info(format!(
                                "Created branch '{}' from '{}' (session {})",
                                result.branch_name, result.parent_branch, result.session_id
                            )))
                            .await;
                    }
                    Ok(Err(e)) => {
                        let _ = event_tx
                            .send(AppEvent::Error(format!("Fork failed: {}", e)))
                            .await;
                    }
                    Err(_) => {
                        let _ = event_tx
                            .send(AppEvent::Error("Fork response channel closed".to_string()))
                            .await;
                    }
                }
            });
        } else {
            self.chat_view.push_message(
                "error",
                "No agent connected. Cannot fork session.".to_string(),
                None,
            );
        }
    }

    fn switch_branch(&mut self, branch_name: &str) {
        self.status = format!("Switching to branch '{}'...", branch_name);

        if let Some(ref tx) = self.agent_command_tx {
            let tx = tx.clone();
            let event_tx = self.event_tx.clone();
            let branch_name = branch_name.to_string();

            tokio::spawn(async move {
                let (response_tx, response_rx) = tokio::sync::oneshot::channel();
                if tx
                    .send(AgentCommand::SwitchBranch {
                        branch_name: branch_name.clone(),
                        response_tx,
                    })
                    .await
                    .is_err()
                {
                    tracing::warn!("Failed to send switch branch command");
                    return;
                }

                match response_rx.await {
                    Ok(Ok(result)) => {
                        let _ = event_tx
                            .send(AppEvent::BranchChanged(result.branch_name.clone()))
                            .await;
                        let _ = event_tx
                            .send(AppEvent::SessionChanged(result.session_id))
                            .await;
                        let _ = event_tx.send(AppEvent::Info(result.message)).await;
                    }
                    Ok(Err(e)) => {
                        let _ = event_tx
                            .send(AppEvent::Error(format!("Switch failed: {}", e)))
                            .await;
                    }
                    Err(_) => {
                        let _ = event_tx
                            .send(AppEvent::Error(
                                "Switch branch response channel closed".to_string(),
                            ))
                            .await;
                    }
                }
            });
        } else {
            self.chat_view.messages.push(ChatMessage::new(
                "error",
                "No agent connected. Cannot switch branches.",
            ));
        }
    }

    fn list_branches(&mut self) {
        self.status = "Loading branches...".to_string();

        if let Some(ref tx) = self.agent_command_tx {
            let tx = tx.clone();
            let event_tx = self.event_tx.clone();

            tokio::spawn(async move {
                let (response_tx, response_rx) = tokio::sync::oneshot::channel();
                if tx
                    .send(AgentCommand::ListBranches { response_tx })
                    .await
                    .is_err()
                {
                    tracing::warn!("Failed to send list branches command");
                    return;
                }

                match response_rx.await {
                    Ok(Ok(branches)) => {
                        if let Some(current) = branches.iter().find(|b| b.is_current) {
                            let _ = event_tx
                                .send(AppEvent::BranchChanged(current.name.clone()))
                                .await;
                            let _ = event_tx
                                .send(AppEvent::SessionChanged(current.session_id.clone()))
                                .await;
                        }

                        let _ = event_tx
                            .send(AppEvent::Info(format_branch_list(&branches)))
                            .await;
                    }
                    Ok(Err(e)) => {
                        let _ = event_tx
                            .send(AppEvent::Error(format!("List branches failed: {}", e)))
                            .await;
                    }
                    Err(_) => {
                        let _ = event_tx
                            .send(AppEvent::Error(
                                "List branches response channel closed".to_string(),
                            ))
                            .await;
                    }
                }
            });
        } else {
            self.chat_view.messages.push(ChatMessage::new(
                "error",
                "No agent connected. Cannot list branches.",
            ));
        }
    }

    fn show_branch_tree(&mut self) {
        self.status = "Loading branch tree...".to_string();

        if let Some(ref tx) = self.agent_command_tx {
            let tx = tx.clone();
            let event_tx = self.event_tx.clone();

            tokio::spawn(async move {
                let (response_tx, response_rx) = tokio::sync::oneshot::channel();
                if tx
                    .send(AgentCommand::BranchTree { response_tx })
                    .await
                    .is_err()
                {
                    tracing::warn!("Failed to send branch tree command");
                    return;
                }

                match response_rx.await {
                    Ok(Ok(tree)) => {
                        let _ = event_tx.send(AppEvent::Info(tree)).await;
                    }
                    Ok(Err(e)) => {
                        let _ = event_tx
                            .send(AppEvent::Error(format!("Branch tree failed: {}", e)))
                            .await;
                    }
                    Err(_) => {
                        let _ = event_tx
                            .send(AppEvent::Error(
                                "Branch tree response channel closed".to_string(),
                            ))
                            .await;
                    }
                }
            });
        } else {
            self.chat_view.messages.push(ChatMessage::new(
                "error",
                "No agent connected. Cannot show branch tree.",
            ));
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn parse_review_target_defaults_to_staged() {
        assert_eq!(parse_review_target(&[]).unwrap(), ReviewTarget::Staged);
    }

    #[test]
    fn parse_review_target_handles_file_path() {
        assert_eq!(
            parse_review_target(&["crates/uira-tui/src/app.rs"]).unwrap(),
            ReviewTarget::File("crates/uira-tui/src/app.rs".to_string())
        );
    }

    #[test]
    fn parse_review_target_handles_revision() {
        if !is_valid_commit_reference("HEAD") {
            return;
        }
        assert_eq!(
            parse_review_target(&["HEAD"]).unwrap(),
            ReviewTarget::Revision("HEAD".to_string())
        );
    }

    #[test]
    fn parse_review_target_rejects_invalid_revision() {
        let result = parse_review_target(&["abc1234abc1234"]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid revision"));
    }

    #[test]
    fn parse_review_target_from_command_supports_quoted_paths() {
        assert_eq!(
            parse_review_target_from_command("/review \"crates/uira-tui/src/app.rs\"").unwrap(),
            ReviewTarget::File("crates/uira-tui/src/app.rs".to_string())
        );
    }

    #[test]
    fn review_prompt_enforces_required_sections() {
        let prompt = build_review_prompt(&ReviewTarget::Staged, "diff --git a/foo b/foo");

        assert!(prompt.contains("## Issues"));
        assert!(prompt.contains("## Suggestions"));
        assert!(prompt.contains("## Praise"));
        assert!(prompt.contains("```diff"));
    }

    #[test]
    fn parse_binary_paths_detects_binary_entries() {
        let binaries = parse_binary_paths("12\t4\tsrc/lib.rs\n-\t-\tassets/logo.png\n");
        assert!(binaries.contains("assets/logo.png"));
        assert!(!binaries.contains("src/lib.rs"));
    }

    #[test]
    fn parse_binary_paths_normalizes_renamed_paths() {
        let binaries = parse_binary_paths("-\t-\tsrc/{old => new}.png\n");
        assert!(binaries.contains("src/new.png"));
        assert!(!binaries.contains("src/old.png"));
    }

    #[test]
    fn filter_binary_sections_removes_binary_diff() {
        let mut binaries = HashSet::new();
        binaries.insert("assets/logo.png".to_string());

        let input = "diff --git a/src/lib.rs b/src/lib.rs\nindex 1..2 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\ndiff --git a/assets/logo.png b/assets/logo.png\nindex 3..4 100644\nBinary files a/assets/logo.png and b/assets/logo.png differ\n";

        let filtered = filter_binary_sections(input, &binaries);
        assert!(filtered.contains("diff --git a/src/lib.rs b/src/lib.rs"));
        assert!(!filtered.contains("diff --git a/assets/logo.png b/assets/logo.png"));
    }

    #[test]
    fn ctrl_g_triggers_external_editor_action() {
        let mut app = App::new();

        let action = app.handle_key_event(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL));

        assert_eq!(action, KeyAction::OpenExternalEditor);
    }

    #[test]
    fn ctrl_l_keeps_chat_history() {
        let mut app = App::new();
        app.add_message("assistant", "first");

        let action = app.handle_key_event(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL));

        assert_eq!(action, KeyAction::None);
        assert_eq!(app.messages.len(), 1);
    }

    #[test]
    fn todo_sidebar_is_enabled_by_default() {
        let app = App::new();
        assert!(app.show_todo_sidebar);
    }

    #[test]
    fn unknown_ctrl_shortcut_does_not_insert_text() {
        let mut app = App::new();

        let action = app.handle_key_event(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL));

        assert_eq!(action, KeyAction::None);
        assert!(app.input.is_empty());
        assert_eq!(app.cursor_pos, 0);
    }

    #[test]
    fn run_editor_command_supports_true_and_cat() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "hello").unwrap();

        App::run_editor_command("true", file.path()).unwrap();
        App::run_editor_command("cat", file.path()).unwrap();
    }

    #[test]
    fn run_editor_command_reports_non_zero_exit() {
        let file = NamedTempFile::new().unwrap();

        let err = App::run_editor_command("false", file.path()).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::Other);
    }

    #[test]
    fn parse_share_command_handles_public_and_description() {
        let parts = vec![
            "/share",
            "--public",
            "--description",
            "Auth",
            "debug",
            "session",
        ];
        let options = parse_share_command(&parts).unwrap();

        assert!(options.public);
        assert_eq!(options.description.as_deref(), Some("Auth debug session"));
    }

    #[test]
    fn parse_share_command_rejects_unknown_flags() {
        let parts = vec!["/share", "--unknown"];
        let err = parse_share_command(&parts).unwrap_err();
        assert!(err.contains("Unknown option"));
    }

    #[test]
    fn render_session_markdown_includes_metadata_and_messages() {
        let messages = vec![
            ChatMessage::new("user", "Fix auth bug"),
            ChatMessage::new("assistant", "I'll help"),
        ];

        let markdown = render_session_markdown(&messages, Some("sess-123"), Some("anthropic/test"));
        assert!(markdown.contains("# Uira Session"));
        assert!(markdown.contains("Session ID: `sess-123`"));
        assert!(markdown.contains("Model: `anthropic/test`"));
        assert!(markdown.contains("### 1. User"));
        assert!(markdown.contains("### 2. Assistant"));
    }
}
