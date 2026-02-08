//! TUI events

use uira_protocol::{ThreadEvent, TodoItem};

#[derive(Debug)]
pub enum AppEvent {
    Agent(ThreadEvent),
    UserInput(String),
    ApprovalRequest(crate::views::ApprovalRequest),
    TodoUpdated(Vec<TodoItem>),
    Info(String),
    BranchChanged(String),
    Redraw,
    Quit,
    Error(String),
}
