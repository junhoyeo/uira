//! Tool execution for the MCP server

use serde_json::Value;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;

pub struct ToolExecutor {
    root_path: PathBuf,
}

impl ToolExecutor {
    pub fn new(root_path: PathBuf) -> Self {
        Self { root_path }
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
        let file_path = args["filePath"].as_str().ok_or("Missing filePath")?;
        let line = args["line"].as_u64().ok_or("Missing line")? as usize;
        let character = args["character"].as_u64().ok_or("Missing character")? as usize;

        // Use TypeScript language server for JS/TS files
        if file_path.ends_with(".ts")
            || file_path.ends_with(".tsx")
            || file_path.ends_with(".js")
            || file_path.ends_with(".jsx")
        {
            self.run_ts_lsp_command("definition", file_path, line, character)
                .await
        } else if file_path.ends_with(".rs") {
            self.run_rust_analyzer_command("goto-definition", file_path, line, character)
                .await
        } else if file_path.ends_with(".py") {
            self.run_pyright_command("definition", file_path, line, character)
                .await
        } else {
            Err(format!("LSP not configured for file type: {}", file_path))
        }
    }

    async fn lsp_find_references(&self, args: Value) -> Result<String, String> {
        let file_path = args["filePath"].as_str().ok_or("Missing filePath")?;
        let line = args["line"].as_u64().ok_or("Missing line")? as usize;
        let character = args["character"].as_u64().ok_or("Missing character")? as usize;

        // For now, use a simple grep-based approach as fallback
        // In production, this would use proper LSP
        self.fallback_find_references(file_path, line, character)
            .await
    }

    async fn lsp_symbols(&self, args: Value) -> Result<String, String> {
        let file_path = args["filePath"].as_str().ok_or("Missing filePath")?;
        let scope = args["scope"].as_str().unwrap_or("document");

        if scope == "workspace" {
            let query = args["query"].as_str().unwrap_or("");
            self.workspace_symbols(query).await
        } else {
            self.document_symbols(file_path).await
        }
    }

    async fn lsp_diagnostics(&self, args: Value) -> Result<String, String> {
        let file_path = args["filePath"].as_str().ok_or("Missing filePath")?;

        // Use tsc for TypeScript files
        if file_path.ends_with(".ts") || file_path.ends_with(".tsx") {
            self.run_tsc_diagnostics(file_path).await
        } else if file_path.ends_with(".rs") {
            self.run_cargo_check(file_path).await
        } else if file_path.ends_with(".py") {
            self.run_pyright_diagnostics(file_path).await
        } else {
            Ok("No diagnostics available for this file type".to_string())
        }
    }

    async fn lsp_hover(&self, args: Value) -> Result<String, String> {
        let file_path = args["filePath"].as_str().ok_or("Missing filePath")?;
        let line = args["line"].as_u64().ok_or("Missing line")? as usize;
        let character = args["character"].as_u64().ok_or("Missing character")? as usize;

        // Fallback: read the line and show context
        self.fallback_hover(file_path, line, character).await
    }

