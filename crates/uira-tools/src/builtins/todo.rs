use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use uira_protocol::{
    ApprovalRequirement, JsonSchema, SandboxPreference, TodoItem, TodoPriority, TodoStatus,
    ToolOutput,
};

use crate::{Tool, ToolContext, ToolError};

#[derive(Clone)]
pub struct TodoStore {
    inner: Arc<RwLock<HashMap<String, Vec<TodoItem>>>>,
    persist_dir: Option<PathBuf>,
}

impl TodoStore {
    fn todo_file_path(dir: &Path, session_id: &str) -> Result<PathBuf, std::io::Error> {
        if session_id.is_empty()
            || session_id.contains("..")
            || session_id.contains('/')
            || session_id.contains('\\')
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid session id",
            ));
        }

        Ok(dir.join(format!("{}.json", session_id)))
    }

    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            persist_dir: None,
        }
    }

    pub fn with_persistence(mut self, dir: PathBuf) -> Self {
        self.persist_dir = Some(dir);
        self
    }

    pub async fn get(&self, session_id: &str) -> Vec<TodoItem> {
        let map = self.inner.read().await;
        map.get(session_id).cloned().unwrap_or_default()
    }

    pub async fn has_incomplete(&self, session_id: &str) -> bool {
        let todos = self.get(session_id).await;
        todos
            .iter()
            .any(|t| matches!(t.status, TodoStatus::Pending | TodoStatus::InProgress))
    }

    pub async fn incomplete_count(&self, session_id: &str) -> usize {
        let todos = self.get(session_id).await;
        todos
            .iter()
            .filter(|t| matches!(t.status, TodoStatus::Pending | TodoStatus::InProgress))
            .count()
    }

    pub async fn incomplete_items(&self, session_id: &str) -> Vec<TodoItem> {
        let todos = self.get(session_id).await;
        todos
            .into_iter()
            .filter(|t| matches!(t.status, TodoStatus::Pending | TodoStatus::InProgress))
            .collect()
    }

    pub async fn update(&self, session_id: &str, todos: Vec<TodoItem>) {
        {
            let mut map = self.inner.write().await;
            map.insert(session_id.to_string(), todos.clone());
        }
        if let Some(ref dir) = self.persist_dir {
            let _ = self.persist(dir, session_id, &todos).await;
        }
    }

    async fn persist(
        &self,
        dir: &PathBuf,
        session_id: &str,
        todos: &[TodoItem],
    ) -> Result<(), std::io::Error> {
        tokio::fs::create_dir_all(dir).await?;
        let path = Self::todo_file_path(dir, session_id)?;
        let json = serde_json::to_string_pretty(todos).map_err(std::io::Error::other)?;
        tokio::fs::write(path, json).await
    }

    pub async fn load_from_disk(&self, session_id: &str) -> Option<Vec<TodoItem>> {
        let dir = self.persist_dir.as_ref()?;
        let path = Self::todo_file_path(dir, session_id).ok()?;
        let content = tokio::fs::read_to_string(path).await.ok()?;
        serde_json::from_str(&content).ok()
    }

    /// List all session IDs that have persisted todo files.
    /// Ported from oh-my-opencode `readSessionTodos` in session-manager/storage.ts.
    pub async fn list_sessions_with_todos(&self) -> Vec<String> {
        let dir = match self.persist_dir.as_ref() {
            Some(d) => d,
            None => return Vec::new(),
        };
        let mut sessions = Vec::new();
        let mut entries = match tokio::fs::read_dir(dir).await {
            Ok(e) => e,
            Err(_) => return Vec::new(),
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if let Some(session_id) = name.strip_suffix(".json") {
                sessions.push(session_id.to_string());
            }
        }
        sessions
    }

    /// Read todos for a session from disk, matching by session ID substring.
    /// Ported from oh-my-opencode `readSessionTodos` which filters todo files
    /// by `f.includes(sessionID)`.
    pub async fn read_session_todos(&self, session_id: &str) -> Vec<TodoItem> {
        if let Some(todos) = self.load_from_disk(session_id).await {
            return todos;
        }

        let dir = match self.persist_dir.as_ref() {
            Some(d) => d,
            None => return Vec::new(),
        };
        let mut entries = match tokio::fs::read_dir(dir).await {
            Ok(e) => e,
            Err(_) => return Vec::new(),
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.contains(session_id) && name_str.ends_with(".json") {
                if let Ok(content) = tokio::fs::read_to_string(entry.path()).await {
                    if let Ok(todos) = serde_json::from_str::<Vec<TodoItem>>(&content) {
                        return todos;
                    }
                }
            }
        }
        Vec::new()
    }

    /// Get summary info about a session's todos.
    /// Ported from oh-my-opencode `getSessionInfo` which includes `has_todos` and `todos`.
    pub async fn session_todo_info(&self, session_id: &str) -> TodoSessionInfo {
        let todos = {
            let mem = self.get(session_id).await;
            if !mem.is_empty() {
                mem
            } else {
                self.read_session_todos(session_id).await
            }
        };
        let total = todos.len();
        let completed = todos
            .iter()
            .filter(|t| t.status == TodoStatus::Completed)
            .count();
        let cancelled = todos
            .iter()
            .filter(|t| t.status == TodoStatus::Cancelled)
            .count();
        let in_progress = todos
            .iter()
            .filter(|t| t.status == TodoStatus::InProgress)
            .count();
        let pending = todos
            .iter()
            .filter(|t| t.status == TodoStatus::Pending)
            .count();

        TodoSessionInfo {
            has_todos: total > 0,
            total,
            completed,
            cancelled,
            in_progress,
            pending,
            todos,
        }
    }

    /// Delete the persisted todo file for a session.
    /// Ported from oh-my-opencode `deleteTodoFile`.
    pub async fn delete_session_todos(&self, session_id: &str) -> Result<(), std::io::Error> {
        let dir = match self.persist_dir.as_ref() {
            Some(d) => d,
            None => return Ok(()),
        };
        let path = Self::todo_file_path(dir, session_id)?;
        match tokio::fs::remove_file(path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TodoSessionInfo {
    pub has_todos: bool,
    pub total: usize,
    pub completed: usize,
    pub cancelled: usize,
    pub in_progress: usize,
    pub pending: usize,
    pub todos: Vec<TodoItem>,
}

impl Default for TodoStore {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
struct TodoWriteInput {
    todos: Vec<TodoInputItem>,
}

#[derive(Debug, Deserialize)]
struct TodoInputItem {
    id: String,
    content: String,
    status: String,
    priority: String,
}

impl TodoInputItem {
    fn into_todo_item(self) -> TodoItem {
        TodoItem {
            id: self.id,
            content: self.content,
            status: match self.status.as_str() {
                "in_progress" => TodoStatus::InProgress,
                "completed" => TodoStatus::Completed,
                "cancelled" => TodoStatus::Cancelled,
                _ => TodoStatus::Pending,
            },
            priority: match self.priority.as_str() {
                "high" => TodoPriority::High,
                "low" => TodoPriority::Low,
                _ => TodoPriority::Medium,
            },
        }
    }
}

pub struct TodoWriteTool {
    store: TodoStore,
}

impl TodoWriteTool {
    pub fn new(store: TodoStore) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for TodoWriteTool {
    fn name(&self) -> &str {
        "TodoWrite"
    }

    fn description(&self) -> &str {
        "Use this tool to create and manage a structured task list for your current coding session."
    }

    fn schema(&self) -> JsonSchema {
        JsonSchema::object()
            .property(
                "todos",
                JsonSchema::array(
                    JsonSchema::object()
                        .property(
                            "id",
                            JsonSchema::string().description("Unique identifier for the todo item"),
                        )
                        .property(
                            "content",
                            JsonSchema::string().description("Brief description of the task"),
                        )
                        .property(
                            "status",
                            JsonSchema::string().description(
                                "Current status: pending, in_progress, completed, cancelled",
                            ),
                        )
                        .property(
                            "priority",
                            JsonSchema::string().description("Priority level: high, medium, low"),
                        )
                        .required(&["id", "content", "status", "priority"]),
                )
                .description("The updated todo list"),
            )
            .required(&["todos"])
    }

    fn approval_requirement(&self, _input: &serde_json::Value) -> ApprovalRequirement {
        ApprovalRequirement::Skip {
            bypass_sandbox: false,
        }
    }

    fn sandbox_preference(&self) -> SandboxPreference {
        SandboxPreference::Forbid
    }

    fn supports_parallel(&self) -> bool {
        false
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        let input: TodoWriteInput =
            serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
                message: e.to_string(),
            })?;

        let todos: Vec<TodoItem> = input
            .todos
            .into_iter()
            .map(|t| t.into_todo_item())
            .collect();
        let pending_count = todos
            .iter()
            .filter(|t| t.status != TodoStatus::Completed && t.status != TodoStatus::Cancelled)
            .count();

        self.store.update(&ctx.session_id, todos.clone()).await;

        let output = serde_json::to_string_pretty(&todos).unwrap_or_default();
        Ok(ToolOutput::text(format!(
            "{} todos ({} remaining)\n{}",
            todos.len(),
            pending_count,
            output
        )))
    }
}

pub struct TodoReadTool {
    store: TodoStore,
}

impl TodoReadTool {
    pub fn new(store: TodoStore) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for TodoReadTool {
    fn name(&self) -> &str {
        "TodoRead"
    }

    fn description(&self) -> &str {
        "Read your current todo list."
    }

    fn schema(&self) -> JsonSchema {
        JsonSchema::object()
    }

    fn approval_requirement(&self, _input: &serde_json::Value) -> ApprovalRequirement {
        ApprovalRequirement::Skip {
            bypass_sandbox: false,
        }
    }

    fn sandbox_preference(&self) -> SandboxPreference {
        SandboxPreference::Forbid
    }

    fn supports_parallel(&self) -> bool {
        true
    }

    async fn execute(
        &self,
        _input: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        let todos = self.store.get(&ctx.session_id).await;
        let pending_count = todos
            .iter()
            .filter(|t| t.status != TodoStatus::Completed && t.status != TodoStatus::Cancelled)
            .count();

        let output = serde_json::to_string_pretty(&todos).unwrap_or_default();
        Ok(ToolOutput::text(format!(
            "{} todos ({} remaining)\n{}",
            todos.len(),
            pending_count,
            output
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_todo_store_get_empty() {
        let store = TodoStore::new();
        let todos = store.get("ses_123").await;
        assert!(todos.is_empty());
    }

    #[tokio::test]
    async fn test_todo_store_update_and_get() {
        let store = TodoStore::new();
        let items = vec![TodoItem {
            id: "1".to_string(),
            content: "Fix bug".to_string(),
            status: TodoStatus::Pending,
            priority: TodoPriority::High,
        }];
        store.update("ses_123", items.clone()).await;
        let result = store.get("ses_123").await;
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].content, "Fix bug");
    }

    #[tokio::test]
    async fn test_todo_store_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let store = TodoStore::new().with_persistence(dir.path().to_path_buf());
        let items = vec![TodoItem {
            id: "1".to_string(),
            content: "Write tests".to_string(),
            status: TodoStatus::InProgress,
            priority: TodoPriority::Medium,
        }];
        store.update("ses_456", items).await;

        let loaded = store.load_from_disk("ses_456").await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].content, "Write tests");
        assert_eq!(loaded[0].status, TodoStatus::InProgress);
    }

    #[tokio::test]
    async fn test_todo_write_tool() {
        let store = TodoStore::new();
        let tool = TodoWriteTool::new(store.clone());
        let ctx = ToolContext::default();
        let input = json!({
            "todos": [
                {"id": "1", "content": "Task A", "status": "pending", "priority": "high"},
                {"id": "2", "content": "Task B", "status": "completed", "priority": "low"}
            ]
        });

        let result = tool.execute(input, &ctx).await.unwrap();
        let text = result.as_text().unwrap();
        assert!(text.contains("2 todos (1 remaining)"));

        let stored = store.get(&ctx.session_id).await;
        assert_eq!(stored.len(), 2);
        assert_eq!(stored[0].status, TodoStatus::Pending);
        assert_eq!(stored[1].status, TodoStatus::Completed);
    }

    #[tokio::test]
    async fn test_todo_read_tool() {
        let store = TodoStore::new();
        let items = vec![
            TodoItem {
                id: "1".to_string(),
                content: "Task A".to_string(),
                status: TodoStatus::Pending,
                priority: TodoPriority::High,
            },
            TodoItem {
                id: "2".to_string(),
                content: "Task B".to_string(),
                status: TodoStatus::InProgress,
                priority: TodoPriority::Medium,
            },
        ];
        store.update("test_session", items).await;

        let tool = TodoReadTool::new(store);
        let mut ctx = ToolContext::default();
        ctx.session_id = "test_session".to_string();

        let result = tool.execute(json!({}), &ctx).await.unwrap();
        let text = result.as_text().unwrap();
        assert!(text.contains("2 todos (2 remaining)"));
        assert!(text.contains("Task A"));
        assert!(text.contains("Task B"));
    }

    #[tokio::test]
    async fn test_todo_input_status_parsing() {
        let item = TodoInputItem {
            id: "1".to_string(),
            content: "test".to_string(),
            status: "in_progress".to_string(),
            priority: "high".to_string(),
        };
        let todo = item.into_todo_item();
        assert_eq!(todo.status, TodoStatus::InProgress);
        assert_eq!(todo.priority, TodoPriority::High);
    }

    #[tokio::test]
    async fn test_todo_input_unknown_status_defaults() {
        let item = TodoInputItem {
            id: "1".to_string(),
            content: "test".to_string(),
            status: "unknown".to_string(),
            priority: "unknown".to_string(),
        };
        let todo = item.into_todo_item();
        assert_eq!(todo.status, TodoStatus::Pending);
        assert_eq!(todo.priority, TodoPriority::Medium);
    }

    #[tokio::test]
    async fn test_list_sessions_with_todos() {
        let dir = tempfile::tempdir().unwrap();
        let store = TodoStore::new().with_persistence(dir.path().to_path_buf());
        let items = vec![TodoItem {
            id: "1".to_string(),
            content: "Task".to_string(),
            status: TodoStatus::Pending,
            priority: TodoPriority::Medium,
        }];
        store.update("ses_aaa", items.clone()).await;
        store.update("ses_bbb", items).await;

        let sessions = store.list_sessions_with_todos().await;
        assert_eq!(sessions.len(), 2);
        assert!(sessions.contains(&"ses_aaa".to_string()));
        assert!(sessions.contains(&"ses_bbb".to_string()));
    }

    #[tokio::test]
    async fn test_read_session_todos() {
        let dir = tempfile::tempdir().unwrap();
        let store = TodoStore::new().with_persistence(dir.path().to_path_buf());
        let items = vec![TodoItem {
            id: "1".to_string(),
            content: "Persisted task".to_string(),
            status: TodoStatus::InProgress,
            priority: TodoPriority::High,
        }];
        store.update("ses_xyz", items).await;

        let fresh_store = TodoStore::new().with_persistence(dir.path().to_path_buf());
        let todos = fresh_store.read_session_todos("ses_xyz").await;
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].content, "Persisted task");
    }

    #[tokio::test]
    async fn test_session_todo_info() {
        let store = TodoStore::new();
        let items = vec![
            TodoItem {
                id: "1".to_string(),
                content: "Done".to_string(),
                status: TodoStatus::Completed,
                priority: TodoPriority::Low,
            },
            TodoItem {
                id: "2".to_string(),
                content: "Active".to_string(),
                status: TodoStatus::InProgress,
                priority: TodoPriority::High,
            },
            TodoItem {
                id: "3".to_string(),
                content: "Waiting".to_string(),
                status: TodoStatus::Pending,
                priority: TodoPriority::Medium,
            },
            TodoItem {
                id: "4".to_string(),
                content: "Skipped".to_string(),
                status: TodoStatus::Cancelled,
                priority: TodoPriority::Low,
            },
        ];
        store.update("ses_info", items).await;

        let info = store.session_todo_info("ses_info").await;
        assert!(info.has_todos);
        assert_eq!(info.total, 4);
        assert_eq!(info.completed, 1);
        assert_eq!(info.cancelled, 1);
        assert_eq!(info.in_progress, 1);
        assert_eq!(info.pending, 1);
        assert_eq!(info.todos.len(), 4);
    }

    #[tokio::test]
    async fn test_delete_session_todos() {
        let dir = tempfile::tempdir().unwrap();
        let store = TodoStore::new().with_persistence(dir.path().to_path_buf());
        let items = vec![TodoItem {
            id: "1".to_string(),
            content: "To delete".to_string(),
            status: TodoStatus::Pending,
            priority: TodoPriority::Medium,
        }];
        store.update("ses_del", items).await;

        assert!(store.load_from_disk("ses_del").await.is_some());

        store.delete_session_todos("ses_del").await.unwrap();

        assert!(store.load_from_disk("ses_del").await.is_none());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_session_is_ok() {
        let dir = tempfile::tempdir().unwrap();
        let store = TodoStore::new().with_persistence(dir.path().to_path_buf());
        assert!(store.delete_session_todos("nonexistent").await.is_ok());
    }

    #[tokio::test]
    async fn test_invalid_session_id_rejected_for_disk_access() {
        let dir = tempfile::tempdir().unwrap();
        let store = TodoStore::new().with_persistence(dir.path().to_path_buf());

        let items = vec![TodoItem {
            id: "1".to_string(),
            content: "Task".to_string(),
            status: TodoStatus::Pending,
            priority: TodoPriority::Medium,
        }];

        let persist_result = store
            .persist(&dir.path().to_path_buf(), "../escape", &items)
            .await;
        assert!(persist_result.is_err());
        assert_eq!(
            persist_result.unwrap_err().kind(),
            std::io::ErrorKind::InvalidInput
        );

        assert!(store.load_from_disk("../escape").await.is_none());

        let delete_result = store.delete_session_todos("../escape").await;
        assert!(delete_result.is_err());
        assert_eq!(
            delete_result.unwrap_err().kind(),
            std::io::ErrorKind::InvalidInput
        );
    }

    #[tokio::test]
    async fn test_session_todo_info_empty() {
        let store = TodoStore::new();
        let info = store.session_todo_info("nonexistent").await;
        assert!(!info.has_todos);
        assert_eq!(info.total, 0);
    }

    #[tokio::test]
    async fn test_has_incomplete() {
        let store = TodoStore::new();
        assert!(!store.has_incomplete("ses_test").await);

        let items = vec![
            TodoItem {
                id: "1".to_string(),
                content: "Done".to_string(),
                status: TodoStatus::Completed,
                priority: TodoPriority::Medium,
            },
            TodoItem {
                id: "2".to_string(),
                content: "Pending".to_string(),
                status: TodoStatus::Pending,
                priority: TodoPriority::High,
            },
        ];
        store.update("ses_test", items).await;
        assert!(store.has_incomplete("ses_test").await);

        let all_done = vec![TodoItem {
            id: "1".to_string(),
            content: "Done".to_string(),
            status: TodoStatus::Completed,
            priority: TodoPriority::Medium,
        }];
        store.update("ses_done", all_done).await;
        assert!(!store.has_incomplete("ses_done").await);
    }

    #[tokio::test]
    async fn test_incomplete_count() {
        let store = TodoStore::new();
        assert_eq!(store.incomplete_count("ses_test").await, 0);

        let items = vec![
            TodoItem {
                id: "1".to_string(),
                content: "Done".to_string(),
                status: TodoStatus::Completed,
                priority: TodoPriority::Medium,
            },
            TodoItem {
                id: "2".to_string(),
                content: "Pending".to_string(),
                status: TodoStatus::Pending,
                priority: TodoPriority::High,
            },
            TodoItem {
                id: "3".to_string(),
                content: "Working".to_string(),
                status: TodoStatus::InProgress,
                priority: TodoPriority::High,
            },
            TodoItem {
                id: "4".to_string(),
                content: "Cancelled".to_string(),
                status: TodoStatus::Cancelled,
                priority: TodoPriority::Low,
            },
        ];
        store.update("ses_test", items).await;
        assert_eq!(store.incomplete_count("ses_test").await, 2);
    }

    #[tokio::test]
    async fn test_incomplete_items() {
        let store = TodoStore::new();
        assert_eq!(store.incomplete_items("ses_test").await.len(), 0);

        let items = vec![
            TodoItem {
                id: "1".to_string(),
                content: "Done".to_string(),
                status: TodoStatus::Completed,
                priority: TodoPriority::Medium,
            },
            TodoItem {
                id: "2".to_string(),
                content: "Pending".to_string(),
                status: TodoStatus::Pending,
                priority: TodoPriority::High,
            },
            TodoItem {
                id: "3".to_string(),
                content: "Working".to_string(),
                status: TodoStatus::InProgress,
                priority: TodoPriority::High,
            },
        ];
        store.update("ses_test", items).await;

        let incomplete = store.incomplete_items("ses_test").await;
        assert_eq!(incomplete.len(), 2);
        assert!(incomplete.iter().any(|t| t.content == "Pending"));
        assert!(incomplete.iter().any(|t| t.content == "Working"));
        assert!(!incomplete.iter().any(|t| t.content == "Done"));
    }
}
