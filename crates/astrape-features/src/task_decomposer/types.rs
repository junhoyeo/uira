use serde::{Deserialize, Serialize};

/// Task type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskType {
    FullstackApp,
    Refactoring,
    BugFix,
    Feature,
    Testing,
    Documentation,
    Infrastructure,
    Migration,
    Optimization,
    Unknown,
}

/// Component role in the system
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComponentRole {
    Frontend,
    Backend,
    Database,
    Api,
    Ui,
    Shared,
    Coordinator,
    Testing,
    Docs,
    Config,
    Module,
}

impl ComponentRole {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "frontend" => Self::Frontend,
            "backend" => Self::Backend,
            "database" => Self::Database,
            "api" => Self::Api,
            "ui" => Self::Ui,
            "shared" => Self::Shared,
            "coordinator" => Self::Coordinator,
            "testing" => Self::Testing,
            "docs" => Self::Docs,
            "config" => Self::Config,
            _ => Self::Module,
        }
    }
}

/// Model tier for task complexity
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelTier {
    Low,
    Medium,
    High,
}

/// Task analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAnalysis {
    /// Original task description
    pub task: String,
    /// Detected task type
    pub task_type: TaskType,
    /// Task complexity score (0-1)
    pub complexity: f64,
    /// Whether task can be parallelized
    pub is_parallelizable: bool,
    /// Estimated number of components
    pub estimated_components: usize,
    /// Key areas identified in the task
    pub areas: Vec<String>,
    /// Technologies/frameworks mentioned
    pub technologies: Vec<String>,
    /// File patterns mentioned or inferred
    pub file_patterns: Vec<String>,
    /// Dependencies between areas
    pub dependencies: Vec<Dependency>,
}

/// Dependency between components
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub from: String,
    pub to: String,
}

/// Component in the decomposition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Component {
    /// Unique component ID
    pub id: String,
    /// Component name
    pub name: String,
    /// Component role/type
    pub role: ComponentRole,
    /// Description of what this component does
    pub description: String,
    /// Whether this component can run in parallel
    pub can_parallelize: bool,
    /// Components this depends on (must complete first)
    pub dependencies: Vec<String>,
    /// Estimated effort/complexity (0-1)
    pub effort: f64,
    /// Technologies used by this component
    pub technologies: Vec<String>,
}

/// File ownership for a component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileOwnership {
    /// Component ID that owns these files
    pub component_id: String,
    /// Glob patterns for files this component owns exclusively
    pub patterns: Vec<String>,
    /// Specific files (non-glob) this component owns
    pub files: Vec<String>,
    /// Files that might overlap with other components
    pub potential_conflicts: Vec<String>,
}

/// Subtask generated from a component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subtask {
    /// Unique subtask ID
    pub id: String,
    /// Subtask name
    pub name: String,
    /// Component this subtask implements
    pub component: Component,
    /// Detailed prompt for worker agent
    pub prompt: String,
    /// File ownership for this subtask
    pub ownership: FileOwnership,
    /// Subtasks that must complete before this one
    pub blocked_by: Vec<String>,
    /// Recommended agent type
    pub agent_type: String,
    /// Recommended model tier
    pub model_tier: ModelTier,
    /// Acceptance criteria
    pub acceptance_criteria: Vec<String>,
    /// Verification steps
    pub verification: Vec<String>,
}

/// Shared file that needs coordinator management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedFile {
    /// File path or glob pattern
    pub pattern: String,
    /// Why this file is shared
    pub reason: String,
    /// Components that need access to this file
    pub shared_by: Vec<String>,
    /// Whether coordinator should manage this
    pub requires_coordinator: bool,
}

/// Complete decomposition result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecompositionResult {
    /// Original task analysis
    pub analysis: TaskAnalysis,
    /// Identified components
    pub components: Vec<Component>,
    /// Generated subtasks with ownership
    pub subtasks: Vec<Subtask>,
    /// Shared files that need coordinator
    pub shared_files: Vec<SharedFile>,
    /// Recommended execution order (by subtask ID)
    pub execution_order: Vec<Vec<String>>,
    /// Overall strategy description
    pub strategy: String,
    /// Warnings or issues detected
    pub warnings: Vec<String>,
}

/// Project context for decomposition
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectContext {
    /// Project root directory
    pub root_dir: Option<String>,
    /// Project type (detected)
    pub project_type: Option<String>,
    /// Technologies in use
    pub technologies: Option<Vec<String>>,
    /// Directory structure
    pub structure: Option<std::collections::HashMap<String, Vec<String>>>,
    /// Existing files that might be affected
    pub existing_files: Option<Vec<String>>,
}

/// Decomposition strategy trait
pub trait DecompositionStrategy {
    fn decompose(&self, analysis: &TaskAnalysis, context: &ProjectContext) -> Vec<Component>;
}
