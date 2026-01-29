//! Uira TUI - Terminal user interface
//!
//! This crate provides a full-screen TUI using ratatui:
//! - Chat display with streaming
//! - Tool approval dialogs
//! - File picker
//! - Syntax highlighting

mod app;
mod events;
mod views;
mod widgets;

pub use app::App;
pub use events::AppEvent;
