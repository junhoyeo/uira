//! Main agent implementation

use futures::StreamExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::timeout;
use uira_protocol::{
    AgentError, AgentState, ApprovalRequirement, ContentBlock, ExecutionResult, Item, Message,
    Role, ThreadEvent, ToolCall,
};
use uira_providers::ModelClient;
use uira_tools::RunOptions;

use crate::{
    approval::{approval_channel, ApprovalReceiver, ApprovalSender},
    events::{EventSender, EventStream},
    rollout::{extract_messages, get_last_turn, get_total_usage, RolloutRecorder, SessionMetaLine},
    streaming::StreamController,
    AgentConfig, AgentControl, AgentLoopError, Session,
};

/// Timeout for approval requests (5 minutes)
const APPROVAL_TIMEOUT: Duration = Duration::from_secs(300);

/// The main agent that orchestrates the conversation loop
pub struct Agent {
    session: Session,
    control: AgentControl,
    state: AgentState,
    event_sender: Option<EventSender>,
    /// Pending tool calls for step-by-step execution
    pending_tool_calls: Option<Vec<ToolCall>>,
    /// Rollout recorder for session persistence
    rollout: Option<RolloutRecorder>,
    /// Whether to use streaming for model responses
    streaming_enabled: bool,
    /// Channel to receive user input (for interactive mode)
    input_rx: Option<mpsc::Receiver<String>>,
    /// Channel to send approval requests (for interactive mode)
    approval_tx: Option<ApprovalSender>,
}

impl Agent {
    /// Create a new agent with the given configuration and model client
    pub fn new(config: AgentConfig, client: Arc<dyn ModelClient>) -> Self {
        Self {
            session: Session::new(config, client),
            control: AgentControl::default(),
            state: AgentState::Idle,
            event_sender: None,
            pending_tool_calls: None,
            rollout: None,
            streaming_enabled: true,
            input_rx: None,
            approval_tx: None,
        }
    }

    /// Create an agent with event streaming enabled
    pub fn with_event_stream(mut self) -> (Self, EventStream) {
        let (sender, stream) = EventStream::channel(100);
        self.event_sender = Some(sender);
        (self, stream)
    }

    /// Enable rollout recording for session persistence
    pub fn with_rollout(mut self) -> Result<Self, AgentLoopError> {
        let meta = SessionMetaLine::new(
            self.session.id.to_string(),
            self.session.client.model(),
            self.session.client.provider(),
            self.session.cwd.clone(),
            format!("{:?}", self.session.config.sandbox_policy),
        );

        let recorder = RolloutRecorder::new(meta).map_err(|e| AgentLoopError::Io(e.to_string()))?;

        self.rollout = Some(recorder);
        Ok(self)
    }

    /// Disable streaming (use blocking chat instead)
    pub fn with_streaming(mut self, enabled: bool) -> Self {
        self.streaming_enabled = enabled;
        self
    }

    /// Enable interactive mode with input and approval channels
    ///
    /// Returns the input sender and approval receiver for the TUI to use.
    pub fn with_interactive(mut self) -> (Self, mpsc::Sender<String>, ApprovalReceiver) {
        // Create input channel (user prompts)
        let (input_tx, input_rx) = mpsc::channel(10);
        self.input_rx = Some(input_rx);

        // Create approval channel
        let (approval_tx, approval_rx) = approval_channel(10);
        self.approval_tx = Some(approval_tx);

        (self, input_tx, approval_rx)
    }

    /// Run in interactive mode, waiting for user input from channel
    ///
    /// This method loops forever, waiting for user prompts and processing them.
    /// It emits events for the TUI to display and handles approvals via the approval channel.
    pub async fn run_interactive(&mut self) -> Result<(), AgentLoopError> {
        // Take the input receiver
        let mut input_rx = self
            .input_rx
            .take()
            .ok_or_else(|| AgentLoopError::Io("No input channel configured".to_string()))?;

        // Emit that we're ready
        self.state = AgentState::WaitingForUser;
        self.emit_event(ThreadEvent::WaitingForInput {
            prompt: "Ready for input...".to_string(),
        })
        .await;

        // Main interactive loop
        loop {
            // Wait for user input
            let prompt = match input_rx.recv().await {
                Some(p) => p,
                None => {
                    // Channel closed, exit gracefully
                    tracing::info!("Input channel closed, exiting interactive mode");
                    break;
                }
            };

            // Check for quit command
            if prompt.trim().eq_ignore_ascii_case("/quit")
                || prompt.trim().eq_ignore_ascii_case("/exit")
            {
                tracing::info!("Quit command received");
                break;
            }

            // Run a single conversation turn
            match self.run(&prompt).await {
                Ok(result) => {
                    tracing::debug!(
                        "Turn completed: {} turns, success={}",
                        result.turns,
                        result.success
                    );
                }
                Err(AgentLoopError::Cancelled) => {
                    tracing::info!("Agent cancelled");
                    self.emit_event(ThreadEvent::ThreadCancelled).await;
                }
                Err(e) => {
                    tracing::error!("Agent error: {}", e);
                    self.emit_event(ThreadEvent::Error {
                        message: e.to_string(),
                        recoverable: true,
                    })
                    .await;
                }
            }

            // Reset state for next input
            self.state = AgentState::WaitingForUser;
            self.emit_event(ThreadEvent::WaitingForInput {
                prompt: "Ready for input...".to_string(),
            })
            .await;
        }

        Ok(())
    }

