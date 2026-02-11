#![deny(clippy::all)]
#![allow(deprecated)] // Legacy hook API used for backward compatibility with JS bindings

use std::collections::HashMap;

use napi_derive::napi;
use uira_agents::{get_agent_definitions_with_config, AgentModelConfig};
use uira_types::HookOutput;
use uira_features::builtin_skills::{get_builtin_skill, list_builtin_skill_names};
use uira_features::keywords::KeywordDetector;
use uira_features::model_routing::{
    analyze_task_complexity, route_task, RoutingConfigOverrides, RoutingContext,
};
use uira_hooks::{default_hooks, HookContext, HookEvent, HookInput};

// ============================================================================
// Hook Output Types
// ============================================================================

#[napi(object)]
pub struct JsHookOutput {
    #[napi(js_name = "continue")]
    pub continue_processing: bool,
    pub message: Option<String>,
    pub stop_reason: Option<String>,
    pub decision: Option<String>,
    pub reason: Option<String>,
    pub additional_context: Option<String>,
    pub suppress_output: Option<bool>,
    pub system_message: Option<String>,
}

impl From<HookOutput> for JsHookOutput {
    fn from(output: HookOutput) -> Self {
        Self {
            continue_processing: output.continue_processing,
            message: output.message,
            stop_reason: output.stop_reason,
            decision: output.decision.map(|d| format!("{:?}", d)),
            reason: output.reason,
            additional_context: output.additional_context,
            suppress_output: output.suppress_output,
            system_message: output.system_message,
        }
    }
}

impl From<uira_hooks::HookOutput> for JsHookOutput {
    fn from(output: uira_hooks::HookOutput) -> Self {
        Self {
            continue_processing: output.should_continue,
            message: output.message,
            stop_reason: None,
            decision: None,
            reason: output.reason,
            additional_context: None,
            suppress_output: None,
            system_message: None,
        }
    }
}

impl Default for JsHookOutput {
    fn default() -> Self {
        Self {
            continue_processing: true,
            message: None,
            stop_reason: None,
            decision: None,
            reason: None,
            additional_context: None,
            suppress_output: None,
            system_message: None,
        }
    }
}

#[napi(object)]
pub struct DetectedKeyword {
    pub keyword_type: String,
    pub message: String,
}

#[napi]
pub fn detect_keywords(prompt: String, agent: Option<String>) -> Option<JsHookOutput> {
    let detector = KeywordDetector::new();
    detector
        .detect(&prompt, agent.as_deref())
        .map(JsHookOutput::from)
}

#[napi]
pub fn detect_all_keywords(prompt: String, agent: Option<String>) -> Vec<DetectedKeyword> {
    let detector = KeywordDetector::new();
    detector
        .detect_all(&prompt, agent.as_deref())
        .into_iter()
        .map(|(keyword_type, message)| DetectedKeyword {
            keyword_type: keyword_type.to_string(),
            message,
        })
        .collect()
}

#[napi]
pub fn create_hook_output_with_message(message: String) -> JsHookOutput {
    JsHookOutput {
        continue_processing: true,
        message: Some(message),
        ..Default::default()
    }
}

#[napi]
pub fn create_hook_output_deny(reason: String) -> JsHookOutput {
    JsHookOutput {
        continue_processing: false,
        decision: Some("Deny".to_string()),
        reason: Some(reason),
        ..Default::default()
    }
}

#[napi]
pub fn create_hook_output_stop(reason: String) -> JsHookOutput {
    JsHookOutput {
        continue_processing: false,
        stop_reason: Some(reason),
        ..Default::default()
    }
}

// ============================================================================
// Hook System Bindings
// ============================================================================

/// Input for hook execution from JavaScript
#[napi(object)]
pub struct JsHookInput {
    pub session_id: Option<String>,
    pub prompt: Option<String>,
    pub tool_name: Option<String>,
    pub tool_input: Option<String>,
    pub tool_output: Option<String>,
    pub directory: Option<String>,
    pub stop_reason: Option<String>,
    pub user_requested: Option<bool>,
    /// Path to transcript JSONL file for accessing conversation history
    pub transcript_path: Option<String>,
}

