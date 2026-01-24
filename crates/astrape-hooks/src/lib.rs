pub mod hook;
pub mod hooks;
pub mod registry;
pub mod types;

pub use hook::{Hook, HookContext, HookResult};
pub use hooks::{
    keyword_detector::{KeywordDetectorHook, KeywordType},
    persistent_mode::{
        check_persistent_modes, reset_todo_continuation_attempts, PersistentMode,
        PersistentModeHook, PersistentModeMetadata, PersistentModeResult,
    },
    ralph::{RalphHook, RalphOptions, RalphState},
    todo_continuation::{
        IncompleteTodosResult, StopContext, Todo, TodoContinuationHook, TodoStatus,
        TODO_CONTINUATION_PROMPT,
    },
    ultrawork::{UltraworkHook, UltraworkState},
};
pub use registry::HookRegistry;
pub use types::{HookEvent, HookInput, HookOutput, HookType};
