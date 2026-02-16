pub mod config;
pub mod events;
pub mod protocol;

pub use config::*;
pub use events::*;

// Selective re-exports from protocol to avoid collisions with:
// - FileChangeType (exists in events::*)
// - HookCommand (exists in config::*)
// Consumers needing protocol-specific versions can use uira_core::protocol::*

// From protocol/events.rs (excluding FileChangeType)
pub use protocol::{
    AgentError, AgentState, ExecutionResult, Item, Progress, ThreadEvent,
};

// From protocol/messages.rs
pub use protocol::{
    ContentBlock, ContentDelta, ImageSource, Message, MessageContent, MessageDelta, ModelResponse,
    Role, StreamChunk, StreamError, StreamMessageStart, ToolCall,
};

// From protocol/tools.rs
pub use protocol::{
    ApprovalRequest, ApprovalRequirement, CacheControl, JsonSchema, ReviewDecision,
    SandboxPreference, SuggestedAction, ToolOutput, ToolOutputContent, ToolResult, ToolSpec,
};

// From protocol/types.rs
pub use protocol::{
    MessageId, ModelTier, Provider, SessionId, StopReason, ThreadId, TodoItem, TodoPriority,
    TodoStatus, TokenUsage, WorkspaceConfig, TODO_CONTINUATION_PROMPT,
};

// From protocol/primitives (excluding HookCommand, HookMatcher, OnFail which collide or depend on HookCommand)
pub use protocol::{
    atomic_write, atomic_write_secure, HookContext, HookEvent, HookEventParseError, HookOutput,
    HookResult, PermissionDecision, PermissionMode, PostToolUseInput, PreCompactInput,
    PreToolUseInput, SessionInfo, StopInput, ToolResponse, UserPromptSubmitInput,
};