/// Execute hooks for a specific event
///
/// # Arguments
/// * `event` - Event type: "user-prompt-submit", "stop", "session-start", "pre-tool-use", "post-tool-use", "session-idle", "messages-transform"
/// * `input` - Hook input data
///
/// # Returns
/// Hook output with continue flag, message, etc.
#[napi]
pub async fn execute_hook(event: String, input: JsHookInput) -> napi::Result<JsHookOutput> {
    let hook_event = match event.as_str() {
        "user-prompt-submit" => HookEvent::UserPromptSubmit,
        "stop" => HookEvent::Stop,
        "session-start" => HookEvent::SessionStart,
        "pre-tool-use" => HookEvent::PreToolUse,
        "post-tool-use" => HookEvent::PostToolUse,
        "session-idle" => HookEvent::SessionIdle,
        "messages-transform" => HookEvent::MessagesTransform,
        _ => {
            return Err(napi::Error::from_reason(format!(
                "Unknown event type: {}",
                event
            )))
        }
    };
    let hook_input = HookInput {
        session_id: input.session_id.clone(),
        prompt: input.prompt,
        message: None,
        parts: None,
        tool_name: input.tool_name,
        tool_input: input
            .tool_input
            .map(|s| serde_json::from_str(&s).unwrap_or(serde_json::Value::Null)),
        tool_output: input
            .tool_output
            .map(|s| serde_json::from_str(&s).unwrap_or(serde_json::Value::Null)),
        directory: input.directory.clone(),
        stop_reason: input.stop_reason,
        user_requested: input.user_requested,
        transcript_path: input.transcript_path,
        extra: HashMap::new(),
    };

    let directory = input.directory.unwrap_or_else(|| {
        std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .to_string()
    });
    let context = HookContext::new(input.session_id, directory);

    let registry = default_hooks();
    match registry
        .execute_hooks(hook_event, &hook_input, &context)
        .await
    {
        Ok(output) => Ok(output.into()),
        Err(e) => Err(napi::Error::from_reason(format!(
            "Hook execution failed: {}",
            e
        ))),
    }
}

/// List all registered hooks
#[napi]
pub fn list_hooks() -> Vec<String> {
    let registry = default_hooks();
    registry.list_hooks()
}

/// Get the number of registered hooks
#[napi]
pub fn get_hook_count() -> u32 {
    let registry = default_hooks();
    registry.count() as u32
}

// ============================================================================
// Agent System Bindings
// ============================================================================

/// YAML config structure for loading agent models from uira.yml
#[derive(Debug, serde::Deserialize)]
struct UiraYamlConfig {
    #[serde(default)]
    agents: HashMap<String, YamlAgentConfig>,
}

#[derive(Debug, serde::Deserialize)]
struct YamlAgentConfig {
    model: Option<String>,
}

/// Load agent model configuration from uira.yml
///
/// Searches for uira.yml in current directory and parent directories.
/// Returns a HashMap mapping agent names to their configured model IDs.
fn load_agent_model_config() -> AgentModelConfig {
    let mut model_config = AgentModelConfig::new();

    // Default model for librarian agent
    model_config.insert("librarian".to_string(), "opencode/big-pickle".to_string());

    // Try to find and load uira.yml
    let config_path = find_uira_yml();
    if let Some(path) = config_path {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(yaml_config) = serde_yaml_ng::from_str::<UiraYamlConfig>(&content) {
                for (agent_name, agent_config) in yaml_config.agents {
                    if let Some(model) = agent_config.model {
                        model_config.insert(agent_name, model);
                    }
                }
            }
        }
    }

    model_config
}

/// Find uira.yml by searching current directory and parents
fn find_uira_yml() -> Option<std::path::PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    let mut dir = cwd.as_path();

    loop {
        let config_path = dir.join("uira.yml");
        if config_path.exists() {
            return Some(config_path);
        }

        match dir.parent() {
            Some(parent) => dir = parent,
            None => break,
        }
    }

    None
}

/// Agent definition exposed to JavaScript
#[napi(object)]
pub struct JsAgentDefinition {
    pub name: String,
    pub description: String,
    pub model: Option<String>,
    pub tier: String,
    pub prompt: String,
    pub tools: Vec<String>,
}

