//! Popular Plugin Patterns
//!
//! Utility functions commonly used in Claude Code hook plugins:
//! - Auto-format
//! - Lint validation
//! - Conventional commit validation
//! - Type checking
//! - Test runner detection

use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;

// =============================================================================
// Auto-format pattern
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatConfig {
    pub extensions: Vec<String>,
    pub command: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRunResult {
    pub success: bool,
    pub message: String,
}

lazy_static! {
    static ref DEFAULT_FORMATTERS: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        m.insert(".ts", "prettier --write");
        m.insert(".tsx", "prettier --write");
        m.insert(".js", "prettier --write");
        m.insert(".jsx", "prettier --write");
        m.insert(".json", "prettier --write");
        m.insert(".css", "prettier --write");
        m.insert(".scss", "prettier --write");
        m.insert(".md", "prettier --write");
        m.insert(".py", "black");
        m.insert(".go", "gofmt -w");
        m.insert(".rs", "rustfmt");
        m
    };

    static ref DEFAULT_LINTERS: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        m.insert(".ts", "eslint --fix");
        m.insert(".tsx", "eslint --fix");
        m.insert(".js", "eslint --fix");
        m.insert(".jsx", "eslint --fix");
        m.insert(".py", "ruff check --fix");
        m.insert(".go", "golangci-lint run");
        m.insert(".rs", "cargo clippy");
        m
    };

    static ref CONVENTIONAL_COMMIT_RE: Regex = Regex::new(
        r"^(feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert)(\([a-z0-9-]+\))?(!)?:\s.+$",
    )
    .unwrap();

    static ref SCOPE_RE: Regex = Regex::new(r"\([a-z0-9-]+\)").unwrap();
}

pub const DEFAULT_COMMIT_TYPES: &[&str] = &[
    "feat", "fix", "docs", "style", "refactor", "perf", "test", "build", "ci", "chore", "revert",
];

pub fn get_formatter(ext: &str) -> Option<&'static str> {
    DEFAULT_FORMATTERS.get(ext).copied()
}

pub fn is_formatter_available(command: &str) -> bool {
    let binary = command.split_whitespace().next().unwrap_or("");
    if binary.is_empty() {
        return false;
    }

    let check_command = if cfg!(windows) { "where" } else { "which" };
    Command::new(check_command)
        .arg(binary)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn format_file(file_path: &str) -> ToolRunResult {
    let ext = file_ext_with_dot(file_path);
    let Some(formatter) = get_formatter(&ext) else {
        return ToolRunResult {
            success: true,
            message: format!("No formatter configured for {}", ext),
        };
    };

    if !is_formatter_available(formatter) {
        return ToolRunResult {
            success: true,
            message: format!("Formatter {} not available", formatter),
        };
    }

    let cmd = format!("{} \"{}\"", formatter, file_path);
    match run_shell_raw(&cmd, None) {
        Ok(_) => ToolRunResult {
            success: true,
            message: format!("Formatted {}", file_path),
        },
        Err(e) => ToolRunResult {
            success: false,
            message: format!("Format failed: {}", e),
        },
    }
}

// =============================================================================
// Lint validation pattern
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintConfig {
    pub command: String,
    pub patterns: Vec<String>,
    pub blocking: bool,
}

pub fn get_linter(ext: &str) -> Option<&'static str> {
    DEFAULT_LINTERS.get(ext).copied()
}

pub fn lint_file(file_path: &str) -> ToolRunResult {
    let ext = file_ext_with_dot(file_path);
    let Some(linter) = get_linter(&ext) else {
        return ToolRunResult {
            success: true,
            message: format!("No linter configured for {}", ext),
        };
    };

    // Check linter available
    let binary = linter.split_whitespace().next().unwrap_or("");
    if binary.is_empty() {
        return ToolRunResult {
            success: true,
            message: format!("Linter {} not available", linter),
        };
    }
    let check_command = if cfg!(windows) { "where" } else { "which" };
    let ok = Command::new(check_command)
        .arg(binary)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !ok {
        return ToolRunResult {
            success: true,
            message: format!("Linter {} not available", linter),
        };
    }

    let cmd = format!("{} \"{}\"", linter, file_path);
    match run_shell_raw(&cmd, None) {
        Ok(_) => ToolRunResult {
            success: true,
            message: format!("Lint passed for {}", file_path),
        },
        Err(_) => ToolRunResult {
            success: false,
            message: format!("Lint errors in {}", file_path),
        },
    }
}

