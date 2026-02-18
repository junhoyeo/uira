//! TUI views

mod approval;
mod chat_view;
mod command_palette;
pub mod dialog_agent;
pub mod dialog_export;
pub mod dialog_fork_timeline;
pub mod dialog_mcp;
pub mod dialog_message_actions;
pub mod dialog_provider;
pub mod dialog_session_list;
pub mod dialog_session_rename;
pub mod dialog_status;
pub mod dialog_subagent;
pub mod dialog_tag;
pub mod dialog_theme_list;
pub mod dialog_timeline;
mod model_selector;
mod question_prompt;
pub mod session_nav;
mod toast;

pub use approval::{ApprovalOverlay, ApprovalRequest, ApprovalView, INLINE_APPROVAL_HEIGHT};
pub use chat_view::ChatView;
pub use command_palette::{CommandPalette, PaletteAction, PaletteCommand};
pub use model_selector::{ModelSelector, MODEL_GROUPS};
pub use question_prompt::{QuestionOption, QuestionPrompt, QuestionPromptAction};
pub use toast::{Toast, ToastManager, ToastVariant};