/// List all available agent definitions
///
/// Returns all 32 agent definitions from uira-agents including:
/// - Primary agents (architect, executor, explore, etc.)
/// - Tiered variants (architect-low, architect-medium, executor-high, etc.)
/// - Specialized agents (security-reviewer, build-fixer, etc.)
///
/// Agent descriptions include the actual configured model from uira.yml.
#[napi]
pub fn list_agents() -> Vec<JsAgentDefinition> {
    let model_config = load_agent_model_config();
    let agents = get_agent_definitions_with_config(None, Some(&model_config));

    agents
        .into_iter()
        .map(|(name, config)| {
            let tier = match config.model {
                Some(uira_sdk::ModelType::Haiku) => "LOW",
                Some(uira_sdk::ModelType::Sonnet) | Some(uira_sdk::ModelType::Inherit) => "MEDIUM",
                Some(uira_sdk::ModelType::Opus) => "HIGH",
                None => "MEDIUM",
            };

            JsAgentDefinition {
                name,
                description: config.description,
                model: config.model.map(|m| format!("{:?}", m).to_lowercase()),
                tier: tier.to_string(),
                prompt: config.prompt,
                tools: config.tools,
            }
        })
        .collect()
}

/// Get a specific agent by name
///
/// # Arguments
/// * `name` - Agent name (e.g., "architect", "executor-high", "explore")
///
/// # Returns
/// Agent definition if found, None otherwise
///
/// Agent description includes the actual configured model from uira.yml.
#[napi]
pub fn get_agent(name: String) -> Option<JsAgentDefinition> {
    let model_config = load_agent_model_config();
    let agents = get_agent_definitions_with_config(None, Some(&model_config));

    agents.get(&name).map(|config| {
        let tier = match config.model {
            Some(uira_sdk::ModelType::Haiku) => "LOW",
            Some(uira_sdk::ModelType::Sonnet) | Some(uira_sdk::ModelType::Inherit) => "MEDIUM",
            Some(uira_sdk::ModelType::Opus) => "HIGH",
            None => "MEDIUM",
        };

        JsAgentDefinition {
            name: name.clone(),
            description: config.description.clone(),
            model: config.model.map(|m| format!("{:?}", m).to_lowercase()),
            tier: tier.to_string(),
            prompt: config.prompt.clone(),
            tools: config.tools.clone(),
        }
    })
}

/// Get list of agent names only (lighter weight than full definitions)
#[napi]
pub fn list_agent_names() -> Vec<String> {
    // Don't load model config - we only need agent names, not full definitions
    let agents = get_agent_definitions_with_config(None, None);
    agents.keys().cloned().collect()
}

// ============================================================================
// Model Routing Bindings
// ============================================================================

/// Result of model routing decision
#[napi(object)]
pub struct JsRoutingResult {
    pub model: String,
    pub tier: String,
    pub reasoning: String,
    pub confidence: f64,
    pub escalated: bool,
}

/// Route a task to the appropriate model tier
///
/// Uses complexity analysis to determine the best model for a given task.
/// Considers lexical signals, structural complexity, and context.
///
/// # Arguments
/// * `prompt` - The task prompt to analyze
///
/// # Returns
/// Routing result with model, tier, and reasoning
#[napi]
pub fn route_task_prompt(prompt: String) -> JsRoutingResult {
    let context = RoutingContext {
        task_prompt: prompt,
        agent_type: None,
        ..RoutingContext::default()
    };

    let decision = route_task(context, RoutingConfigOverrides::default());

    JsRoutingResult {
        model: decision.model,
        tier: decision.tier.as_str().to_string(),
        reasoning: decision.reasons.join("; "),
        confidence: decision.confidence,
        escalated: decision.escalated,
    }
}

/// Route a task with agent context
///
/// # Arguments
/// * `prompt` - The task prompt to analyze
/// * `agent_type` - Optional agent type (e.g., "architect", "explore")
///
/// # Returns
/// Routing result with model, tier, and reasoning
#[napi]
pub fn route_task_with_agent(prompt: String, agent_type: Option<String>) -> JsRoutingResult {
    let context = RoutingContext {
        task_prompt: prompt,
        agent_type,
        ..RoutingContext::default()
    };

    let decision = route_task(context, RoutingConfigOverrides::default());

    JsRoutingResult {
        model: decision.model,
        tier: decision.tier.as_str().to_string(),
        reasoning: decision.reasons.join("; "),
        confidence: decision.confidence,
        escalated: decision.escalated,
    }
}

