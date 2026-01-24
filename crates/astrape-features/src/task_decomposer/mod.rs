pub mod types;

pub use types::*;

use std::collections::{HashMap, HashSet};

/// Main entry point: decompose a task into parallelizable subtasks
pub fn decompose_task(task: &str, project_context: ProjectContext) -> DecompositionResult {
    // Step 1: Analyze the task
    let analysis = analyze_task(task, &project_context);

    // Step 2: Identify parallelizable components
    let components = identify_components(&analysis, &project_context);

    // Step 3: Identify shared files
    let shared_files = identify_shared_files(&components, &project_context);

    // Step 4: Generate subtasks with file ownership
    let mut subtasks = generate_subtasks(&components, &analysis, &project_context);

    // Step 5: Assign non-overlapping file ownership
    assign_file_ownership(&mut subtasks, &shared_files, &project_context);

    // Step 6: Determine execution order
    let execution_order = calculate_execution_order(&subtasks);

    // Step 7: Validate decomposition
    let warnings = validate_decomposition(&subtasks, &shared_files);

    let strategy = explain_strategy(&analysis, &components);

    DecompositionResult {
        analysis: analysis.clone(),
        components,
        subtasks,
        shared_files,
        execution_order,
        strategy,
        warnings,
    }
}

/// Analyze task to understand structure and requirements
pub fn analyze_task(task: &str, context: &ProjectContext) -> TaskAnalysis {
    let lower = task.to_lowercase();

    // Detect task type
    let task_type = detect_task_type(&lower);

    // Detect complexity signals
    let complexity = estimate_complexity(&lower, &task_type);

    // Extract areas and technologies
    let areas = extract_areas(&lower, &task_type);
    let technologies = extract_technologies(&lower, context);
    let file_patterns = extract_file_patterns(&lower, context);

    // Detect dependencies
    let dependencies = analyze_dependencies(&areas, &task_type);

    // Determine if parallelizable
    let is_parallelizable = complexity > 0.3 && areas.len() >= 2;
    let estimated_components = if is_parallelizable {
        areas.len().clamp(2, 6)
    } else {
        1
    };

    TaskAnalysis {
        task: task.to_string(),
        task_type,
        complexity,
        is_parallelizable,
        estimated_components,
        areas,
        technologies,
        file_patterns,
        dependencies,
    }
}

/// Identify parallelizable components from analysis
pub fn identify_components(analysis: &TaskAnalysis, context: &ProjectContext) -> Vec<Component> {
    if !analysis.is_parallelizable {
        // Single component for non-parallelizable tasks
        return vec![Component {
            id: "main".to_string(),
            name: "Main Task".to_string(),
            role: ComponentRole::Module,
            description: analysis.task.clone(),
            can_parallelize: false,
            dependencies: vec![],
            effort: analysis.complexity,
            technologies: analysis.technologies.clone(),
        }];
    }

    // Select appropriate strategy
    let strategy = select_strategy(analysis);
    strategy.decompose(analysis, context)
}

/// Generate subtasks from components
pub fn generate_subtasks(
    components: &[Component],
    analysis: &TaskAnalysis,
    context: &ProjectContext,
) -> Vec<Subtask> {
    components
        .iter()
        .map(|component| Subtask {
            id: component.id.clone(),
            name: component.name.clone(),
            component: component.clone(),
            prompt: generate_prompt_for_component(component, analysis, context),
            ownership: FileOwnership {
                component_id: component.id.clone(),
                patterns: vec![],
                files: vec![],
                potential_conflicts: vec![],
            },
            blocked_by: component.dependencies.clone(),
            agent_type: select_agent_type(component),
            model_tier: select_model_tier(component),
            acceptance_criteria: generate_acceptance_criteria(component, analysis),
            verification: generate_verification_steps(component, analysis),
        })
        .collect()
}

