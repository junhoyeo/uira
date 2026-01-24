pub mod hook;
pub mod hooks;
pub mod registry;
pub mod types;

pub use hook::{Hook, HookContext, HookResult};
pub use hooks::{
    keyword_detector::{KeywordDetectorHook, KeywordType},
    notepad::{
        NotepadConfig, NotepadHook, NotepadStats, PriorityContextResult, PruneResult,
        DEFAULT_NOTEPAD_CONFIG, MANUAL_HEADER, NOTEPAD_FILENAME, PRIORITY_HEADER,
        WORKING_MEMORY_HEADER,
    },
    persistent_mode::{
        check_persistent_modes, reset_todo_continuation_attempts, PersistentMode,
        PersistentModeHook, PersistentModeMetadata, PersistentModeResult,
    },
    ralph::{RalphHook, RalphOptions, RalphState},
    think_mode::{ThinkModeHook, ThinkModeState, ThinkingConfig, THINKING_CONFIGS},
    todo_continuation::{
        IncompleteTodosResult, StopContext, Todo, TodoContinuationHook, TodoStatus,
        TODO_CONTINUATION_PROMPT,
    },
    ultraqa::{UltraQAExitReason, UltraQAGoalType, UltraQAHook, UltraQAResult, UltraQAState},
    ultrawork::{UltraworkHook, UltraworkState},
};
pub use registry::HookRegistry;
pub use types::{HookEvent, HookInput, HookOutput, HookType};
