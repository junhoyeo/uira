//! TUI events

use uira_types::{ThreadEvent, TodoItem};

#[derive(Debug)]
pub enum AppEvent {
    Agent(ThreadEvent),
    TracingLog(String),
    UserInput(String),
    ApprovalRequest(crate::views::ApprovalRequest),
    TodoUpdated(Vec<TodoItem>),
    Info(String),
    BranchChanged(String),
    Redraw,
    Quit,
    Error(String),
}
