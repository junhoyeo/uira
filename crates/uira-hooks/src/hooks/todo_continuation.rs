use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use uira_protocol::{TodoItem, TodoPriority, TodoStatus};

/// Lenient deserialization struct for backward-compatible parsing of todo files
/// written by Claude Code (`~/.claude/todos/`) which use optional fields.
#[derive(Deserialize)]
struct RawTodo {
    content: String,
    status: RawTodoStatus,
    #[serde(default)]
    priority: Option<String>,
    #[serde(default)]
    id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum RawTodoStatus {
    Pending,
    InProgress,
    Completed,
    Cancelled,
}

impl RawTodo {
    fn into_item(self, index: usize) -> TodoItem {
        TodoItem {
            id: self.id.unwrap_or_else(|| format!("todo-{}", index)),
            content: self.content,
            status: match self.status {
                RawTodoStatus::Pending => TodoStatus::Pending,
                RawTodoStatus::InProgress => TodoStatus::InProgress,
                RawTodoStatus::Completed => TodoStatus::Completed,
                RawTodoStatus::Cancelled => TodoStatus::Cancelled,
            },
            priority: match self.priority.as_deref() {
                Some("high") => TodoPriority::High,
                Some("low") => TodoPriority::Low,
                _ => TodoPriority::Medium,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct IncompleteTodosResult {
    pub count: usize,
    pub todos: Vec<TodoItem>,
    pub total: usize,
}

#[derive(Debug, Clone, Default)]
pub struct StopContext {
    pub stop_reason: Option<String>,
    pub user_requested: Option<bool>,
}

impl StopContext {
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

pub struct TodoContinuationHook {
    _max_attempts: usize,
}

impl TodoContinuationHook {
    pub fn new() -> Self {
        Self { _max_attempts: 5 }
    }

    fn get_todo_file_paths(session_id: Option<&str>, directory: &str) -> Vec<PathBuf> {
        let mut paths = Vec::new();

        if let Some(home) = dirs::home_dir() {
            let claude_dir = home.join(".claude");

            if let Some(sid) = session_id {
                paths.push(claude_dir.join("sessions").join(sid).join("todos.json"));
                paths.push(claude_dir.join("todos").join(format!("{}.json", sid)));
            }

            let todos_dir = claude_dir.join("todos");
            if todos_dir.exists() {
                if let Ok(entries) = fs::read_dir(&todos_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().is_some_and(|e| e == "json") {
                            paths.push(path);
                        }
                    }
                }
            }

            let uira_dir = home.join(".uira");
            if let Some(sid) = session_id {
                paths.push(uira_dir.join("todos").join(format!("{}.json", sid)));
            }

            let uira_todos_dir = uira_dir.join("todos");
            if uira_todos_dir.exists() {
                if let Ok(entries) = fs::read_dir(&uira_todos_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().is_some_and(|e| e == "json") {
                            paths.push(path);
                        }
                    }
                }
            }
        }

        let dir = Path::new(directory);
        paths.push(dir.join(".uira").join("todos.json"));
        paths.push(dir.join(".claude").join("todos.json"));

        paths
    }

    fn parse_todo_file(path: &Path) -> Vec<TodoItem> {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        // Uira-native format: Vec<TodoItem> (strict, all fields required)
        if let Ok(todos) = serde_json::from_str::<Vec<TodoItem>>(&content) {
            return todos;
        }

        // Claude Code format: Vec<RawTodo> (lenient, optional id/priority)
        if let Ok(raw_todos) = serde_json::from_str::<Vec<RawTodo>>(&content) {
            return raw_todos
                .into_iter()
                .enumerate()
                .map(|(i, t)| t.into_item(i))
                .collect();
        }

        #[derive(Deserialize)]
        struct ItemWrapper {
            todos: Vec<TodoItem>,
        }
        if let Ok(wrapper) = serde_json::from_str::<ItemWrapper>(&content) {
            return wrapper.todos;
        }

        #[derive(Deserialize)]
        struct RawWrapper {
            todos: Vec<RawTodo>,
        }
        if let Ok(wrapper) = serde_json::from_str::<RawWrapper>(&content) {
            return wrapper
                .todos
                .into_iter()
                .enumerate()
                .map(|(i, t)| t.into_item(i))
                .collect();
        }

        Vec::new()
    }

    /// Check for incomplete todos from in-memory items (e.g., from TodoStore).
    pub fn check_incomplete_from_items(
        items: &[TodoItem],
        stop_context: Option<&StopContext>,
    ) -> IncompleteTodosResult {
        if let Some(ctx) = stop_context {
            if ctx.is_user_abort() {
                return IncompleteTodosResult {
                    count: 0,
                    todos: Vec::new(),
                    total: 0,
                };
            }
        }

        let incomplete: Vec<TodoItem> = items
            .iter()
            .filter(|t| matches!(t.status, TodoStatus::Pending | TodoStatus::InProgress))
            .cloned()
            .collect();

        IncompleteTodosResult {
            count: incomplete.len(),
            todos: incomplete,
            total: items.len(),
        }
    }

    /// Check for incomplete todos across all filesystem locations.
    pub fn check_incomplete_todos(
        session_id: Option<&str>,
        directory: &str,
        stop_context: Option<&StopContext>,
    ) -> IncompleteTodosResult {
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
        let mut all_todos: Vec<TodoItem> = Vec::new();
        let mut incomplete_todos: Vec<TodoItem> = Vec::new();

        for path in paths {
            if !path.exists() {
                continue;
            }

            let todos = Self::parse_todo_file(&path);

            for todo in todos {
                let key = format!("{}:{}", todo.content, todo.status);
                if seen_contents.contains(&key) {
                    continue;
                }
                seen_contents.insert(key);

                if matches!(todo.status, TodoStatus::Pending | TodoStatus::InProgress) {
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

    pub fn get_next_pending_todo(result: &IncompleteTodosResult) -> Option<&TodoItem> {
        if let Some(todo) = result
            .todos
            .iter()
            .find(|t| t.status == TodoStatus::InProgress)
        {
            return Some(todo);
        }

        result
            .todos
            .iter()
            .find(|t| t.status == TodoStatus::Pending)
    }

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

pub const TODO_CONTINUATION_PROMPT: &str = r#"[SYSTEM REMINDER - TODO CONTINUATION]

Incomplete tasks remain in your todo list. Continue working on the next pending task.

- Proceed without asking for permission
- Mark each task complete when finished
- Do not stop until all tasks are done"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn make_item(id: &str, content: &str, status: TodoStatus) -> TodoItem {
        TodoItem {
            id: id.to_string(),
            content: content.to_string(),
            status,
            priority: TodoPriority::Medium,
        }
    }

    #[test]
    fn test_todo_is_incomplete() {
        let pending = make_item("1", "Test", TodoStatus::Pending);
        assert!(matches!(
            pending.status,
            TodoStatus::Pending | TodoStatus::InProgress
        ));

        let in_progress = make_item("2", "Test", TodoStatus::InProgress);
        assert!(matches!(
            in_progress.status,
            TodoStatus::Pending | TodoStatus::InProgress
        ));

        let completed = make_item("3", "Test", TodoStatus::Completed);
        assert!(!matches!(
            completed.status,
            TodoStatus::Pending | TodoStatus::InProgress
        ));

        let cancelled = make_item("4", "Test", TodoStatus::Cancelled);
        assert!(!matches!(
            cancelled.status,
            TodoStatus::Pending | TodoStatus::InProgress
        ));
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
            make_item("1", "First pending", TodoStatus::Pending),
            make_item("2", "In progress", TodoStatus::InProgress),
            make_item("3", "Second pending", TodoStatus::Pending),
        ];

        let result = IncompleteTodosResult {
            count: 3,
            todos,
            total: 3,
        };

        let next = TodoContinuationHook::get_next_pending_todo(&result);
        assert!(next.is_some());
        assert_eq!(next.unwrap().content, "In progress");
    }

    #[test]
    fn test_check_incomplete_from_items() {
        let items = vec![
            make_item("1", "Done", TodoStatus::Completed),
            make_item("2", "Pending", TodoStatus::Pending),
            make_item("3", "Working", TodoStatus::InProgress),
            make_item("4", "Cancelled", TodoStatus::Cancelled),
        ];

        let result = TodoContinuationHook::check_incomplete_from_items(&items, None);
        assert_eq!(result.count, 2);
        assert_eq!(result.total, 4);
        assert_eq!(result.todos.len(), 2);
    }

    #[test]
    fn test_check_incomplete_from_items_respects_user_abort() {
        let items = vec![make_item("1", "Pending", TodoStatus::Pending)];

        let ctx = StopContext {
            stop_reason: None,
            user_requested: Some(true),
        };
        let result = TodoContinuationHook::check_incomplete_from_items(&items, Some(&ctx));
        assert_eq!(result.count, 0);
    }
}
