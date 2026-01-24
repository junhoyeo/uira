//! Todo Continuation Enforcer Hook
//!
//! Prevents stopping when incomplete tasks remain in the todo list.
//! Forces the agent to continue until all tasks are marked complete.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::hook::{Hook, HookContext, HookResult};
use crate::types::{HookEvent, HookInput, HookOutput};

/// Todo item structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Todo {
    pub content: String,
    pub status: TodoStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

/// Todo status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
    Cancelled,
}

impl Todo {
    pub fn is_incomplete(&self) -> bool {
        matches!(self.status, TodoStatus::Pending | TodoStatus::InProgress)
    }
}

/// Result of checking for incomplete todos
#[derive(Debug, Clone)]
pub struct IncompleteTodosResult {
    /// Number of incomplete todos
    pub count: usize,
    /// The incomplete todos
    pub todos: Vec<Todo>,
    /// Total number of todos
    pub total: usize,
}

/// Stop context from hook event
#[derive(Debug, Clone, Default)]
pub struct StopContext {
    pub stop_reason: Option<String>,
    pub user_requested: Option<bool>,
}

impl StopContext {
    /// Check if stop was due to user abort
    pub fn is_user_abort(&self) -> bool {
        if self.user_requested == Some(true) {
            return true;
        }

        let abort_patterns = [
            "user_cancel",
            "user_interrupt",
            "ctrl_c",
            "manual_stop",
            "aborted",
            "abort",
            "cancel",
            "interrupt",
        ];

        if let Some(reason) = &self.stop_reason {
            let reason_lower = reason.to_lowercase();
            return abort_patterns
                .iter()
                .any(|pattern| reason_lower.contains(pattern));
        }

        false
    }
}

/// Todo Continuation Hook
pub struct TodoContinuationHook {
    max_attempts: usize,
}

impl TodoContinuationHook {
    pub fn new() -> Self {
        Self { max_attempts: 5 }
    }

    pub fn with_max_attempts(mut self, max: usize) -> Self {
        self.max_attempts = max;
        self
    }

    /// Get possible todo file locations
    fn get_todo_file_paths(session_id: Option<&str>, directory: &str) -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // Get home directory
        if let Some(home) = dirs::home_dir() {
            let claude_dir = home.join(".claude");

            // Session-specific todos
            if let Some(sid) = session_id {
                paths.push(claude_dir.join("sessions").join(sid).join("todos.json"));
                paths.push(claude_dir.join("todos").join(format!("{}.json", sid)));
            }

            // Global todos directory
            let todos_dir = claude_dir.join("todos");
            if todos_dir.exists() {
                if let Ok(entries) = fs::read_dir(&todos_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().map_or(false, |e| e == "json") {
                            paths.push(path);
                        }
                    }
                }
            }
        }

        // Project-specific todos
        let dir = Path::new(directory);
        paths.push(dir.join(".omc").join("todos.json"));
        paths.push(dir.join(".claude").join("todos.json"));

        paths
    }

    /// Parse todo file content
    fn parse_todo_file(path: &Path) -> Vec<Todo> {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        // Try parsing as array
        if let Ok(todos) = serde_json::from_str::<Vec<Todo>>(&content) {
            return todos;
        }

        // Try parsing as object with todos field
        #[derive(Deserialize)]
        struct TodosWrapper {
            todos: Vec<Todo>,
        }

        if let Ok(wrapper) = serde_json::from_str::<TodosWrapper>(&content) {
            return wrapper.todos;
        }

        Vec::new()
    }

    /// Check for incomplete todos across all possible locations
    pub fn check_incomplete_todos(
        session_id: Option<&str>,
        directory: &str,
        stop_context: Option<&StopContext>,
    ) -> IncompleteTodosResult {
        // If user aborted, don't force continuation
        if let Some(ctx) = stop_context {
            if ctx.is_user_abort() {
                return IncompleteTodosResult {
                    count: 0,
                    todos: Vec::new(),
                    total: 0,
                };
            }
        }

        let paths = Self::get_todo_file_paths(session_id, directory);
        let mut seen_contents: HashSet<String> = HashSet::new();
        let mut all_todos: Vec<Todo> = Vec::new();
        let mut incomplete_todos: Vec<Todo> = Vec::new();

        for path in paths {
            if !path.exists() {
                continue;
            }

            let todos = Self::parse_todo_file(&path);

            for todo in todos {
                // Deduplicate by content + status
                let key = format!("{}:{:?}", todo.content, todo.status);
                if seen_contents.contains(&key) {
                    continue;
                }
                seen_contents.insert(key);

                if todo.is_incomplete() {
                    incomplete_todos.push(todo.clone());
                }
                all_todos.push(todo);
            }
        }

        IncompleteTodosResult {
            count: incomplete_todos.len(),
            todos: incomplete_todos,
            total: all_todos.len(),
        }
    }

    /// Get the next pending todo
    pub fn get_next_pending_todo(result: &IncompleteTodosResult) -> Option<&Todo> {
        // First try to find one that's in_progress
        if let Some(todo) = result.todos.iter().find(|t| t.status == TodoStatus::InProgress) {
            return Some(todo);
        }

        // Otherwise return first pending
        result.todos.iter().find(|t| t.status == TodoStatus::Pending)
    }

    /// Format todo status string
    pub fn format_todo_status(result: &IncompleteTodosResult) -> String {
        if result.count == 0 {
            return format!("All tasks complete ({} total)", result.total);
        }

        format!(
            "{}/{} completed, {} remaining",
            result.total - result.count,
            result.total,
            result.count
        )
    }
}

