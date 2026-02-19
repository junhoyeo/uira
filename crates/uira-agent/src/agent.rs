//! Main agent implementation

use crate::telemetry::{SessionSpan, TurnSpan};
use chrono::{DateTime, Utc};
use futures::StreamExt;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::timeout;
use uira_core::{
    AgentError, AgentState, ApprovalRequirement, ContentBlock, ExecutionResult, Item, Message,
    MessageContent, Role, SessionId, ThreadEvent, ToolCall,
};
use uira_core::{Event, EventBus, SessionEndReason};
use uira_orchestration::hooks::hooks::keyword_detector::KeywordDetectorHook;
use uira_providers::ModelClient;

use crate::{
    approval::{approval_channel, ApprovalReceiver, ApprovalSender},
    events::{EventSender, EventStream},
    session::{extract_messages, get_last_turn, get_total_usage, SessionMetaLine, SessionRecorder},
    streaming::StreamController,
    AgentCommand, AgentConfig, AgentControl, AgentLoopError, BranchInfo, CommandReceiver,
    CommandSender, ForkResult, Session, SwitchBranchResult,
};

/// Timeout for approval requests (5 minutes)
const APPROVAL_TIMEOUT: Duration = Duration::from_secs(300);

fn get_git_branch() -> String {
    Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".to_string())
}

struct BranchState {
    parent: Option<String>,
    session: Session,
}

pub struct Agent {
    session: Session,
    branches: HashMap<String, BranchState>,
    current_branch: String,
    current_branch_parent: Option<String>,
    control: AgentControl,
    state: AgentState,
    event_sender: Option<EventSender>,
    event_bus: Option<Arc<dyn EventBus>>,
    pending_tool_calls: Option<Vec<ToolCall>>,
    session_recorder: Option<SessionRecorder>,
    streaming_enabled: bool,
    input_rx: Option<mpsc::Receiver<Message>>,
    approval_tx: Option<ApprovalSender>,
    command_rx: Option<CommandReceiver>,
    keyword_detector: KeywordDetectorHook,
    last_tool_output: Option<String>,
}

impl Agent {
    pub fn new(config: AgentConfig, client: Arc<dyn ModelClient>) -> Self {
        Self::new_with_executor(config, client, None)
    }

    pub fn new_with_executor(
        config: AgentConfig,
        client: Arc<dyn ModelClient>,
        executor: Option<Arc<dyn uira_orchestration::AgentExecutor>>,
    ) -> Self {
        Self {
            session: Session::new_with_executor(config, client, executor),
            branches: HashMap::new(),
            current_branch: get_git_branch(),
            current_branch_parent: None,
            control: AgentControl::default(),
            state: AgentState::Idle,
            event_sender: None,
            event_bus: None,
            pending_tool_calls: None,
            session_recorder: None,
            streaming_enabled: true,
            input_rx: None,
            approval_tx: None,
            command_rx: None,
            keyword_detector: KeywordDetectorHook::new(),
            last_tool_output: None,
        }
    }

    /// Create an agent with event streaming enabled
    pub fn with_event_stream(mut self) -> (Self, EventStream) {
        let (sender, stream) = EventStream::channel(100);
        self.event_sender = Some(sender);
        (self, stream)
    }

    /// Set event sender directly (for child agents sharing parent's channel)
    pub fn with_event_sender(mut self, sender: EventSender) -> Self {
        self.event_sender = Some(sender);
        self
    }

    /// Enable session recording for session persistence
    pub fn with_session_recording(mut self) -> Result<Self, AgentLoopError> {
        let meta = SessionMetaLine::new(
            self.session.id.to_string(),
            self.session.client.model(),
            self.session.client.provider(),
            self.session.cwd.clone(),
            format!("{:?}", self.session.config.sandbox_policy),
        );

        let recorder = SessionRecorder::new(meta).map_err(|e| AgentLoopError::Io(e.to_string()))?;

        self.session_recorder = Some(recorder);
        Ok(self)
    }

    /// Disable streaming (use blocking chat instead)
    pub fn with_streaming(mut self, enabled: bool) -> Self {
        self.streaming_enabled = enabled;
        self
    }

