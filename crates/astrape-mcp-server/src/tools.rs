//! Tool execution for the MCP server

use astrape_oxc::{LintRule, Linter, Severity};
use astrape_tools::{LspClient, LspClientImpl, ToolContent, ToolOutput};
use serde_json::Value;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;

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

    pub async fn execute(&self, tool_name: &str, args: Value) -> Result<String, String> {
        match tool_name {
            // LSP Tools - delegate to language servers
            "lsp_goto_definition" => self.lsp_goto_definition(args).await,
            "lsp_find_references" => self.lsp_find_references(args).await,
            "lsp_symbols" => self.lsp_symbols(args).await,
            "lsp_diagnostics" => self.lsp_diagnostics(args).await,
            "lsp_hover" => self.lsp_hover(args).await,
            "lsp_rename" => self.lsp_rename(args).await,

            // AST-grep Tools - use sg CLI
            "ast_search" => self.ast_search(args).await,
            "ast_replace" => self.ast_replace(args).await,

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
            self.run_oxc_lint(file_path, severity_filter).await
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

    // =========================================================================
    // AST-grep Tools Implementation
    // =========================================================================

    async fn ast_search(&self, args: Value) -> Result<String, String> {
        let pattern = args["pattern"].as_str().ok_or("Missing pattern")?;
        let lang = args["lang"].as_str().ok_or("Missing lang")?;

        let mut cmd = Command::new("sg");
        cmd.arg("scan")
            .arg("--pattern")
            .arg(pattern)
            .arg("--lang")
            .arg(lang)
            .arg("--json")
            .current_dir(&self.root_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Add paths if specified
        if let Some(paths) = args["paths"].as_array() {
            for path in paths {
                if let Some(p) = path.as_str() {
                    cmd.arg(p);
                }
            }
        }

        // Add globs if specified
        if let Some(globs) = args["globs"].as_array() {
            for glob in globs {
                if let Some(g) = glob.as_str() {
                    cmd.arg("--globs").arg(g);
                }
            }
        }

        let output = cmd.output().await.map_err(|e| {
            format!(
                "Failed to run ast-grep: {}. Install with: cargo install ast-grep",
                e
            )
        })?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.trim().is_empty() {
                Ok("No matches found".to_string())
            } else {
                Ok(stdout.to_string())
            }
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("ast-grep error: {}", stderr))
        }
    }

    async fn ast_replace(&self, args: Value) -> Result<String, String> {
        let pattern = args["pattern"].as_str().ok_or("Missing pattern")?;
        let rewrite = args["rewrite"].as_str().ok_or("Missing rewrite")?;
        let lang = args["lang"].as_str().ok_or("Missing lang")?;
        let dry_run = args["dryRun"].as_bool().unwrap_or(true);

        let mut cmd = Command::new("sg");
        cmd.arg("scan")
            .arg("--pattern")
            .arg(pattern)
            .arg("--rewrite")
            .arg(rewrite)
            .arg("--lang")
            .arg(lang)
            .current_dir(&self.root_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if !dry_run {
            cmd.arg("--update-all");
        }

        // Add paths if specified
        if let Some(paths) = args["paths"].as_array() {
            for path in paths {
                if let Some(p) = path.as_str() {
                    cmd.arg(p);
                }
            }
        }

        let output = cmd
            .output()
            .await
            .map_err(|e| format!("Failed to run ast-grep: {}", e))?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if dry_run {
                if stdout.trim().is_empty() {
                    Ok("No matches found for replacement".to_string())
                } else {
                    Ok(format!("[DRY RUN] Would replace:\n{}", stdout))
                }
            } else {
                Ok(format!("Replaced matches:\n{}", stdout))
            }
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("ast-grep error: {}", stderr))
        }
    }

    async fn run_oxc_lint(&self, file_path: &str, severity_filter: &str) -> Result<String, String> {
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
}