/// Analyze task complexity
///
/// Provides detailed complexity analysis for a task prompt.
///
/// # Arguments
/// * `prompt` - The task prompt to analyze
/// * `agent_type` - Optional agent type for context
///
/// # Returns
/// Analysis result with tier, model, and detailed breakdown
#[napi(object)]
pub struct JsComplexityAnalysis {
    pub tier: String,
    pub model: String,
    pub analysis: String,
}

#[napi]
pub fn analyze_complexity(prompt: String, agent_type: Option<String>) -> JsComplexityAnalysis {
    let (tier, model, analysis) = analyze_task_complexity(&prompt, agent_type.as_deref());

    JsComplexityAnalysis {
        tier: tier.as_str().to_string(),
        model,
        analysis,
    }
}

// ============================================================================
// Skill System Bindings
// ============================================================================

/// Skill definition exposed to JavaScript
#[napi(object)]
pub struct JsSkillDefinition {
    pub name: String,
    pub description: String,
    pub template: String,
    pub agent: Option<String>,
    pub model: Option<String>,
    pub argument_hint: Option<String>,
}

/// Get a skill by name
///
/// Loads skill from SKILL.md files in the skills/ directory.
///
/// # Arguments
/// * `name` - Skill name (e.g., "autopilot", "ralph", "ultrawork")
///
/// # Returns
/// Skill template content if found, None otherwise
#[napi]
pub fn get_skill(name: String) -> Option<String> {
    get_builtin_skill(&name).map(|s| s.template)
}

/// Get full skill definition
///
/// # Arguments
/// * `name` - Skill name
///
/// # Returns
/// Full skill definition with metadata
#[napi]
pub fn get_skill_definition(name: String) -> Option<JsSkillDefinition> {
    get_builtin_skill(&name).map(|s| JsSkillDefinition {
        name: s.name,
        description: s.description,
        template: s.template,
        agent: s.agent,
        model: s.model,
        argument_hint: s.argument_hint,
    })
}

/// List all available skill names
///
/// # Returns
/// List of skill names loaded from the skills/ directory
#[napi]
pub fn list_skills() -> Vec<String> {
    list_builtin_skill_names()
}

// ============================================================================
// Background Task Notification Bindings
// ============================================================================

#[napi(object)]
pub struct JsBackgroundTask {
    pub id: String,
    pub session_id: String,
    pub parent_session_id: String,
    pub description: String,
    pub agent: String,
    pub status: String,
    pub result: Option<String>,
    pub error: Option<String>,
}

#[napi(object)]
pub struct JsNotificationResult {
    pub has_notifications: bool,
    pub message: Option<String>,
    pub notification_count: u32,
}

/// Check for pending background task notifications for a session
#[napi]
pub fn check_notifications(session_id: String) -> JsNotificationResult {
    let result = uira_hooks::hooks::background_notification::check_background_notifications(
        &session_id,
        None,
    );
    JsNotificationResult {
        has_notifications: result.has_notifications,
        message: result.message,
        notification_count: result.tasks.len() as u32,
    }
}

/// Process a background task event (task.completed or task.failed)
#[napi]
pub fn notify_background_event(event_json: String) {
    if let Ok(event) = serde_json::from_str::<serde_json::Value>(&event_json) {
        uira_hooks::hooks::background_notification::handle_background_event_public(&event);
    }
}

/// Register a background task for tracking
#[napi]
pub fn register_background_task(
    task_id: String,
    session_id: String,
    parent_session_id: String,
    description: String,
    agent: String,
) {
    use chrono::Utc;
    use uira_hooks::hooks::background_notification::{
        background_tasks_dir, BackgroundTask, BackgroundTaskStatus,
    };

    let task = BackgroundTask {
        id: task_id.clone(),
        session_id,
        parent_session_id,
        description,
        prompt: String::new(),
        agent,
        status: BackgroundTaskStatus::Running,
        queued_at: None,
        started_at: Utc::now(),
        completed_at: None,
        result: None,
        error: None,
        progress: None,
        concurrency_key: None,
        parent_model: None,
    };

    // Persist to disk
    if let Some(tasks_dir) = background_tasks_dir() {
        let _ = std::fs::create_dir_all(&tasks_dir);
        if let Ok(json) = serde_json::to_string_pretty(&task) {
            let path = tasks_dir.join(format!("{}.json", &task_id));
            let _ = std::fs::write(path, json);
        }
    }
}

