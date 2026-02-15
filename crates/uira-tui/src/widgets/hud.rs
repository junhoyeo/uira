use std::collections::HashMap;
use std::time::Instant;

use ratatui::prelude::*;
use ratatui::text::{Line, Span};

const MAX_CONCURRENT: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackgroundTaskStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone)]
pub struct BackgroundTaskInfo {
    pub task_id: String,
    pub description: String,
    pub agent_name: String,
    pub agent_code: char,
    pub start_time: Instant,
    pub status: BackgroundTaskStatus,
    pub model: Option<String>,
}

#[derive(Debug, Default)]
pub struct BackgroundTaskRegistry {
    tasks: HashMap<String, BackgroundTaskInfo>,
}

impl BackgroundTaskRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn on_spawned(&mut self, task_id: String, description: String, agent_name: String) {
        let agent_code = agent_code_for_name(&agent_name);
        self.tasks.insert(
            task_id.clone(),
            BackgroundTaskInfo {
                task_id,
                description,
                agent_name,
                agent_code,
                start_time: Instant::now(),
                status: BackgroundTaskStatus::Running,
                model: None,
            },
        );
    }

    pub fn on_progress(&mut self, task_id: &str, status: &str) {
        if let Some(_task) = self.tasks.get_mut(task_id) {
            let _ = status;
        }
    }

    pub fn on_completed(&mut self, task_id: &str, success: bool) {
        if let Some(task) = self.tasks.get_mut(task_id) {
            task.status = if success {
                BackgroundTaskStatus::Completed
            } else {
                BackgroundTaskStatus::Failed
            };
        }
    }

    pub fn running_count(&self) -> usize {
        self.tasks
            .values()
            .filter(|task| task.status == BackgroundTaskStatus::Running)
            .count()
    }

    pub fn running_tasks(&self) -> Vec<&BackgroundTaskInfo> {
        self.tasks
            .values()
            .filter(|task| task.status == BackgroundTaskStatus::Running)
            .collect()
    }

    pub fn has_running_tasks(&self) -> bool {
        self.running_count() > 0
    }
}

pub fn agent_code_for_name(name: &str) -> char {
    match name.to_lowercase().as_str() {
        s if s.contains("explore") => 'e',
        s if s.contains("architect") => 'A',
        s if s.contains("executor") => 'x',
        s if s.contains("designer") => 'd',
        s if s.contains("writer") => 'w',
        s if s.contains("researcher") => 'r',
        s if s.contains("scientist") => 's',
        s if s.contains("qa") || s.contains("tester") => 'q',
        s if s.contains("build") || s.contains("fixer") => 'b',
        s if s.contains("critic") => 'c',
        s if s.contains("planner") => 'p',
        s if s.contains("vision") => 'v',
        _ => name.chars().next().unwrap_or('?').to_ascii_lowercase(),
    }
}

pub fn format_bg_indicator(running: usize) -> Span<'static> {
    let color = if running >= MAX_CONCURRENT {
        Color::Yellow
    } else if running >= MAX_CONCURRENT - 1 {
        Color::Cyan
    } else {
        Color::Green
    };
    Span::styled(
        format!("bg:{}/{}", running, MAX_CONCURRENT),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    )
}

pub fn format_duration(elapsed: std::time::Duration) -> String {
    let secs = elapsed.as_secs();
    if secs < 10 {
        String::new()
    } else if secs < 60 {
        format!("({}s)", secs)
    } else if secs < 600 {
        format!("({}m)", secs / 60)
    } else {
        "!".to_string()
    }
}

pub fn render_hud_line(registry: &BackgroundTaskRegistry) -> Line<'static> {
    let running = registry.running_count();
    if running == 0 {
        return Line::from("");
    }

    let mut spans = vec![format_bg_indicator(running)];
    spans.push(Span::raw(" "));

    for task in registry.running_tasks() {
        let duration = format_duration(task.start_time.elapsed());
        let task_span = format!("{}{} ", task.agent_code, duration);
        spans.push(Span::styled(task_span, Style::default().fg(Color::Gray)));
    }

    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_registry_lifecycle() {
        let mut reg = BackgroundTaskRegistry::new();
        assert_eq!(reg.running_count(), 0);

        reg.on_spawned("t1".into(), "desc".into(), "explore".into());
        assert_eq!(reg.running_count(), 1);

        reg.on_completed("t1", true);
        assert_eq!(reg.running_count(), 0);
    }

    #[test]
    fn test_agent_codes() {
        assert_eq!(agent_code_for_name("explore"), 'e');
        assert_eq!(agent_code_for_name("architect"), 'A');
        assert_eq!(agent_code_for_name("executor"), 'x');
        assert_eq!(agent_code_for_name("unknown-agent"), 'u');
    }

    #[test]
    fn test_bg_indicator_colors() {
        let green = format_bg_indicator(3);
        assert_eq!(green.content, "bg:3/5");
        assert_eq!(green.style.fg, Some(Color::Green));

        let cyan = format_bg_indicator(4);
        assert_eq!(cyan.content, "bg:4/5");
        assert_eq!(cyan.style.fg, Some(Color::Cyan));

        let yellow = format_bg_indicator(5);
        assert_eq!(yellow.content, "bg:5/5");
        assert_eq!(yellow.style.fg, Some(Color::Yellow));
    }

    #[test]
    fn test_duration_formatting() {
        assert_eq!(format_duration(Duration::from_secs(5)), "");
        assert_eq!(format_duration(Duration::from_secs(30)), "(30s)");
        assert_eq!(format_duration(Duration::from_secs(120)), "(2m)");
        assert_eq!(format_duration(Duration::from_secs(700)), "!");
    }

    #[test]
    fn test_hud_line_rendering() {
        let mut reg = BackgroundTaskRegistry::new();
        reg.on_spawned("t1".into(), "search".into(), "explore".into());
        reg.on_spawned("t2".into(), "plan".into(), "architect".into());
        reg.on_spawned("t3".into(), "build".into(), "executor".into());

        let line = render_hud_line(&reg);
        let rendered = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();

        assert!(rendered.contains("bg:3/5"));
        assert!(rendered.contains('e'));
        assert!(rendered.contains('A'));
        assert!(rendered.contains('x'));
    }
}
