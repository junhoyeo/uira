//! Persistent Mode Hook
//!
//! Unified handler for persistent work modes: ultrawork, ralph, and todo-continuation.
//! This hook intercepts Stop events and enforces work continuation based on:
//! 1. Active ralph with incomplete promise
//! 2. Active ultrawork mode with pending todos
//! 3. Any pending todos (general enforcement)
//!
//! Priority order: Ralph > Ultrawork > Todo Continuation

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::RwLock;

use crate::hook::{Hook, HookContext, HookResult};
use crate::hooks::todo_continuation::{
    StopContext, TodoContinuationHook, TODO_CONTINUATION_PROMPT,
};
use crate::hooks::ultrawork::UltraworkHook;
use crate::types::{HookEvent, HookInput, HookOutput};

const MAX_TODO_CONTINUATION_ATTEMPTS: u32 = 5;

lazy_static::lazy_static! {
    static ref TODO_CONTINUATION_ATTEMPTS: RwLock<HashMap<String, u32>> = RwLock::new(HashMap::new());
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersistentMode {
    Ralph,
    Ultrawork,
    TodoContinuation,
    None,
}

#[derive(Debug, Clone)]
pub struct PersistentModeResult {
    pub should_block: bool,
    pub message: String,
    pub mode: PersistentMode,
    pub metadata: PersistentModeMetadata,
}

#[derive(Debug, Clone, Default)]
pub struct PersistentModeMetadata {
    pub todo_count: Option<usize>,
    pub iteration: Option<u32>,
    pub max_iterations: Option<u32>,
    pub reinforcement_count: Option<u32>,
    pub todo_continuation_attempts: Option<u32>,
}

fn track_todo_continuation_attempt(session_id: &str) -> u32 {
    let mut attempts = TODO_CONTINUATION_ATTEMPTS.write().unwrap();
    let current = attempts.get(session_id).copied().unwrap_or(0);
    let next = current + 1;
    attempts.insert(session_id.to_string(), next);
    next
}

pub fn reset_todo_continuation_attempts(session_id: &str) {
    let mut attempts = TODO_CONTINUATION_ATTEMPTS.write().unwrap();
    attempts.remove(session_id);
}
fn check_ultrawork(
    session_id: Option<&str>,
    directory: &str,
    has_incomplete_todos: bool,
) -> Option<PersistentModeResult> {
    let state = UltraworkHook::read_state(Some(directory))?;

    if !state.active {
        return None;
    }

    if let (Some(state_sid), Some(sid)) = (&state.session_id, session_id) {
        if state_sid != sid {
            return None;
        }
    }

    if !has_incomplete_todos {
        UltraworkHook::deactivate(Some(directory));
        return Some(PersistentModeResult {
            should_block: false,
            message:
                "[ULTRAWORK COMPLETE] All tasks finished. Ultrawork mode deactivated. Well done!"
                    .to_string(),
            mode: PersistentMode::None,
            metadata: PersistentModeMetadata::default(),
        });
    }

    let new_state = UltraworkHook::increment_reinforcement(Some(directory))?;
    let message = UltraworkHook::get_persistence_message(&new_state);

    Some(PersistentModeResult {
        should_block: true,
        message,
        mode: PersistentMode::Ultrawork,
        metadata: PersistentModeMetadata {
            reinforcement_count: Some(new_state.reinforcement_count),
            ..Default::default()
        },
    })
}

fn check_todo_continuation(
    session_id: Option<&str>,
    directory: &str,
) -> Option<PersistentModeResult> {
    let result = TodoContinuationHook::check_incomplete_todos(session_id, directory, None);

    if result.count == 0 {
        if let Some(sid) = session_id {
            reset_todo_continuation_attempts(sid);
        }
        return None;
    }

    let attempt_count = session_id.map(track_todo_continuation_attempt).unwrap_or(1);

    if attempt_count > MAX_TODO_CONTINUATION_ATTEMPTS {
        return Some(PersistentModeResult {
            should_block: false,
            message: format!(
                "[TODO CONTINUATION LIMIT] Attempted {} continuations without progress. {} tasks remain incomplete. Consider reviewing the stuck tasks or asking the user for guidance.",
                MAX_TODO_CONTINUATION_ATTEMPTS,
                result.count
            ),
            mode: PersistentMode::None,
            metadata: PersistentModeMetadata {
                todo_count: Some(result.count),
                todo_continuation_attempts: Some(attempt_count),
                ..Default::default()
            },
        });
    }

    let next_todo = TodoContinuationHook::get_next_pending_todo(&result);
    let next_task_info = next_todo
        .map(|t| format!("\n\nNext task: \"{}\" ({})", t.content, t.status))
        .unwrap_or_default();

    let attempt_info = if attempt_count > 1 {
        format!(
            "\n[Continuation attempt {}/{}]",
            attempt_count, MAX_TODO_CONTINUATION_ATTEMPTS
        )
    } else {
        String::new()
    };

    let message = format!(
        "<todo-continuation>\n\n{}\n\n[Status: {} of {} tasks remaining]{}{}\n\n</todo-continuation>\n\n---\n\n",
        TODO_CONTINUATION_PROMPT,
        result.count,
        result.total,
        next_task_info,
        attempt_info
    );

    Some(PersistentModeResult {
        should_block: true,
        message,
        mode: PersistentMode::TodoContinuation,
        metadata: PersistentModeMetadata {
            todo_count: Some(result.count),
            todo_continuation_attempts: Some(attempt_count),
            ..Default::default()
        },
    })
}

pub fn check_persistent_modes(
    session_id: Option<&str>,
    directory: &str,
    stop_context: Option<&StopContext>,
) -> PersistentModeResult {
    if let Some(ctx) = stop_context {
        if ctx.is_user_abort() {
            return PersistentModeResult {
                should_block: false,
                message: String::new(),
                mode: PersistentMode::None,
                metadata: PersistentModeMetadata::default(),
            };
        }
    }

    let todo_result =
        TodoContinuationHook::check_incomplete_todos(session_id, directory, stop_context);
    let has_incomplete_todos = todo_result.count > 0;

    // Ralph is now handled by the RalphHook directly for full goal-based verification
    if let Some(result) = check_ultrawork(session_id, directory, has_incomplete_todos) {
        if result.should_block {
            return result;
        }
    }

    if has_incomplete_todos {
        if let Some(result) = check_todo_continuation(session_id, directory) {
            if result.should_block {
                return result;
            }
        }
    }

    PersistentModeResult {
        should_block: false,
        message: String::new(),
        mode: PersistentMode::None,
        metadata: PersistentModeMetadata::default(),
    }
}

pub struct PersistentModeHook;

impl PersistentModeHook {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PersistentModeHook {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Hook for PersistentModeHook {
    fn name(&self) -> &str {
        "persistent-mode"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::Stop]
    }

    async fn execute(
        &self,
        _event: HookEvent,
        input: &HookInput,
        context: &HookContext,
    ) -> HookResult {
        let stop_context = StopContext {
            stop_reason: input.stop_reason.clone(),
            user_requested: input.user_requested,
        };

        let result = check_persistent_modes(
            input.session_id.as_deref(),
            &context.directory,
            Some(&stop_context),
        );

        if result.should_block {
            Ok(HookOutput::block_with_reason(result.message))
        } else if !result.message.is_empty() {
            Ok(HookOutput::continue_with_message(result.message))
        } else {
            Ok(HookOutput::pass())
        }
    }

    fn priority(&self) -> i32 {
        200
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_persistent_mode_result_default() {
        let result = PersistentModeResult {
            should_block: false,
            message: String::new(),
            mode: PersistentMode::None,
            metadata: PersistentModeMetadata::default(),
        };
        assert!(!result.should_block);
        assert_eq!(result.mode, PersistentMode::None);
    }

    #[test]
    fn test_persistent_mode_metadata_default() {
        let metadata = PersistentModeMetadata::default();
        assert!(metadata.todo_count.is_none());
        assert!(metadata.iteration.is_none());
        assert!(metadata.reinforcement_count.is_none());
    }
}
