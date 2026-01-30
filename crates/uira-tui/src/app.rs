//! Main TUI application

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use futures::StreamExt;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Terminal,
};
use std::io::Stdout;
use std::sync::Arc;
use tokio::sync::mpsc;
use uira_agent::{Agent, AgentConfig, ApprovalReceiver};
use uira_protocol::{AgentState, Item, ThreadEvent};
use uira_providers::ModelClient;

use crate::views::{ApprovalOverlay, ApprovalRequest, ModelSelector, MODEL_GROUPS};
use crate::widgets::ChatMessage;
use crate::AppEvent;

/// Maximum size for the streaming buffer (1MB)
const MAX_STREAMING_BUFFER_SIZE: usize = 1024 * 1024;

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
    agent_input_tx: Option<mpsc::Sender<String>>,
    current_model: Option<String>,
}

impl App {
    pub fn new() -> Self {
        let (event_tx, event_rx) = mpsc::channel(100);
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
            approval_overlay: ApprovalOverlay::new(),
            model_selector: ModelSelector::new(),
            agent_input_tx: None,
            current_model: None,
        }
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

    /// Run with an agent for interactive mode
    pub async fn run_with_agent(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
        config: AgentConfig,
        client: Arc<dyn ModelClient>,
    ) -> std::io::Result<()> {
        // Create agent with event streaming and interactive channels
        let (agent, event_stream) = Agent::new(config, client).with_event_stream();
        let (mut agent, input_tx, approval_rx) = agent.with_interactive();

        // Store input sender for sending user prompts
        self.agent_input_tx = Some(input_tx);

        // Spawn event handler - forwards agent events to app
        let event_tx = self.event_tx.clone();
        tokio::spawn(async move {
            let mut stream = event_stream;
            while let Some(event) = stream.next().await {
                let _ = event_tx.send(AppEvent::Agent(event)).await;
            }
        });

        // Spawn approval handler - forwards approval requests to app with oneshot channels
        spawn_approval_handler(approval_rx, self.event_tx.clone());

        // Spawn agent's interactive loop
        tokio::spawn(async move {
            if let Err(e) = agent.run_interactive().await {
                tracing::error!("Agent error: {}", e);
            }
        });

        self.run(terminal).await
    }