    /// Attach an EventBus for unified event publishing
    ///
    /// Events will be converted from ThreadEvent to Event and published to the bus.
    /// This allows the new subscriber-based event system to receive agent events.
    pub fn with_event_bus(mut self, bus: Arc<dyn EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// Attach an EventSystem which includes EventBus and HookEventAdapter
    ///
    /// This is the recommended way to integrate the new event system.
    /// The EventSystem must be started separately after calling this method.
    pub fn with_event_system(self, event_system: &crate::event_system::EventSystem) -> Self {
        self.with_event_bus(event_system.bus())
    }

    pub fn with_interactive(
        mut self,
    ) -> (Self, mpsc::Sender<Message>, ApprovalReceiver, CommandSender) {
        let (input_tx, input_rx) = mpsc::channel(10);
        self.input_rx = Some(input_rx);

        let (approval_tx, approval_rx) = approval_channel(10);
        self.approval_tx = Some(approval_tx);

        let (command_tx, command_rx) = mpsc::channel(10);
        self.command_rx = Some(command_rx);

        (self, input_tx, approval_rx, command_tx)
    }

    async fn handle_interactive_command(&mut self, command: AgentCommand) {
        match command {
            AgentCommand::Interrupt => {
                tracing::debug!("Interrupt command ignored while waiting for input");
            }
            AgentCommand::SwitchClient(new_client) => {
                let model = new_client.model().to_string();
                let provider = new_client.provider().to_string();
                self.session.set_client(new_client);
                tracing::info!("Switched to {} ({})", model, provider);
                self.emit_event(ThreadEvent::ModelSwitched { model, provider })
                    .await;
            }
            AgentCommand::Fork {
                branch_name,
                message_count,
                response_tx,
            } => {
                let result = self.handle_fork(branch_name, message_count).await;
                let _ = response_tx.send(result);
            }
            AgentCommand::SwitchBranch {
                branch_name,
                response_tx,
            } => {
                let result = self.handle_switch_branch(branch_name);
                let _ = response_tx.send(result);
            }
            AgentCommand::ListBranches { response_tx } => {
                let _ = response_tx.send(Ok(self.list_branches()));
            }
            AgentCommand::BranchTree { response_tx } => {
                let _ = response_tx.send(Ok(self.render_branch_tree()));
            }
        }
    }

    pub async fn run_interactive(&mut self) -> Result<(), AgentLoopError> {
        let mut input_rx = self
            .input_rx
            .take()
            .ok_or_else(|| AgentLoopError::Io("No input channel configured".to_string()))?;

        let mut command_rx = self.command_rx.take();
        let mut deferred_commands = VecDeque::new();

        self.state = AgentState::WaitingForUser;
        self.emit_event(ThreadEvent::WaitingForInput {
            prompt: "Ready for input...".to_string(),
        })
        .await;

        enum InteractiveInput {
            Message(Message),
            Prompt(String),
        }

        loop {
            if let Some(command) = deferred_commands.pop_front() {
                self.handle_interactive_command(command).await;
                continue;
            }

            let input_message = tokio::select! {
                command = async {
                    match &mut command_rx {
                        Some(rx) => rx.recv().await,
                        None => std::future::pending().await,
                    }
                } => {
                    match command {
                        Some(command) => self.handle_interactive_command(command).await,
                        None => command_rx = None,
                    }
                    continue;
                }
                input = input_rx.recv() => {
                    match input {
                        Some(message) => message,
                        None => {
                            tracing::info!("Input channel closed, exiting interactive mode");
                            break;
                        }
                    }
                }
            };

            if let MessageContent::Text(text) = &input_message.content {
                if text.trim().eq_ignore_ascii_case("/quit")
                    || text.trim().eq_ignore_ascii_case("/exit")
                {
                    tracing::info!("Quit command received");
                    break;
                }
            }

            let mut current_input = InteractiveInput::Message(input_message);
            let mut continuation_attempts: usize = 0;
            loop {
                self.control.clear_cancelled();
                let cancel_signal = self.control.cancel_signal();
                let mut pending_commands = VecDeque::new();

                let run_result = {
                    let run_future: std::pin::Pin<
                        Box<
                            dyn std::future::Future<
                                    Output = Result<ExecutionResult, AgentLoopError>,
                                > + Send
                                + '_,
                        >,
                    > = match current_input {
                        InteractiveInput::Message(message) => {
                            if let MessageContent::Text(text) = &message.content {
                                Box::pin(self.run_prompt_owned(text.clone()))
                            } else {
                                Box::pin(self.run_message(message))
                            }
                        }
                        InteractiveInput::Prompt(prompt) => Box::pin(self.run_prompt_owned(prompt)),
                    };
                    tokio::pin!(run_future);

                    loop {
                        if let Some(command) = deferred_commands.pop_front() {
                            match command {
                                AgentCommand::Interrupt => {
                                    cancel_signal.store(true, Ordering::SeqCst);
                                }
                                other => pending_commands.push_back(other),
                            }
                            continue;
                        }

                        tokio::select! {
                            result = &mut run_future => break result,
                            command = async {
                                match &mut command_rx {
                                    Some(rx) => rx.recv().await,
                                    None => std::future::pending().await,
                                }
                            } => {
                                match command {
                                    Some(AgentCommand::Interrupt) => {
                                        cancel_signal.store(true, Ordering::SeqCst);
                                    }
                                    Some(other) => pending_commands.push_back(other),
                                    None => {
                                        command_rx = None;
                                    }
                                }
                            }
                        }
                    }
                };

                deferred_commands.extend(pending_commands);

                let (was_error, was_cancel) = match run_result {
                    Ok(result) => {
                        tracing::debug!(
                            "Turn completed: {} turns, success={}",
                            result.turns,
                            result.success
                        );
                        (false, false)
                    }
                    Err(AgentLoopError::Cancelled) => {
                        tracing::info!("Agent cancelled");
                        self.emit_event(ThreadEvent::ThreadCancelled).await;
                        self.control.clear_cancelled();
                        (false, true)
                    }
                    Err(e) => {
                        tracing::error!("Agent error: {}", e);
                        self.emit_event(ThreadEvent::Error {
                            message: e.to_string(),
                            recoverable: true,
                        })
                        .await;
                        (true, false)
                    }
                };

                // Check todo continuation: if incomplete todos remain, auto-inject prompt
                let should_continue = self.session.config.todo_continuation
                    && !was_error
                    && !was_cancel
                    && continuation_attempts < self.session.config.max_continuation_attempts
                    && self
                        .session
                        .todo_store
                        .has_incomplete(&self.session.id.to_string())
                        .await;

                if should_continue {
                    continuation_attempts += 1;
                    let incomplete_count = self
                        .session
                        .todo_store
                        .incomplete_count(&self.session.id.to_string())
                        .await;
                    let total = self
                        .session
                        .todo_store
                        .get(&self.session.id.to_string())
                        .await
                        .len();

                    self.emit_event(ThreadEvent::ContentDelta {
                        delta: format!(
                            "\n[Todo continuation: {} incomplete tasks, auto-resuming...]\n",
                            incomplete_count
                        ),
                    })
                    .await;

                    current_input = InteractiveInput::Prompt(format!(
                        "{}\n\n[Status: {}/{} completed, {} remaining]",
                        uira_core::TODO_CONTINUATION_PROMPT,
                        total.saturating_sub(incomplete_count),
                        total,
                        incomplete_count
                    ));
                    continue;
                }

                break;
            }

            self.state = AgentState::WaitingForUser;
            self.emit_event(ThreadEvent::WaitingForInput {
                prompt: "Ready for input...".to_string(),
            })
            .await;
        }

        Ok(())
    }

    /// Resume from a session file
    pub fn resume_from_session(
        config: AgentConfig,
        client: Arc<dyn ModelClient>,
        session_path: PathBuf,
    ) -> Result<Self, AgentLoopError> {
        // Load items from session log
        let items =
            SessionRecorder::load(&session_path).map_err(|e| AgentLoopError::Io(e.to_string()))?;

        // Create agent
        let mut agent = Self::new(config, client);

        // Restore messages to context
        let messages = extract_messages(&items);
        for msg in messages {
            agent
                .session
                .context
                .add_message(msg)
                .map_err(AgentLoopError::Context)?;
        }

        // Restore turn count and usage
        agent.session.turn = get_last_turn(&items);
        agent.session.usage = get_total_usage(&items);

        // Open session log for appending
        let recorder =
            SessionRecorder::open(session_path).map_err(|e| AgentLoopError::Io(e.to_string()))?;
        agent.session_recorder = Some(recorder);

        Ok(agent)
    }

    /// Get the agent control handle
    pub fn control(&self) -> &AgentControl {
        &self.control
    }

    /// Get the current state
    pub fn state(&self) -> AgentState {
        self.state
    }

    /// Get the session
    pub fn session(&self) -> &Session {
        &self.session
    }

    /// Get the session recording path if recording is enabled
    pub fn session_path(&self) -> Option<&PathBuf> {
        self.session_recorder.as_ref().map(|r| r.path())
    }

    async fn handle_fork(
        &mut self,
        branch_name: Option<String>,
        message_count: Option<usize>,
    ) -> Result<ForkResult, String> {
        let branch_name = match branch_name {
            Some(name) => {
                let trimmed = name.trim();
                if trimmed.is_empty() {
                    return Err("Branch name cannot be empty".to_string());
                }
                if trimmed.len() > 64 {
                    return Err("Branch name must be 64 characters or fewer".to_string());
                }
                if !trimmed
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                {
                    return Err(
                        "Branch name must contain only letters, numbers, '-' or '_'".to_string()
                    );
                }
                trimmed.to_string()
            }
            None => self.next_branch_name(),
        };

        if self.branch_exists(&branch_name) {
            return Err(format!("Branch '{}' already exists", branch_name));
        }

        let forked_session = match message_count {
            Some(count) => self.session.fork_at_message(count),
            None => self.session.fork(),
        };

        let forked_session_id = forked_session.id.to_string();
        let parent_branch = self.current_branch.clone();
        let parent_session_id = self.session.id.to_string();
        let fork_point_message_id = forked_session.forked_from_message.clone();
        let msg_count = forked_session.context.messages().len();

        self.branches.insert(
            branch_name.clone(),
            BranchState {
                parent: Some(parent_branch.clone()),
                session: forked_session,
            },
        );

        if let Some(ref mut recorder) = self.session_recorder {
            if let Err(e) = recorder.record_fork(
                SessionId::from_string(forked_session_id.clone()),
                fork_point_message_id.clone(),
                msg_count,
            ) {
                tracing::warn!("Failed to record fork event: {}", e);
            }
        }

        if let Some(bus) = &self.event_bus {
            let event = uira_core::Event::SessionForked {
                session_id: forked_session_id.clone(),
                parent_id: parent_session_id.clone(),
                fork_point_message_id: fork_point_message_id.map(|id| id.to_string()),
            };
            bus.publish(event);
        }

        tracing::info!(
            "Forked session {} from {} at message {}",
            forked_session_id,
            parent_session_id,
            message_count.map_or("end".to_string(), |c| c.to_string())
        );

        Ok(ForkResult {
            branch_name,
            session_id: forked_session_id,
            parent_branch,
        })
    }

    fn handle_switch_branch(&mut self, branch_name: String) -> Result<SwitchBranchResult, String> {
        let branch_name = branch_name.trim().to_string();
        if branch_name.is_empty() {
            return Err("Branch name cannot be empty".to_string());
        }

        if branch_name == self.current_branch {
            return Ok(SwitchBranchResult {
                branch_name,
                session_id: self.session.id.to_string(),
                message: format!("Already on branch '{}'", self.current_branch),
            });
        }

        let target = self
            .branches
            .remove(&branch_name)
            .ok_or_else(|| format!("Branch '{}' not found", branch_name))?;

        let previous_branch_name = std::mem::replace(&mut self.current_branch, branch_name.clone());
        let previous_parent =
            std::mem::replace(&mut self.current_branch_parent, target.parent.clone());
        let previous_session = std::mem::replace(&mut self.session, target.session);

        self.branches.insert(
            previous_branch_name,
            BranchState {
                parent: previous_parent,
                session: previous_session,
            },
        );

        Ok(SwitchBranchResult {
            branch_name: branch_name.clone(),
            session_id: self.session.id.to_string(),
            message: format!(
                "Switched to branch '{}' (session {})",
                branch_name, self.session.id
            ),
        })
    }

    fn list_branches(&self) -> Vec<BranchInfo> {
        let mut infos = Vec::with_capacity(self.branches.len() + 1);
        infos.push(BranchInfo {
            name: self.current_branch.clone(),
            parent: self.current_branch_parent.clone(),
            session_id: self.session.id.to_string(),
            is_current: true,
        });

        for (name, branch) in &self.branches {
            infos.push(BranchInfo {
                name: name.clone(),
                parent: branch.parent.clone(),
                session_id: branch.session.id.to_string(),
                is_current: false,
            });
        }

        infos.sort_by(|a, b| a.name.cmp(&b.name));

        if let Some(index) = infos.iter().position(|b| b.is_current) {
            let current = infos.remove(index);
            infos.insert(0, current);
        }

        infos
    }

    fn render_branch_tree(&self) -> String {
        let branches = self.list_branches();
        if branches.is_empty() {
            return "No branches available.".to_string();
        }

        let mut nodes: HashMap<String, BranchInfo> = HashMap::new();
        for branch in branches {
            nodes.insert(branch.name.clone(), branch);
        }

        let mut children: HashMap<String, Vec<String>> = HashMap::new();
        let mut roots: Vec<String> = Vec::new();

        for node in nodes.values() {
            if let Some(parent) = &node.parent {
                if nodes.contains_key(parent) {
                    children
                        .entry(parent.clone())
                        .or_default()
                        .push(node.name.clone());
                    continue;
                }
            }
            roots.push(node.name.clone());
        }

        roots.sort();
        for child_names in children.values_mut() {
            child_names.sort();
        }

        let mut lines = vec!["Session Branch Tree:".to_string()];
        for (idx, root) in roots.iter().enumerate() {
            Self::append_branch_tree_node(
                root,
                &nodes,
                &children,
                "",
                idx == roots.len() - 1,
                &mut lines,
            );
        }

        lines.join("\n")
    }

    fn append_branch_tree_node(
        name: &str,
        nodes: &HashMap<String, BranchInfo>,
        children: &HashMap<String, Vec<String>>,
        prefix: &str,
        is_last: bool,
        lines: &mut Vec<String>,
    ) {
        let Some(node) = nodes.get(name) else {
            return;
        };

        let connector = if prefix.is_empty() {
            ""
        } else if is_last {
            "└── "
        } else {
            "├── "
        };

        let current_marker = if node.is_current { " <- current" } else { "" };
        lines.push(format!(
            "{}{}{} ({}){}",
            prefix,
            connector,
            node.name,
            Self::short_session_id(&node.session_id),
            current_marker
        ));

        if let Some(child_nodes) = children.get(name) {
            let child_prefix = if prefix.is_empty() {
                "    ".to_string()
            } else if is_last {
                format!("{}    ", prefix)
            } else {
                format!("{}│   ", prefix)
            };

            for (index, child_name) in child_nodes.iter().enumerate() {
                Self::append_branch_tree_node(
                    child_name,
                    nodes,
                    children,
                    &child_prefix,
                    index == child_nodes.len() - 1,
                    lines,
                );
            }
        }
    }

    fn short_session_id(session_id: &str) -> &str {
        session_id.get(..8).unwrap_or(session_id)
    }

    fn branch_exists(&self, name: &str) -> bool {
        name == self.current_branch || self.branches.contains_key(name)
    }

    fn next_branch_name(&self) -> String {
        let mut index = 1;
        loop {
            let candidate = format!("branch-{}", index);
            if !self.branch_exists(&candidate) {
                return candidate;
            }
            index += 1;
        }
    }

    pub async fn run_message(
        &mut self,
        message: Message,
    ) -> Result<ExecutionResult, AgentLoopError> {
        self.state = AgentState::Thinking;

        let session_span =
            SessionSpan::new(&self.session.id.to_string(), self.session.client.model());
        let _session_guard = session_span.enter();

        self.emit_event(ThreadEvent::ThreadStarted {
            thread_id: self.session.id.to_string(),
        })
        .await;

        let effective_message = self.apply_keyword_detection_to_message(message).await;

        self.record_message(effective_message.clone());
        self.session
            .context
            .add_message(effective_message)
            .map_err(AgentLoopError::Context)?;

        self.run_turn_loop().await
    }

    async fn apply_keyword_detection_to_message(&mut self, message: Message) -> Message {
        let text_content = match &message.content {
            MessageContent::Text(text) => Some(text.clone()),
            MessageContent::Blocks(blocks) => {
                let texts: Vec<String> = blocks
                    .iter()
                    .filter_map(|block| {
                        if let ContentBlock::Text { text } = block {
                            Some(text.clone())
                        } else {
                            None
                        }
                    })
                    .collect();
                if texts.is_empty() {
                    None
                } else {
                    Some(texts.join("\n"))
                }
            }
            MessageContent::ToolCalls(_) => None,
        };

        let Some(text) = text_content else {
            return message;
        };

        let Some(keyword_msg) = self.keyword_detector.detect_and_message(&text) else {
            return message;
        };

        self.emit_event(ThreadEvent::ContentDelta {
            delta: format!("{}\n\n", keyword_msg),
        })
        .await;

        match message.content {
            MessageContent::Text(original_text) => Message {
                role: message.role,
                content: MessageContent::Text(format!("{}\n\n{}", keyword_msg, original_text)),
                name: message.name,
                tool_call_id: message.tool_call_id,
            },
            MessageContent::Blocks(blocks) => {
                let mut new_blocks = Vec::with_capacity(blocks.len() + 1);
                new_blocks.push(ContentBlock::text(format!("{}\n\n", keyword_msg)));
                new_blocks.extend(blocks);
                Message {
                    role: message.role,
                    content: MessageContent::Blocks(new_blocks),
                    name: message.name,
                    tool_call_id: message.tool_call_id,
                }
            }
            MessageContent::ToolCalls(_) => message,
        }
    }

    pub async fn run(&mut self, prompt: &str) -> Result<ExecutionResult, AgentLoopError> {
        self.state = AgentState::Thinking;

        let session_span =
            SessionSpan::new(&self.session.id.to_string(), self.session.client.model());
        let _session_guard = session_span.enter();

        self.emit_event(ThreadEvent::ThreadStarted {
            thread_id: self.session.id.to_string(),
        })
        .await;

        let effective_prompt =
            if let Some(keyword_msg) = self.keyword_detector.detect_and_message(prompt) {
                self.emit_event(ThreadEvent::ContentDelta {
                    delta: format!("{}\n\n", keyword_msg),
                })
                .await;
                format!("{}\n\n{}", keyword_msg, prompt)
            } else {
                prompt.to_string()
            };

        let user_message = Message::user_prompt(&effective_prompt);
        self.record_message(user_message.clone());
        self.session
            .context
            .add_message(user_message)
            .map_err(AgentLoopError::Context)?;

        self.run_turn_loop().await
    }

    async fn run_prompt_owned(
        &mut self,
        prompt: String,
    ) -> Result<ExecutionResult, AgentLoopError> {
        self.run(&prompt).await
    }

    async fn run_turn_loop(&mut self) -> Result<ExecutionResult, AgentLoopError> {
        loop {
            // Check for cancellation
            if self.control.is_cancelled() {
                self.state = AgentState::Cancelled;
                self.record_event(ThreadEvent::ThreadCancelled);
                return Err(AgentLoopError::Cancelled);
            }

            // Check for max turns
            if self.session.is_max_turns_exceeded() {
                self.state = AgentState::Failed;
                return Ok(ExecutionResult::failure(
                    AgentError::MaxTurnsExceeded {
                        turns: self.session.turn,
                    },
                    self.session.turn,
                    self.session.usage.clone(),
                ));
            }

            // Start a new turn
            let turn_number = self.session.start_turn();
            let turn_span = TurnSpan::new(turn_number);
            let _turn_guard = turn_span.enter();

            self.emit_event(ThreadEvent::TurnStarted { turn_number })
                .await;

            // Get model response (streaming or blocking)
            let tool_specs = self.session.tool_specs();
            let response = if self.streaming_enabled {
                self.get_response_streaming(&tool_specs).await?
            } else {
                self.session
                    .client
                    .chat(self.session.context.messages(), &tool_specs)
                    .await
                    .map_err(AgentLoopError::Provider)?
            };

            // Record usage
            self.session.record_usage(response.usage.clone());
            self.record_turn(turn_number, response.usage.clone());

            // Add assistant message to context
            let assistant_message =
                Message::with_blocks(uira_core::Role::Assistant, response.content.clone());
            self.record_message(assistant_message.clone());
            self.session
                .context
                .add_message(assistant_message)
                .map_err(AgentLoopError::Context)?;

            // Emit turn completed
            self.emit_event(ThreadEvent::TurnCompleted {
                turn_number,
                usage: response.usage.clone(),
            })
            .await;

            // Check if we should continue (tool calls) or stop
            if response.has_tool_calls() {
                // Process tool calls
                self.state = AgentState::ExecutingTool;

                let tool_calls = response.tool_calls();
                let tool_results = self.execute_tool_calls(&tool_calls).await?;

                // Add tool results to context
                let tool_result_message = Message::with_blocks(Role::User, tool_results);
                self.record_message(tool_result_message.clone());
                self.session
                    .context
                    .add_message(tool_result_message)
                    .map_err(AgentLoopError::Context)?;

                self.state = AgentState::Thinking;
            } else {
                // No tool calls, we're done
                self.state = AgentState::Complete;
                self.emit_event(ThreadEvent::ThreadCompleted {
                    usage: self.session.usage.clone(),
                })
                .await;

                let output = {
                    let text = response.text();
                    if text.is_empty() {
                        self.last_tool_output.take().unwrap_or_default()
                    } else {
                        text
                    }
                };

                return Ok(ExecutionResult::success(
                    output,
                    self.session.turn,
                    self.session.usage.clone(),
                ));
            }
        }
    }

    /// Get model response with streaming, emitting ContentDelta events
    async fn get_response_streaming(
        &mut self,
        tool_specs: &[uira_core::ToolSpec],
    ) -> Result<uira_core::ModelResponse, AgentLoopError> {
        let stream = self
            .session
            .client
            .chat_stream(self.session.context.messages(), tool_specs)
            .await
            .map_err(AgentLoopError::Provider)?;

        let mut controller = StreamController::new();
        let mut stream = std::pin::pin!(stream);

        loop {
            if self.control.is_cancelled() {
                return Err(AgentLoopError::Cancelled);
            }

            let next_chunk = tokio::select! {
                result = stream.next() => result,
                _ = tokio::time::sleep(Duration::from_millis(50)) => continue,
            };

            let Some(result) = next_chunk else {
                break;
            };

            let chunk = result.map_err(AgentLoopError::Provider)?;
            let outputs = controller.push(chunk);

            for output in outputs {
                match output {
                    crate::streaming::StreamOutput::Text(line) => {
                        self.emit_event(ThreadEvent::ContentDelta {
                            delta: format!("{}\n", line),
                        })
                        .await;
                    }
                    crate::streaming::StreamOutput::Thinking(thinking) => {
                        self.emit_event(ThreadEvent::ThinkingDelta { thinking })
                            .await;
                    }
                }
            }
        }

        Ok(controller.into_response())
    }

    /// Execute a single step of the agent loop
    ///
    /// This allows fine-grained control over execution - useful for TUI/debugging.
    /// Returns the new state after the step completes.
    pub async fn step(&mut self) -> Result<AgentState, AgentLoopError> {
        match self.state {
            AgentState::Idle => {
                // Nothing to do in idle state - need to call run() with a prompt first
                Ok(AgentState::Idle)
            }
            AgentState::Thinking => {
                // Check for cancellation
                if self.control.is_cancelled() {
                    self.state = AgentState::Cancelled;
                    self.record_event(ThreadEvent::ThreadCancelled);
                    return Ok(AgentState::Cancelled);
                }

                // Check for max turns
                if self.session.is_max_turns_exceeded() {
                    self.state = AgentState::Failed;
                    return Ok(AgentState::Failed);
                }

                // Start a new turn
                let turn_number = self.session.start_turn();
                self.emit_event(ThreadEvent::TurnStarted { turn_number })
                    .await;

                // Get model response (streaming or blocking)
                let tool_specs = self.session.tool_specs();
                let response = if self.streaming_enabled {
                    self.get_response_streaming(&tool_specs).await?
                } else {
                    self.session
                        .client
                        .chat(self.session.context.messages(), &tool_specs)
                        .await
                        .map_err(AgentLoopError::Provider)?
                };

                // Record usage
                self.session.record_usage(response.usage.clone());
                self.record_turn(turn_number, response.usage.clone());

                // Add assistant message to context
                let assistant_message =
                    Message::with_blocks(Role::Assistant, response.content.clone());
                self.record_message(assistant_message.clone());
                self.session
                    .context
                    .add_message(assistant_message)
                    .map_err(AgentLoopError::Context)?;

                // Emit turn completed
                self.emit_event(ThreadEvent::TurnCompleted {
                    turn_number,
                    usage: response.usage.clone(),
                })
                .await;

                // Determine next state based on response
                if response.has_tool_calls() {
                    // Store pending tool calls for next step
                    let tool_calls = response.tool_calls();
                    self.pending_tool_calls = Some(tool_calls);
                    self.state = AgentState::ExecutingTool;
                } else {
                    // No tool calls, we're done
                    self.state = AgentState::Complete;
                    self.emit_event(ThreadEvent::ThreadCompleted {
                        usage: self.session.usage.clone(),
                    })
                    .await;
                }

                Ok(self.state)
            }
            AgentState::ExecutingTool => {
                // Execute pending tool calls
                if let Some(tool_calls) = self.pending_tool_calls.take() {
                    let tool_results = self.execute_tool_calls(&tool_calls).await?;

                    // Add tool results to context
                    let tool_result_message = Message::with_blocks(Role::User, tool_results);
                    self.record_message(tool_result_message.clone());
                    self.session
                        .context
                        .add_message(tool_result_message)
                        .map_err(AgentLoopError::Context)?;

                    // Go back to thinking for next turn
                    self.state = AgentState::Thinking;
                } else {
                    // No pending tool calls, should not happen
                    self.state = AgentState::Thinking;
                }

                Ok(self.state)
            }
            AgentState::WaitingForApproval => {
                // Waiting for external approval - state will be updated externally
                Ok(AgentState::WaitingForApproval)
            }
            AgentState::WaitingForUser => {
                // Waiting for user input - state will be updated externally
                Ok(AgentState::WaitingForUser)
            }
            AgentState::Complete | AgentState::Cancelled | AgentState::Failed => {
                // Terminal states - no more steps
                Ok(self.state)
            }
        }
    }

    /// Start a new run with a prompt (sets up for step-by-step execution)
    pub async fn start(&mut self, prompt: &str) -> Result<(), AgentLoopError> {
        self.state = AgentState::Thinking;
        self.pending_tool_calls = None;

        // Emit thread started event
        self.emit_event(ThreadEvent::ThreadStarted {
            thread_id: self.session.id.to_string(),
        })
        .await;

        // Add user message
        let user_message = Message::user_prompt(prompt);
        self.record_message(user_message.clone());
        self.session
            .context
            .add_message(user_message)
            .map_err(AgentLoopError::Context)?;

        Ok(())
    }

    /// Check if the agent is in a terminal state
    pub fn is_done(&self) -> bool {
        matches!(
            self.state,
            AgentState::Complete | AgentState::Cancelled | AgentState::Failed
        )
    }

    /// Get the final result if the agent is complete
    fn last_assistant_text(&self) -> Option<String> {
        let text = self
            .session
            .context
            .messages()
            .iter()
            .rev()
            .find(|m| m.role == Role::Assistant)
            .map(|m| match &m.content {
                uira_core::MessageContent::Text(s) => s.clone(),
                uira_core::MessageContent::Blocks(blocks) => blocks
                    .iter()
                    .filter_map(|b| {
                        if let ContentBlock::Text { text } = b {
                            Some(text.as_str())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(""),
                uira_core::MessageContent::ToolCalls(_) => String::new(),
            })
            .unwrap_or_default();

        if text.trim().is_empty() {
            None
        } else {
            Some(text)
        }
    }

    pub fn result(&self) -> Option<ExecutionResult> {
        match self.state {
            AgentState::Complete => {
                let last_text = self.last_assistant_text().unwrap_or_default();

                Some(ExecutionResult::success(
                    last_text,
                    self.session.turn,
                    self.session.usage.clone(),
                ))
            }
            AgentState::Failed => Some(ExecutionResult::failure(
                AgentError::MaxTurnsExceeded {
                    turns: self.session.turn,
                },
                self.session.turn,
                self.session.usage.clone(),
            )),
            _ => None,
        }
    }

    /// Pause the agent
    pub fn pause(&mut self) {
        self.control.pause();
    }

    /// Resume the agent
    pub fn resume(&mut self) {
        self.control.resume();
    }

    /// Cancel the agent
    pub fn cancel(&mut self) {
        self.control.cancel();
        self.state = AgentState::Cancelled;
        self.record_event(ThreadEvent::ThreadCancelled);
    }

    async fn emit_event(&self, event: ThreadEvent) {
        if let Some(ref sender) = self.event_sender {
            let _ = sender.send(event.clone()).await;
        }

        if let Some(ref bus) = self.event_bus {
            let unified_event = match &event {
                ThreadEvent::ThreadCompleted { .. } => Event::SessionEnded {
                    session_id: self.session.id.to_string(),
                    reason: SessionEndReason::Completed,
                    last_response: self.last_assistant_text(),
                },
                ThreadEvent::ThreadCancelled => Event::SessionEnded {
                    session_id: self.session.id.to_string(),
                    reason: SessionEndReason::Cancelled,
                    last_response: self.last_assistant_text(),
                },
                _ => event.into(),
            };
            bus.publish(unified_event);
        }
    }

    /// Record a message to the session log
    fn record_message(&mut self, message: Message) {
        if let Some(ref mut recorder) = self.session_recorder {
            if let Err(e) = recorder.record_message(message) {
                tracing::warn!("Failed to record message to session log: {}", e);
            }
        }
    }

    /// Record a tool call to the session log
    fn record_tool_call(&mut self, id: &str, name: &str, input: &serde_json::Value) {
        if let Some(ref mut recorder) = self.session_recorder {
            if let Err(e) = recorder.record_tool_call(id, name, input.clone()) {
                tracing::warn!("Failed to record tool call to session log: {}", e);
            }
        }
    }

    /// Record a tool result to the session log
    fn record_tool_result(&mut self, id: &str, output: &str, is_error: bool) {
        if let Some(ref mut recorder) = self.session_recorder {
            if let Err(e) = recorder.record_tool_result(id, output, is_error) {
                tracing::warn!("Failed to record tool result to session log: {}", e);
            }
        }
    }

    /// Record turn context to the session log
    fn record_turn(&mut self, turn: usize, usage: uira_core::TokenUsage) {
        if let Some(ref mut recorder) = self.session_recorder {
            if let Err(e) = recorder.record_turn(turn, usage) {
                tracing::warn!("Failed to record turn to session log: {}", e);
            }
        }
    }

    /// Record a thread event to the session log
    fn record_event(&mut self, event: ThreadEvent) {
        if let Some(ref mut recorder) = self.session_recorder {
            if let Err(e) = recorder.record_event(event) {
                tracing::warn!("Failed to record event to session log: {}", e);
            }
        }
    }

    async fn emit_background_event_from_tool_output(&self, tool_name: &str, output: &str) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(output) else {
            return;
        };

        match tool_name {
            "delegate_task" => {
                let Some(task_id) = value.get("taskId").and_then(|v| v.as_str()) else {
                    return;
                };
                let description = value
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Background task started")
                    .to_string();
                let agent = value
                    .get("agent")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                self.emit_event(ThreadEvent::BackgroundTaskSpawned {
                    task_id: task_id.to_string(),
                    description,
                    agent,
                })
                .await;
            }
            "background_output" => {
                let Some(task_id) = value.get("taskId").and_then(|v| v.as_str()) else {
                    return;
                };
                let Some(status) = value.get("status").and_then(|v| v.as_str()) else {
                    return;
                };

                match status {
                    "queued" | "pending" | "running" => {
                        let message = value
                            .get("progress")
                            .and_then(|p| p.get("lastTool"))
                            .and_then(|v| v.as_str())
                            .map(|tool| format!("last tool: {tool}"));
                        self.emit_event(ThreadEvent::BackgroundTaskProgress {
                            task_id: task_id.to_string(),
                            status: status.to_string(),
                            message,
                        })
                        .await;
                    }
                    "completed" | "error" | "cancelled" => {
                        let started_at = value
                            .get("startedAt")
                            .and_then(|v| v.as_str())
                            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                            .map(|dt| dt.with_timezone(&Utc));
                        let completed_at = value
                            .get("completedAt")
                            .and_then(|v| v.as_str())
                            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                            .map(|dt| dt.with_timezone(&Utc));
                        let duration_secs = started_at
                            .zip(completed_at)
                            .map(|(start, end)| {
                                (end - start).num_milliseconds().max(0) as f64 / 1000.0
                            })
                            .unwrap_or(0.0);
                        let result_preview = value
                            .get("result")
                            .and_then(|v| v.as_str())
                            .map(|s| s.chars().take(200).collect::<String>())
                            .or_else(|| {
                                value
                                    .get("error")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.chars().take(200).collect::<String>())
                            });
                        self.emit_event(ThreadEvent::BackgroundTaskCompleted {
                            task_id: task_id.to_string(),
                            success: status == "completed",
                            result_preview,
                            duration_secs,
                        })
                        .await;
                    }
                    _ => {}
                }
            }
            "background_cancel" => {
                let Some(task_id) = value.get("taskId").and_then(|v| v.as_str()) else {
                    return;
                };
                let status = value
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("cancelled");
                if status == "cancelled" {
                    self.emit_event(ThreadEvent::BackgroundTaskCompleted {
                        task_id: task_id.to_string(),
                        success: false,
                        result_preview: Some("Task cancelled".to_string()),
                        duration_secs: 0.0,
                    })
                    .await;
                }
            }
            _ => {}
        }
    }

    /// Execute tool calls and return results as content blocks
    ///
    /// This method handles approval flow at the Agent level:
    /// 1. Check cancellation and approval for each tool (sequential - requires user interaction)
    /// 2. Execute approved tools in parallel where possible (parallel-safe tools run concurrently)
    /// 3. Emit results in original order
    async fn execute_tool_calls(
        &mut self,
        tool_calls: &[ToolCall],
    ) -> Result<Vec<ContentBlock>, AgentLoopError> {
        let mut results = Vec::new();
        let ctx = self.session.tool_context();

        // Phase 1: Check approvals sequentially (requires user interaction)
        // Collect approved calls for parallel execution
        let mut approved_calls: Vec<(String, String, serde_json::Value)> = Vec::new();

        for call in tool_calls {
            // Check for cancellation between approval checks
            if self.control.is_cancelled() {
                return Err(AgentLoopError::Cancelled);
            }

            if let Some(permission_action) = self
                .session
                .orchestrator
                .evaluate_permission(&call.name, &call.input)
            {
                use uira_security::Action as PermAction;
                match permission_action {
                    PermAction::Deny => {
                        let error_msg = format!("Permission denied for tool: {}", call.name);
                        results.push(ContentBlock::tool_error(&call.id, &error_msg));
                        self.record_tool_result(&call.id, &error_msg, true);
                        self.emit_event(ThreadEvent::ItemCompleted {
                            item: Item::ToolResult {
                                tool_call_id: call.id.clone(),
                                output: error_msg,
                                is_error: true,
                            },
                        })
                        .await;
                        continue;
                    }
                    PermAction::Allow => {
                        approved_calls.push((
                            call.id.clone(),
                            call.name.clone(),
                            call.input.clone(),
                        ));
                        continue;
                    }
                    PermAction::Ask => {}
                }
            }

            // ALWAYS check for Forbidden tools (security critical)
            let requirement = self
                .session
                .orchestrator
                .approval_requirement_for(&call.name, &call.input);

            // Forbidden MUST be enforced regardless of full_auto
            if let ApprovalRequirement::Forbidden { reason } = &requirement {
                return Err(AgentLoopError::ToolForbidden {
                    tool: call.name.clone(),
                    reason: reason.clone(),
                });
            }

            // Handle approval (skip only NeedsApproval in full_auto mode)
            if !ctx.full_auto {
                match requirement {
                    ApprovalRequirement::NeedsApproval { reason } => {
                        if let Some(cached) = self
                            .session
                            .orchestrator
                            .check_approval_cache(&call.name, &call.input)
                            .await
                        {
                            if cached.is_approve() {
                                tracing::debug!(
                                    tool = %call.name,
                                    cached_decision = ?cached,
                                    "approval_cache_hit"
                                );
                                approved_calls.push((
                                    call.id.clone(),
                                    call.name.clone(),
                                    call.input.clone(),
                                ));
                                continue;
                            }
                        }

                        if let Some(ref approval_tx) = self.approval_tx {
                            self.emit_event(ThreadEvent::ItemStarted {
                                item: Item::ApprovalRequest {
                                    id: call.id.clone(),
                                    tool_name: call.name.clone(),
                                    input: call.input.clone(),
                                    reason: reason.clone(),
                                },
                            })
                            .await;

                            let decision = timeout(
                                APPROVAL_TIMEOUT,
                                approval_tx.request_approval(
                                    &call.id,
                                    &call.name,
                                    call.input.clone(),
                                    &reason,
                                ),
                            )
                            .await
                            .map_err(|_| {
                                AgentLoopError::ApprovalTimeout {
                                    tool: call.name.clone(),
                                    timeout_secs: APPROVAL_TIMEOUT.as_secs(),
                                }
                            })??;

                            self.session
                                .orchestrator
                                .store_approval(&call.name, &call.input, &decision)
                                .await;

                            self.emit_event(ThreadEvent::ItemCompleted {
                                item: Item::ApprovalDecision {
                                    request_id: call.id.clone(),
                                    approved: decision.is_approved(),
                                },
                            })
                            .await;

                            if decision.is_denied() {
                                let deny_reason =
                                    if let uira_core::ReviewDecision::Deny { reason } = &decision {
                                        reason.clone().unwrap_or_default()
                                    } else {
                                        String::new()
                                    };

                                let error_msg = format!("Tool execution denied: {}", deny_reason);
                                results.push(ContentBlock::tool_error(&call.id, &error_msg));
                                self.record_tool_result(&call.id, &error_msg, true);
                                self.emit_event(ThreadEvent::ItemCompleted {
                                    item: Item::ToolResult {
                                        tool_call_id: call.id.clone(),
                                        output: error_msg,
                                        is_error: true,
                                    },
                                })
                                .await;
                                continue;
                            }
                        } else {
                            let error_msg = format!(
                                "Tool '{}' requires approval but no approval channel is configured",
                                call.name
                            );
                            results.push(ContentBlock::tool_error(&call.id, &error_msg));
                            self.record_tool_result(&call.id, &error_msg, true);
                            self.emit_event(ThreadEvent::ItemCompleted {
                                item: Item::ToolResult {
                                    tool_call_id: call.id.clone(),
                                    output: error_msg,
                                    is_error: true,
                                },
                            })
                            .await;
                            continue;
                        }
                    }
                    ApprovalRequirement::Skip { .. } => {
                        // No approval needed
                    }
                    ApprovalRequirement::Forbidden { .. } => {
                        // Already handled above
                    }
                }
            }

            // Tool is approved - add to batch for parallel execution
            approved_calls.push((call.id.clone(), call.name.clone(), call.input.clone()));
        }

        // Phase 2: Emit tool start events and build call_id->name mapping
        let mut call_id_to_name: HashMap<String, String> =
            HashMap::with_capacity(approved_calls.len());
        for (id, name, input) in &approved_calls {
            call_id_to_name.insert(id.clone(), name.clone());
            self.record_tool_call(id, name, input);
            self.emit_event(ThreadEvent::ItemStarted {
                item: Item::ToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    input: input.clone(),
                },
            })
            .await;
        }

        // Phase 3: Execute tools in parallel where possible
        // ToolCallRuntime handles read/write lock semantics:
        // - Parallel-safe tools (Read, Glob, Grep): run concurrently with read lock
        // - Mutating tools (Write, Edit, Bash): run exclusively with write lock
        let execution_results = self
            .session
            .parallel_runtime
            .execute_batch_with_ids(approved_calls, &ctx)
            .await;

        // Phase 4: Process results and emit events (must be sequential)
        let mut todo_updated = false;
        for (call_id, result) in execution_results {
            let tool_name = call_id_to_name.get(&call_id).map(|s| s.as_str());
            match result {
                Ok(output) => {
                    let content = output.as_text().unwrap_or("").to_string();
                    results.push(ContentBlock::tool_result(&call_id, &content));

                    if !content.is_empty() {
                        self.last_tool_output = Some(content.clone());
                    }

                    self.record_tool_result(&call_id, &content, false);
                    self.emit_event(ThreadEvent::ItemCompleted {
                        item: Item::ToolResult {
                            tool_call_id: call_id,
                            output: content.clone(),
                            is_error: false,
                        },
                    })
                    .await;

                    if let Some(name) = tool_name {
                        self.emit_background_event_from_tool_output(name, &content)
                            .await;
                    }

                    if tool_name == Some("TodoWrite") {
                        todo_updated = true;
                    }
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    results.push(ContentBlock::tool_error(&call_id, &error_msg));

                    self.record_tool_result(&call_id, &error_msg, true);
                    self.emit_event(ThreadEvent::ItemCompleted {
                        item: Item::ToolResult {
                            tool_call_id: call_id,
                            output: error_msg,
                            is_error: true,
                        },
                    })
                    .await;
                }
            }
        }

        // Phase 5: Emit TodoUpdated event for TUI sidebar
        if todo_updated {
            let todos = self.session.todo_store.get(&ctx.session_id).await;
            self.emit_event(ThreadEvent::TodoUpdated { todos }).await;
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    // Tests would require mocking the model client
}
