//! AST tool provider - ast-grep based code search and replace

use crate::tools::provider::ToolProvider;
use crate::tools::{ToolContext, ToolError};
use ast_grep_language::{LanguageExt, SupportLang};
use async_trait::async_trait;
use glob::glob;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use uira_core::{JsonSchema, ToolOutput, ToolSpec};
use walkdir::WalkDir;

fn get_extensions_for_lang(lang: &str) -> &'static [&'static str] {
    match lang {
        "typescript" | "tsx" => &["ts", "tsx", "mts", "cts"],
        "javascript" | "jsx" => &["js", "jsx", "mjs", "cjs"],
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

pub struct AstToolProvider;

impl AstToolProvider {
    pub fn new() -> Self {
        Self
    }

    fn is_path_within_root(path: &std::path::Path, root: &std::path::Path) -> bool {
        match (path.canonicalize(), root.canonicalize()) {
            (Ok(canonical_path), Ok(canonical_root)) => canonical_path.starts_with(&canonical_root),
            _ => false,
        }
    }

    fn validate_path_input(p: &str) -> bool {
        !p.starts_with('/') && !p.starts_with('\\') && !p.contains("..")
    }

    fn collect_files(
        &self,
        root_path: &std::path::Path,
        args: &Value,
        lang: &str,
    ) -> Result<Vec<PathBuf>, ToolError> {
        let extensions = get_extensions_for_lang(lang);
        let mut files = Vec::new();
        let mut seen = HashSet::new();

        let has_paths = args["paths"].as_array().is_some_and(|a| !a.is_empty());
        let has_globs = args["globs"].as_array().is_some_and(|a| !a.is_empty());
        let explicit_filters = has_paths || has_globs;

        if let Some(paths) = args["paths"].as_array() {
            for path in paths {
                if let Some(p) = path.as_str() {
                    if !Self::validate_path_input(p) {
                        continue;
                    }
                    let full_path = root_path.join(p);
                    if !Self::is_path_within_root(&full_path, root_path) {
                        continue;
                    }
                    if full_path.is_file() && seen.insert(full_path.clone()) {
                        files.push(full_path);
                    } else if full_path.is_dir() {
                        Self::walk_dir(&full_path, root_path, extensions, &mut files, &mut seen);
                    }
                }
            }
        }

        if let Some(globs) = args["globs"].as_array() {
            for g in globs {
                if let Some(pattern) = g.as_str() {
                    if !Self::validate_path_input(pattern) {
                        continue;
                    }
                    let full_pattern = root_path.join(pattern);
                    if let Ok(entries) = glob(full_pattern.to_string_lossy().as_ref()) {
                        for entry in entries.flatten() {
                            if !Self::is_path_within_root(&entry, root_path) {
                                continue;
                            }
                            if entry.is_file() && seen.insert(entry.clone()) {
                                files.push(entry);
                            }
                        }
                    }
                }
            }
        }

        if files.is_empty() && !explicit_filters {
            Self::walk_dir(root_path, root_path, extensions, &mut files, &mut seen);
        }

        Ok(files)
    }

    fn walk_dir(
        dir: &std::path::Path,
        root_path: &std::path::Path,
        extensions: &[&str],
        files: &mut Vec<PathBuf>,
        seen: &mut HashSet<PathBuf>,
    ) {
        for entry in WalkDir::new(dir)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                !name.starts_with('.') && name != "node_modules" && name != "target"
            })
            .flatten()
        {
            let path = entry.path();
            if path.is_file() {
                let path_buf = path.to_path_buf();
                if !Self::is_path_within_root(&path_buf, root_path) {
                    continue;
                }
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if extensions.contains(&ext) && seen.insert(path_buf.clone()) {
                        files.push(path_buf);
                    }
                }
            }
        }
    }

    fn ast_search(
        &self,
        root_path: &std::path::Path,
        input: &Value,
    ) -> Result<ToolOutput, ToolError> {
        let pattern = input["pattern"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput {
                message: "Missing 'pattern' parameter".to_string(),
            })?;

        let lang_str = input["lang"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput {
                message: "Missing 'lang' parameter".to_string(),
            })?;

        let context_lines = input["context"].as_u64().unwrap_or(0) as usize;

        let lang: SupportLang = lang_str.parse().map_err(|_| ToolError::ExecutionFailed {
            message: format!("Unsupported language: {}", lang_str),
        })?;

        let files = self.collect_files(root_path, input, lang_str)?;
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
            Ok(ToolOutput::text("No matches found"))
        } else {
            Ok(ToolOutput::text(
                serde_json::to_string_pretty(&results).unwrap_or_default(),
            ))
        }
    }

    fn ast_replace(
        &self,
        root_path: &std::path::Path,
        input: &Value,
        ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        let pattern = input["pattern"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput {
                message: "Missing 'pattern' parameter".to_string(),
            })?;

        let rewrite = input["rewrite"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput {
                message: "Missing 'rewrite' parameter".to_string(),
            })?;

        let lang_str = input["lang"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidInput {
                message: "Missing 'lang' parameter".to_string(),
            })?;

        let dry_run = input["dryRun"].as_bool().unwrap_or(true);

        if !dry_run && ctx.sandbox_type != uira_sandbox::SandboxType::None {
            return Err(ToolError::SandboxDenied {
                message: "ast_replace write mode is not available in sandboxed sessions; use dryRun=true or disable sandbox for this run".to_string(),
                retryable: false,
            });
        }

        let lang: SupportLang = lang_str.parse().map_err(|_| ToolError::ExecutionFailed {
            message: format!("Unsupported language: {}", lang_str),
        })?;

        let files = self.collect_files(root_path, input, lang_str)?;
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

            let mut edits: Vec<(usize, usize, Vec<u8>, String, usize)> = Vec::new();
            for m in &matches {
                let node = m.get_node();
                let edit = m.replace_by(rewrite);
                let start = edit.position;
                let end = start + edit.deleted_length;
                let line = node.start_pos().line() + 1;
                let original = node.text().to_string();
                edits.push((start, end, edit.inserted_text, original, line));
            }

            edits.sort_by(|a, b| b.0.cmp(&a.0));

            let mut buf = content.clone().into_bytes();
            let mut last_start = usize::MAX;
            let mut skipped_overlaps = 0;

            for (start, end, inserted, original, line) in edits {
                if end > last_start {
                    skipped_overlaps += 1;
                    continue;
                }
                if end > buf.len() {
                    continue;
                }

                let replacement = String::from_utf8_lossy(&inserted).to_string();
                results.push(json!({
                    "file": file_path.to_string_lossy(),
                    "line": line,
                    "original": original,
                    "replacement": replacement,
                }));

                buf.splice(start..end, inserted);
                last_start = start;
            }

            if skipped_overlaps > 0 {
                results.push(json!({
                    "file": file_path.to_string_lossy(),
                    "warning": format!("Skipped {} overlapping matches", skipped_overlaps),
                }));
            }

            let new_content = String::from_utf8(buf).map_err(|e| ToolError::ExecutionFailed {
                message: format!(
                    "Invalid UTF-8 after replacement in {}: {}",
                    file_path.display(),
                    e
                ),
            })?;

            if !dry_run {
                fs::write(&file_path, &new_content).map_err(|e| ToolError::ExecutionFailed {
                    message: format!("Failed to write {}: {}", file_path.display(), e),
                })?;
                files_modified += 1;
            }
        }

        if results.is_empty() {
            return Ok(ToolOutput::text("No matches found for replacement"));
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

        Ok(ToolOutput::text(
            serde_json::to_string_pretty(&output).unwrap_or_default(),
        ))
    }
}

