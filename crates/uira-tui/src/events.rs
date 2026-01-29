//! TUI events

use uira_protocol::ThreadEvent;

/// Application events
#[derive(Debug, Clone)]
pub enum AppEvent {
    /// Agent event from the execution loop
    Agent(ThreadEvent),
    /// User input received
    UserInput(String),
    /// Request redraw
    Redraw,
    /// Quit the application
    Quit,
    /// Error occurred
    Error(String),
}
