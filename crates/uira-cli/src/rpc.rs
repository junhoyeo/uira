use crate::session::list_sessions;
use futures::StreamExt;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot, Mutex};
use uira_agent::{Agent, AgentCommand, AgentConfig, ApprovalReceiver, EventStream};
use uira_orchestration::ModelRegistry;
use uira_providers::ModelClient;
use uira_core::{AgentState, Item, Message, ReviewDecision, ThreadEvent};

const JSONRPC_VERSION: &str = "2.0";
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const SERVER_ERROR: i64 = -32000;

type SharedState = Arc<Mutex<RpcState>>;

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[serde(default)]
    jsonrpc: Option<String>,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcNotification {
    jsonrpc: &'static str,
    method: String,
    params: Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i64,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

#[derive(Clone)]
struct RpcWriter {
    stdout: Arc<Mutex<tokio::io::Stdout>>,
}

impl RpcWriter {
    fn new() -> Self {
        Self {
            stdout: Arc::new(Mutex::new(tokio::io::stdout())),
        }
    }

    async fn send_result(&self, id: Value, result: Value) -> io::Result<()> {
        self.send(JsonRpcResponse {
            jsonrpc: JSONRPC_VERSION,
            id: Some(id),
            result: Some(result),
            error: None,
        })
        .await
    }

    async fn send_error(
        &self,
        id: Option<Value>,
        code: i64,
        message: impl Into<String>,
        data: Option<Value>,
    ) -> io::Result<()> {
        self.send(JsonRpcResponse {
            jsonrpc: JSONRPC_VERSION,
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data,
            }),
        })
        .await
    }

    async fn send_notification(&self, method: impl Into<String>, params: Value) -> io::Result<()> {
        let payload = serde_json::to_string(&JsonRpcNotification {
            jsonrpc: JSONRPC_VERSION,
            method: method.into(),
            params,
        })
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;

        let mut stdout = self.stdout.lock().await;
        stdout.write_all(payload.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await
    }

    async fn send(&self, response: JsonRpcResponse) -> io::Result<()> {
        let payload = serde_json::to_string(&response)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;

        let mut stdout = self.stdout.lock().await;
        stdout.write_all(payload.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await
    }
}

struct PendingApprovalEntry {
    request_id: Option<Value>,
    tool_name: String,
    input: Value,
    reason: String,
    response_tx: oneshot::Sender<ReviewDecision>,
}

struct RpcState {
    input_tx: mpsc::Sender<Message>,
    command_tx: mpsc::Sender<AgentCommand>,
    cancel_signal: Arc<AtomicBool>,
    active_chat_request: Option<Value>,
    session_id: Option<String>,
    agent_state: AgentState,
    pending_approvals: HashMap<String, PendingApprovalEntry>,
}

impl RpcState {
    fn new(
        input_tx: mpsc::Sender<Message>,
        command_tx: mpsc::Sender<AgentCommand>,
        cancel_signal: Arc<AtomicBool>,
        session_id: String,
    ) -> Self {
        Self {
            input_tx,
            command_tx,
            cancel_signal,
            active_chat_request: None,
            session_id: Some(session_id),
            agent_state: AgentState::WaitingForUser,
            pending_approvals: HashMap::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ChatParams {
    message: String,
}

#[derive(Debug, Default, Deserialize)]
struct SessionCreateParams {
    branch_name: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct SessionListParams {
    limit: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
struct ToolDecisionParams {
    request_id: Option<String>,
    reason: Option<String>,
}

pub async fn run_rpc_mode(
    agent_config: AgentConfig,
    client: Arc<dyn ModelClient>,
) -> Result<(), Box<dyn std::error::Error>> {
    let working_directory = agent_config
        .working_directory
        .as_ref()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|| {
            std::env::current_dir()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        });

    let mut event_system = uira_agent::create_event_system(working_directory);
    event_system.start();

    let agent = Agent::new(agent_config, client)
        .with_event_system(&event_system)
        .with_session_recording()?;
    let (agent, event_stream) = agent.with_event_stream();
    let cancel_signal = agent.control().cancel_signal();
    let session_id = agent.session().id.to_string();
    let (mut agent, input_tx, approval_rx, command_tx) = agent.with_interactive();

    let state = Arc::new(Mutex::new(RpcState::new(
        input_tx,
        command_tx,
        cancel_signal,
        session_id,
    )));
    let writer = RpcWriter::new();

    spawn_event_forwarder(event_stream, state.clone(), writer.clone());
    spawn_approval_forwarder(approval_rx, state.clone(), writer.clone());

    tokio::spawn(async move {
        if let Err(error) = agent.run_interactive().await {
            tracing::error!(error = %error, "RPC agent loop terminated");
        }
    });

    let stdin = BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();

    while let Some(line) = lines.next_line().await? {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let request = match serde_json::from_str::<JsonRpcRequest>(line) {
            Ok(request) => request,
            Err(error) => {
                writer
                    .send_error(None, PARSE_ERROR, format!("Parse error: {error}"), None)
                    .await?;
                continue;
            }
        };

        handle_request(request, &state, &writer).await?;
    }

    Ok(())
}

fn spawn_event_forwarder(event_stream: EventStream, state: SharedState, writer: RpcWriter) {
    tokio::spawn(async move {
        let mut stream = event_stream;
        while let Some(event) = stream.next().await {
            if let Err(error) = handle_agent_event(event, &state, &writer).await {
                tracing::error!(error = %error, "failed to send RPC event response");
                break;
            }
        }
    });
}

fn spawn_approval_forwarder(
    mut approval_rx: ApprovalReceiver,
    state: SharedState,
    writer: RpcWriter,
) {
    tokio::spawn(async move {
        while let Some(pending) = approval_rx.recv().await {
            let approval_id = pending.id.clone();
            let tool_name = pending.tool_name.clone();
            let input = pending.input.clone();
            let reason = pending.reason.clone();

            let maybe_request_id = {
                let mut guard = state.lock().await;
                guard.agent_state = AgentState::WaitingForApproval;
                let request_id = guard.active_chat_request.clone();
                guard.pending_approvals.insert(
                    approval_id.clone(),
                    PendingApprovalEntry {
                        request_id: request_id.clone(),
                        tool_name: tool_name.clone(),
                        input: input.clone(),
                        reason: reason.clone(),
                        response_tx: pending.response_tx,
                    },
                );
                request_id
            };

            if let Some(request_id) = maybe_request_id {
                let payload = json!({
                    "type": "approval_required",
                    "chat_request_id": request_id,
                    "request_id": approval_id,
                    "tool": tool_name,
                    "args": input,
                    "reason": reason,
                });

                if let Err(error) = writer.send_notification("chat.event", payload).await {
                    tracing::error!(error = %error, "failed to send approval request to RPC client");
                    break;
                }
            }
        }
    });
}

async fn handle_agent_event(
    event: ThreadEvent,
    state: &SharedState,
    writer: &RpcWriter,
) -> io::Result<()> {
    let mut stream_notification: Option<Value> = None;
    let mut stream_result: Option<(Value, Value)> = None;
    let mut stream_error: Option<(Option<Value>, String)> = None;

    {
        let mut guard = state.lock().await;

        match event {
            ThreadEvent::ThreadStarted { thread_id } => {
                guard.session_id = Some(thread_id);
                guard.agent_state = AgentState::Thinking;
            }
            ThreadEvent::TurnStarted { .. } => {
                guard.agent_state = AgentState::Thinking;
            }
            ThreadEvent::TurnCompleted { .. } => {
                guard.agent_state = AgentState::Thinking;
            }
            ThreadEvent::ContentDelta { delta } => {
                if let Some(request_id) = guard.active_chat_request.clone() {
                    stream_notification = Some(json!({
                        "type": "chunk",
                        "chat_request_id": request_id,
                        "content": delta
                    }));
                }
            }
            ThreadEvent::ThinkingDelta { thinking } => {
                if let Some(request_id) = guard.active_chat_request.clone() {
                    stream_notification = Some(json!({
                        "type": "chunk",
                        "chat_request_id": request_id,
                        "content": thinking,
                        "channel": "thinking",
                    }));
                }
            }
            ThreadEvent::ItemStarted {
                item: Item::ToolCall { name, input, .. },
            } => {
                guard.agent_state = AgentState::ExecutingTool;
                if let Some(request_id) = guard.active_chat_request.clone() {
                    stream_notification = Some(json!({
                        "type": "tool_call",
                        "chat_request_id": request_id,
                        "tool": name,
                        "args": input,
                    }));
                }
            }
            ThreadEvent::ItemStarted {
                item: Item::ApprovalRequest { .. },
            } => {
                guard.agent_state = AgentState::WaitingForApproval;
            }
            ThreadEvent::ItemCompleted {
                item: Item::ApprovalDecision { approved, .. },
            } => {
                guard.agent_state = if approved {
                    AgentState::ExecutingTool
                } else {
                    AgentState::Thinking
                };
            }
            ThreadEvent::ThreadCompleted { usage } => {
                guard.agent_state = AgentState::Complete;
                if let Some(request_id) = guard.active_chat_request.take() {
                    stream_result = Some((request_id, json!({ "type": "done", "usage": usage })));
                }

                clear_pending_approvals(&mut guard, "Chat request completed");
                guard.cancel_signal.store(false, Ordering::SeqCst);
            }
            ThreadEvent::ThreadCancelled => {
                guard.agent_state = AgentState::Cancelled;
                if let Some(request_id) = guard.active_chat_request.take() {
                    stream_result =
                        Some((request_id, json!({ "type": "done", "status": "cancelled" })));
                }

                clear_pending_approvals(&mut guard, "Chat request cancelled");
                guard.cancel_signal.store(false, Ordering::SeqCst);
            }
            ThreadEvent::Error { message, .. } => {
                guard.agent_state = AgentState::Failed;
                let request_id = guard.active_chat_request.take();
                clear_pending_approvals(&mut guard, "Chat request failed");
                guard.cancel_signal.store(false, Ordering::SeqCst);
                stream_error = Some((request_id, message));
            }
            ThreadEvent::WaitingForInput { .. } => {
                if guard.active_chat_request.is_none() {
                    guard.agent_state = AgentState::WaitingForUser;
                }
            }
            _ => {}
        }
    }

    if let Some(payload) = stream_notification {
        writer.send_notification("chat.event", payload).await?;
    }

    if let Some((request_id, payload)) = stream_result {
        writer.send_result(request_id, payload).await?;
    }

    if let Some((request_id, message)) = stream_error {
        writer
            .send_error(request_id, SERVER_ERROR, message, None)
            .await?;
    }

    Ok(())
}

fn clear_pending_approvals(state: &mut RpcState, reason: &str) {
    for (_, pending) in state.pending_approvals.drain() {
        let _ = pending.response_tx.send(ReviewDecision::Deny {
            reason: Some(reason.to_string()),
        });
    }
}

async fn handle_request(
    request: JsonRpcRequest,
    state: &SharedState,
    writer: &RpcWriter,
) -> io::Result<()> {
    if request.jsonrpc.as_deref() != Some(JSONRPC_VERSION) {
        return writer
            .send_error(
                request.id,
                INVALID_REQUEST,
                "Invalid JSON-RPC version",
                None,
            )
            .await;
    }

    let Some(id) = request.id.clone() else {
        return writer
            .send_error(None, INVALID_REQUEST, "Request id is required", None)
            .await;
    };

    match request.method.as_str() {
        "chat" => handle_chat(id, request.params, state, writer).await,
        "cancel" => handle_cancel(id, state, writer).await,
        "status" => handle_status(id, state, writer).await,
        "session.create" => handle_session_create(id, request.params, state, writer).await,
        "session.list" => handle_session_list(id, request.params, state, writer).await,
        "model.list" => handle_model_list(id, writer).await,
        "tool.approve" => handle_tool_decision(id, request.params, state, writer, true).await,
        "tool.reject" => handle_tool_decision(id, request.params, state, writer, false).await,
        _ => {
            writer
                .send_error(
                    Some(id),
                    METHOD_NOT_FOUND,
                    format!("Method not found: {}", request.method),
                    None,
                )
                .await
        }
    }
}

async fn handle_chat(
    id: Value,
    params: Option<Value>,
    state: &SharedState,
    writer: &RpcWriter,
) -> io::Result<()> {
    let params = match parse_params::<ChatParams>(params) {
        Ok(params) => params,
        Err(message) => {
            return writer
                .send_error(Some(id), INVALID_PARAMS, message, None)
                .await;
        }
    };

    let input_tx = {
        let mut guard = state.lock().await;
        if guard.active_chat_request.is_some() {
            None
        } else {
            guard.cancel_signal.store(false, Ordering::SeqCst);
            guard.active_chat_request = Some(id.clone());
            guard.agent_state = AgentState::Thinking;
            Some(guard.input_tx.clone())
        }
    };

    let Some(input_tx) = input_tx else {
        return writer
            .send_error(
                Some(id),
                SERVER_ERROR,
                "Another chat request is already running",
                None,
            )
            .await;
    };

    if let Err(error) = input_tx.send(Message::user_prompt(&params.message)).await {
        let mut guard = state.lock().await;
        guard.active_chat_request = None;
        guard.agent_state = AgentState::Failed;
        return writer
            .send_error(
                Some(id),
                SERVER_ERROR,
                format!("Failed to submit chat request: {error}"),
                None,
            )
            .await;
    }

    Ok(())
}

async fn handle_cancel(id: Value, state: &SharedState, writer: &RpcWriter) -> io::Result<()> {
    let (had_active_request, approvals) = {
        let mut guard = state.lock().await;
        let had_active_request = guard.active_chat_request.is_some();
        if had_active_request {
            guard.cancel_signal.store(true, Ordering::SeqCst);
        }

        let approvals = guard.pending_approvals.drain().collect::<Vec<_>>();
        (had_active_request, approvals)
    };

    for (_, pending) in approvals {
        let _ = pending.response_tx.send(ReviewDecision::Deny {
            reason: Some("Cancelled by RPC request".to_string()),
        });
    }

    writer
        .send_result(
            id,
            json!({
                "cancelled": had_active_request,
            }),
        )
        .await
}

async fn handle_status(id: Value, state: &SharedState, writer: &RpcWriter) -> io::Result<()> {
    let (session_id, agent_state, active_request_id, pending_approvals) = {
        let guard = state.lock().await;
        let pending_approvals = guard
            .pending_approvals
            .iter()
            .map(|(approval_id, approval)| {
                json!({
                    "request_id": approval_id,
                    "chat_request_id": approval.request_id,
                    "tool": approval.tool_name,
                    "args": approval.input,
                    "reason": approval.reason,
                })
            })
            .collect::<Vec<_>>();

        (
            guard.session_id.clone(),
            guard.agent_state,
            guard.active_chat_request.clone(),
            pending_approvals,
        )
    };

    writer
        .send_result(
            id,
            json!({
                "session_id": session_id,
                "agent_state": agent_state,
                "active_request_id": active_request_id,
                "pending_approvals": pending_approvals,
            }),
        )
        .await
}

async fn handle_session_create(
    id: Value,
    params: Option<Value>,
    state: &SharedState,
    writer: &RpcWriter,
) -> io::Result<()> {
    let params = match parse_params::<SessionCreateParams>(params) {
        Ok(params) => params,
        Err(message) => {
            return writer
                .send_error(Some(id), INVALID_PARAMS, message, None)
                .await;
        }
    };

    let command_tx = {
        let guard = state.lock().await;
        if guard.active_chat_request.is_some() {
            None
        } else {
            Some(guard.command_tx.clone())
        }
    };

    let Some(command_tx) = command_tx else {
        return writer
            .send_error(
                Some(id),
                SERVER_ERROR,
                "Cannot create session while a chat request is running",
                None,
            )
            .await;
    };

    let (fork_response_tx, fork_response_rx) = oneshot::channel();
    if let Err(error) = command_tx
        .send(AgentCommand::Fork {
            branch_name: params.branch_name,
            message_count: Some(0),
            response_tx: fork_response_tx,
        })
        .await
    {
        return writer
            .send_error(
                Some(id),
                SERVER_ERROR,
                format!("Failed to create session: {error}"),
                None,
            )
            .await;
    }

    let fork_result = match tokio::time::timeout(Duration::from_secs(5), fork_response_rx).await {
        Ok(Ok(Ok(result))) => result,
        Ok(Ok(Err(message))) => {
            return writer
                .send_error(Some(id), SERVER_ERROR, message, None)
                .await;
        }
        Ok(Err(error)) => {
            return writer
                .send_error(
                    Some(id),
                    SERVER_ERROR,
                    format!("Failed to receive session creation result: {error}"),
                    None,
                )
                .await;
        }
        Err(_) => {
            return writer
                .send_error(
                    Some(id),
                    SERVER_ERROR,
                    "Timed out waiting for session creation",
                    None,
                )
                .await;
        }
    };

    let (switch_response_tx, switch_response_rx) = oneshot::channel();
    if let Err(error) = command_tx
        .send(AgentCommand::SwitchBranch {
            branch_name: fork_result.branch_name.clone(),
            response_tx: switch_response_tx,
        })
        .await
    {
        return writer
            .send_error(
                Some(id),
                SERVER_ERROR,
                format!("Failed to switch to new session branch: {error}"),
                None,
            )
            .await;
    }

    match tokio::time::timeout(Duration::from_secs(5), switch_response_rx).await {
        Ok(Ok(Ok(_result))) => {}
        Ok(Ok(Err(message))) => {
            return writer
                .send_error(Some(id), SERVER_ERROR, message, None)
                .await;
        }
        Ok(Err(error)) => {
            return writer
                .send_error(
                    Some(id),
                    SERVER_ERROR,
                    format!("Failed to receive branch switch result: {error}"),
                    None,
                )
                .await;
        }
        Err(_) => {
            return writer
                .send_error(
                    Some(id),
                    SERVER_ERROR,
                    "Timed out waiting for branch switch",
                    None,
                )
                .await;
        }
    }

    {
        let mut guard = state.lock().await;
        guard.session_id = Some(fork_result.session_id.clone());
        guard.cancel_signal.store(false, Ordering::SeqCst);
    }

    writer
        .send_result(
            id,
            json!({
                "session_id": fork_result.session_id,
                "branch": fork_result.branch_name,
                "parent_branch": fork_result.parent_branch,
            }),
        )
        .await
}

async fn handle_session_list(
    id: Value,
    params: Option<Value>,
    state: &SharedState,
    writer: &RpcWriter,
) -> io::Result<()> {
    let params = match parse_params::<SessionListParams>(params) {
        Ok(params) => params,
        Err(message) => {
            return writer
                .send_error(Some(id), INVALID_PARAMS, message, None)
                .await;
        }
    };

    let limit = params.limit.unwrap_or(20).clamp(1, 1000);
    let sessions = match list_sessions(limit) {
        Ok(entries) => entries
            .into_iter()
            .map(|entry| {
                json!({
                    "session_id": entry.thread_id,
                    "timestamp": entry.timestamp.to_rfc3339(),
                    "provider": entry.provider,
                    "model": entry.model,
                    "turns": entry.turns,
                    "parent_id": entry.parent_id,
                    "fork_count": entry.fork_count,
                    "path": entry.path,
                })
            })
            .collect::<Vec<_>>(),
        Err(error) => {
            return writer
                .send_error(
                    Some(id),
                    SERVER_ERROR,
                    format!("Failed to list sessions: {error}"),
                    None,
                )
                .await;
        }
    };

    let current_session_id = {
        let guard = state.lock().await;
        guard.session_id.clone()
    };

    writer
        .send_result(
            id,
            json!({
                "current_session_id": current_session_id,
                "sessions": sessions,
            }),
        )
        .await
}

async fn handle_model_list(id: Value, writer: &RpcWriter) -> io::Result<()> {
    let registry = ModelRegistry::new();

    let mut providers = ["anthropic", "openai", "opencode"]
        .iter()
        .filter_map(|provider| {
            registry.get_provider(provider).map(|models| {
                json!({
                    "provider": provider,
                    "models": {
                        "opus": models.opus,
                        "sonnet": models.sonnet,
                        "haiku": models.haiku,
                    }
                })
            })
        })
        .collect::<Vec<_>>();

    providers.push(json!({
        "provider": "google",
        "models": {
            "default": "gemini-1.5-pro"
        }
    }));

    providers.push(json!({
        "provider": "ollama",
        "models": {
            "default": "llama3.1"
        }
    }));

    writer
        .send_result(
            id,
            json!({
                "providers": providers,
            }),
        )
        .await
}

async fn handle_tool_decision(
    id: Value,
    params: Option<Value>,
    state: &SharedState,
    writer: &RpcWriter,
    approved: bool,
) -> io::Result<()> {
    let params = match parse_params::<ToolDecisionParams>(params) {
        Ok(params) => params,
        Err(message) => {
            return writer
                .send_error(Some(id), INVALID_PARAMS, message, None)
                .await;
        }
    };

    enum LookupError {
        InvalidParams(String),
        Server(String),
    }

    let lookup = {
        let mut guard = state.lock().await;

        match pick_approval_id(params.request_id.as_deref(), &guard.pending_approvals) {
            Ok(approval_id) => match guard.pending_approvals.remove(&approval_id) {
                Some(pending) => {
                    if guard.pending_approvals.is_empty() && guard.active_chat_request.is_some() {
                        guard.agent_state = AgentState::Thinking;
                    }

                    Ok((approval_id, pending))
                }
                None => Err(LookupError::Server(
                    "Pending approval disappeared before decision was applied".to_string(),
                )),
            },
            Err(message) => Err(LookupError::InvalidParams(message)),
        }
    };

    let (approval_id, pending) = match lookup {
        Ok(result) => result,
        Err(LookupError::InvalidParams(message)) => {
            return writer
                .send_error(Some(id), INVALID_PARAMS, message, None)
                .await;
        }
        Err(LookupError::Server(message)) => {
            return writer
                .send_error(Some(id), SERVER_ERROR, message, None)
                .await;
        }
    };

    let decision = if approved {
        ReviewDecision::Approve
    } else {
        ReviewDecision::Deny {
            reason: params.reason,
        }
    };

    if pending.response_tx.send(decision).is_err() {
        return writer
            .send_error(
                Some(id),
                SERVER_ERROR,
                "Failed to deliver approval decision",
                None,
            )
            .await;
    }

    writer
        .send_result(
            id,
            json!({
                "request_id": approval_id,
                "decision": if approved { "approved" } else { "rejected" },
            }),
        )
        .await
}

fn pick_approval_id(
    requested_id: Option<&str>,
    pending: &HashMap<String, PendingApprovalEntry>,
) -> Result<String, String> {
    if let Some(request_id) = requested_id {
        if pending.contains_key(request_id) {
            return Ok(request_id.to_string());
        }

        return Err(format!(
            "No pending approval found for request_id '{request_id}'"
        ));
    }

    match pending.len() {
        0 => Err("No pending approvals available".to_string()),
        1 => pending
            .keys()
            .next()
            .cloned()
            .ok_or_else(|| "No pending approvals available".to_string()),
        _ => Err("Multiple pending approvals exist. Provide request_id.".to_string()),
    }
}

fn parse_params<T: DeserializeOwned>(params: Option<Value>) -> Result<T, String> {
    let value = params.unwrap_or_else(|| json!({}));
    serde_json::from_value(value).map_err(|error| format!("Invalid params: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_approval_id_requires_explicit_id_when_multiple_pending() {
        let mut pending = HashMap::new();
        let (tx_a, _rx_a) = oneshot::channel();
        let (tx_b, _rx_b) = oneshot::channel();

        pending.insert(
            "a".to_string(),
            PendingApprovalEntry {
                request_id: None,
                tool_name: "bash".to_string(),
                input: json!({}),
                reason: "reason-a".to_string(),
                response_tx: tx_a,
            },
        );
        pending.insert(
            "b".to_string(),
            PendingApprovalEntry {
                request_id: None,
                tool_name: "edit".to_string(),
                input: json!({}),
                reason: "reason-b".to_string(),
                response_tx: tx_b,
            },
        );

        let error = pick_approval_id(None, &pending).unwrap_err();
        assert!(error.contains("Multiple pending approvals"));
    }

    #[test]
    fn pick_approval_id_uses_single_pending_entry() {
        let mut pending = HashMap::new();
        let (tx, _rx) = oneshot::channel();

        pending.insert(
            "only".to_string(),
            PendingApprovalEntry {
                request_id: None,
                tool_name: "bash".to_string(),
                input: json!({}),
                reason: "reason".to_string(),
                response_tx: tx,
            },
        );

        assert_eq!(pick_approval_id(None, &pending).unwrap(), "only");
    }
}