/// Assign non-overlapping file ownership to subtasks
pub fn assign_file_ownership(
    subtasks: &mut [Subtask],
    shared_files: &[SharedFile],
    context: &ProjectContext,
) {
    let mut assignments: HashMap<String, HashSet<String>> = HashMap::new();

    for subtask in subtasks.iter_mut() {
        let patterns = infer_file_patterns(&subtask.component, context);
        let files = infer_specific_files(&subtask.component, context);

        subtask.ownership.patterns = patterns.clone();
        subtask.ownership.files = files;

        // Track assignments for conflict detection
        for pattern in &patterns {
            assignments
                .entry(pattern.clone())
                .or_default()
                .insert(subtask.id.clone());
        }
    }

    // Detect conflicts
    for subtask in subtasks.iter_mut() {
        let mut conflicts = vec![];

        for pattern in &subtask.ownership.patterns {
            if let Some(owners) = assignments.get(pattern) {
                if owners.len() > 1 {
                    // Check if it's a shared file
                    let is_shared = shared_files.iter().any(|sf| &sf.pattern == pattern);
                    if !is_shared {
                        conflicts.push(pattern.clone());
                    }
                }
            }
        }

        subtask.ownership.potential_conflicts = conflicts;
    }
}

/// Identify files that should be managed by coordinator
pub fn identify_shared_files(
    components: &[Component],
    _context: &ProjectContext,
) -> Vec<SharedFile> {
    let mut shared_files = vec![];

    // Common shared files
    let common_shared = [
        "package.json",
        "tsconfig.json",
        "package-lock.json",
        "yarn.lock",
        "pnpm-lock.yaml",
        "README.md",
        ".gitignore",
        ".env",
        ".env.example",
        "docker-compose.yml",
        "Dockerfile",
    ];

    for file in &common_shared {
        let shared_by: Vec<String> = components
            .iter()
            .filter(|c| c.role != ComponentRole::Coordinator)
            .map(|c| c.id.clone())
            .collect();

        if !shared_by.is_empty() {
            shared_files.push(SharedFile {
                pattern: file.to_string(),
                reason: "Common configuration file".to_string(),
                shared_by,
                requires_coordinator: true,
            });
        }
    }

    shared_files
}

// ============================================================================
// Helper Functions
// ============================================================================

fn detect_task_type(task: &str) -> TaskType {
    if task.contains("fullstack")
        || task.contains("full stack")
        || (task.contains("frontend") && task.contains("backend"))
    {
        return TaskType::FullstackApp;
    }

    if task.contains("refactor") || task.contains("restructure") {
        return TaskType::Refactoring;
    }

    if task.contains("fix")
        || task.contains("bug")
        || task.contains("error")
        || task.contains("issue")
    {
        return TaskType::BugFix;
    }

    if task.contains("feature") || task.contains("add") || task.contains("implement") {
        return TaskType::Feature;
    }

    if task.contains("test") || task.contains("testing") {
        return TaskType::Testing;
    }

    if task.contains("document") || task.contains("docs") {
        return TaskType::Documentation;
    }

    if task.contains("deploy") || task.contains("infra") || task.contains("ci/cd") {
        return TaskType::Infrastructure;
    }

    if task.contains("migrate") || task.contains("migration") {
        return TaskType::Migration;
    }

    if task.contains("optimize") || task.contains("performance") {
        return TaskType::Optimization;
    }

    TaskType::Unknown
}

fn estimate_complexity(task: &str, task_type: &TaskType) -> f64 {
    let mut score: f64 = match task_type {
        TaskType::FullstackApp => 0.9,
        TaskType::Refactoring => 0.7,
        TaskType::BugFix => 0.4,
        TaskType::Feature => 0.6,
        TaskType::Testing => 0.5,
        TaskType::Documentation => 0.3,
        TaskType::Infrastructure => 0.8,
        TaskType::Migration => 0.8,
        TaskType::Optimization => 0.7,
        TaskType::Unknown => 0.5,
    };

    // Length factor
    if task.len() > 200 {
        score += 0.1;
    }
    if task.len() > 500 {
        score += 0.1;
    }

    // Complexity keywords
    let complex_keywords = [
        "multiple",
        "complex",
        "advanced",
        "integrate",
        "system",
        "architecture",
        "scalable",
        "real-time",
        "distributed",
    ];

    for keyword in &complex_keywords {
        if task.contains(keyword) {
            score += 0.05;
        }
    }

    score.min(1.0)
}

