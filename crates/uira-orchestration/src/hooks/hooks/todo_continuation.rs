use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use super::super::hook::{Hook, HookContext, HookResult};
use super::super::types::{HookEvent, HookInput, HookOutput};
use uira_core::{TodoItem, TodoPriority, TodoStatus, UIRA_DIR};

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

            let uira_dir = home.join(UIRA_DIR);
            if let Some(sid) = session_id {
                paths.push(uira_dir.join("todos").join(format!("{}.json", sid)));
            }
        }

        let dir = Path::new(directory);
        paths.push(dir.join(UIRA_DIR).join("todos.json"));
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
}

impl Default for TodoContinuationHook {
    fn default() -> Self {
        Self::new()
    }
}

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
        let stop_context = StopContext {
            stop_reason: input.stop_reason.clone(),
            user_requested: input.user_requested,
        };

        let result = Self::check_incomplete_todos(
            input.session_id.as_deref(),
            &context.directory,
            Some(&stop_context),
        );

        if result.count == 0 {
            return Ok(HookOutput::pass());
        }

        Ok(HookOutput::continue_with_message(format!(
            "[TODO CONTINUATION] {} pending task(s) remain. Continue execution until all todos are complete.",
            result.count
        )))
    }

    fn priority(&self) -> i32 {
        100
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::tempdir;

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
    fn test_todo_file_paths_are_session_scoped() {
        let paths = TodoContinuationHook::get_todo_file_paths(Some("sid-123"), "/tmp/project");

        let home_todos = paths
            .iter()
            .filter(|p| p.to_string_lossy().contains("/.claude/todos/"))
            .collect::<Vec<_>>();
        assert!(home_todos
            .iter()
            .all(|p| p.to_string_lossy().contains("sid-123")));

        let home_uira_todos = paths
            .iter()
            .filter(|p| p.to_string_lossy().contains("/.uira/todos/"))
            .collect::<Vec<_>>();
        assert!(home_uira_todos
            .iter()
            .all(|p| p.to_string_lossy().contains("sid-123")));
    }

    #[tokio::test]
    async fn test_hook_emits_message_for_incomplete_todos() {
        let temp = tempdir().unwrap();
        let todo_dir = temp.path().join(".uira");
        std::fs::create_dir_all(&todo_dir).unwrap();

        let todos = vec![make_item("1", "Pending task", TodoStatus::Pending)];
        std::fs::write(
            todo_dir.join("todos.json"),
            serde_json::to_string(&todos).unwrap(),
        )
        .unwrap();

        let hook = TodoContinuationHook::new();
        let input = HookInput {
            session_id: Some("test-session".to_string()),
            prompt: None,
            message: None,
            parts: None,
            tool_name: None,
            tool_input: None,
            tool_output: None,
            directory: Some(temp.path().to_string_lossy().to_string()),
            stop_reason: None,
            user_requested: Some(false),
            transcript_path: None,
            extra: HashMap::new(),
        };
        let context = HookContext::new(
            Some("test-session".to_string()),
            temp.path().to_string_lossy().to_string(),
        );

        let output = hook
            .execute(HookEvent::Stop, &input, &context)
            .await
            .unwrap();
        assert!(output.should_continue);
        assert!(output
            .message
            .as_deref()
            .unwrap_or_default()
            .contains("TODO CONTINUATION"));
    }
}