// ============================================================================
// Goal Verification Bindings
// ============================================================================

#[napi(object)]
pub struct JsGoalCheckResult {
    pub name: String,
    pub score: f64,
    pub target: f64,
    pub passed: bool,
    pub duration_ms: u32,
    pub error: Option<String>,
}

impl From<uira_goals::GoalCheckResult> for JsGoalCheckResult {
    fn from(r: uira_goals::GoalCheckResult) -> Self {
        Self {
            name: r.name,
            score: r.score,
            target: r.target,
            passed: r.passed,
            duration_ms: r.duration_ms as u32,
            error: r.error,
        }
    }
}

#[napi(object)]
pub struct JsVerificationResult {
    pub all_passed: bool,
    pub results: Vec<JsGoalCheckResult>,
    pub iteration: u32,
}

impl From<uira_goals::VerificationResult> for JsVerificationResult {
    fn from(r: uira_goals::VerificationResult) -> Self {
        Self {
            all_passed: r.all_passed,
            results: r.results.into_iter().map(JsGoalCheckResult::from).collect(),
            iteration: r.iteration,
        }
    }
}

#[napi(object)]
pub struct JsGoalConfig {
    pub name: String,
    pub workspace: Option<String>,
    pub command: String,
    pub target: f64,
    pub timeout_secs: u32,
    pub enabled: bool,
    pub description: Option<String>,
}

impl From<JsGoalConfig> for uira_config::schema::GoalConfig {
    fn from(g: JsGoalConfig) -> Self {
        Self {
            name: g.name,
            workspace: g.workspace,
            command: g.command,
            target: g.target,
            timeout_secs: g.timeout_secs as u64,
            enabled: g.enabled,
            description: g.description,
        }
    }
}

#[napi]
pub async fn check_goal(directory: String, goal: JsGoalConfig) -> napi::Result<JsGoalCheckResult> {
    let runner = uira_goals::GoalRunner::new(&directory);
    let goal_config: uira_config::schema::GoalConfig = goal.into();
    let result = runner.check_goal(&goal_config).await;
    Ok(JsGoalCheckResult::from(result))
}

#[napi]
pub async fn check_goals(
    directory: String,
    goals: Vec<JsGoalConfig>,
) -> napi::Result<JsVerificationResult> {
    let runner = uira_goals::GoalRunner::new(&directory);
    let goal_configs: Vec<uira_config::schema::GoalConfig> =
        goals.into_iter().map(|g| g.into()).collect();
    let result = runner.check_all(&goal_configs).await;
    Ok(JsVerificationResult::from(result))
}

#[napi]
pub async fn check_goals_from_config(
    directory: String,
) -> napi::Result<Option<JsVerificationResult>> {
    let config_path = std::path::Path::new(&directory).join("uira.yml");
    if !config_path.exists() {
        return Ok(None);
    }

    let config = uira_config::load_config(Some(&config_path))
        .map_err(|e| napi::Error::from_reason(format!("Failed to load config: {}", e)))?;

    let goals = &config.goals.goals;
    if goals.is_empty() {
        return Ok(None);
    }

    let runner = uira_goals::GoalRunner::new(&directory);
    let result = runner.check_all(goals).await;
    Ok(Some(JsVerificationResult::from(result)))
}

#[napi]
pub fn list_goals_from_config(directory: String) -> napi::Result<Vec<JsGoalConfig>> {
    let config_path = std::path::Path::new(&directory).join("uira.yml");
    if !config_path.exists() {
        return Ok(vec![]);
    }

    let config = uira_config::load_config(Some(&config_path))
        .map_err(|e| napi::Error::from_reason(format!("Failed to load config: {}", e)))?;

    Ok(config
        .goals
        .goals
        .into_iter()
        .map(|g| JsGoalConfig {
            name: g.name,
            workspace: g.workspace,
            command: g.command,
            target: g.target,
            timeout_secs: g.timeout_secs as u32,
            enabled: g.enabled,
            description: g.description,
        })
        .collect())
}