fn extract_areas(task: &str, _task_type: &TaskType) -> Vec<String> {
    let mut areas = vec![];

    let area_keywords: HashMap<&str, Vec<&str>> = [
        (
            "frontend",
            vec!["frontend", "ui", "react", "vue", "angular", "component"],
        ),
        (
            "backend",
            vec!["backend", "server", "api", "endpoint", "service"],
        ),
        (
            "database",
            vec!["database", "db", "schema", "migration", "model"],
        ),
        ("auth", vec!["auth", "authentication", "login", "user"]),
        ("testing", vec!["test", "testing", "spec", "unit test"]),
        ("docs", vec!["document", "docs", "readme", "guide"]),
        ("config", vec!["config", "setup", "environment"]),
    ]
    .iter()
    .cloned()
    .collect();

    for (area, keywords) in &area_keywords {
        if keywords.iter().any(|kw| task.contains(kw)) {
            areas.push(area.to_string());
        }
    }

    if areas.is_empty() {
        areas.push("main".to_string());
    }

    areas
}

fn extract_technologies(task: &str, context: &ProjectContext) -> Vec<String> {
    let mut techs = vec![];

    let tech_keywords = [
        "react",
        "vue",
        "angular",
        "next",
        "nuxt",
        "express",
        "fastify",
        "nest",
        "typescript",
        "javascript",
        "node",
        "postgres",
        "mysql",
        "mongodb",
        "redis",
        "docker",
        "kubernetes",
    ];

    for tech in &tech_keywords {
        if task.contains(tech) {
            techs.push(tech.to_string());
        }
    }

    // Add from context
    if let Some(context_techs) = &context.technologies {
        techs.extend(context_techs.clone());
    }

    // Deduplicate
    let unique: HashSet<String> = techs.into_iter().collect();
    unique.into_iter().collect()
}

fn extract_file_patterns(_task: &str, _context: &ProjectContext) -> Vec<String> {
    vec![]
}

fn analyze_dependencies(areas: &[String], _task_type: &TaskType) -> Vec<Dependency> {
    let mut deps = vec![];

    // Common dependencies
    if areas.contains(&"frontend".to_string()) && areas.contains(&"backend".to_string()) {
        deps.push(Dependency {
            from: "frontend".to_string(),
            to: "backend".to_string(),
        });
    }

    if areas.contains(&"backend".to_string()) && areas.contains(&"database".to_string()) {
        deps.push(Dependency {
            from: "backend".to_string(),
            to: "database".to_string(),
        });
    }

    if areas.contains(&"testing".to_string()) {
        // Testing depends on everything else
        for area in areas {
            if area != "testing" {
                deps.push(Dependency {
                    from: "testing".to_string(),
                    to: area.clone(),
                });
            }
        }
    }

    deps
}

fn select_strategy(analysis: &TaskAnalysis) -> Box<dyn DecompositionStrategy> {
    match analysis.task_type {
        TaskType::FullstackApp => Box::new(FullstackStrategy),
        TaskType::Refactoring => Box::new(RefactoringStrategy),
        TaskType::BugFix => Box::new(BugFixStrategy),
        TaskType::Feature => Box::new(FeatureStrategy),
        _ => Box::new(DefaultStrategy),
    }
}

// ============================================================================
// Decomposition Strategies
// ============================================================================

