pub mod keyword_detector;
pub mod notepad;
pub mod persistent_mode;
pub mod ralph;
pub mod think_mode;
pub mod todo_continuation;
pub mod ultraqa;
pub mod ultrawork;

pub use keyword_detector::{KeywordDetectorHook, KeywordType};
pub use notepad::{
    NotepadConfig, NotepadHook, NotepadStats, PriorityContextResult, PruneResult,
    DEFAULT_NOTEPAD_CONFIG, MANUAL_HEADER, NOTEPAD_FILENAME, PRIORITY_HEADER,
    WORKING_MEMORY_HEADER,
};
pub use persistent_mode::{
    check_persistent_modes, reset_todo_continuation_attempts, PersistentMode, PersistentModeHook,
    PersistentModeMetadata, PersistentModeResult,
};
pub use ralph::{RalphHook, RalphOptions, RalphState};
pub use think_mode::{
    ThinkModeHook, ThinkModeState, ThinkingConfig, THINKING_CONFIGS,
};
pub use todo_continuation::{
    IncompleteTodosResult, StopContext, Todo, TodoContinuationHook, TodoStatus,
    TODO_CONTINUATION_PROMPT,
};
pub use ultraqa::{UltraQAExitReason, UltraQAGoalType, UltraQAHook, UltraQAResult, UltraQAState};
pub use ultrawork::{UltraworkHook, UltraworkState};
