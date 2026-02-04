use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
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
        let path = dir.join(format!("{}.json", session_id));
        let json = serde_json::to_string_pretty(todos).map_err(std::io::Error::other)?;
        tokio::fs::write(path, json).await
    }

    pub async fn load_from_disk(&self, session_id: &str) -> Option<Vec<TodoItem>> {
        let dir = self.persist_dir.as_ref()?;
        let path = dir.join(format!("{}.json", session_id));
        let content = tokio::fs::read_to_string(path).await.ok()?;
        serde_json::from_str(&content).ok()
    }
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
}