    async fn lsp_rename(&self, args: Value) -> Result<String, String> {
        let file_path = args["filePath"].as_str().ok_or("Missing filePath")?;
        let line = args["line"].as_u64().ok_or("Missing line")? as usize;
        let character = args["character"].as_u64().ok_or("Missing character")? as usize;
        let new_name = args["newName"].as_str().ok_or("Missing newName")?;

        // Use ast-grep for rename as it's more reliable
        self.ast_rename(file_path, line, character, new_name).await
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

    // =========================================================================
    // Helper Methods
    // =========================================================================

    async fn run_ts_lsp_command(
        &self,
        _action: &str,
        file_path: &str,
        line: usize,
        character: usize,
    ) -> Result<String, String> {
        // For now, provide helpful guidance
        Ok(format!(
            "TypeScript LSP action at {}:{}:{}\n\
            To get definition, use: Read tool to view the file, then Grep to find the symbol definition.\n\
            TypeScript Language Server can be started with: typescript-language-server --stdio",
            file_path, line, character
        ))
    }

    async fn run_rust_analyzer_command(
        &self,
        _action: &str,
        file_path: &str,
        line: usize,
        character: usize,
    ) -> Result<String, String> {
        Ok(format!(
            "Rust analyzer action at {}:{}:{}\n\
            rust-analyzer provides this via LSP. Use Grep to find symbol definitions in Rust code.",
            file_path, line, character
        ))
    }

    async fn run_pyright_command(
        &self,
        _action: &str,
        file_path: &str,
        line: usize,
        character: usize,
    ) -> Result<String, String> {
        Ok(format!(
            "Python LSP action at {}:{}:{}\n\
            Pyright provides this via LSP. Use Grep to find symbol definitions in Python code.",
            file_path, line, character
        ))
    }

    async fn fallback_find_references(
        &self,
        file_path: &str,
        line: usize,
        _character: usize,
    ) -> Result<String, String> {
        // Read the file to get the symbol at the position
        let content = tokio::fs::read_to_string(file_path)
            .await
            .map_err(|e| format!("Failed to read file: {}", e))?;

        let lines: Vec<&str> = content.lines().collect();
        if line == 0 || line > lines.len() {
            return Err("Line number out of range".to_string());
        }

        let target_line = lines[line - 1];
        Ok(format!(
            "Line {}: {}\n\nTo find references, use Grep tool to search for the symbol name.",
            line, target_line
        ))
    }

    async fn workspace_symbols(&self, query: &str) -> Result<String, String> {
        // Use ripgrep to search for symbols
        let output = Command::new("rg")
            .arg("--json")
            .arg("-e")
            .arg(format!(
                r"(function|class|const|let|var|def|fn|struct|enum|trait|impl)\s+\w*{}\w*",
                regex::escape(query)
            ))
            .current_dir(&self.root_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| format!("Failed to search: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.trim().is_empty() {
            Ok(format!("No symbols matching '{}' found", query))
        } else {
            Ok(stdout.to_string())
        }
    }

    async fn document_symbols(&self, file_path: &str) -> Result<String, String> {
        // Use ripgrep to find symbols in the file
        let output = Command::new("rg")
            .arg("--no-filename")
            .arg("--line-number")
            .arg("-e")
            .arg(
                r"^(export\s+)?(async\s+)?(function|class|const|let|var|interface|type|enum)\s+\w+",
            )
            .arg(file_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| format!("Failed to search symbols: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.trim().is_empty() {
            Ok("No symbols found in file".to_string())
        } else {
            Ok(format!("Symbols in {}:\n{}", file_path, stdout))
        }
    }

    async fn run_tsc_diagnostics(&self, file_path: &str) -> Result<String, String> {
        let output = Command::new("npx")
            .arg("tsc")
            .arg("--noEmit")
            .arg("--pretty")
            .arg(file_path)
            .current_dir(&self.root_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| format!("Failed to run tsc: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            Ok("No TypeScript errors found".to_string())
        } else {
            Ok(format!("{}\n{}", stdout, stderr))
        }
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

    async fn fallback_hover(
        &self,
        file_path: &str,
        line: usize,
        _character: usize,
    ) -> Result<String, String> {
        let content = tokio::fs::read_to_string(file_path)
            .await
            .map_err(|e| format!("Failed to read file: {}", e))?;

        let lines: Vec<&str> = content.lines().collect();
        if line == 0 || line > lines.len() {
            return Err("Line number out of range".to_string());
        }

        // Show context around the line
        let start = line.saturating_sub(3);
        let end = (line + 2).min(lines.len());

        let mut result = String::new();
        for (i, l) in lines[start..end].iter().enumerate() {
            let line_num = start + i + 1;
            let marker = if line_num == line { ">" } else { " " };
            result.push_str(&format!("{} {:4}: {}\n", marker, line_num, l));
        }

        Ok(result)
    }

    async fn ast_rename(
        &self,
        file_path: &str,
        _line: usize,
        _character: usize,
        new_name: &str,
    ) -> Result<String, String> {
        // For rename, we'd need to identify the symbol first
        // This is a simplified version
        Ok(format!(
            "To rename to '{}' in {}, use ast_replace with the old name as pattern and '{}' as rewrite.\n\
            Example: ast_replace(pattern=\"oldName\", rewrite=\"{}\", lang=\"typescript\")",
            new_name, file_path, new_name, new_name
        ))
    }
}
