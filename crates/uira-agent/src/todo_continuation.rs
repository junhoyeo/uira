//! Todo Continuation Enforcer
//!
//! Ported from oh-my-opencode's `src/hooks/todo-continuation-enforcer.ts`.
//!
//! When the agent goes idle after completing a run, this module checks if there are
//! incomplete todos. If so, it generates a continuation prompt that auto-injects
//! into the agent loop to keep it working until all todos are done.

use uira_protocol::{TodoItem, TodoStatus};

const CONTINUATION_PROMPT: &str = "[SYSTEM DIRECTIVE: OH-MY-OPENCODE - TODO CONTINUATION]\n\n\
    Incomplete tasks remain in your todo list. Continue working on the next pending task.\n\n\
    - Proceed without asking for permission\n\
    - Mark each task complete when finished\n\
    - Do not stop until all tasks are done";

/// Maximum default auto-continuation attempts before stopping (prevents infinite loops)
const DEFAULT_MAX_ATTEMPTS: usize = 10;

/// Watches for incomplete todos and generates continuation prompts.
///
/// Integrates into the agent's `run_interactive()` loop: after each `run()` completes,
/// the enforcer checks the todo store. If incomplete items remain, it returns a
/// continuation prompt that gets fed back into `run()` instead of waiting for user input.
pub struct TodoContinuationEnforcer {
    enabled: bool,
    max_continuation_attempts: usize,
    current_attempts: usize,
    skip_on_error: bool,
    skip_on_cancel: bool,
}

impl TodoContinuationEnforcer {
    pub fn new() -> Self {
        Self {
            enabled: true,
            max_continuation_attempts: DEFAULT_MAX_ATTEMPTS,
            current_attempts: 0,
            skip_on_error: true,
            skip_on_cancel: true,
        }
    }

    /// Enable or disable the enforcer.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Set the maximum number of auto-continuation attempts before giving up.
    pub fn with_max_attempts(mut self, max: usize) -> Self {
        self.max_continuation_attempts = max;
        self
    }

    /// Whether to skip continuation when the last run ended in error.
    pub fn with_skip_on_error(mut self, skip: bool) -> Self {
        self.skip_on_error = skip;
        self
    }

    /// Whether to skip continuation when the last run was cancelled.
    pub fn with_skip_on_cancel(mut self, skip: bool) -> Self {
        self.skip_on_cancel = skip;
        self
    }

    /// Check if continuation should happen. Returns the prompt to inject if so.
    ///
    /// Call this after each `run()` completes in the interactive loop.
    /// If it returns `Some(prompt)`, feed that prompt back into `run()` instead
    /// of waiting for user input.
    pub fn check_and_generate_prompt(
        &mut self,
        todos: &[TodoItem],
        last_run_was_error: bool,
        last_run_was_cancel: bool,
    ) -> Option<String> {
        if !self.enabled {
            return None;
        }

        if self.skip_on_error && last_run_was_error {
            tracing::debug!("todo_continuation: skipped (last run was error)");
            return None;
        }

        if self.skip_on_cancel && last_run_was_cancel {
            tracing::debug!("todo_continuation: skipped (last run was cancelled)");
            return None;
        }

        if self.current_attempts >= self.max_continuation_attempts {
            tracing::warn!(
                "todo_continuation: max attempts ({}) reached, stopping",
                self.max_continuation_attempts
            );
            return None;
        }

        if todos.is_empty() {
            return None;
        }

        let incomplete = todos
            .iter()
            .filter(|t| t.status != TodoStatus::Completed && t.status != TodoStatus::Cancelled)
            .count();

        if incomplete == 0 {
            tracing::debug!("todo_continuation: all todos complete");
            return None;
        }

        self.current_attempts += 1;
        let total = todos.len();
        let completed = total - incomplete;

        tracing::info!(
            "todo_continuation: injecting continuation (attempt {}/{}, {}/{} completed, {} remaining)",
            self.current_attempts,
            self.max_continuation_attempts,
            completed,
            total,
            incomplete,
        );

        Some(format!(
            "{}\n\n[Status: {}/{} completed, {} remaining]",
            CONTINUATION_PROMPT, completed, total, incomplete
        ))
    }

    /// Reset the attempt counter. Call when the user provides genuine new input.
    pub fn reset(&mut self) {
        self.current_attempts = 0;
    }

    /// Get the current attempt count.
    pub fn attempts(&self) -> usize {
        self.current_attempts
    }

    /// Check if the enforcer is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

impl Default for TodoContinuationEnforcer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uira_protocol::{TodoItem, TodoPriority, TodoStatus};