// =============================================================================
// Commit message validation pattern
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitConfig {
    pub types: Vec<String>,
    pub max_subject_length: usize,
    pub require_scope: bool,
    pub require_body: bool,
}

impl Default for CommitConfig {
    fn default() -> Self {
        Self {
            types: DEFAULT_COMMIT_TYPES.iter().map(|s| s.to_string()).collect(),
            max_subject_length: 72,
            require_scope: false,
            require_body: false,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommitConfigOverrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_subject_length: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub require_scope: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub require_body: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitValidationResult {
    pub valid: bool,
    pub errors: Vec<String>,
}

pub fn validate_commit_message(
    message: &str,
    config: Option<CommitConfigOverrides>,
) -> CommitValidationResult {
    let cfg = config.unwrap_or_default();
    let mut errors: Vec<String> = Vec::new();

    let trimmed = message.trim();
    let lines: Vec<&str> = trimmed.split('\n').collect();
    let subject = lines.first().copied().unwrap_or("");

    if subject.is_empty() {
        errors.push("Commit message cannot be empty".to_string());
        return CommitValidationResult {
            valid: false,
            errors,
        };
    }

    if !CONVENTIONAL_COMMIT_RE.is_match(subject) {
        errors.push(
            "Subject must follow conventional commit format: type(scope?): description".to_string(),
        );
        errors.push(format!(
            "Allowed types: {}",
            DEFAULT_COMMIT_TYPES.join(", ")
        ));
    }

    let max_len = cfg.max_subject_length.unwrap_or(72);
    if subject.len() > max_len {
        errors.push(format!("Subject line exceeds {} characters", max_len));
    }

    if cfg.require_scope.unwrap_or(false) && !SCOPE_RE.is_match(subject) {
        errors.push("Scope is required in commit message".to_string());
    }

    if cfg.require_body.unwrap_or(false) {
        let body_line = lines.get(2).copied().unwrap_or("");
        if lines.len() < 3 || body_line.is_empty() {
            errors.push("Commit body is required".to_string());
        }
    }

    CommitValidationResult {
        valid: errors.is_empty(),
        errors,
    }
}

// =============================================================================
// Type checking pattern
// =============================================================================

pub fn run_type_check(directory: &str) -> ToolRunResult {
    let tsconfig_path = Path::new(directory).join("tsconfig.json");
    if !tsconfig_path.exists() {
        return ToolRunResult {
            success: true,
            message: "No tsconfig.json found".to_string(),
        };
    }

    // Check TypeScript installed
    let check_command = if cfg!(windows) { "where" } else { "which" };
    let ok = Command::new(check_command)
        .arg("tsc")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !ok {
        return ToolRunResult {
            success: true,
            message: "TypeScript not installed".to_string(),
        };
    }

    match run_shell_command("tsc --noEmit", Some(directory)) {
        Ok(_) => ToolRunResult {
            success: true,
            message: "Type check passed".to_string(),
        },
        Err(_) => ToolRunResult {
            success: false,
            message: "Type errors found".to_string(),
        },
    }
}

// =============================================================================
// Test runner pattern
// =============================================================================

pub fn run_tests(directory: &str) -> ToolRunResult {
    let package_json_path = Path::new(directory).join("package.json");
    if package_json_path.exists() {
        match fs::read_to_string(&package_json_path)
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        {
            Some(pkg) => {
                let has_test = pkg.get("scripts").and_then(|v| v.get("test")).is_some();
                if has_test {
                    return match run_shell_command("npm test", Some(directory)) {
                        Ok(_) => ToolRunResult {
                            success: true,
                            message: "Tests passed".to_string(),
                        },
                        Err(_) => ToolRunResult {
                            success: false,
                            message: "Tests failed".to_string(),
                        },
                    };
                }
            }
            None => {
                return ToolRunResult {
                    success: false,
                    message: "Tests failed".to_string(),
                };
            }
        }
    }

    let pytest_ini = Path::new(directory).join("pytest.ini");
    let pyproject = Path::new(directory).join("pyproject.toml");
    if pytest_ini.exists() || pyproject.exists() {
        return match run_shell_command("pytest", Some(directory)) {
            Ok(_) => ToolRunResult {
                success: true,
                message: "Tests passed".to_string(),
            },
            Err(_) => ToolRunResult {
                success: false,
                message: "Tests failed".to_string(),
            },
        };
    }

    ToolRunResult {
        success: true,
        message: "No test runner found".to_string(),
    }
}

// =============================================================================
// Pre-commit validation
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreCommitCheck {
    pub name: String,
    pub passed: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreCommitResult {
    pub can_commit: bool,
    pub checks: Vec<PreCommitCheck>,
}

pub fn run_pre_commit_checks(directory: &str, commit_message: Option<&str>) -> PreCommitResult {
    let mut checks: Vec<PreCommitCheck> = Vec::new();

    let type_check = run_type_check(directory);
    checks.push(PreCommitCheck {
        name: "Type Check".to_string(),
        passed: type_check.success,
        message: type_check.message,
    });

    if let Some(msg) = commit_message {
        let res = validate_commit_message(msg, None);
        checks.push(PreCommitCheck {
            name: "Commit Message".to_string(),
            passed: res.valid,
            message: if res.valid {
                "Valid format".to_string()
            } else {
                res.errors.join("; ")
            },
        });
    }

    let can_commit = checks.iter().all(|c| c.passed);
    PreCommitResult { can_commit, checks }
}

pub fn get_pre_commit_reminder_message(result: &PreCommitResult) -> String {
    if result.can_commit {
        return String::new();
    }

    let failed: Vec<&PreCommitCheck> = result.checks.iter().filter(|c| !c.passed).collect();
    let failed_lines = failed
        .iter()
        .map(|c| format!("- {}: {}", c.name, c.message))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "<pre-commit-validation>\n\n[PRE-COMMIT CHECKS FAILED]\n\nThe following checks did not pass:\n{}\n\nPlease fix these issues before committing.\n\n</pre-commit-validation>\n\n---\n\n",
        failed_lines
    )
}

pub fn get_auto_format_message(file_path: &str, result: &ToolRunResult) -> String {
    if result.success {
        return String::new();
    }

    format!(
        "<auto-format>\n\n[FORMAT WARNING]\n\nFile {} could not be auto-formatted:\n{}\n\nPlease check the file manually.\n\n</auto-format>\n\n---\n\n",
        file_path, result.message
    )
}

// =============================================================================
// Helpers
// =============================================================================

fn file_ext_with_dot(file_path: &str) -> String {
    Path::new(file_path)
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| format!(".{}", s))
        .unwrap_or_default()
}

fn run_shell_command(command: &str, cwd: Option<&str>) -> Result<ToolRunResult, String> {
    // Backwards-compat helper used by TS ports: only success is relevant.
    run_shell_raw(command, cwd).map(|_| ToolRunResult {
        success: true,
        message: String::new(),
    })
}

fn run_shell_raw(command: &str, cwd: Option<&str>) -> Result<(), String> {
    let mut cmd = if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(command);
        c
    } else {
        let mut c = Command::new("sh");
        c.arg("-c").arg(command);
        c
    };

    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }

    let output = cmd.output().map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let combined = format!("{}{}", stdout, stderr);
        Err(combined.trim().to_string())
    }
}

// =============================================================================
// Tests (pure logic only; no external command reliance)
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_formatter() {
        assert_eq!(get_formatter(".ts"), Some("prettier --write"));
        assert_eq!(get_formatter(".unknown"), None);
    }

    #[test]
    fn test_validate_commit_message_valid() {
        let res = validate_commit_message("feat(core): add thing", None);
        assert!(res.valid);
        assert!(res.errors.is_empty());
    }

    #[test]
    fn test_validate_commit_message_invalid() {
        let res = validate_commit_message("not a conventional commit", None);
        assert!(!res.valid);
        assert!(res.errors.iter().any(|e| e.contains("conventional")));
    }

    #[test]
    fn test_validate_commit_message_require_scope_and_body() {
        let res = validate_commit_message(
            "feat: add thing\n\n",
            Some(CommitConfigOverrides {
                require_scope: Some(true),
                require_body: Some(true),
                ..Default::default()
            }),
        );
        assert!(!res.valid);
        assert!(res.errors.iter().any(|e| e.contains("Scope is required")));
        assert!(res
            .errors
            .iter()
            .any(|e| e.contains("Commit body is required")));
    }

    #[test]
    fn test_get_pre_commit_reminder_message() {
        let result = PreCommitResult {
            can_commit: false,
            checks: vec![PreCommitCheck {
                name: "Type Check".to_string(),
                passed: false,
                message: "Type errors found".to_string(),
            }],
        };

        let msg = get_pre_commit_reminder_message(&result);
        assert!(msg.contains("PRE-COMMIT CHECKS FAILED"));
        assert!(msg.contains("Type Check"));
    }
}