struct FullstackStrategy;
impl DecompositionStrategy for FullstackStrategy {
    fn decompose(&self, analysis: &TaskAnalysis, _context: &ProjectContext) -> Vec<Component> {
        let mut components = vec![];

        // Frontend component
        if analysis.areas.contains(&"frontend".to_string())
            || analysis.areas.contains(&"ui".to_string())
        {
            // Only add backend dependency if backend is also being built
            let frontend_deps = if analysis.areas.contains(&"backend".to_string())
                || analysis.areas.contains(&"api".to_string())
            {
                vec!["backend".to_string()]
            } else {
                vec![]
            };
            components.push(Component {
                id: "frontend".to_string(),
                name: "Frontend".to_string(),
                role: ComponentRole::Frontend,
                description: "Frontend UI and components".to_string(),
                can_parallelize: true,
                dependencies: frontend_deps,
                effort: 0.4,
                technologies: analysis
                    .technologies
                    .iter()
                    .filter(|t| ["react", "vue", "angular", "next"].contains(&t.as_str()))
                    .cloned()
                    .collect(),
            });
        }

        // Backend component
        if analysis.areas.contains(&"backend".to_string())
            || analysis.areas.contains(&"api".to_string())
        {
            let deps = if analysis.areas.contains(&"database".to_string()) {
                vec!["database".to_string()]
            } else {
                vec![]
            };

            components.push(Component {
                id: "backend".to_string(),
                name: "Backend".to_string(),
                role: ComponentRole::Backend,
                description: "Backend API and business logic".to_string(),
                can_parallelize: true,
                dependencies: deps,
                effort: 0.4,
                technologies: analysis
                    .technologies
                    .iter()
                    .filter(|t| ["express", "fastify", "nest", "node"].contains(&t.as_str()))
                    .cloned()
                    .collect(),
            });
        }

        // Database component
        if analysis.areas.contains(&"database".to_string()) {
            components.push(Component {
                id: "database".to_string(),
                name: "Database".to_string(),
                role: ComponentRole::Database,
                description: "Database schema and migrations".to_string(),
                can_parallelize: true,
                dependencies: vec![],
                effort: 0.2,
                technologies: analysis
                    .technologies
                    .iter()
                    .filter(|t| ["postgres", "mysql", "mongodb"].contains(&t.as_str()))
                    .cloned()
                    .collect(),
            });
        }

        // Shared/coordinator component
        components.push(Component {
            id: "shared".to_string(),
            name: "Shared".to_string(),
            role: ComponentRole::Shared,
            description: "Shared types, utilities, and configuration".to_string(),
            can_parallelize: true,
            dependencies: vec![],
            effort: 0.2,
            technologies: vec![],
        });

        components
    }
}

struct RefactoringStrategy;
impl DecompositionStrategy for RefactoringStrategy {
    fn decompose(&self, analysis: &TaskAnalysis, _context: &ProjectContext) -> Vec<Component> {
        analysis
            .areas
            .iter()
            .map(|area| Component {
                id: area.clone(),
                name: format!("Refactor {}", area),
                role: ComponentRole::Module,
                description: format!("Refactor {} module", area),
                can_parallelize: true,
                dependencies: vec![],
                effort: analysis.complexity / analysis.areas.len() as f64,
                technologies: vec![],
            })
            .collect()
    }
}

struct BugFixStrategy;
impl DecompositionStrategy for BugFixStrategy {
    fn decompose(&self, analysis: &TaskAnalysis, _context: &ProjectContext) -> Vec<Component> {
        vec![Component {
            id: "bugfix".to_string(),
            name: "Fix Bug".to_string(),
            role: ComponentRole::Module,
            description: analysis.task.clone(),
            can_parallelize: false,
            dependencies: vec![],
            effort: analysis.complexity,
            technologies: vec![],
        }]
    }
}

struct FeatureStrategy;
impl DecompositionStrategy for FeatureStrategy {
    fn decompose(&self, analysis: &TaskAnalysis, _context: &ProjectContext) -> Vec<Component> {
        analysis
            .areas
            .iter()
            .map(|area| Component {
                id: area.clone(),
                name: format!("Implement {}", area),
                role: ComponentRole::parse(area),
                description: format!("Implement {} for the feature", area),
                can_parallelize: true,
                dependencies: vec![],
                effort: analysis.complexity / analysis.areas.len() as f64,
                technologies: vec![],
            })
            .collect()
    }
}

struct DefaultStrategy;
impl DecompositionStrategy for DefaultStrategy {
    fn decompose(&self, analysis: &TaskAnalysis, _context: &ProjectContext) -> Vec<Component> {
        vec![Component {
            id: "main".to_string(),
            name: "Main Task".to_string(),
            role: ComponentRole::Module,
            description: analysis.task.clone(),
            can_parallelize: false,
            dependencies: vec![],
            effort: analysis.complexity,
            technologies: vec![],
        }]
    }
}

// ============================================================================
// Subtask Generation Helpers
// ============================================================================

