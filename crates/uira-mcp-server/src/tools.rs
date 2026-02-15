use ast_grep_language::{LanguageExt, SupportLang};
use glob::glob;
use once_cell::sync::Lazy;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::Command;
use uira_orchestration::{LspClient, LspClientImpl, ToolContent, ToolOutput};
use uira_oxc::{LintRule, Linter, Severity};
use walkdir::WalkDir;

use uira_core::load_config;
use uira_orchestration::background_agent::{
    get_background_manager, BackgroundManager, BackgroundTaskConfig, BackgroundTaskStatus,
    LaunchInput,
};

use crate::anthropic_client;
use crate::router::{route_model, ModelPath};

static BACKGROUND_MANAGER: Lazy<Arc<BackgroundManager>> =
    Lazy::new(|| get_background_manager(BackgroundTaskConfig::default()));

fn get_extensions_for_lang(lang: &str) -> &'static [&'static str] {
    match lang {
        "typescript" => &["ts", "tsx", "mts", "cts"],
        "javascript" => &["js", "jsx", "mjs", "cjs"],
        "python" => &["py", "pyi"],
        "rust" => &["rs"],
        "go" => &["go"],
        "java" => &["java"],
        "c" => &["c", "h"],
        "cpp" => &["cpp", "hpp", "cc", "cxx", "c++", "h++"],
        "ruby" => &["rb", "rake"],
        "swift" => &["swift"],
        "kotlin" => &["kt", "kts"],
        "css" => &["css"],
        "html" => &["html", "htm"],
        "json" => &["json"],
        "yaml" => &["yaml", "yml"],
        "bash" => &["sh", "bash"],
        _ => &[],
    }
}

