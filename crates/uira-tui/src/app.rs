//! Main TUI application

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use futures::StreamExt;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Terminal,
};
use std::io::Stdout;
use std::sync::Arc;
use tokio::sync::mpsc;
use uira_agent::{Agent, AgentConfig};
use uira_protocol::{AgentState, Item, ThreadEvent};
use uira_providers::ModelClient;

use crate::widgets::{ChatMessage, ChatWidget};
use crate::AppEvent;

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
        let (agent, event_stream) = Agent::new(config, client).with_event_stream();
        let _agent = Arc::new(tokio::sync::Mutex::new(agent));

        // Spawn event handler
        let event_tx = self.event_tx.clone();
        tokio::spawn(async move {
            let mut stream = event_stream;
            while let Some(event) = stream.next().await {
                let _ = event_tx.send(AppEvent::Agent(event)).await;
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
    }

    fn render_chat(&self, frame: &mut ratatui::Frame, area: Rect) {
        let block = Block::default()
            .title(" Uira ")
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::Cyan));

        let chat = ChatWidget::new(&self.messages).block(block);
        frame.render_widget(chat, area);
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

        let spans = vec![
            Span::styled(
                format!(" {} ", state_str.0),
                Style::default().fg(Color::Black).bg(state_str.1),
            ),
            Span::raw(" "),
            Span::styled(&self.status, Style::default().fg(Color::DarkGray)),
        ];

        let status = Paragraph::new(Line::from(spans))
            .style(Style::default().bg(Color::DarkGray).fg(Color::White));

        frame.render_widget(status, area);
    }

    fn render_input(&self, frame: &mut ratatui::Frame, area: Rect) {
        let block = Block::default()
            .title(" Input (Enter to send, Ctrl+C to quit) ")
            .borders(Borders::ALL)
            .style(if self.input_focused {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::Gray)
            });

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

        // Send input event
        let tx = self.event_tx.clone();
        tokio::spawn(async move {
            let _ = tx.send(AppEvent::UserInput(input)).await;
        });

        self.status = "Processing...".to_string();
    }

    fn handle_app_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Quit => self.should_quit = true,
            AppEvent::Agent(thread_event) => self.handle_agent_event(thread_event),
            AppEvent::UserInput(_) => {
                // Already handled in submit_input
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
            ThreadEvent::ItemStarted { item } => match item {
                Item::ToolCall { name, .. } => {
                    self.agent_state = AgentState::ExecutingTool;
                    self.status = format!("Executing: {}", name);
                }
                Item::AgentMessage { content } => {
                    // Start streaming
                    self.streaming_buffer = Some(content);
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
                _ => {}
            },
            ThreadEvent::ThreadCompleted { usage } => {
                self.agent_state = AgentState::Complete;
                self.status = format!(
                    "Complete (total: {} in / {} out tokens)",
                    usage.input_tokens, usage.output_tokens
                );
            }
            ThreadEvent::Error { message, .. } => {
                self.agent_state = AgentState::Failed;
                self.status = format!("Error: {}", message);
                self.messages.push(ChatMessage {
                    role: "error".to_string(),
                    content: message,
                });
            }
            _ => {}
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