fn generate_prompt_for_component(
    component: &Component,
    analysis: &TaskAnalysis,
    _context: &ProjectContext,
) -> String {
    let mut prompt = format!("{}\n\n", component.description);

    prompt.push_str("CONTEXT:\n");
    prompt.push_str(&format!("- Task Type: {:?}\n", analysis.task_type));
    prompt.push_str(&format!("- Component Role: {:?}\n", component.role));

    if !component.technologies.is_empty() {
        prompt.push_str(&format!(
            "- Technologies: {}\n",
            component.technologies.join(", ")
        ));
    }

    prompt.push_str("\nYour responsibilities:\n");
    prompt.push_str(&format!("1. {}\n", component.description));
    prompt.push_str("2. Ensure code quality and follow best practices\n");
    prompt.push_str("3. Write tests for your changes\n");
    prompt.push_str("4. Update documentation as needed\n");

    if !component.dependencies.is_empty() {
        prompt.push_str(&format!(
            "\nDependencies: This component depends on {} completing first.\n",
            component.dependencies.join(", ")
        ));
    }

    prompt
}

fn select_agent_type(component: &Component) -> String {
    match component.role {
        ComponentRole::Frontend | ComponentRole::Ui => "astrape:designer".to_string(),
        ComponentRole::Backend
        | ComponentRole::Database
        | ComponentRole::Api
        | ComponentRole::Shared
        | ComponentRole::Config
        | ComponentRole::Module => "astrape:executor".to_string(),
        ComponentRole::Coordinator => "astrape:architect".to_string(),
        ComponentRole::Testing => "astrape:qa-tester".to_string(),
        ComponentRole::Docs => "astrape:writer".to_string(),
    }
}

fn select_model_tier(component: &Component) -> ModelTier {
    if component.effort < 0.3 {
        ModelTier::Low
    } else if component.effort < 0.7 {
        ModelTier::Medium
    } else {
        ModelTier::High
    }
}

fn generate_acceptance_criteria(component: &Component, _analysis: &TaskAnalysis) -> Vec<String> {
    let mut criteria = vec![
        format!("{} implementation is complete", component.name),
        "Code compiles without errors".to_string(),
        "Tests pass".to_string(),
    ];

    match component.role {
        ComponentRole::Frontend | ComponentRole::Ui => {
            criteria.push("UI components render correctly".to_string());
            criteria.push("Responsive design works on all screen sizes".to_string());
        }
        ComponentRole::Backend | ComponentRole::Api => {
            criteria.push("API endpoints return expected responses".to_string());
            criteria.push("Error handling is implemented".to_string());
        }
        ComponentRole::Database => {
            criteria.push("Database schema is correct".to_string());
            criteria.push("Migrations run successfully".to_string());
        }
        _ => {}
    }

    criteria
}

fn generate_verification_steps(component: &Component, _analysis: &TaskAnalysis) -> Vec<String> {
    let mut steps = vec![
        "Run TypeScript compiler: tsc --noEmit".to_string(),
        "Run linter: eslint".to_string(),
        "Run tests: npm test".to_string(),
    ];

    match component.role {
        ComponentRole::Frontend | ComponentRole::Ui => {
            steps.push("Visual inspection of UI components".to_string());
        }
        ComponentRole::Backend | ComponentRole::Api => {
            steps.push("Test API endpoints with curl or Postman".to_string());
        }
        _ => {}
    }

    steps
}

fn infer_file_patterns(component: &Component, _context: &ProjectContext) -> Vec<String> {
    match component.role {
        ComponentRole::Frontend | ComponentRole::Ui => {
            vec![
                "src/components/**".to_string(),
                "src/pages/**".to_string(),
                "src/styles/**".to_string(),
            ]
        }
        ComponentRole::Backend | ComponentRole::Api => {
            vec![
                "src/api/**".to_string(),
                "src/routes/**".to_string(),
                "src/controllers/**".to_string(),
            ]
        }
        ComponentRole::Database => {
            vec![
                "src/db/**".to_string(),
                "src/models/**".to_string(),
                "migrations/**".to_string(),
            ]
        }
        ComponentRole::Shared => {
            vec![
                "src/types/**".to_string(),
                "src/utils/**".to_string(),
                "src/lib/**".to_string(),
            ]
        }
        ComponentRole::Testing => {
            vec![
                "**/*.test.ts".to_string(),
                "**/*.spec.ts".to_string(),
                "tests/**".to_string(),
            ]
        }
        ComponentRole::Docs => vec!["docs/**".to_string(), "*.md".to_string()],
        _ => vec![format!("src/{}/**", component.id)],
    }
}

fn infer_specific_files(_component: &Component, _context: &ProjectContext) -> Vec<String> {
    vec![]
}