    /// Resume from a rollout file
    pub fn resume_from_rollout(
        config: AgentConfig,
        client: Arc<dyn ModelClient>,
        rollout_path: PathBuf,
    ) -> Result<Self, AgentLoopError> {
        // Load items from rollout
        let items =
            RolloutRecorder::load(&rollout_path).map_err(|e| AgentLoopError::Io(e.to_string()))?;

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

        // Open rollout for appending
        let recorder =
            RolloutRecorder::open(rollout_path).map_err(|e| AgentLoopError::Io(e.to_string()))?;
        agent.rollout = Some(recorder);

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

    /// Get the rollout path if recording is enabled
    pub fn rollout_path(&self) -> Option<&PathBuf> {
        self.rollout.as_ref().map(|r| r.path())
    }

    /// Run the agent with the given prompt
    pub async fn run(&mut self, prompt: &str) -> Result<ExecutionResult, AgentLoopError> {
        self.state = AgentState::Thinking;

        // Emit thread started event
        self.emit_event(ThreadEvent::ThreadStarted {
            thread_id: self.session.id.to_string(),
        })
        .await;

        // Add user message
        let user_message = Message::user(prompt);
        self.record_message(user_message.clone());
        self.session
            .context
            .add_message(user_message)
            .map_err(AgentLoopError::Context)?;

        // Main agent loop
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
            self.emit_event(ThreadEvent::TurnStarted { turn_number })
                .await;

            // Get model response (streaming or blocking)
            let response = if self.streaming_enabled {
                self.get_response_streaming().await?
            } else {
                self.session
                    .client
                    .chat(self.session.context.messages(), &[])
                    .await
                    .map_err(AgentLoopError::Provider)?
            };

            // Record usage
            self.session.record_usage(response.usage.clone());
            self.record_turn(turn_number, response.usage.clone());

            // Add assistant message to context
            let assistant_message =
                Message::with_blocks(uira_protocol::Role::Assistant, response.content.clone());
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

                return Ok(ExecutionResult::success(
                    response.text(),
                    self.session.turn,
                    self.session.usage.clone(),
                ));
            }
        }
    }

    /// Get model response with streaming, emitting ContentDelta events
    async fn get_response_streaming(
        &mut self,
    ) -> Result<uira_protocol::ModelResponse, AgentLoopError> {
        let stream = self
            .session
            .client
            .chat_stream(self.session.context.messages(), &[])
            .await
            .map_err(AgentLoopError::Provider)?;

        let mut controller = StreamController::new();
        let mut stream = std::pin::pin!(stream);

        while let Some(result) = stream.next().await {
            let chunk = result.map_err(AgentLoopError::Provider)?;
            let new_lines = controller.push(chunk);

            // Emit each committed line as ContentDelta
            for line in new_lines {
                self.emit_event(ThreadEvent::ContentDelta {
                    delta: format!("{}\n", line),
                })
                .await;
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
                let response = if self.streaming_enabled {
                    self.get_response_streaming().await?
                } else {
                    self.session
                        .client
                        .chat(self.session.context.messages(), &[])
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
        let user_message = Message::user(prompt);
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
    pub fn result(&self) -> Option<ExecutionResult> {
        match self.state {
            AgentState::Complete => {
                // Get the last assistant message text
                let last_text = self
                    .session
                    .context
                    .messages()
                    .iter()
                    .rev()
                    .find(|m| m.role == Role::Assistant)
                    .map(|m| match &m.content {
                        uira_protocol::MessageContent::Text(s) => s.clone(),
                        uira_protocol::MessageContent::Blocks(blocks) => blocks
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
                        uira_protocol::MessageContent::ToolCalls(_) => String::new(),
                    })
                    .unwrap_or_default();

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
            let _ = sender.send(event).await;
        }
    }

    /// Record a message to the rollout
    fn record_message(&mut self, message: Message) {
        if let Some(ref mut rollout) = self.rollout {
            if let Err(e) = rollout.record_message(message) {
                tracing::warn!("Failed to record message to rollout: {}", e);
            }
        }
    }

    /// Record a tool call to the rollout
    fn record_tool_call(&mut self, id: &str, name: &str, input: &serde_json::Value) {
        if let Some(ref mut rollout) = self.rollout {
            if let Err(e) = rollout.record_tool_call(id, name, input.clone()) {
                tracing::warn!("Failed to record tool call to rollout: {}", e);
            }
        }
    }

    /// Record a tool result to the rollout
    fn record_tool_result(&mut self, id: &str, output: &str, is_error: bool) {
        if let Some(ref mut rollout) = self.rollout {
            if let Err(e) = rollout.record_tool_result(id, output, is_error) {
                tracing::warn!("Failed to record tool result to rollout: {}", e);
            }
        }
    }

    /// Record turn context to the rollout
    fn record_turn(&mut self, turn: usize, usage: uira_protocol::TokenUsage) {
        if let Some(ref mut rollout) = self.rollout {
            if let Err(e) = rollout.record_turn(turn, usage) {
                tracing::warn!("Failed to record turn to rollout: {}", e);
            }
        }
    }

    /// Record a thread event to the rollout
    fn record_event(&mut self, event: ThreadEvent) {
        if let Some(ref mut rollout) = self.rollout {
            if let Err(e) = rollout.record_event(event) {
                tracing::warn!("Failed to record event to rollout: {}", e);
            }
        }
    }

    /// Execute tool calls and return results as content blocks
    ///
    /// This method handles approval flow at the Agent level:
    /// 1. Check cancellation between tools
    /// 2. Check approval requirement (unless full_auto mode)
    /// 3. Request approval via the approval channel if needed
    /// 4. Execute with skip_approval since we handled it here
    async fn execute_tool_calls(
        &mut self,
        tool_calls: &[ToolCall],
    ) -> Result<Vec<ContentBlock>, AgentLoopError> {
        let mut results = Vec::new();
        let ctx = self.session.tool_context();

        for call in tool_calls {
            // 1. Check for cancellation between tools
            if self.control.is_cancelled() {
                return Err(AgentLoopError::Cancelled);
            }

            // 2. Handle approval at Agent level (unless full_auto)
            if !ctx.full_auto {
                if let Some(tool) = self.session.orchestrator.router().get(&call.name) {
                    let requirement = tool.approval_requirement(&call.input);

                    match requirement {
                        ApprovalRequirement::NeedsApproval { reason } => {
                            if let Some(ref approval_tx) = self.approval_tx {
                                // Emit approval request event for TUI display
                                self.emit_event(ThreadEvent::ItemStarted {
                                    item: Item::ApprovalRequest {
                                        id: call.id.clone(),
                                        tool_name: call.name.clone(),
                                        input: call.input.clone(),
                                        reason: reason.clone(),
                                    },
                                })
                                .await;

                                // Request approval with timeout via the Agent's channel
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

                                // Emit approval decision event
                                self.emit_event(ThreadEvent::ItemCompleted {
                                    item: Item::ApprovalDecision {
                                        request_id: call.id.clone(),
                                        approved: decision.is_approved(),
                                    },
                                })
                                .await;

                                // If denied, add error result and continue to next tool
                                if decision.is_denied() {
                                    let deny_reason =
                                        if let uira_protocol::ReviewDecision::Deny { reason } =
                                            &decision
                                        {
                                            reason.clone().unwrap_or_default()
                                        } else {
                                            String::new()
                                        };

                                    let error_msg =
                                        format!("Tool execution denied: {}", deny_reason);
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
                        }
                        ApprovalRequirement::Forbidden { reason } => {
                            return Err(AgentLoopError::ToolForbidden {
                                tool: call.name.clone(),
                                reason,
                            });
                        }
                        ApprovalRequirement::Skip { .. } => {
                            // No approval needed
                        }
                    }
                }
            }

            // Record and emit tool start
            self.record_tool_call(&call.id, &call.name, &call.input);
            self.emit_event(ThreadEvent::ItemStarted {
                item: Item::ToolCall {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    input: call.input.clone(),
                },
            })
            .await;

            // 3. Execute the tool with skip_approval since we handled it above
            let result = self
                .session
                .orchestrator
                .run_with_options(
                    &call.name,
                    call.input.clone(),
                    &ctx,
                    RunOptions::skip_approval(),
                )
                .await;

            match result {
                Ok(output) => {
                    let content = output.as_text().unwrap_or("").to_string();
                    results.push(ContentBlock::tool_result(&call.id, &content));

                    // Record and emit tool result
                    self.record_tool_result(&call.id, &content, false);
                    self.emit_event(ThreadEvent::ItemCompleted {
                        item: Item::ToolResult {
                            tool_call_id: call.id.clone(),
                            output: content,
                            is_error: false,
                        },
                    })
                    .await;
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    results.push(ContentBlock::tool_error(&call.id, &error_msg));

                    // Record and emit tool error
                    self.record_tool_result(&call.id, &error_msg, true);
                    self.emit_event(ThreadEvent::ItemCompleted {
                        item: Item::ToolResult {
                            tool_call_id: call.id.clone(),
                            output: error_msg,
                            is_error: true,
                        },
                    })
                    .await;
                }
            }
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    // Tests would require mocking the model client
}
