//! Main TUI application

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use futures::StreamExt;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Terminal,
};
use std::io::Stdout;
use std::sync::Arc;
use tokio::sync::mpsc;
use uira_agent::{Agent, AgentConfig, ApprovalReceiver};
use uira_protocol::{AgentState, Item, ThreadEvent};
use uira_providers::ModelClient;

use crate::views::{ApprovalOverlay, ApprovalRequest};
use crate::widgets::ChatMessage;
use crate::AppEvent;

/// Maximum size for the streaming buffer (1MB)
const MAX_STREAMING_BUFFER_SIZE: usize = 1024 * 1024;

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

/// Main TUI application state
pub struct App {
    /// Whether the app should quit
    should_quit: bool,
    /// Event sender for internal communication
    event_tx: mpsc::Sender<AppEvent>,
    /// Event receiver
    event_rx: mpsc::Receiver<AppEvent>,
    /// Chat messages
    messages: Vec<ChatMessage>,
    /// Input buffer
    input: String,
    /// Input cursor position
    cursor_pos: usize,
    /// Agent state
    agent_state: AgentState,
    /// Current status message
    status: String,
    /// Scroll offset for chat
    scroll: u16,
    /// Is input focused
    input_focused: bool,
    /// Current streaming message buffer
    streaming_buffer: Option<String>,
    /// Approval overlay for tool approvals
    approval_overlay: ApprovalOverlay,
    /// Input sender to agent (for interactive mode)
    agent_input_tx: Option<mpsc::Sender<String>>,
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
            scroll: 0,
            input_focused: true,
            streaming_buffer: None,
            approval_overlay: ApprovalOverlay::new(),
            agent_input_tx: None,
        }
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

    fn render(&self, frame: &mut ratatui::Frame) {
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
    }

    fn render_chat(&self, frame: &mut ratatui::Frame, area: Rect) {
        let block = Block::default()
            .title(" Uira ")
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::Cyan));

        // Build list items from messages
        let mut items: Vec<ListItem> = self
            .messages
            .iter()
            .map(|msg| {
                let style = match msg.role.as_str() {
                    "user" => Style::default().fg(Color::Green),
                    "assistant" => Style::default().fg(Color::Cyan),
                    "tool" => Style::default().fg(Color::Magenta),
                    "error" => Style::default().fg(Color::Red),
                    "system" => Style::default().fg(Color::Yellow),
                    _ => Style::default(),
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("{}: ", msg.role),
                        style.add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(&msg.content, style),
                ]))
            })
            .collect();

        // Render streaming buffer as in-progress message with blinking cursor
        if let Some(ref buffer) = self.streaming_buffer {
            if !buffer.is_empty() {
                let streaming_item = ListItem::new(Line::from(vec![
                    Span::styled(
                        "assistant: ",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(buffer.as_str(), Style::default().fg(Color::Cyan)),
                    Span::styled(
                        "▌",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::SLOW_BLINK),
                    ),
                ]));
                items.push(streaming_item);
            }
        }

        let list = List::new(items).block(block);
        frame.render_widget(list, area);
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

        // Display input with cursor
        let display_input = if self.cursor_pos >= self.input.len() {
            format!("{}_", self.input)
        } else {
            let (before, after) = self.input.split_at(self.cursor_pos);
            format!("{}|{}", before, after)
        };

        let input_paragraph = Paragraph::new(display_input).wrap(Wrap { trim: false });

        frame.render_widget(block, area);
        frame.render_widget(input_paragraph, inner);
    }

    fn handle_key_event(&mut self, key: KeyEvent) {
        // Approval overlay takes priority for key handling
        if self.approval_overlay.handle_key(key.code) {
            // Key was consumed by overlay
            if !self.approval_overlay.is_active() {
                // Overlay finished, update state
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

        // Input handling
        if self.input_focused {
            match key.code {
                KeyCode::Char(c) => {
                    self.input.insert(self.cursor_pos, c);
                    self.cursor_pos += 1;
                }
                KeyCode::Backspace => {
                    if self.cursor_pos > 0 {
                        self.cursor_pos -= 1;
                        self.input.remove(self.cursor_pos);
                    }
                }
                KeyCode::Delete => {
                    if self.cursor_pos < self.input.len() {
                        self.input.remove(self.cursor_pos);
                    }
                }
                KeyCode::Left => {
                    if self.cursor_pos > 0 {
                        self.cursor_pos -= 1;
                    }
                }
                KeyCode::Right => {
                    if self.cursor_pos < self.input.len() {
                        self.cursor_pos += 1;
                    }
                }
                KeyCode::Home => {
                    self.cursor_pos = 0;
                }
                KeyCode::End => {
                    self.cursor_pos = self.input.len();
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
                    // Scroll up
                    self.scroll = self.scroll.saturating_add(1);
                }
                KeyCode::Down => {
                    // Scroll down
                    self.scroll = self.scroll.saturating_sub(1);
                }
                _ => {}
            }
        }
    }

    fn submit_input(&mut self, input: String) {
        // Add user message to chat
        self.messages.push(ChatMessage {
            role: "user".to_string(),
            content: input.clone(),
        });

        // Send input to agent if connected
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
                self.messages.push(ChatMessage {
                    role: "system".to_string(),
                    content: format!("Error: {}", msg),
                });
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
                // Accumulate streaming content with size limit
                if let Some(ref mut buffer) = self.streaming_buffer {
                    // Only append if within size limit
                    if buffer.len() + delta.len() <= MAX_STREAMING_BUFFER_SIZE {
                        buffer.push_str(&delta);
                    }
                    // Silently drop if over limit (could truncate from front as alternative)
                } else {
                    self.streaming_buffer = Some(delta);
                }
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
                    self.messages.push(ChatMessage {
                        role: role.to_string(),
                        content: output,
                    });
                    self.agent_state = AgentState::Thinking;
                }
                Item::AgentMessage { content } => {
                    // Complete message
                    self.streaming_buffer = None;
                    self.messages.push(ChatMessage {
                        role: "assistant".to_string(),
                        content,
                    });
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

                // Flush streaming buffer if present
                if let Some(buffer) = self.streaming_buffer.take() {
                    if !buffer.is_empty() {
                        self.messages.push(ChatMessage {
                            role: "assistant".to_string(),
                            content: buffer,
                        });
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
                self.messages.push(ChatMessage {
                    role: "error".to_string(),
                    content: message,
                });
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
                self.messages.push(ChatMessage {
                    role: "system".to_string(),
                    content: format!(
                        "{} Goal '{}': {:.1}% (target: {:.1}%)",
                        icon, goal, score, target
                    ),
                });
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
                self.messages.push(ChatMessage {
                    role: "system".to_string(),
                    content: format!("Ralph continuing: {} (confidence: {}%)", reason, confidence),
                });
            }
            ThreadEvent::RalphCircuitBreak { reason, iteration } => {
                self.messages.push(ChatMessage {
                    role: "system".to_string(),
                    content: format!("Ralph stopped at iteration {}: {}", iteration, reason),
                });
                self.agent_state = AgentState::Complete;
            }
            // Background Task Events
            ThreadEvent::BackgroundTaskSpawned {
                task_id,
                description,
                ..
            } => {
                self.messages.push(ChatMessage {
                    role: "system".to_string(),
                    content: format!("Background task started: {} ({})", description, task_id),
                });
            }
            ThreadEvent::BackgroundTaskProgress {
                task_id,
                status,
                message,
            } => {
                let msg = message.map(|m| format!(": {}", m)).unwrap_or_default();
                self.messages.push(ChatMessage {
                    role: "system".to_string(),
                    content: format!("Background task {} - {}{}", task_id, status, msg),
                });
            }
            ThreadEvent::BackgroundTaskCompleted {
                task_id, success, ..
            } => {
                let status = if success { "completed" } else { "failed" };
                self.messages.push(ChatMessage {
                    role: "system".to_string(),
                    content: format!("Background task {}: {}", task_id, status),
                });
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

    /// Add a message to the chat
    pub fn add_message(&mut self, role: &str, content: &str) {
        self.messages.push(ChatMessage {
            role: role.to_string(),
            content: content.to_string(),
        });
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