    fn render(&mut self, frame: &mut ratatui::Frame) {
        let area = frame.area();

        // Main layout: Chat area, Status bar, Input area
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(3),    // Chat
                Constraint::Length(1), // Status
                Constraint::Length(3), // Input
            ])
            .split(area);

        // Render chat
        self.render_chat(frame, chunks[0]);

        // Render status bar
        self.render_status(frame, chunks[1]);

        // Render input
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

    fn render_chat(&mut self, frame: &mut ratatui::Frame, area: Rect) {
        let block = Block::default()
            .title(" Uira ")
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::Cyan));

        let inner_width = area.width.saturating_sub(2) as usize;

        let mut items: Vec<ListItem> = self
            .messages
            .iter()
            .map(|msg| {
                let (prefix, style) = if msg.role == "thinking" {
                    (
                        "thinking: ",
                        Style::default()
                            .fg(Color::Gray)
                            .add_modifier(Modifier::ITALIC),
                    )
                } else {
                    let s = match msg.role.as_str() {
                        "user" => Style::default().fg(Color::Green),
                        "assistant" => Style::default().fg(Color::Cyan),
                        "tool" => Style::default().fg(Color::Magenta),
                        "error" => Style::default().fg(Color::Red),
                        "system" => Style::default().fg(Color::Yellow),
                        _ => Style::default(),
                    };
                    ("", s)
                };

                let role_prefix = if prefix.is_empty() {
                    format!("{}: ", msg.role)
                } else {
                    prefix.to_string()
                };

                let lines = wrap_message(&role_prefix, &msg.content, inner_width, style);
                ListItem::new(Text::from(lines))
            })
            .collect();

        if let Some(ref buffer) = self.thinking_buffer {
            if !buffer.is_empty() {
                let style = Style::default()
                    .fg(Color::Gray)
                    .add_modifier(Modifier::ITALIC);
                let lines = wrap_message("> Thinking: ", buffer, inner_width, style);
                items.push(ListItem::new(Text::from(lines)));
            }
        }

        if let Some(ref buffer) = self.streaming_buffer {
            if !buffer.is_empty() {
                let style = Style::default().fg(Color::Cyan);
                let mut lines = wrap_message("assistant: ", buffer, inner_width, style);
                if let Some(last) = lines.last_mut() {
                    last.spans.push(Span::styled(
                        "▌",
                        Style::default()
                            .fg(Color::Yellow)
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
            AgentState::Idle => ("Idle", Color::Gray),
            AgentState::Thinking => ("Thinking...", Color::Yellow),
            AgentState::ExecutingTool => ("Executing tool...", Color::Magenta),
            AgentState::WaitingForApproval => ("Awaiting approval", Color::Red),
            AgentState::WaitingForUser => ("Waiting for input", Color::Blue),
            AgentState::Complete => ("Complete", Color::Green),
            AgentState::Cancelled => ("Cancelled", Color::Red),
            AgentState::Failed => ("Failed", Color::Red),
        };

        let mut spans = vec![
            Span::styled(
                format!(" {} ", state_str.0),
                Style::default().fg(Color::Black).bg(state_str.1),
            ),
            Span::raw(" "),
            Span::styled(&self.status, Style::default().fg(Color::DarkGray)),
        ];

        // Show pending approval count
        let pending = self.approval_overlay.pending_count();
        if pending > 0 {
            spans.push(Span::raw(" | "));
            spans.push(Span::styled(
                format!("{} pending approval(s)", pending),
                Style::default().fg(Color::Yellow),
            ));
        }

        let status = Paragraph::new(Line::from(spans))
            .style(Style::default().bg(Color::DarkGray).fg(Color::White));

        frame.render_widget(status, area);
    }

    fn render_input(&self, frame: &mut ratatui::Frame, area: Rect) {
        let title = if self.approval_overlay.is_active() {
            " Input (approval overlay active) "
        } else {
            " Input (Enter to send, Ctrl+C to quit) "
        };

        let block = Block::default().title(title).borders(Borders::ALL).style(
            if self.input_focused && !self.approval_overlay.is_active() {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::Gray)
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

    fn push_message(&mut self, role: &str, content: String) {
        self.messages.push(ChatMessage {
            role: role.to_string(),
            content,
        });
        self.scroll_to_bottom();
    }

    fn handle_key_event(&mut self, key: KeyEvent) {
        // Model selector takes priority for key handling
        if self.model_selector.is_active() {
            if let Some(selected_model) = self.model_selector.handle_key(key.code) {
                self.current_model = Some(selected_model.clone());
                self.status = format!("Model: {}", selected_model);
                self.messages.push(ChatMessage {
                    role: "system".to_string(),
                    content: format!(
                        "Switched to model: {}\nNote: Model change takes effect on next request.",
                        selected_model
                    ),
                });
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
                    if !self.input.is_empty() {
                        let input = std::mem::take(&mut self.input);
                        self.cursor_pos = 0;
                        self.submit_input(input);
                    }
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

        self.push_message("user", input.clone());

        if let Some(ref tx) = self.agent_input_tx {
            let tx = tx.clone();
            tokio::spawn(async move {
                if tx.send(input).await.is_err() {
                    tracing::warn!("Agent input channel closed");
                }
            });
            self.status = "Processing...".to_string();
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
                    content: "Available commands:\n  /help, /h, /?     - Show this help\n  /exit, /quit, /q  - Exit the application\n  /auth, /status    - Show current status\n  /models           - List available models\n  /model <name>     - Switch to a different model\n  /clear            - Clear chat history".to_string(),
                });
            }
            "/auth" | "/status" => {
                let status_msg = if self.agent_input_tx.is_some() {
                    "Agent connected"
                } else {
                    "No agent connected"
                };
                self.messages.push(ChatMessage {
                    role: "system".to_string(),
                    content: format!("Status: {}\nState: {:?}", status_msg, self.agent_state),
                });
            }
            "/clear" => {
                self.messages.clear();
                self.status = "Chat cleared".to_string();
            }
            "/models" => {
                self.model_selector.open(self.current_model.clone());
            }
            "/model" => {
                if let Some(model_name) = parts.get(1) {
                    let found = MODEL_GROUPS
                        .iter()
                        .any(|group| group.models.contains(model_name));

                    if found {
                        self.current_model = Some((*model_name).to_string());
                        self.status = format!("Model: {}", model_name);
                        self.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: format!(
                                "Switched to model: {}\nNote: Model change takes effect on next request.",
                                model_name
                            ),
                        });
                    } else {
                        self.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: format!(
                                "Unknown model: {}. Use /models to see available options.",
                                model_name
                            ),
                        });
                    }
                } else {
                    let current = self
                        .current_model
                        .as_deref()
                        .unwrap_or("(default from config)");
                    self.messages.push(ChatMessage {
                        role: "system".to_string(),
                        content: format!("Current model: {}\nUsage: /model <name>", current),
                    });
                }
            }
            _ => {
                self.messages.push(ChatMessage {
                    role: "system".to_string(),
                    content: format!(
                        "Unknown command: {}. Type /help for available commands.",
                        command
                    ),
                });
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
            AppEvent::Redraw => {
                // Force redraw (handled automatically)
            }
            AppEvent::Error(msg) => {
                self.status = format!("Error: {}", msg);
                self.push_message("system", format!("Error: {}", msg));
            }
        }
    }

    fn handle_agent_event(&mut self, event: ThreadEvent) {
        match event {
            ThreadEvent::ThreadStarted { .. } => {
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
                Item::ToolCall { name, .. } => {
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
                    output, is_error, ..
                } => {
                    let role = if is_error { "error" } else { "tool" };
                    self.push_message(role, output);
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
            // Catch-all for unknown variants (due to #[non_exhaustive])
            _ => {
                tracing::debug!("Unknown ThreadEvent variant");
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
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
