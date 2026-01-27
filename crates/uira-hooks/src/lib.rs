pub mod hook;
pub mod hooks;
pub mod registry;
pub mod types;

pub use hook::{Hook, HookContext, HookResult};
pub use hooks::{
    agent_usage_reminder::{
        clear_agent_usage_state, load_agent_usage_state, save_agent_usage_state,
        AgentUsageReminderHook, AgentUsageState, AGENT_TOOLS, REMINDER_MESSAGE, TARGET_TOOLS,
    },
    autopilot::{
        AutopilotConfig, AutopilotHook, AutopilotPhase, AutopilotSignal, AutopilotState,
        AUTOPILOT_STATE_FILE,
    },
    background_notification::{
        background_tasks_dir, check_background_notifications, handle_background_event_public,
        process_background_notification, BackgroundNotificationHook,
        BackgroundNotificationHookConfig, BackgroundNotificationHookInput,
        BackgroundNotificationHookOutput, BackgroundNotificationManager, BackgroundTask,
        BackgroundTaskStatus, NotificationCheckResult, TaskProgress, MANAGER,
    },
    comment_checker::{CommentCheckerHook, HOOK_NAME as COMMENT_CHECKER_HOOK_NAME},
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
    ralph::{CompletionSignal, CompletionSignals, RalphHook, RalphOptions, RalphState, SignalType},
    think_mode::{ThinkModeHook, ThinkModeState, ThinkingConfig, THINKING_CONFIGS},
    todo_continuation::{
        IncompleteTodosResult, StopContext, Todo, TodoContinuationHook, TodoStatus,
        TODO_CONTINUATION_PROMPT,
    },
    ultrapilot::{
        FileOwnership, IntegrationResult, UltrapilotConfig, UltrapilotHook, UltrapilotState,
        WorkerState, WorkerStatus,
    },
    ultraqa::{UltraQAExitReason, UltraQAGoalType, UltraQAHook, UltraQAResult, UltraQAState},
    ultrawork::{UltraworkHook, UltraworkState},
};
pub use registry::{default_hooks, HookRegistry};
pub use types::{HookEvent, HookInput, HookOutput, HookType};

pub use uira_goals::{GoalCheckResult, GoalRunner, VerificationResult, VerifyOptions};