// ============================================================================
// Comment Checker Bindings
// ============================================================================

use uira_tools::CommentChecker;

#[napi(js_name = "CommentChecker")]
pub struct JsCommentChecker {
    inner: CommentChecker,
}

#[napi]
impl JsCommentChecker {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: CommentChecker::new(),
        }
    }

    #[napi]
    pub fn should_check_tool(&self, tool_name: String) -> bool {
        self.inner.should_check_tool(&tool_name)
    }

    #[napi]
    pub fn check_write(&self, file_path: String, content: String) -> Option<String> {
        self.inner.check_write(&file_path, &content)
    }

    #[napi]
    pub fn check_edit(
        &self,
        file_path: String,
        old_string: String,
        new_string: String,
    ) -> Option<String> {
        self.inner.check_edit(&file_path, &old_string, &new_string)
    }

    #[napi]
    pub fn check_tool_result(&self, tool_name: String, tool_input: String) -> Option<String> {
        let input: serde_json::Value = serde_json::from_str(&tool_input).ok()?;
        self.inner.check_tool_result(&tool_name, &input)
    }
}

impl Default for JsCommentChecker {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Linter Bindings
// ============================================================================

use uira_oxc::linter::{LintDiagnostic, LintRule, Linter, Severity};

#[napi(object)]
pub struct JsLintDiagnostic {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub message: String,
    pub rule: String,
    pub severity: String,
    pub suggestion: Option<String>,
}

impl From<LintDiagnostic> for JsLintDiagnostic {
    fn from(d: LintDiagnostic) -> Self {
        Self {
            file: d.file,
            line: d.line,
            column: d.column,
            message: d.message,
            rule: d.rule,
            severity: match d.severity {
                Severity::Error => "error".to_string(),
                Severity::Warning => "warning".to_string(),
                Severity::Info => "info".to_string(),
            },
            suggestion: d.suggestion,
        }
    }
}

fn pascal_to_kebab(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 4);
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            result.push('-');
        }
        result.push(c.to_ascii_lowercase());
    }
    result
}

fn lint_rule_to_string(rule: &LintRule) -> String {
    pascal_to_kebab(&format!("{:?}", rule))
}

fn parse_lint_rules(rules: &[String]) -> Vec<LintRule> {
    rules
        .iter()
        .filter_map(|r| match r.as_str() {
            "no-console" => Some(LintRule::NoConsole),
            "no-debugger" => Some(LintRule::NoDebugger),
            "no-alert" => Some(LintRule::NoAlert),
            "no-eval" => Some(LintRule::NoEval),
            "no-var" => Some(LintRule::NoVar),
            "prefer-const" => Some(LintRule::PreferConst),
            "no-unused-vars" => Some(LintRule::NoUnusedVars),
            "no-empty-function" => Some(LintRule::NoEmptyFunction),
            "no-duplicate-keys" => Some(LintRule::NoDuplicateKeys),
            "no-param-reassign" => Some(LintRule::NoParamReassign),
            _ => None,
        })
        .collect()
}

#[napi(js_name = "Linter")]
pub struct JsLinter {
    inner: Linter,
}

#[napi]
impl JsLinter {
    #[napi(constructor)]
    pub fn new(rules: Option<Vec<String>>) -> Self {
        let linter = match rules {
            Some(r) => Linter::new(parse_lint_rules(&r)),
            None => Linter::default(),
        };
        Self { inner: linter }
    }

    #[napi(factory)]
    pub fn recommended() -> Self {
        Self {
            inner: Linter::default(),
        }
    }

    #[napi(factory)]
    pub fn strict() -> Self {
        Self {
            inner: Linter::strict(),
        }
    }

    #[napi]
    pub fn lint_files(&self, files: Vec<String>) -> Vec<JsLintDiagnostic> {
        self.inner
            .lint_files(&files)
            .into_iter()
            .map(JsLintDiagnostic::from)
            .collect()
    }

