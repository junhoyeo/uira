//! Todo Continuation Enforcer
//!
//! Ported from oh-my-opencode's `src/hooks/todo-continuation-enforcer.ts`.
//!
//! When the agent goes idle after completing a run, this module checks if there are
//! incomplete todos. If so, it generates a continuation prompt that auto-injects
//! into the agent loop to keep it working until all todos are done.

use uira_hooks::hooks::todo_continuation::{TodoContinuationHook, TODO_CONTINUATION_PROMPT};
use uira_protocol::TodoItem;

const DEFAULT_MAX_ATTEMPTS: usize = 10;

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

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn with_max_attempts(mut self, max: usize) -> Self {
        self.max_continuation_attempts = max;
        self
    }

    pub fn with_skip_on_error(mut self, skip: bool) -> Self {
        self.skip_on_error = skip;
        self
    }

    pub fn with_skip_on_cancel(mut self, skip: bool) -> Self {
        self.skip_on_cancel = skip;
        self
    }

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

        let result = TodoContinuationHook::check_incomplete_from_items(todos, None);

        if result.count == 0 {
            tracing::debug!("todo_continuation: all todos complete");
            return None;
        }

        self.current_attempts += 1;
        let completed = result.total - result.count;

        tracing::info!(
            "todo_continuation: injecting continuation (attempt {}/{}, {}/{} completed, {} remaining)",
            self.current_attempts,
            self.max_continuation_attempts,
            completed,
            result.total,
            result.count,
        );

        Some(format!(
            "{}\n\n[Status: {}/{} completed, {} remaining]",
            TODO_CONTINUATION_PROMPT, completed, result.total, result.count
        ))
    }

    pub fn reset(&mut self) {
        self.current_attempts = 0;
    }

    pub fn attempts(&self) -> usize {
        self.current_attempts
    }

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
