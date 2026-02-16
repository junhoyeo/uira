//! TUI events

use uira_core::{ThreadEvent, TodoItem};

#[derive(Debug)]
pub enum AppEvent {
    Agent(ThreadEvent),
    TracingLog(String),
    UserInput(String),
    ApprovalRequest(crate::views::ApprovalRequest),
    TodoUpdated(Vec<TodoItem>),
    Info(String),
    BranchChanged(String),
    SessionChanged(String),
    Redraw,
    Quit,
    Error(String),
}
