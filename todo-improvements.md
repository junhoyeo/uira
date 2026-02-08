# TODO System Improvements

## 1. Priority Visualization in TUI

현재 TodoPriority enum이 있지만 TUI에서 표시되지 않음:

```rust
// crates/uira-tui/src/app.rs - render_todo_sidebar 함수 개선

let (indicator, color, priority_marker) = match (todo.status, todo.priority) {
    (TodoStatus::Completed, _) => ("✓", Color::Green, ""),
    (TodoStatus::InProgress, TodoPriority::Critical) => ("•", Color::Red, "!!! "),
    (TodoStatus::InProgress, TodoPriority::High) => ("•", Color::Yellow, "!! "),
    (TodoStatus::InProgress, TodoPriority::Medium) => ("•", Color::Yellow, "! "),
    (TodoStatus::InProgress, TodoPriority::Low) => ("•", Color::Gray, ""),
    (TodoStatus::Cancelled, _) => ("✗", Color::DarkGray, ""),
    (TodoStatus::Pending, TodoPriority::Critical) => (" ", Color::Red, "!!! "),
    (TodoStatus::Pending, TodoPriority::High) => (" ", Color::LightRed, "!! "),
    (TodoStatus::Pending, TodoPriority::Medium) => (" ", Color::Gray, "! "),
    (TodoStatus::Pending, TodoPriority::Low) => (" ", Color::DarkGray, ""),
};

let prefix = format!("[{}] {}", indicator, priority_marker);
```

## 2. Keyboard Shortcuts

TODO 관리를 위한 단축키 추가:

```rust
// crates/uira-tui/src/app.rs - handle_key_event에 추가

KeyCode::Char('t') if modifiers.contains(KeyModifiers::CONTROL) => {
    // Toggle TODO sidebar visibility
    self.show_todo_sidebar = !self.show_todo_sidebar;
}
KeyCode::Char('d') if modifiers.contains(KeyModifiers::CONTROL) => {
    // Quick TODO done - mark current TODO as completed
    if let Some(current_todo_id) = self.get_current_todo() {
        self.update_todo_status(current_todo_id, TodoStatus::Completed);
    }
}
```

## 3. TODO Filtering

상태/우선순위별 필터링:

```rust
#[derive(Clone, Debug)]
pub enum TodoFilter {
    All,
    Incomplete,  // Pending + InProgress
    ByPriority(TodoPriority),
    ByStatus(TodoStatus),
}

impl App {
    fn filtered_todos(&self) -> Vec<&TodoItem> {
        self.todos.iter()
            .filter(|todo| match self.todo_filter {
                TodoFilter::All => true,
                TodoFilter::Incomplete => matches!(
                    todo.status, 
                    TodoStatus::Pending | TodoStatus::InProgress
                ),
                TodoFilter::ByPriority(p) => todo.priority == p,
                TodoFilter::ByStatus(s) => todo.status == s,
            })
            .collect()
    }
}
```

## 4. TODO Notifications

중요 TODO 완료시 시스템 알림:

```rust
// TodoWrite tool에서 complete 액션시
if todo.priority == TodoPriority::Critical || todo.priority == TodoPriority::High {
    ctx.send_notification(format!(
        "🎉 Completed high-priority TODO: {}",
        todo.content
    )).await?;
}
```

## 5. TODO Analytics

진행 상황 추적:

```rust
pub struct TodoStats {
    total: usize,
    completed: usize,
    in_progress: usize,
    pending: usize,
    completion_rate: f32,
    avg_completion_time: Option<Duration>,
}

impl TodoStore {
    pub async fn get_stats(&self, session_id: &str) -> TodoStats {
        let todos = self.get(session_id).await;
        let total = todos.len();
        let completed = todos.iter().filter(|t| t.status == TodoStatus::Completed).count();
        // ... 나머지 통계 계산
    }
}
```

## 6. Auto-Priority Adjustment

오래된 TODO의 우선순위 자동 상향:

```rust
pub async fn auto_escalate_priority(&self, session_id: &str) {
    let mut todos = self.get(session_id).await;
    let now = Utc::now();
    
    for todo in &mut todos {
        if todo.status == TodoStatus::Pending {
            let age = now - todo.created_at;
            if age > Duration::days(7) && todo.priority == TodoPriority::Low {
                todo.priority = TodoPriority::Medium;
            } else if age > Duration::days(14) && todo.priority == TodoPriority::Medium {
                todo.priority = TodoPriority::High;
            }
        }
    }
    
    self.update(session_id, todos).await;
}
```

## Implementation Priority

1. **즉시 구현 가능**: Priority visualization, Keyboard shortcuts
2. **중간 난이도**: Filtering, Stats
3. **복잡함**: Auto-escalation, Notifications (시스템 통합 필요)