    fn make_todo(id: &str, content: &str, status: TodoStatus) -> TodoItem {
        TodoItem {
            id: id.to_string(),
            content: content.to_string(),
            status,
            priority: TodoPriority::Medium,
        }
    }

    #[test]
    fn test_continuation_generates_prompt_with_incomplete_todos() {
        let mut enforcer = TodoContinuationEnforcer::new();
        let todos = vec![
            make_todo("1", "Task A", TodoStatus::Completed),
            make_todo("2", "Task B", TodoStatus::Pending),
            make_todo("3", "Task C", TodoStatus::InProgress),
        ];

        let prompt = enforcer.check_and_generate_prompt(&todos, false, false);
        assert!(prompt.is_some());
        let prompt = prompt.unwrap();
        assert!(prompt.contains("TODO CONTINUATION"));
        assert!(prompt.contains("[Status: 1/3 completed, 2 remaining]"));
    }

    #[test]
    fn test_continuation_returns_none_when_all_complete() {
        let mut enforcer = TodoContinuationEnforcer::new();
        let todos = vec![
            make_todo("1", "Task A", TodoStatus::Completed),
            make_todo("2", "Task B", TodoStatus::Completed),
            make_todo("3", "Task C", TodoStatus::Cancelled),
        ];

        let prompt = enforcer.check_and_generate_prompt(&todos, false, false);
        assert!(prompt.is_none());
    }

    #[test]
    fn test_continuation_returns_none_when_empty() {
        let mut enforcer = TodoContinuationEnforcer::new();
        let prompt = enforcer.check_and_generate_prompt(&[], false, false);
        assert!(prompt.is_none());
    }

    #[test]
    fn test_continuation_respects_max_attempts() {
        let mut enforcer = TodoContinuationEnforcer::new().with_max_attempts(2);
        let todos = vec![make_todo("1", "Task A", TodoStatus::Pending)];

        // Attempt 1 - should work
        assert!(enforcer
            .check_and_generate_prompt(&todos, false, false)
            .is_some());
        assert_eq!(enforcer.attempts(), 1);

        // Attempt 2 - should work
        assert!(enforcer
            .check_and_generate_prompt(&todos, false, false)
            .is_some());
        assert_eq!(enforcer.attempts(), 2);

        // Attempt 3 - should be blocked
        assert!(enforcer
            .check_and_generate_prompt(&todos, false, false)
            .is_none());
        assert_eq!(enforcer.attempts(), 2);
    }

    #[test]
    fn test_continuation_skips_on_error() {
        let mut enforcer = TodoContinuationEnforcer::new();
        let todos = vec![make_todo("1", "Task A", TodoStatus::Pending)];

        // With error
        assert!(enforcer
            .check_and_generate_prompt(&todos, true, false)
            .is_none());

        // Without error
        assert!(enforcer
            .check_and_generate_prompt(&todos, false, false)
            .is_some());
    }

    #[test]
    fn test_continuation_skips_on_cancel() {
        let mut enforcer = TodoContinuationEnforcer::new();
        let todos = vec![make_todo("1", "Task A", TodoStatus::Pending)];

        // With cancel
        assert!(enforcer
            .check_and_generate_prompt(&todos, false, true)
            .is_none());

        // Without cancel
        assert!(enforcer
            .check_and_generate_prompt(&todos, false, false)
            .is_some());
    }

    #[test]
    fn test_continuation_skip_on_error_can_be_disabled() {
        let mut enforcer = TodoContinuationEnforcer::new().with_skip_on_error(false);
        let todos = vec![make_todo("1", "Task A", TodoStatus::Pending)];

        // Even with error, should still generate
        assert!(enforcer
            .check_and_generate_prompt(&todos, true, false)
            .is_some());
    }

    #[test]
    fn test_continuation_reset() {
        let mut enforcer = TodoContinuationEnforcer::new().with_max_attempts(1);
        let todos = vec![make_todo("1", "Task A", TodoStatus::Pending)];

        // Use up the attempt
        assert!(enforcer
            .check_and_generate_prompt(&todos, false, false)
            .is_some());
        assert!(enforcer
            .check_and_generate_prompt(&todos, false, false)
            .is_none());

        // Reset
        enforcer.reset();
        assert_eq!(enforcer.attempts(), 0);

        // Should work again
        assert!(enforcer
            .check_and_generate_prompt(&todos, false, false)
            .is_some());
    }

    #[test]
    fn test_continuation_disabled() {
        let mut enforcer = TodoContinuationEnforcer::new().enabled(false);
        let todos = vec![make_todo("1", "Task A", TodoStatus::Pending)];

        assert!(enforcer
            .check_and_generate_prompt(&todos, false, false)
            .is_none());
        assert!(!enforcer.is_enabled());
    }
}