fn calculate_execution_order(subtasks: &[Subtask]) -> Vec<Vec<String>> {
    let mut order = vec![];
    let mut completed = HashSet::new();
    let mut remaining: HashSet<String> = subtasks.iter().map(|st| st.id.clone()).collect();

    while !remaining.is_empty() {
        let mut batch = vec![];

        for subtask in subtasks {
            if remaining.contains(&subtask.id) {
                // Check if all dependencies are completed
                let can_run = subtask.blocked_by.iter().all(|dep| completed.contains(dep));

                if can_run {
                    batch.push(subtask.id.clone());
                }
            }
        }

        if batch.is_empty() {
            // Circular dependency or error
            order.push(remaining.iter().cloned().collect());
            break;
        }

        order.push(batch.clone());

        for id in &batch {
            remaining.remove(id);
            completed.insert(id.clone());
        }
    }

    order
}

fn validate_decomposition(subtasks: &[Subtask], shared_files: &[SharedFile]) -> Vec<String> {
    let mut warnings = vec![];

    // Check for ownership overlaps
    let mut pattern_owners: HashMap<String, Vec<String>> = HashMap::new();

    for subtask in subtasks {
        for pattern in &subtask.ownership.patterns {
            pattern_owners
                .entry(pattern.clone())
                .or_default()
                .push(subtask.id.clone());
        }
    }

    for (pattern, owners) in &pattern_owners {
        if owners.len() > 1 {
            let is_shared = shared_files.iter().any(|sf| &sf.pattern == pattern);
            if !is_shared {
                warnings.push(format!(
                    "Pattern \"{}\" is owned by multiple subtasks: {}",
                    pattern,
                    owners.join(", ")
                ));
            }
        }
    }

    // Check for subtasks with no file ownership
    for subtask in subtasks {
        if subtask.ownership.patterns.is_empty() && subtask.ownership.files.is_empty() {
            warnings.push(format!(
                "Subtask \"{}\" has no file ownership assigned",
                subtask.id
            ));
        }
    }

    warnings
}

fn explain_strategy(analysis: &TaskAnalysis, components: &[Component]) -> String {
    let mut explanation = format!("Task Type: {:?}\n", analysis.task_type);
    explanation.push_str(&format!(
        "Parallelizable: {}\n",
        if analysis.is_parallelizable {
            "Yes"
        } else {
            "No"
        }
    ));
    explanation.push_str(&format!("Components: {}\n\n", components.len()));

    if analysis.is_parallelizable {
        explanation.push_str(&format!(
            "This task has been decomposed into {} parallel components:\n",
            components.len()
        ));
        for component in components {
            explanation.push_str(&format!("- {} ({:?})\n", component.name, component.role));
        }
    } else {
        explanation.push_str(
            "This task is not suitable for parallelization and will be executed as a single component.\n",
        );
    }

    explanation
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_task_type() {
        assert_eq!(
            detect_task_type("build a fullstack app"),
            TaskType::FullstackApp
        );
        assert_eq!(
            detect_task_type("refactor the codebase"),
            TaskType::Refactoring
        );
        assert_eq!(detect_task_type("fix the bug"), TaskType::BugFix);
        assert_eq!(detect_task_type("add a new feature"), TaskType::Feature);
    }

    #[test]
    fn test_estimate_complexity() {
        let simple = estimate_complexity("fix typo", &TaskType::BugFix);
        assert!(simple < 0.5);

        let complex = estimate_complexity(
            "build a complex distributed system with multiple microservices",
            &TaskType::FullstackApp,
        );
        assert!(complex > 0.8);
    }

    #[test]
    fn test_extract_areas() {
        let areas = extract_areas("build frontend and backend", &TaskType::Feature);
        assert!(areas.contains(&"frontend".to_string()));
        assert!(areas.contains(&"backend".to_string()));
    }

    #[test]
    fn test_decompose_simple_task() {
        let context = ProjectContext::default();
        let result = decompose_task("fix a simple bug", context);

        assert_eq!(result.components.len(), 1);
        assert!(!result.analysis.is_parallelizable);
    }

    #[test]
    fn test_decompose_fullstack_task() {
        let context = ProjectContext::default();
        let result = decompose_task("build a fullstack app with frontend and backend", context);

        assert!(result.analysis.is_parallelizable);
        assert!(result.components.len() > 1);
    }
}