impl Default for AstToolProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolProvider for AstToolProvider {
    fn specs(&self) -> Vec<ToolSpec> {
        vec![
            ToolSpec::new(
                "ast_search",
                "Search code using AST patterns with ast-grep",
                JsonSchema::object()
                    .property(
                        "pattern",
                        JsonSchema::string().description("AST pattern to search for"),
                    )
                    .property(
                        "lang",
                        JsonSchema::string().description(
                            "Programming language (rust, typescript, javascript, python, go, etc.)",
                        ),
                    )
                    .property(
                        "paths",
                        JsonSchema::array(JsonSchema::string())
                            .description("Optional paths to search in"),
                    )
                    .property(
                        "globs",
                        JsonSchema::array(JsonSchema::string())
                            .description("Optional glob patterns"),
                    )
                    .property(
                        "context",
                        JsonSchema::number().description("Number of context lines to include"),
                    )
                    .required(&["pattern", "lang"]),
            ),
            ToolSpec::new(
                "ast_replace",
                "Replace code using AST patterns with ast-grep",
                JsonSchema::object()
                    .property(
                        "pattern",
                        JsonSchema::string().description("AST pattern to match"),
                    )
                    .property(
                        "rewrite",
                        JsonSchema::string().description("Replacement pattern"),
                    )
                    .property(
                        "lang",
                        JsonSchema::string().description("Programming language"),
                    )
                    .property(
                        "paths",
                        JsonSchema::array(JsonSchema::string())
                            .description("Optional paths to search in"),
                    )
                    .property(
                        "globs",
                        JsonSchema::array(JsonSchema::string())
                            .description("Optional glob patterns"),
                    )
                    .property(
                        "dryRun",
                        JsonSchema::boolean()
                            .description("If true (default), only show what would change"),
                    )
                    .required(&["pattern", "rewrite", "lang"]),
            ),
        ]
    }

    fn handles(&self, name: &str) -> bool {
        matches!(name, "ast_search" | "ast_replace")
    }

    async fn execute(
        &self,
        name: &str,
        input: Value,
        ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        let root_path = ctx.cwd.clone();

        match name {
            "ast_search" => self.ast_search(&root_path, &input),
            "ast_replace" => self.ast_replace(&root_path, &input, ctx),
            _ => Err(ToolError::NotFound {
                name: name.to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ast_provider_handles() {
        let provider = AstToolProvider::new();
        assert!(provider.handles("ast_search"));
        assert!(provider.handles("ast_replace"));
        assert!(!provider.handles("lsp_goto_definition"));
        assert!(!provider.handles("read_file"));
    }

    #[test]
    fn test_ast_provider_specs() {
        let provider = AstToolProvider::new();
        let specs = provider.specs();
        assert_eq!(specs.len(), 2);
        assert!(specs.iter().any(|s| s.name == "ast_search"));
        assert!(specs.iter().any(|s| s.name == "ast_replace"));
    }

    #[test]
    fn test_get_extensions() {
        assert_eq!(get_extensions_for_lang("rust"), &["rs"]);
        assert_eq!(
            get_extensions_for_lang("typescript"),
            &["ts", "tsx", "mts", "cts"]
        );
        assert_eq!(get_extensions_for_lang("unknown"), &[] as &[&str]);
    }
}
