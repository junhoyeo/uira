pub mod agent_usage_reminder;
pub mod auto_slash_command;
pub mod autopilot;
pub mod background_notification;
pub mod directory_readme_injector;
pub mod empty_message_sanitizer;
pub mod keyword_detector;
pub mod learner;
pub mod non_interactive_env;
pub mod notepad;
pub mod omc_orchestrator;
pub mod persistent_mode;
pub mod plugin_patterns;
pub mod preemptive_compaction;
pub mod ralph;
pub mod recovery;
pub mod rules_injector;
pub mod think_mode;
pub mod thinking_block_validator;
pub mod todo_continuation;
pub mod ultrapilot;
pub mod ultraqa;
pub mod ultrawork;

pub use agent_usage_reminder::{
    clear_agent_usage_state, load_agent_usage_state, save_agent_usage_state, AgentUsageReminderHook,
    AgentUsageState, AGENT_TOOLS, REMINDER_MESSAGE, TARGET_TOOLS,
};
pub use autopilot::{
    detect_any_signal, detect_signal, expected_signal_for_phase, validate_state,
    validate_transition, AutopilotConfig, AutopilotHook, AutopilotPhase, AutopilotSignal,
    AutopilotState, AUTOPILOT_STATE_FILE,
};
pub use auto_slash_command::{
    AutoSlashCommandHook, AutoSlashCommandHookInput, AutoSlashCommandHookOutput,
    AutoSlashCommandResult, CommandInfo, CommandMetadata, CommandScope, ExecuteResult,
    ParsedSlashCommand,
};
pub use background_notification::{
    check_background_notifications, process_background_notification, BackgroundNotificationHook,
    BackgroundNotificationHookConfig, BackgroundNotificationHookInput,
    BackgroundNotificationHookOutput, BackgroundNotificationManager, BackgroundTask,
    BackgroundTaskStatus, NotificationCheckResult, TaskProgress,
};
pub use directory_readme_injector::{
    clear_injected_paths, get_readmes_for_path, load_injected_paths, save_injected_paths,
    DirectoryReadmeInjectorHook, InjectedPathsData, README_FILENAME, TRACKED_TOOLS,
};
pub use empty_message_sanitizer::{
    error_patterns, EmptyMessageSanitizerConfig, EmptyMessageSanitizerHook,
    EmptyMessageSanitizerInput, EmptyMessageSanitizerOutput, ErrorPatterns,
    MessageInfo as EmptyMessageInfo, MessagePart as EmptyMessagePart,
    MessageWithParts as EmptyMessageWithParts, DEBUG_PREFIX as EMPTY_MESSAGE_DEBUG_PREFIX,
    HOOK_NAME as EMPTY_MESSAGE_HOOK_NAME, PLACEHOLDER_TEXT as EMPTY_MESSAGE_PLACEHOLDER_TEXT,
};
pub use keyword_detector::{KeywordDetectorHook, KeywordType};
pub use learner::{
    clear_detection_state, clear_loader_cache, create_content_hash,
    detect_extractable_moment, find_matching_skills, find_skill_files, generate_extraction_prompt,
    generate_skill_frontmatter, get_detection_stats, get_last_detection, get_promotion_candidates,
    get_skills_dir, is_learner_enabled, list_promotable_learnings, load_all_skills,
    load_all_skills_cached, load_config, load_skill_by_id, process_response_for_detection,
    promote_learning, save_config, validate_extraction_request, validate_skill_metadata, write_skill,
    DetectionConfig, DetectionResult, LearnerConfig, LearnerHook, LearnedSkill, PatternType,
    PromotionCandidate, QualityValidation, SkillExtractionRequest, SkillFileCandidate, SkillMetadata,
    SkillScope, SkillSource, SkillInjectionResult, MAX_SKILL_CONTENT_LENGTH,
};
pub use non_interactive_env::{
    is_non_interactive, BeforeCommandResult, NonInteractiveEnvConfig, NonInteractiveEnvHook,
    PatternGroup, ShellCommandPatterns, Workarounds, HOOK_NAME as NON_INTERACTIVE_ENV_HOOK_NAME,
    NON_INTERACTIVE_ENV, SHELL_COMMAND_PATTERNS,
};
pub use notepad::{
    NotepadConfig, NotepadHook, NotepadStats, PriorityContextResult, PruneResult,
    DEFAULT_NOTEPAD_CONFIG, MANUAL_HEADER, NOTEPAD_FILENAME, PRIORITY_HEADER,
    WORKING_MEMORY_HEADER,
};
pub use omc_orchestrator::{OmcOrchestratorHook, HOOK_NAME as OMC_ORCHESTRATOR_HOOK_NAME};
pub use persistent_mode::{
    check_persistent_modes, reset_todo_continuation_attempts, PersistentMode, PersistentModeHook,
    PersistentModeMetadata, PersistentModeResult,
};
pub use plugin_patterns::{
    format_file, get_auto_format_message, get_formatter, get_linter, get_pre_commit_reminder_message,
    lint_file, run_pre_commit_checks, run_tests, run_type_check, validate_commit_message,
    CommitConfig, CommitConfigOverrides, CommitValidationResult, FormatConfig, LintConfig,
    PreCommitCheck, PreCommitResult, ToolRunResult,
};
pub use preemptive_compaction::{
    analyze_context_usage, claude_default_context_limit, estimate_tokens, get_session_token_estimate,
    reset_session_token_estimate, CompactionAction, ContextUsageResult, PreemptiveCompactionConfig,
    PreemptiveCompactionHook, CHARS_PER_TOKEN, COMPACTION_COOLDOWN_MS, COMPACTION_SUCCESS_MESSAGE,
    CONTEXT_CRITICAL_MESSAGE, CONTEXT_WARNING_MESSAGE, CRITICAL_THRESHOLD, DEFAULT_THRESHOLD,
    MAX_WARNINGS, MIN_TOKENS_FOR_COMPACTION,
};
pub use ralph::{RalphHook, RalphOptions, RalphState};
pub use recovery::{
    detect_context_limit_error, detect_edit_error, detect_recoverable_error,
    handle_context_window_recovery, handle_edit_error_recovery, handle_recovery,
    handle_session_recovery, parse_token_limit_error, process_edit_output, RecoveryConfig,
    RecoveryErrorType, RecoveryHook, RecoveryInput, RecoveryResult,
    CONTEXT_LIMIT_RECOVERY_MESSAGE, CONTEXT_LIMIT_SHORT_MESSAGE, EDIT_ERROR_PATTERNS,
    EDIT_ERROR_REMINDER, NON_EMPTY_CONTENT_RECOVERY_MESSAGE, PLACEHOLDER_TEXT,
    RECOVERY_FAILED_MESSAGE, TRUNCATION_APPLIED_MESSAGE,
};
pub use rules_injector::{RuleFileCandidate, RuleMetadata, RuleToInject, RulesInjectorHook};
pub use think_mode::{ThinkModeHook, ThinkModeState, ThinkingConfig, THINKING_CONFIGS};
pub use thinking_block_validator::{
    get_validation_stats, is_extended_thinking_model, prepend_thinking_block, validate_message,
    validate_messages, MessageInfo as ThinkingMessageInfo, MessagePart as ThinkingMessagePart,
    MessageWithParts as ThinkingMessageWithParts, ThinkingBlockValidatorHook, ValidationResult,
    ValidationStats, CONTENT_PART_TYPES, DEFAULT_THINKING_CONTENT, HOOK_NAME as THINKING_HOOK_NAME,
    PREVENTED_ERROR, SYNTHETIC_THINKING_ID_PREFIX, THINKING_MODEL_PATTERNS, THINKING_PART_TYPES,
};
pub use todo_continuation::{
    IncompleteTodosResult, StopContext, Todo, TodoContinuationHook, TodoStatus,
    TODO_CONTINUATION_PROMPT,
};
pub use ultrapilot::{
    FileOwnership, IntegrationResult, UltrapilotConfig, UltrapilotHook, UltrapilotState,
    WorkerState, WorkerStatus,
};
pub use ultraqa::{UltraQAExitReason, UltraQAGoalType, UltraQAHook, UltraQAResult, UltraQAState};
pub use ultrawork::{UltraworkHook, UltraworkState};