impl Default for TodoContinuationHook {
    fn default() -> Self {
        Self::new()
    }
}

/// The todo continuation prompt message
pub const TODO_CONTINUATION_PROMPT: &str = r#"[SYSTEM REMINDER - TODO CONTINUATION]

Incomplete tasks remain in your todo list. Continue working on the next pending task.

- Proceed without asking for permission
- Mark each task complete when finished
- Do not stop until all tasks are done"#;

#[async_trait]
impl Hook for TodoContinuationHook {
    fn name(&self) -> &str {
        "todo-continuation"
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
        // Build stop context from input
        let stop_context = StopContext {
            stop_reason: input.stop_reason.clone(),
            user_requested: input.user_requested,
        };

        // Check for incomplete todos
        let result = Self::check_incomplete_todos(
            input.session_id.as_deref(),
            &context.directory,
            Some(&stop_context),
        );

        if result.count == 0 {
            return Ok(HookOutput::pass());
        }

        // Build continuation message
        let next_todo = Self::get_next_pending_todo(&result);
        let next_task_info = next_todo
            .map(|t| format!("\n\nNext task: \"{}\" ({:?})", t.content, t.status))
            .unwrap_or_default();

        let message = format!(
            "{}\n\n[Status: {} of {} tasks remaining]{}",
            TODO_CONTINUATION_PROMPT, result.count, result.total, next_task_info
        );

        // Block stop and inject continuation message
        Ok(HookOutput::block_with_reason(message))
    }

    fn priority(&self) -> i32 {
        50 // Lower priority than ralph and ultrawork
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_todo_is_incomplete() {
        let pending = Todo {
            content: "Test".to_string(),
            status: TodoStatus::Pending,
            priority: None,
            id: None,
        };
        assert!(pending.is_incomplete());

        let in_progress = Todo {
            content: "Test".to_string(),
            status: TodoStatus::InProgress,
            priority: None,
            id: None,
        };
        assert!(in_progress.is_incomplete());

        let completed = Todo {
            content: "Test".to_string(),
            status: TodoStatus::Completed,
            priority: None,
            id: None,
        };
        assert!(!completed.is_incomplete());

        let cancelled = Todo {
            content: "Test".to_string(),
            status: TodoStatus::Cancelled,
            priority: None,
            id: None,
        };
        assert!(!cancelled.is_incomplete());
    }

    #[test]
    fn test_stop_context_is_user_abort() {
        let ctx = StopContext {
            stop_reason: Some("user_cancel".to_string()),
            user_requested: None,
        };
        assert!(ctx.is_user_abort());

        let ctx = StopContext {
            stop_reason: None,
            user_requested: Some(true),
        };
        assert!(ctx.is_user_abort());

        let ctx = StopContext {
            stop_reason: Some("normal_completion".to_string()),
            user_requested: Some(false),
        };
        assert!(!ctx.is_user_abort());

        let ctx = StopContext::default();
        assert!(!ctx.is_user_abort());
    }

    #[test]
    fn test_format_todo_status() {
        let result = IncompleteTodosResult {
            count: 3,
            todos: Vec::new(),
            total: 10,
        };
        assert_eq!(
            TodoContinuationHook::format_todo_status(&result),
            "7/10 completed, 3 remaining"
        );

        let result = IncompleteTodosResult {
            count: 0,
            todos: Vec::new(),
            total: 5,
        };
        assert_eq!(
            TodoContinuationHook::format_todo_status(&result),
            "All tasks complete (5 total)"
        );
    }

    #[test]
    fn test_get_next_pending_todo() {
        let todos = vec![
            Todo {
                content: "First pending".to_string(),
                status: TodoStatus::Pending,
                priority: None,
                id: None,
            },
            Todo {
                content: "In progress".to_string(),
                status: TodoStatus::InProgress,
                priority: None,
                id: None,
            },
            Todo {
                content: "Second pending".to_string(),
                status: TodoStatus::Pending,
                priority: None,
                id: None,
            },
        ];

        let result = IncompleteTodosResult {
            count: 3,
            todos,
            total: 3,
        };

        // Should return in_progress first
        let next = TodoContinuationHook::get_next_pending_todo(&result);
        assert!(next.is_some());
        assert_eq!(next.unwrap().content, "In progress");
    }
}