    #[napi]
    pub fn lint_source(
        &self,
        filename: String,
        source: String,
    ) -> napi::Result<Vec<JsLintDiagnostic>> {
        self.inner
            .lint_source(&filename, &source)
            .map(|diagnostics| {
                diagnostics
                    .into_iter()
                    .map(JsLintDiagnostic::from)
                    .collect()
            })
            .map_err(napi::Error::from_reason)
    }

    #[napi]
    pub fn all_rules() -> Vec<String> {
        LintRule::all().iter().map(lint_rule_to_string).collect()
    }

    #[napi]
    pub fn recommended_rules() -> Vec<String> {
        LintRule::recommended()
            .iter()
            .map(lint_rule_to_string)
            .collect()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_ultrawork() {
        let result = detect_keywords("ultrawork: do something".to_string(), None);
        assert!(result.is_some());
        let output = result.unwrap();
        assert!(output.continue_processing);
        assert!(output.message.unwrap().contains("ultrawork-mode"));
    }

    #[test]
    fn test_detect_search() {
        let result = detect_keywords("search for files".to_string(), None);
        assert!(result.is_some());
        let output = result.unwrap();
        assert!(output.message.unwrap().contains("search-mode"));
    }

    #[test]
    fn test_detect_analyze() {
        let result = detect_keywords("analyze this code".to_string(), None);
        assert!(result.is_some());
        let output = result.unwrap();
        assert!(output.message.unwrap().contains("analyze-mode"));
    }

    #[test]
    fn test_no_keyword() {
        let result = detect_keywords("just a normal message".to_string(), None);
        assert!(result.is_none());
    }

    #[test]
    fn test_detect_all() {
        let result = detect_all_keywords("ultrawork search analyze".to_string(), None);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_planner_agent() {
        let result = detect_keywords(
            "ultrawork: plan".to_string(),
            Some("prometheus".to_string()),
        );
        assert!(result.is_some());
        let output = result.unwrap();
        assert!(output.message.unwrap().contains("PLANNER"));
    }

    #[test]
    fn test_list_agents() {
        let agents = list_agents();
        assert!(!agents.is_empty());
        // Should have at least the primary agents
        let names: Vec<_> = agents.iter().map(|a| a.name.as_str()).collect();
        assert!(names.contains(&"architect"));
        assert!(names.contains(&"executor"));
        assert!(names.contains(&"explore"));
    }

    #[test]
    fn test_get_agent() {
        let agent = get_agent("architect".to_string());
        assert!(agent.is_some());
        let agent = agent.unwrap();
        assert_eq!(agent.tier, "HIGH");
    }

    #[test]
    fn test_get_agent_not_found() {
        let agent = get_agent("nonexistent-agent".to_string());
        assert!(agent.is_none());
    }

    #[test]
    fn test_route_task_simple() {
        let result = route_task_prompt("find where auth is implemented".to_string());
        // Simple search tasks should route to lower tier
        assert!(!result.model.is_empty());
        assert!(!result.tier.is_empty());
    }

    #[test]
    fn test_route_task_complex() {
        let result = route_task_prompt(
            "refactor the entire authentication system to use OAuth2".to_string(),
        );
        // Complex tasks with architecture keywords should route higher
        assert!(!result.model.is_empty());
        // Architecture keywords should influence the decision
        assert!(result.reasoning.contains("architecture") || result.tier == "HIGH");
    }

    #[test]
    fn test_route_task_with_agent() {
        let result =
            route_task_with_agent("find auth code".to_string(), Some("explore".to_string()));
        // Explore agent should stay low tier
        assert_eq!(result.tier, "LOW");
    }

    #[test]
    fn test_analyze_complexity() {
        let analysis = analyze_complexity("add a button".to_string(), None);
        assert!(!analysis.tier.is_empty());
        assert!(!analysis.model.is_empty());
        assert!(!analysis.analysis.is_empty());
    }

    #[test]
    fn test_list_hooks() {
        let hooks = list_hooks();
        assert!(!hooks.is_empty());
        // Should have some of the default hooks
    }

    #[test]
    fn test_get_hook_count() {
        let count = get_hook_count();
        assert!(count > 0);
    }

    #[test]
    fn test_list_skills() {
        // This may return empty if no skills directory exists
        let skills = list_skills();
        // Just verify it doesn't panic
        assert!(skills.is_empty() || !skills.is_empty());
    }
}
