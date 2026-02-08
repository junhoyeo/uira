//! Main TUI application

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use futures::StreamExt;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Terminal,
};
use std::collections::HashMap;
use std::io::{Stdout, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::process::Command as TokioCommand;
use tokio::sync::mpsc;
use uira_agent::{Agent, AgentCommand, AgentConfig, ApprovalReceiver, BranchInfo, CommandSender};
use uira_protocol::Provider;
use uira_protocol::{
    AgentState, ContentBlock, ImageSource, Item, Message, MessageContent, Role, ThreadEvent,
    TodoItem, TodoPriority, TodoStatus,
};
use uira_providers::{
    AnthropicClient, GeminiClient, ModelClient, OllamaClient, OpenAIClient, OpenCodeClient,
    ProviderConfig, SecretString,
};

use crate::views::{ApprovalOverlay, ApprovalRequest, ModelSelector, MODEL_GROUPS};
use crate::widgets::ChatMessage;
use crate::{AppEvent, Theme, ThemeOverrides};

/// Maximum size for the streaming buffer (1MB)
const MAX_STREAMING_BUFFER_SIZE: usize = 1024 * 1024;
/// Maximum image size for prompt attachments (10MB)
const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;

#[derive(Clone, Debug)]
struct PendingImage {
    label: String,
    media_type: String,
    data: String,
    size_bytes: usize,
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

fn parse_review_target(arguments: &[&str]) -> ReviewTarget {
    if arguments.is_empty() {
        return ReviewTarget::Staged;
    }

    let target = arguments.join(" ");
    if is_commit_reference(&target) {
        ReviewTarget::Revision(target)
    } else {
        ReviewTarget::File(target)
    }
}

fn is_commit_reference(target: &str) -> bool {
    target == "HEAD"
        || target.starts_with("HEAD~")
        || target.starts_with("HEAD^")
        || (target.len() >= 7
            && target.len() <= 40
            && target.chars().all(|ch| ch.is_ascii_hexdigit()))
}

fn run_git_command(args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .map_err(|err| format!("Failed to run `git {}`: {}", args.join(" "), err))?;

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

fn collect_review_content(target: &ReviewTarget) -> Result<String, String> {
    match target {
        ReviewTarget::Staged => run_git_command(&["diff", "--staged", "--no-color"]),
        ReviewTarget::File(path) => {
            let staged = run_git_command(&["diff", "--staged", "--no-color", "--", path])?;
            let unstaged = run_git_command(&["diff", "--no-color", "--", path])?;

            let mut chunks = Vec::new();
            if !staged.is_empty() {
                chunks.push(format!("### Staged changes\n{}", staged));
            }
            if !unstaged.is_empty() {
                chunks.push(format!("### Unstaged changes\n{}", unstaged));
            }

            Ok(chunks.join("\n\n"))
        }
        ReviewTarget::Revision(revision) => {
            run_git_command(&["show", "--no-color", "--patch", revision])
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

fn wrap_message(prefix: &str, content: &str, max_width: usize, style: Style) -> Vec<Line<'static>> {
    let prefix_len = prefix.chars().count();
    let content_width = max_width.saturating_sub(prefix_len);

    if content_width == 0 {
        return vec![Line::from(Span::styled(prefix.to_string(), style))];
    }

    let mut lines = Vec::new();
    let mut first = true;

    for paragraph in content.split('\n') {
        let chars: Vec<char> = paragraph.chars().collect();
        if chars.is_empty() {
            let line_prefix = if first { prefix } else { "" };
            lines.push(Line::from(Span::styled(line_prefix.to_string(), style)));
            first = false;
            continue;
        }

        let mut i = 0;
        while i < chars.len() {
            let width = if first { content_width } else { max_width };
            let end = (i + width).min(chars.len());
            let chunk: String = chars[i..end].iter().collect();

            let line = if first {
                Line::from(vec![
                    Span::styled(prefix.to_string(), style.add_modifier(Modifier::BOLD)),
                    Span::styled(chunk, style),
                ])
            } else {
                Line::from(Span::styled(chunk, style))
            };

            lines.push(line);
            first = false;
            i = end;
        }
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(prefix.to_string(), style)));
    }

    lines
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
        let short_id = branch.session_id.get(..8).unwrap_or(&branch.session_id);

        lines.push(format!(
            "{} {} -> {}{}",
            marker, branch.name, short_id, parent
        ));
    }

    lines.join("\n")
}

fn truncate_chars(input: &str, max_chars: usize) -> String {
    let mut chars = input.chars();
    let mut output: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        output.push_str("...");
    }
    output
}

fn summarize_tool_output(output: &str) -> String {
    let total_lines = output.lines().count();
    if total_lines == 0 {
        return "no output".to_string();
    }

    let first_non_empty = output
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(str::trim)
        .unwrap_or("(empty lines)");

    let preview = truncate_chars(first_non_empty, 80);
    if total_lines == 1 {
        preview
    } else {
        format!(
            "{} (+{} more lines)",
            preview,
            total_lines.saturating_sub(1)
        )
    }
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

pub struct App {
    should_quit: bool,
    event_tx: mpsc::Sender<AppEvent>,
    event_rx: mpsc::Receiver<AppEvent>,
    messages: Vec<ChatMessage>,
    input: String,
    cursor_pos: usize,
    agent_state: AgentState,
    status: String,
    list_state: ListState,
    input_focused: bool,
    streaming_buffer: Option<String>,
    thinking_buffer: Option<String>,
    approval_overlay: ApprovalOverlay,
    model_selector: ModelSelector,
    agent_input_tx: Option<mpsc::Sender<Message>>,
    agent_command_tx: Option<CommandSender>,
    current_model: Option<String>,
    session_id: Option<String>,
    current_branch: String,
    todos: Vec<TodoItem>,
    show_todo_sidebar: bool,
    todo_list_state: ListState,
    theme: Theme,
    theme_overrides: ThemeOverrides,
    tool_call_names: HashMap<String, String>,
    pending_images: Vec<PendingImage>,
}

impl App {
    pub fn new() -> Self {
        let (event_tx, event_rx) = mpsc::channel(100);
        let theme = Theme::default();
        let mut approval_overlay = ApprovalOverlay::new();
        approval_overlay.set_theme(theme.clone());
        let mut model_selector = ModelSelector::new();
        model_selector.set_theme(theme.clone());
        Self {
            should_quit: false,
            event_tx,
            event_rx,
            messages: Vec::new(),
            input: String::new(),
            cursor_pos: 0,
            agent_state: AgentState::Idle,
            status: "Ready".to_string(),
            list_state: ListState::default(),
            input_focused: true,
            streaming_buffer: None,
            thinking_buffer: None,
            approval_overlay,
            model_selector,
            agent_input_tx: None,
            agent_command_tx: None,
            current_model: None,
            session_id: None,
            current_branch: "main".to_string(),
            todos: Vec::new(),
            show_todo_sidebar: true,
            todo_list_state: ListState::default(),
            theme,
            theme_overrides: ThemeOverrides::default(),
            tool_call_names: HashMap::new(),
            pending_images: Vec::new(),
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
        Ok(())
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
                if let Event::Key(key) = event::read()? {
                    self.handle_key_event(key);
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
                let _ = event_tx.send(AppEvent::Agent(event)).await;
            }
        });

        spawn_approval_handler(approval_rx, self.event_tx.clone());

        tokio::spawn(async move {
            if let Err(e) = agent.run_interactive().await {
                tracing::error!("Agent error: {}", e);
            }
        });

        self.run(terminal).await
    }

    fn render(&mut self, frame: &mut ratatui::Frame) {
        let area = frame.area();

        let main_area = if !self.todos.is_empty() {
            let h_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(75), Constraint::Percentage(25)])
                .split(area);
            self.render_todo_sidebar(frame, h_chunks[1]);
            h_chunks[0]
        } else {
            area
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(3),
                Constraint::Length(1),
                Constraint::Length(3),
            ])
            .split(main_area);

        self.render_chat(frame, chunks[0]);
        self.render_status(frame, chunks[1]);
        self.render_input(frame, chunks[2]);

        // Render approval overlay on top if active
        if self.approval_overlay.is_active() {
            self.approval_overlay.render(frame, area);
        }

        // Render model selector overlay on top
        if self.model_selector.is_active() {
            self.model_selector.render(frame, area);
        }
    }

    fn message_style(&self, role: &str) -> Style {
        match role {
            "user" => Style::default().fg(self.theme.accent),
            "assistant" => Style::default().fg(self.theme.fg),
            "tool" => Style::default().fg(self.theme.accent),
            "error" => Style::default().fg(self.theme.error),
            "system" => Style::default().fg(self.theme.warning),
            _ => Style::default().fg(self.theme.fg),
        }
    }

    fn render_chat(&mut self, frame: &mut ratatui::Frame, area: Rect) {
        let block = Block::default()
            .title(" Uira ")
            .borders(Borders::ALL)
            .style(Style::default().fg(self.theme.borders).bg(self.theme.bg));

        let inner_width = area.width.saturating_sub(2) as usize;

        let mut items: Vec<ListItem> = self
            .messages
            .iter()
            .map(|msg| {
                let (prefix, style) = if msg.role == "thinking" {
                    (
                        "thinking: ",
                        Style::default()
                            .fg(self.theme.borders)
                            .add_modifier(Modifier::ITALIC),
                    )
                } else {
                    let s = self.message_style(msg.role.as_str());
                    ("", s)
                };

                let role_prefix = if prefix.is_empty() {
                    format!("{}: ", msg.role)
                } else {
                    prefix.to_string()
                };

                if let Some(tool_output) = &msg.tool_output {
                    let body = if tool_output.collapsed {
                        format!(
                            "▶ {}: {} [Tab/Enter to expand]",
                            tool_output.tool_name, tool_output.summary
                        )
                    } else {
                        format!("▼ {}:\n{}", tool_output.tool_name, msg.content)
                    };

                    let lines = wrap_message(&role_prefix, &body, inner_width, style);
                    return ListItem::new(Text::from(lines));
                }

                let lines = wrap_message(&role_prefix, &msg.content, inner_width, style);
                ListItem::new(Text::from(lines))
            })
            .collect();

        if let Some(ref buffer) = self.thinking_buffer {
            if !buffer.is_empty() {
                let style = Style::default()
                    .fg(self.theme.borders)
                    .add_modifier(Modifier::ITALIC);
                let lines = wrap_message("> Thinking: ", buffer, inner_width, style);
                items.push(ListItem::new(Text::from(lines)));
            }
        }

        if let Some(ref buffer) = self.streaming_buffer {
            if !buffer.is_empty() {
                let style = self.message_style("assistant");
                let mut lines = wrap_message("assistant: ", buffer, inner_width, style);
                if let Some(last) = lines.last_mut() {
                    last.spans.push(Span::styled(
                        "▌",
                        Style::default()
                            .fg(self.theme.warning)
                            .add_modifier(Modifier::SLOW_BLINK),
                    ));
                }
                items.push(ListItem::new(Text::from(lines)));
            }
        }

        let list = List::new(items)
            .block(block)
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        frame.render_stateful_widget(list, area, &mut self.list_state);
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
            Span::styled(
                format!("branch: {}", self.current_branch),
                Style::default().fg(self.theme.accent),
            ),
        ];

        // Show pending approval count
        let pending = self.approval_overlay.pending_count();
        if pending > 0 {
            spans.push(Span::raw(" | "));
            spans.push(Span::styled(
                format!("{} pending approval(s)", pending),
                Style::default().fg(self.theme.warning),
            ));
        }

        let status = Paragraph::new(Line::from(spans))
            .style(Style::default().bg(self.theme.bg).fg(self.theme.fg));

        frame.render_widget(status, area);
    }

    fn render_input(&self, frame: &mut ratatui::Frame, area: Rect) {
        let pending_label = if self.pending_images.is_empty() {
            String::new()
        } else {
            format!(" | {} image(s) attached", self.pending_images.len())
        };

        let title = if self.approval_overlay.is_active() {
            format!(" Input (approval overlay active{}) ", pending_label)
        } else {
            format!(
                " Input (Enter to send, Ctrl+G external editor, Ctrl+C to quit{}) ",
                pending_label
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

    fn render_todo_sidebar(&self, frame: &mut ratatui::Frame, area: Rect) {
        let completed = self
            .todos
            .iter()
            .filter(|t| t.status == TodoStatus::Completed)
            .count();
        let total = self.todos.len();
        let title = format!(" Todos ({}/{}) ", completed, total);

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .style(Style::default().fg(self.theme.borders));

        let inner = block.inner(area);
        let max_width = inner.width.saturating_sub(1) as usize;

        let items: Vec<ListItem> = self
            .todos
            .iter()
            .map(|todo| {
                let (indicator, status_color) = match todo.status {
                    TodoStatus::Completed => ("✓", self.theme.success),
                    TodoStatus::InProgress => ("•", self.theme.warning),
                    TodoStatus::Cancelled => ("✗", self.theme.borders),
                    TodoStatus::Pending => (" ", self.theme.borders),
                };

                // Priority marker and color override
                let (priority_marker, color) = match (todo.status, todo.priority) {
                    (TodoStatus::Completed, _) => ("", status_color),
                    (TodoStatus::Cancelled, _) => ("", status_color),
                    (_, TodoPriority::High) => ("⚡ ", self.theme.error),
                    (_, TodoPriority::Medium) => ("• ", self.theme.warning),
                    (_, TodoPriority::Low) => ("", status_color),
                };

                let prefix = format!("[{}] {}", indicator, priority_marker);
                let prefix_chars = prefix.chars().count();
                let content_chars = todo.content.chars().count();
                let content =
                    if prefix_chars + content_chars > max_width && max_width > prefix_chars + 3 {
                        let truncate_len = max_width.saturating_sub(prefix_chars + 3);
                        let truncated: String = todo.content.chars().take(truncate_len).collect();
                        format!("{}...", truncated)
                    } else {
                        todo.content.clone()
                    };

                ListItem::new(Line::from(vec![
                    Span::styled(prefix, Style::default().fg(color)),
                    Span::styled(content, Style::default().fg(color)),
                ]))
            })
            .collect();

        let list = List::new(items).block(block);
        frame.render_widget(list, area);
    }

    fn scroll_up(&mut self) {
        let total = self.total_items();
        let i = match self.list_state.selected() {
            Some(i) => i.saturating_sub(1),
            None if total > 0 => total - 1,
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn scroll_down(&mut self) {
        let total = self.total_items();
        if total == 0 {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => (i + 1).min(total - 1),
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn scroll_to_bottom(&mut self) {
        let total = self.total_items();
        if total > 0 {
            self.list_state.select(Some(total - 1));
        }
    }

    fn total_items(&self) -> usize {
        let mut count = self.messages.len();
        if self.thinking_buffer.as_ref().is_some_and(|b| !b.is_empty()) {
            count += 1;
        }
        if self
            .streaming_buffer
            .as_ref()
            .is_some_and(|b| !b.is_empty())
        {
            count += 1;
        }
        count
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

    fn selected_message_index(&self) -> Option<usize> {
        self.list_state
            .selected()
            .filter(|&index| index < self.messages.len())
    }

    fn toggle_selected_tool_output(&mut self) -> bool {
        let Some(index) = self.selected_message_index() else {
            return false;
        };

        let Some(tool_output) = self
            .messages
            .get_mut(index)
            .and_then(|msg| msg.tool_output.as_mut())
        else {
            return false;
        };

        tool_output.collapsed = !tool_output.collapsed;
        let action = if tool_output.collapsed {
            "Collapsed"
        } else {
            "Expanded"
        };
        self.status = format!("{} {} output", action, tool_output.tool_name);
        true
    }

    fn set_all_tool_outputs_collapsed(&mut self, collapsed: bool) -> usize {
        let mut updated = 0;
        for message in &mut self.messages {
            if let Some(tool_output) = message.tool_output.as_mut() {
                if tool_output.collapsed != collapsed {
                    tool_output.collapsed = collapsed;
                    updated += 1;
                }
            }
        }
        updated
    }

    fn collapse_all_tool_outputs(&mut self) {
        let updated = self.set_all_tool_outputs_collapsed(true);
        if updated == 0 {
            self.status = "No expanded tool output to collapse".to_string();
        } else {
            self.status = format!("Collapsed {} tool output item(s)", updated);
        }
    }

    fn expand_all_tool_outputs(&mut self) {
        let updated = self.set_all_tool_outputs_collapsed(false);
        if updated == 0 {
            self.status = "No collapsed tool output to expand".to_string();
        } else {
            self.status = format!("Expanded {} tool output item(s)", updated);
        }
    }

    fn push_message(&mut self, role: &str, content: String) {
        self.messages.push(ChatMessage::new(role, content));
        self.scroll_to_bottom();
    }

    fn push_tool_message(
        &mut self,
        role: &str,
        tool_name: String,
        content: String,
        is_collapsed: bool,
    ) {
        let summary = summarize_tool_output(&content);
        self.messages.push(ChatMessage::tool(
            role,
            content,
            tool_name,
            summary,
            is_collapsed,
        ));
        self.scroll_to_bottom();
    }

    fn handle_key_event(&mut self, key: KeyEvent) {
        if self.model_selector.is_active() {
            if let Some(selected_model) = self.model_selector.handle_key(key.code) {
                self.switch_model(&selected_model);
            }
            return;
        }

        // Approval overlay takes priority for key handling
        if self.approval_overlay.handle_key(key.code) {
            if !self.approval_overlay.is_active() {
                self.agent_state = AgentState::Thinking;
            }
            return;
        }

        // Global shortcuts
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') | KeyCode::Char('q') => {
                    self.should_quit = true;
                    return;
                }
                KeyCode::Char('l') => {
                    // Clear screen
                    self.messages.clear();
                    return;
                }
                KeyCode::Char('o') | KeyCode::Char('O') => {
                    if key.modifiers.contains(KeyModifiers::SHIFT) {
                        self.expand_all_tool_outputs();
                    } else {
                        self.collapse_all_tool_outputs();
                    }
                    return;
                }
                _ => {}
            }
        }

        if key.modifiers.is_empty() && self.input.is_empty() {
            match key.code {
                KeyCode::Char(c) if c.eq_ignore_ascii_case(&'t') => {
                    self.toggle_todo_sidebar();
                    return;
                }
                KeyCode::Char(c) if c.eq_ignore_ascii_case(&'d') => {
                    self.mark_selected_todo_done();
                    return;
                }
                _ => {}
            }
        }

        // Input handling (cursor_pos is char index, not byte index for UTF-8 safety)
        if self.input_focused {
            let char_count = self.input.chars().count();
            match key.code {
                KeyCode::Char(c) => {
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
                    self.cursor_pos = 0;
                }
                KeyCode::End => {
                    self.cursor_pos = char_count;
                }
                KeyCode::Enter => {
                    if self.input.is_empty()
                        && self.pending_images.is_empty()
                        && self.toggle_selected_tool_output()
                    {
                        return;
                    }

                    if !self.input.trim().is_empty() || !self.pending_images.is_empty() {
                        let input = std::mem::take(&mut self.input);
                        self.cursor_pos = 0;
                        self.submit_input(input);
                    }
                }
                KeyCode::Tab => {
                    let _ = self.toggle_selected_tool_output();
                }
                KeyCode::Esc => {
                    self.should_quit = true;
                }
                KeyCode::Up => {
                    self.scroll_up();
                }
                KeyCode::Down => {
                    self.scroll_down();
                }
                _ => {}
            }
        }
    }

    fn submit_input(&mut self, input: String) {
        if input.starts_with('/') {
            self.handle_slash_command(&input);
            return;
        }

        let pending_images = std::mem::take(&mut self.pending_images);
        let has_images = !pending_images.is_empty();

        let display_message = Self::format_user_display(&input, &pending_images);
        self.push_message("user", display_message);

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
            self.agent_state = AgentState::Thinking;
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

    fn resolve_image_path(raw_path: &str) -> Result<PathBuf, String> {
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
            std::env::current_dir()
                .map_err(|e| format!("failed to read current directory: {}", e))?
                .join(expanded)
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
        let path = Self::resolve_image_path(raw_path)?;
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

    fn run_review_command(&mut self, args: &[&str], raw_command: &str) {
        let target = parse_review_target(args);
        let target_description = target.description();

        let content = match collect_review_content(&target) {
            Ok(content) => content,
            Err(err) => {
                self.push_message("error", format!("Failed to gather review input: {}", err));
                return;
            }
        };

        if content.is_empty() {
            self.push_message(
                "system",
                format!("No diff found for {}.", target_description),
            );
            return;
        }

        self.push_message("user", raw_command.to_string());
        self.push_message(
            "system",
            format!(
                "Starting review for {}. Output will be grouped into issues, suggestions, and praise.",
                target_description
            ),
        );

        let review_prompt = build_review_prompt(&target, &content);
        if let Some(ref tx) = self.agent_input_tx {
            let tx = tx.clone();
            tokio::spawn(async move {
                if tx.send(Message::user(review_prompt)).await.is_err() {
                    tracing::warn!("Agent input channel closed");
                }
            });
            self.status = "Processing review...".to_string();
            self.agent_state = AgentState::Thinking;
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
                self.messages.push(ChatMessage {
                    role: "system".to_string(),
                    content: "Available commands:\n  /help, /h, /?       - Show this help\n  /exit, /quit, /q    - Exit the application\n  /auth, /status      - Show current status\n  /models             - List available models\n  /model <name>       - Switch to a different model\n  /theme              - List available themes\n  /theme <name>       - Switch theme\n  /image <path>       - Attach image for next prompt\n  /screenshot         - Capture and attach screenshot\n  /fork [name|count]  - Fork session (optional branch name or keep first N messages)\n  /switch <branch>    - Switch to branch\n  /branches           - List branches\n  /tree               - Show branch tree\n  /review             - Review staged changes\n  /review <file>      - Review changes for a specific file\n  /review HEAD~1      - Review a specific commit\n  /share              - Share session to GitHub Gist\n  /clear              - Clear chat history and pending attachments"
                        .to_string(),
                    tool_output: None,
                });
            }
            "/auth" | "/status" => {
                let status_msg = if self.agent_input_tx.is_some() {
                    "Agent connected"
                } else {
                    "No agent connected"
                };
                self.push_message(
                    "system",
                    format!("Status: {}\nState: {:?}", status_msg, self.agent_state),
                );
            }
            "/clear" => {
                self.messages.clear();
                self.pending_images.clear();
                self.status = "Chat cleared".to_string();
            }
            "/image" => {
                let path_arg = input[command.len()..].trim();
                if path_arg.is_empty() {
                    self.messages.push(ChatMessage {
                        role: "system".to_string(),
                        content: "Usage: /image <path>".to_string(),
                        tool_output: None,
                    });
                    return;
                }

                let parsed_path = Self::strip_optional_quotes(path_arg);
                match self.attach_image_from_path(parsed_path) {
                    Ok(image) => {
                        let image_label = image.label.clone();
                        let image_size = Self::format_size(image.size_bytes);
                        self.pending_images.push(image);
                        self.status = format!("Attached {} image(s)", self.pending_images.len());
                        self.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: format!(
                                "Attached image: {} ({})\nIt will be sent with your next prompt.",
                                image_label, image_size
                            ),
                            tool_output: None,
                        });
                    }
                    Err(error) => {
                        self.messages.push(ChatMessage {
                            role: "error".to_string(),
                            content: format!("Failed to attach image: {}", error),
                            tool_output: None,
                        });
                    }
                }
            }
            "/screenshot" => match self.capture_and_attach_screenshot() {
                Ok(image) => {
                    let image_label = image.label.clone();
                    let image_size = Self::format_size(image.size_bytes);
                    self.pending_images.push(image);
                    self.status = format!("Attached {} image(s)", self.pending_images.len());
                    self.messages.push(ChatMessage {
                        role: "system".to_string(),
                        content: format!(
                            "Captured screenshot: {} ({})\nIt will be sent with your next prompt.",
                            image_label, image_size
                        ),
                        tool_output: None,
                    });
                }
                Err(error) => {
                    self.messages.push(ChatMessage {
                        role: "error".to_string(),
                        content: format!("Failed to capture screenshot: {}", error),
                        tool_output: None,
                    });
                }
            },
            "/review" => {
                let args: Vec<&str> = parts.iter().skip(1).copied().collect();
                self.run_review_command(&args, input);
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
                    self.messages.push(ChatMessage {
                        role: "system".to_string(),
                        content: "Usage: /switch <branch>".to_string(),
                        tool_output: None,
                    });
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
                    self.messages.push(ChatMessage {
                        role: "error".to_string(),
                        content: format!(
                            "{}\nUsage: /share [--public] [--description <text>]",
                            err
                        ),
                        tool_output: None,
                    });
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
                        self.push_message(
                            "system",
                            format!(
                                "Unknown model: {}. Use /models to see available options.",
                                model_name
                            ),
                        );
                    }
                } else {
                    let current = self
                        .current_model
                        .as_deref()
                        .unwrap_or("(default from config)");
                    self.push_message(
                        "system",
                        format!("Current model: {}\nUsage: /model <name>", current),
                    );
                }
            }
            "/theme" => {
                if let Some(theme_name) = parts.get(1) {
                    match self.set_theme_by_name(theme_name) {
                        Ok(()) => {
                            self.status = format!("Theme changed to {}", self.theme.name);
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: format!("Theme set to {}", self.theme.name),
                                tool_output: None,
                            });
                        }
                        Err(err) => {
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: err,
                                tool_output: None,
                            });
                        }
                    }
                } else {
                    self.messages.push(ChatMessage {
                        role: "system".to_string(),
                        content: format!(
                            "Current theme: {}\nAvailable themes: {}\nUsage: /theme <name>",
                            self.theme.name,
                            Theme::available_names().join(", ")
                        ),
                        tool_output: None,
                    });
                }
            }
            _ => {
                self.push_message(
                    "system",
                    format!(
                        "Unknown command: {}. Type /help for available commands.",
                        command
                    ),
                );
            }
        }
    }

    fn handle_app_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Quit => self.should_quit = true,
            AppEvent::Agent(thread_event) => self.handle_agent_event(thread_event),
            AppEvent::UserInput(_) => {
                // Already handled in submit_input
            }
            AppEvent::ApprovalRequest(request) => {
                // Enqueue approval and update state
                self.approval_overlay.enqueue(request);
                self.agent_state = AgentState::WaitingForApproval;
            }
            AppEvent::TodoUpdated(todos) => {
                self.todos = todos;
            }
            AppEvent::Info(message) => {
                self.status = message.clone();
                self.push_message("system", message);
            }
            AppEvent::BranchChanged(branch_name) => {
                self.current_branch = branch_name;
            }
            AppEvent::Redraw => {}
            AppEvent::Error(msg) => {
                self.status = format!("Error: {}", msg);
                self.push_message("system", format!("Error: {}", msg));
            }
        }
    }

    fn handle_agent_event(&mut self, event: ThreadEvent) {
        match event {
            ThreadEvent::ThreadStarted { thread_id } => {
                self.session_id = Some(thread_id);
                self.agent_state = AgentState::Thinking;
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
            ThreadEvent::ContentDelta { delta } => {
                if let Some(ref mut buffer) = self.streaming_buffer {
                    if buffer.len() + delta.len() <= MAX_STREAMING_BUFFER_SIZE {
                        buffer.push_str(&delta);
                    }
                } else {
                    self.streaming_buffer = Some(delta);
                }
                self.scroll_to_bottom();
            }
            ThreadEvent::ThinkingDelta { thinking } => {
                if let Some(ref mut buffer) = self.thinking_buffer {
                    if buffer.len() + thinking.len() <= MAX_STREAMING_BUFFER_SIZE {
                        buffer.push_str(&thinking);
                    }
                } else {
                    self.thinking_buffer = Some(thinking);
                }
                self.scroll_to_bottom();
            }
            ThreadEvent::ItemStarted { item } => match item {
                Item::ToolCall { id, name, .. } => {
                    self.tool_call_names.insert(id, name.clone());
                    self.agent_state = AgentState::ExecutingTool;
                    self.status = format!("Executing: {}", name);
                }
                Item::AgentMessage { content } => {
                    // Start streaming
                    self.streaming_buffer = Some(content);
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
                    self.agent_state = AgentState::WaitingForApproval;
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
                    let tool_name = self
                        .tool_call_names
                        .remove(&tool_call_id)
                        .unwrap_or_else(|| "tool".to_string());
                    let role = if is_error { "error" } else { "tool" };
                    self.push_tool_message(role, tool_name, output, false);
                    self.agent_state = AgentState::Thinking;
                }
                Item::AgentMessage { content } => {
                    self.streaming_buffer = None;
                    if let Some(thinking) = self.thinking_buffer.take() {
                        if !thinking.is_empty() {
                            self.push_message("thinking", thinking);
                        }
                    }
                    self.push_message("assistant", content);
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
                        self.agent_state = AgentState::ExecutingTool;
                    } else {
                        self.agent_state = AgentState::Thinking;
                    }
                }
                _ => {}
            },
            ThreadEvent::ThreadCompleted { usage } => {
                self.agent_state = AgentState::Complete;
                self.status = format!(
                    "Complete (total: {} in / {} out tokens)",
                    usage.input_tokens, usage.output_tokens
                );

                if let Some(thinking) = self.thinking_buffer.take() {
                    if !thinking.is_empty() {
                        self.push_message("thinking", thinking);
                    }
                }

                if let Some(buffer) = self.streaming_buffer.take() {
                    if !buffer.is_empty() {
                        self.push_message("assistant", buffer);
                    }
                }
            }
            ThreadEvent::ThreadCancelled => {
                self.agent_state = AgentState::Cancelled;
                self.status = "Cancelled".to_string();
            }
            ThreadEvent::Error { message, .. } => {
                self.agent_state = AgentState::Failed;
                self.status = format!("Error: {}", message);
                self.push_message("error", message);
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
                self.push_message(
                    "system",
                    format!(
                        "{} Goal '{}': {:.1}% (target: {:.1}%)",
                        icon, goal, score, target
                    ),
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
                self.push_message(
                    "system",
                    format!("Ralph continuing: {} (confidence: {}%)", reason, confidence),
                );
            }
            ThreadEvent::RalphCircuitBreak { reason, iteration } => {
                self.push_message(
                    "system",
                    format!("Ralph stopped at iteration {}: {}", iteration, reason),
                );
                self.agent_state = AgentState::Complete;
            }
            ThreadEvent::BackgroundTaskSpawned {
                task_id,
                description,
                ..
            } => {
                self.push_message(
                    "system",
                    format!("Background task started: {} ({})", description, task_id),
                );
            }
            ThreadEvent::BackgroundTaskProgress {
                task_id,
                status,
                message,
            } => {
                let msg = message.map(|m| format!(": {}", m)).unwrap_or_default();
                self.push_message(
                    "system",
                    format!("Background task {} - {}{}", task_id, status, msg),
                );
            }
            ThreadEvent::BackgroundTaskCompleted {
                task_id, success, ..
            } => {
                let status = if success { "completed" } else { "failed" };
                self.push_message("system", format!("Background task {}: {}", task_id, status));
            }
            ThreadEvent::ModelSwitched { model, provider } => {
                self.status = format!("Model: {}/{}", provider, model);
                self.push_message("system", format!("Switched to {}/{}", provider, model));
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
        self.push_message(role, content.to_string());
    }

    /// Set the status message
    pub fn set_status(&mut self, status: &str) {
        self.status = status.to_string();
    }

    /// Set the agent state
    pub fn set_agent_state(&mut self, state: AgentState) {
        self.agent_state = state;
    }

    fn switch_model(&mut self, model_str: &str) {
        self.current_model = Some(model_str.to_string());
        self.status = format!("Switching to {}...", model_str);

        match create_client_for_model(model_str) {
            Ok(new_client) => {
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
                    self.push_message(
                        "system",
                        format!(
                            "Model set to: {}\nNote: No agent connected, change will apply when agent starts.",
                            model_str
                        ),
                    );
                }
            }
            Err(e) => {
                self.status = "Model switch failed".to_string();
                self.push_message(
                    "error",
                    format!("Failed to create client for {}: {}", model_str, e),
                );
            }
        }
    }

    fn share_session(&mut self, options: ShareCommandOptions) {
        if self.messages.is_empty() {
            self.messages.push(ChatMessage {
                role: "system".to_string(),
                content: "No messages to share yet. Start a conversation first.".to_string(),
                tool_output: None,
            });
            return;
        }

        let messages = self.messages.clone();
        let session_id = self.session_id.clone();
        let model = self.current_model.clone();
        let event_tx = self.event_tx.clone();
        self.status = "Sharing session to GitHub Gist...".to_string();
        self.push_message(
            "system",
            "Warning: sharing may include sensitive content from system/tool outputs. Review before sharing.\nUse /share --public only when intentional."
                .to_string(),
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
            self.push_message(
                "error",
                "No agent connected. Cannot fork session.".to_string(),
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
                    Ok(Ok(message)) => {
                        let _ = event_tx
                            .send(AppEvent::BranchChanged(branch_name.clone()))
                            .await;
                        let _ = event_tx.send(AppEvent::Info(message)).await;
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
            self.messages.push(ChatMessage {
                role: "error".to_string(),
                content: "No agent connected. Cannot switch branches.".to_string(),
                tool_output: None,
            });
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
            self.messages.push(ChatMessage {
                role: "error".to_string(),
                content: "No agent connected. Cannot list branches.".to_string(),
                tool_output: None,
            });
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
            self.messages.push(ChatMessage {
                role: "error".to_string(),
                content: "No agent connected. Cannot show branch tree.".to_string(),
                tool_output: None,
            });
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
        assert_eq!(parse_review_target(&[]), ReviewTarget::Staged);
    }

    #[test]
    fn parse_review_target_handles_file_path() {
        assert_eq!(
            parse_review_target(&["crates/uira-tui/src/app.rs"]),
            ReviewTarget::File("crates/uira-tui/src/app.rs".to_string())
        );
    }

    #[test]
    fn parse_review_target_handles_revision() {
        assert_eq!(
            parse_review_target(&["HEAD~1"]),
            ReviewTarget::Revision("HEAD~1".to_string())
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
    fn ctrl_g_triggers_external_editor_action() {
        let mut app = App::new();

        let action = app.handle_key_event(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL));

        assert_eq!(action, KeyAction::OpenExternalEditor);
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
            ChatMessage {
                role: "user".to_string(),
                content: "Fix auth bug".to_string(),
                tool_output: None,
            },
            ChatMessage {
                role: "assistant".to_string(),
                content: "I'll help".to_string(),
                tool_output: None,
            },
        ];

        let markdown = render_session_markdown(&messages, Some("sess-123"), Some("anthropic/test"));
        assert!(markdown.contains("# Uira Session"));
        assert!(markdown.contains("Session ID: `sess-123`"));
        assert!(markdown.contains("Model: `anthropic/test`"));
        assert!(markdown.contains("### 1. User"));
        assert!(markdown.contains("### 2. Assistant"));
    }
}