fn extract_text(output: ToolOutput) -> String {
    output
        .content
        .into_iter()
        .map(|c| match c {
            ToolContent::Text { text } => text,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub struct ToolExecutor {
    root_path: PathBuf,
    lsp_client: LspClientImpl,
}

impl ToolExecutor {
    pub fn new(root_path: PathBuf) -> Self {
        let lsp_client = LspClientImpl::new(root_path.clone());
        Self {
            root_path,
            lsp_client,
        }
    }

    fn collect_files_for_ast(&self, args: &Value, lang: &str) -> Result<Vec<PathBuf>, String> {
        let extensions = get_extensions_for_lang(lang);
        let mut files = Vec::new();
        let mut seen = HashSet::new();

        if let Some(paths) = args["paths"].as_array() {
            for path in paths {
                if let Some(p) = path.as_str() {
                    let full_path = self.root_path.join(p);
                    if full_path.is_file() && seen.insert(full_path.clone()) {
                        files.push(full_path);
                    } else if full_path.is_dir() {
                        self.walk_dir_for_extensions(&full_path, extensions, &mut files, &mut seen);
                    }
                }
            }
        }

        if let Some(globs) = args["globs"].as_array() {
            for g in globs {
                if let Some(pattern) = g.as_str() {
                    let full_pattern = self.root_path.join(pattern);
                    if let Ok(entries) = glob(full_pattern.to_string_lossy().as_ref()) {
                        for entry in entries.flatten() {
                            if entry.is_file() && seen.insert(entry.clone()) {
                                files.push(entry);
                            }
                        }
                    }
                }
            }
        }

        if files.is_empty() {
            self.walk_dir_for_extensions(&self.root_path, extensions, &mut files, &mut seen);
        }

        Ok(files)
    }

    fn walk_dir_for_extensions(
        &self,
        dir: &PathBuf,
        extensions: &[&str],
        files: &mut Vec<PathBuf>,
        seen: &mut HashSet<PathBuf>,
    ) {
        for entry in WalkDir::new(dir)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                !name.starts_with('.') && name != "node_modules" && name != "target"
            })
            .flatten()
        {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if extensions.contains(&ext) && seen.insert(path.to_path_buf()) {
                        files.push(path.to_path_buf());
                    }
                }
            }
        }
    }

    pub async fn execute(&self, tool_name: &str, args: Value) -> Result<String, String> {
        match tool_name {
            // LSP Tools - delegate to language servers
            "lsp_goto_definition" => self.lsp_goto_definition(args).await,
            "lsp_find_references" => self.lsp_find_references(args).await,
            "lsp_symbols" => self.lsp_symbols(args).await,
            "lsp_diagnostics" => self.lsp_diagnostics(args).await,
            "lsp_hover" => self.lsp_hover(args).await,
            "lsp_rename" => self.lsp_rename(args).await,

            // AST-grep Tools - native integration via ast-grep-language
            "ast_search" => self.ast_search(args).await,
            "ast_replace" => self.ast_replace(args).await,

            // Agent delegation with model routing
            "delegate_task" => self.delegate_task(args).await,

            // Background task tools
            "background_output" => self.background_output(args).await,
            "background_cancel" => self.background_cancel(args).await,

            _ => Err(format!("Unknown tool: {}", tool_name)),
        }
    }

    // =========================================================================
    // LSP Tools Implementation
    // =========================================================================

    async fn lsp_goto_definition(&self, args: Value) -> Result<String, String> {
        self.lsp_client
            .goto_definition(args)
            .await
            .map(extract_text)
            .map_err(|e| e.to_string())
    }

    async fn lsp_find_references(&self, args: Value) -> Result<String, String> {
        self.lsp_client
            .find_references(args)
            .await
            .map(extract_text)
            .map_err(|e| e.to_string())
    }

    async fn lsp_symbols(&self, args: Value) -> Result<String, String> {
        self.lsp_client
            .symbols(args)
            .await
            .map(extract_text)
            .map_err(|e| e.to_string())
    }

    async fn lsp_diagnostics(&self, args: Value) -> Result<String, String> {
        let file_path = args["filePath"].as_str().ok_or("Missing filePath")?;
        let severity_filter = args["severity"].as_str().unwrap_or("all");

        let is_js_ts = file_path.ends_with(".ts")
            || file_path.ends_with(".tsx")
            || file_path.ends_with(".js")
            || file_path.ends_with(".jsx")
            || file_path.ends_with(".mjs")
            || file_path.ends_with(".cjs");

        if is_js_ts {
            self.run_oxc_lint(file_path, severity_filter)
        } else if file_path.ends_with(".rs") {
            self.run_cargo_check(file_path).await
        } else if file_path.ends_with(".py") {
            self.run_pyright_diagnostics(file_path).await
        } else {
            Ok("No diagnostics available for this file type".to_string())
        }
    }

    async fn lsp_hover(&self, args: Value) -> Result<String, String> {
        self.lsp_client
            .hover(args)
            .await
            .map(extract_text)
            .map_err(|e| e.to_string())
    }

    async fn lsp_rename(&self, args: Value) -> Result<String, String> {
        self.lsp_client
            .rename(args)
            .await
            .map(extract_text)
            .map_err(|e| e.to_string())
    }

    async fn ast_search(&self, args: Value) -> Result<String, String> {
        let pattern = args["pattern"].as_str().ok_or("Missing pattern")?;
        let lang_str = args["lang"].as_str().ok_or("Missing lang")?;
        let context_lines = args["context"].as_u64().unwrap_or(0) as usize;

        let lang: SupportLang = lang_str
            .parse()
            .map_err(|_| format!("Unsupported language: {}", lang_str))?;

        let files = self.collect_files_for_ast(&args, lang_str)?;
        let mut results = Vec::new();

        for file_path in files {
            let content = match fs::read_to_string(&file_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let grep = lang.ast_grep(&content);
            let root = grep.root();

            for m in root.find_all(pattern) {
                let node = m.get_node();
                let start = node.start_pos();
                let end = node.end_pos();
                let matched_text = node.text().to_string();
                let start_line = start.line();
                let end_line = end.line();

                let mut result = json!({
                    "file": file_path.to_string_lossy(),
                    "start": { "line": start_line + 1, "column": start.column(node) + 1 },
                    "end": { "line": end_line + 1, "column": end.column(node) + 1 },
                    "text": matched_text,
                });

                if context_lines > 0 {
                    let lines: Vec<&str> = content.lines().collect();
                    let ctx_start = start_line.saturating_sub(context_lines);
                    let ctx_end = (end_line + context_lines + 1).min(lines.len());
                    let context: Vec<String> = lines[ctx_start..ctx_end]
                        .iter()
                        .enumerate()
                        .map(|(i, line)| format!("{:4}| {}", ctx_start + i + 1, line))
                        .collect();
                    result["context"] = json!(context.join("\n"));
                }

                results.push(result);
            }
        }

        if results.is_empty() {
            Ok("No matches found".to_string())
        } else {
            serde_json::to_string_pretty(&results)
                .map_err(|e| format!("Failed to serialize results: {}", e))
        }
    }

    async fn ast_replace(&self, args: Value) -> Result<String, String> {
        let pattern = args["pattern"].as_str().ok_or("Missing pattern")?;
        let rewrite = args["rewrite"].as_str().ok_or("Missing rewrite")?;
        let lang_str = args["lang"].as_str().ok_or("Missing lang")?;
        let dry_run = args["dryRun"].as_bool().unwrap_or(true);

        let lang: SupportLang = lang_str
            .parse()
            .map_err(|_| format!("Unsupported language: {}", lang_str))?;

        let files = self.collect_files_for_ast(&args, lang_str)?;
        let mut results = Vec::new();
        let mut files_modified = 0;

        for file_path in files {
            let content = match fs::read_to_string(&file_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let grep = lang.ast_grep(&content);
            let root = grep.root();
            let matches: Vec<_> = root.find_all(pattern).collect();

            if matches.is_empty() {
                continue;
            }

            let mut new_content = content.clone();
            let mut offset: i64 = 0;

            for m in &matches {
                let node = m.get_node();
                let edit = m.replace_by(rewrite);
                let inserted = String::from_utf8_lossy(&edit.inserted_text);
                let start_byte = (edit.position as i64 + offset) as usize;
                let end_byte = start_byte + edit.deleted_length;

                let before = &new_content[..start_byte];
                let after = &new_content[end_byte..];
                new_content = format!("{}{}{}", before, inserted, after);

                offset += edit.inserted_text.len() as i64 - edit.deleted_length as i64;

                let pos = node.start_pos();
                results.push(json!({
                    "file": file_path.to_string_lossy(),
                    "line": pos.line() + 1,
                    "original": node.text(),
                    "replacement": inserted,
                }));
            }

            if !dry_run {
                fs::write(&file_path, &new_content)
                    .map_err(|e| format!("Failed to write {}: {}", file_path.display(), e))?;
                files_modified += 1;
            }
        }

        if results.is_empty() {
            return Ok("No matches found for replacement".to_string());
        }

        let summary = if dry_run {
            format!(
                "[DRY RUN] {} replacements in {} files",
                results.len(),
                results
                    .iter()
                    .map(|r| r["file"].as_str().unwrap_or(""))
                    .collect::<HashSet<_>>()
                    .len()
            )
        } else {
            format!(
                "Applied {} replacements in {} files",
                results.len(),
                files_modified
            )
        };

        let output = json!({
            "summary": summary,
            "replacements": results,
        });

        serde_json::to_string_pretty(&output)
            .map_err(|e| format!("Failed to serialize results: {}", e))
    }

    fn run_oxc_lint(&self, file_path: &str, severity_filter: &str) -> Result<String, String> {
        let linter = Linter::new(LintRule::recommended());
        let diagnostics = linter.lint_file(file_path)?;

        if diagnostics.is_empty() {
            return Ok("No diagnostics found".to_string());
        }

        let filtered: Vec<_> = diagnostics
            .iter()
            .filter(|d| match severity_filter {
                "error" => matches!(d.severity, Severity::Error),
                "warning" => matches!(d.severity, Severity::Error | Severity::Warning),
                _ => true,
            })
            .collect();

        if filtered.is_empty() {
            return Ok("No diagnostics found".to_string());
        }

        let mut output = String::new();
        for d in filtered {
            let severity_str = match d.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
                Severity::Info => "info",
            };
            output.push_str(&format!(
                "{}:{}:{} {} [{}]: {}\n",
                d.file, d.line, d.column, severity_str, d.rule, d.message
            ));
            if let Some(suggestion) = &d.suggestion {
                output.push_str(&format!("  suggestion: {}\n", suggestion));
            }
        }

        Ok(output)
    }

    async fn run_cargo_check(&self, _file_path: &str) -> Result<String, String> {
        let output = Command::new("cargo")
            .arg("check")
            .arg("--message-format=short")
            .current_dir(&self.root_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| format!("Failed to run cargo check: {}", e))?;

        let stderr = String::from_utf8_lossy(&output.stderr);
        if output.status.success() {
            Ok("No Rust errors found".to_string())
        } else {
            Ok(stderr.to_string())
        }
    }

    async fn run_pyright_diagnostics(&self, file_path: &str) -> Result<String, String> {
        let output = Command::new("npx")
            .arg("pyright")
            .arg("--outputjson")
            .arg(file_path)
            .current_dir(&self.root_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| format!("Failed to run pyright: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.trim().is_empty() {
            Ok("No Python errors found".to_string())
        } else {
            Ok(stdout.to_string())
        }
    }

    // =========================================================================
    // Agent Delegation with Model Routing
    // =========================================================================

    async fn delegate_task(&self, args: Value) -> Result<String, String> {
        let run_in_background = args["runInBackground"].as_bool().unwrap_or(false);

        let agent = args["agent"].as_str().ok_or("Missing 'agent' parameter")?;

        if !agent
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(format!(
                "Invalid agent name '{}': only alphanumeric, hyphens, and underscores allowed",
                agent
            ));
        }

        let prompt = args["prompt"]
            .as_str()
            .ok_or("Missing 'prompt' parameter")?;

        // Model resolution priority:
        // 1. Explicit model parameter
        // 2. Agent config from uira.yml
        // 3. Default fallback
        let model = if let Some(explicit_model) = args["model"].as_str() {
            explicit_model.to_string()
        } else {
            // Try to load agent model from config
            load_config(None)
                .ok()
                .and_then(|config| {
                    config
                        .agents
                        .agents
                        .get(agent)
                        .and_then(|agent_config| agent_config.model.clone())
                })
                .unwrap_or_else(|| "claude-3-5-sonnet-20241022".to_string())
        };
        let model = model.as_str();

        let description = args["description"].as_str().unwrap_or(prompt);

        let allowed_tools: Option<Vec<String>> = args["allowedTools"].as_array().map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        });

        if run_in_background {
            let input = LaunchInput {
                description: description.to_string(),
                prompt: prompt.to_string(),
                agent: agent.to_string(),
                parent_session_id: "mcp".to_string(),
                model: Some(model.to_string()),
            };

            let task = BACKGROUND_MANAGER
                .launch(input)
                .map_err(|e| format!("Failed to launch background task: {}", e))?;

            let task_id = task.id.clone();
            let model_owned = model.to_string();
            let prompt_owned = prompt.to_string();
            let allowed_tools_owned = allowed_tools.clone();

            let handle = tokio::spawn(async move {
                let result = match route_model(&model_owned) {
                    ModelPath::Anthropic => {
                        anthropic_client::query(&prompt_owned, &model_owned, allowed_tools_owned)
                            .await
                    }
                    ModelPath::DirectProvider => {
                        Err("OpenCode proxy support has been removed. Use native Anthropic or OpenAI providers.".to_string())
                    }
                };

                match result {
                    Ok(output) => {
                        BACKGROUND_MANAGER.complete_task(&task_id, output);
                    }
                    Err(e) => {
                        BACKGROUND_MANAGER.fail_task(&task_id, e);
                    }
                }
            });

            // Spawn a watcher to handle panics/aborts
            let task_id_watcher = task.id.clone();
            tokio::spawn(async move {
                if let Err(e) = handle.await {
                    let error_msg = if e.is_panic() {
                        "Task panicked during execution".to_string()
                    } else if e.is_cancelled() {
                        "Task was cancelled by runtime".to_string()
                    } else {
                        format!("Task failed: {}", e)
                    };
                    BACKGROUND_MANAGER.fail_task(&task_id_watcher, error_msg);
                }
            });

            Ok(serde_json::to_string_pretty(&serde_json::json!({
                "taskId": task.id,
                "status": "running",
                "message": "Task started in background. Use background_output to get results."
            }))
            .unwrap())
        } else {
            match route_model(model) {
                ModelPath::Anthropic => {
                    tracing::info!(agent = %agent, model = %model, ?allowed_tools, "Spawning agent via Claude Agent SDK");
                    anthropic_client::query(prompt, model, allowed_tools).await
                }
                ModelPath::DirectProvider => {
                    Err("OpenCode proxy support has been removed. Use native Anthropic or OpenAI providers.".to_string())
                }
            }
        }
    }

    async fn background_output(&self, args: Value) -> Result<String, String> {
        let task_id = args["taskId"]
            .as_str()
            .ok_or("Missing 'taskId' parameter")?;
        let block = args["block"].as_bool().unwrap_or(false);
        let timeout_secs = args["timeout"].as_u64().unwrap_or(120);

        if block {
            let start = std::time::Instant::now();
            let timeout = std::time::Duration::from_secs(timeout_secs);

            loop {
                if let Some(task) = BACKGROUND_MANAGER.get_task(task_id) {
                    match task.status {
                        BackgroundTaskStatus::Completed => {
                            return Ok(task
                                .result
                                .unwrap_or_else(|| "Task completed with no output".to_string()));
                        }
                        BackgroundTaskStatus::Error => {
                            return Err(task
                                .error
                                .unwrap_or_else(|| "Task failed with unknown error".to_string()));
                        }
                        BackgroundTaskStatus::Cancelled => {
                            return Ok(serde_json::to_string_pretty(&serde_json::json!({
                                "taskId": task_id,
                                "status": "cancelled",
                                "message": "Task was cancelled"
                            }))
                            .unwrap());
                        }
                        _ => {
                            if start.elapsed() > timeout {
                                return Ok(serde_json::to_string_pretty(&serde_json::json!({
                                    "taskId": task_id,
                                    "status": format!("{:?}", task.status).to_lowercase(),
                                    "message": "Timeout waiting for task completion"
                                }))
                                .unwrap());
                            }
                            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        }
                    }
                } else {
                    return Err(format!("Task not found: {}", task_id));
                }
            }
        } else if let Some(task) = BACKGROUND_MANAGER.get_task(task_id) {
            // Non-blocking: return current status
            let status_str = match task.status {
                BackgroundTaskStatus::Queued => "queued",
                BackgroundTaskStatus::Pending => "pending",
                BackgroundTaskStatus::Running => "running",
                BackgroundTaskStatus::Completed => "completed",
                BackgroundTaskStatus::Error => "error",
                BackgroundTaskStatus::Cancelled => "cancelled",
            };

            let mut response = serde_json::json!({
                "taskId": task.id,
                "status": status_str,
                "agent": task.agent,
                "startedAt": task.started_at.to_rfc3339(),
            });

            if let Some(completed_at) = task.completed_at {
                response["completedAt"] = serde_json::json!(completed_at.to_rfc3339());
            }

            if let Some(result) = task.result {
                response["result"] = serde_json::json!(result);
            }

            if let Some(error) = task.error {
                response["error"] = serde_json::json!(error);
            }

            if let Some(progress) = task.progress {
                response["progress"] = serde_json::json!({
                    "toolCalls": progress.tool_calls,
                    "lastTool": progress.last_tool,
                    "lastUpdate": progress.last_update.to_rfc3339(),
                });
            }

            Ok(serde_json::to_string_pretty(&response).unwrap())
        } else {
            Err(format!("Task not found: {}", task_id))
        }
    }

    async fn background_cancel(&self, args: Value) -> Result<String, String> {
        let cancel_all = args["all"].as_bool().unwrap_or(false);

        if cancel_all {
            let tasks = BACKGROUND_MANAGER.get_all_tasks();
            let mut cancelled = 0;
            for task in tasks {
                if !matches!(
                    task.status,
                    BackgroundTaskStatus::Completed
                        | BackgroundTaskStatus::Error
                        | BackgroundTaskStatus::Cancelled
                ) {
                    BACKGROUND_MANAGER.cancel_task(&task.id);
                    cancelled += 1;
                }
            }
            Ok(serde_json::to_string_pretty(&serde_json::json!({
                "cancelled": cancelled,
                "message": format!("Cancelled {} background task(s)", cancelled)
            }))
            .unwrap())
        } else if let Some(task_id) = args["taskId"].as_str() {
            if let Some(task) = BACKGROUND_MANAGER.cancel_task(task_id) {
                let status_str = match task.status {
                    BackgroundTaskStatus::Queued => "queued",
                    BackgroundTaskStatus::Pending => "pending",
                    BackgroundTaskStatus::Running => "running",
                    BackgroundTaskStatus::Completed => "completed",
                    BackgroundTaskStatus::Error => "error",
                    BackgroundTaskStatus::Cancelled => "cancelled",
                };
                let message = if task.status == BackgroundTaskStatus::Cancelled {
                    "Task cancelled successfully"
                } else {
                    "Task was already in terminal state"
                };
                Ok(serde_json::to_string_pretty(&serde_json::json!({
                    "taskId": task.id,
                    "status": status_str,
                    "message": message
                }))
                .unwrap())
            } else {
                Err(format!("Task not found: {}", task_id))
            }
        } else {
            Err("Must provide either 'taskId' or 'all: true'".to_string())
        }
    }
}
