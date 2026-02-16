use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct WorkflowConfig {
    pub model: String,
    pub provider: String,
    pub max_iterations: u32,
    pub working_directory: PathBuf,
    pub auto_stage: bool,
    pub staged_only: bool,
    pub files: Vec<String>,
    pub task_options: TaskOptions,
}

#[derive(Debug, Clone, Default)]
pub struct TaskOptions {
    pub severity: Option<String>,
    pub languages: Vec<String>,
    pub pragma_format: Option<String>,
    pub include_docstrings: bool,
}

impl Default for WorkflowConfig {
    fn default() -> Self {
        let cwd = std::env::current_dir().unwrap_or_default();
        Self {
            model: uira_core::DEFAULT_ANTHROPIC_MODEL.to_string(),
            provider: "anthropic".to_string(),
            max_iterations: 10,
            working_directory: cwd,
            auto_stage: false,
            staged_only: false,
            files: vec![],
            task_options: TaskOptions::default(),
        }
    }
}
