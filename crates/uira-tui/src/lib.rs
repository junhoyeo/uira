//! Uira TUI - Terminal user interface
//!
//! This crate provides a full-screen TUI using ratatui:
//! - Chat display with streaming
//! - Tool approval dialogs
//! - File picker
//! - Syntax highlighting

mod app;
mod events;
mod theme;
pub mod views;
mod widgets;

pub use app::App;
pub use events::AppEvent;
pub use theme::{Theme, ThemeOverrides};
pub use views::{ApprovalOverlay, ApprovalRequest, ApprovalView};
