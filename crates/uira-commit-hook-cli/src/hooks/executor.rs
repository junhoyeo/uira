use crate::config::{Command, HookConfig};
use crate::hooks::OnFail;
use anyhow::{Context, Result};
use colored::Colorize;
use rayon::prelude::*;
use std::process::{Command as ProcessCommand, Stdio};

pub struct HookExecutor {
    hook_name: String,
}

impl HookExecutor {
    pub fn new(hook_name: String) -> Self {
        Self { hook_name }
    }

    pub fn execute(&self, hook_config: &HookConfig) -> Result<()> {
        println!(
            "{} Running {} hook...",
            "⚡".bright_yellow(),
            self.hook_name.bright_cyan()
        );

        if hook_config.parallel {
            self.execute_parallel(&hook_config.commands)
        } else {
            self.execute_sequential(&hook_config.commands)
        }
    }

    fn execute_parallel(&self, commands: &[Command]) -> Result<()> {
        let results: Vec<Result<()>> = commands
            .par_iter()
            .map(|cmd| self.run_command(cmd))
            .collect();

        for result in results {
            result?;
        }

        Ok(())
    }

    fn execute_sequential(&self, commands: &[Command]) -> Result<()> {
        for cmd in commands {
            self.run_command(cmd)?;
        }
        Ok(())
    }

    fn run_command(&self, cmd: &Command) -> Result<()> {
        let name = cmd.name.as_deref().unwrap_or("unnamed");
        println!("  {} {}", "→".bright_blue(), name.bright_white());

        let shell_cmd = self.expand_variables(&cmd.run);

        let output = if cfg!(target_os = "windows") {
            ProcessCommand::new("cmd")
                .args(["/C", &shell_cmd])
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .output()
        } else {
            ProcessCommand::new("sh")
                .arg("-c")
                .arg(&shell_cmd)
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .output()
        }
        .with_context(|| format!("Failed to execute command: {}", cmd.run))?;

        if !output.status.success() {
            let exit_code = output.status.code().unwrap_or(-1);
            match cmd.on_fail {
                OnFail::Stop => {
                    anyhow::bail!("Command '{}' failed with exit code: {}", name, exit_code);
                }
                OnFail::Warn => {
                    println!(
                        "  {} {} (exit code: {}, continuing due to on_fail: warn)",
                        "⚠".bright_yellow(),
                        name.bright_white(),
                        exit_code
                    );
                    return Ok(());
                }
                OnFail::Continue => {
                    println!(
                        "  {} {} (exit code: {}, ignored)",
                        "•".bright_black(),
                        name.bright_white(),
                        exit_code
                    );
                    return Ok(());
                }
            }
        }

        println!("  {} {}", "✓".bright_green(), name.bright_white());
        Ok(())
    }

    fn expand_variables(&self, command: &str) -> String {
        let mut expanded = command.to_string();

        if expanded.contains("{staged_files}") {
            let staged_files = self.get_staged_files().unwrap_or_default();
            expanded = expanded.replace("{staged_files}", &staged_files);
        }

        if expanded.contains("{all_files}") {
            let all_files = self.get_all_files().unwrap_or_default();
            expanded = expanded.replace("{all_files}", &all_files);
        }

        expanded
    }

    fn get_staged_files(&self) -> Result<String> {
        let output = ProcessCommand::new("git")
            .args(["diff", "--cached", "--name-only", "--diff-filter=ACMR"])
            .output()
            .context("Failed to get staged files")?;

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn get_all_files(&self) -> Result<String> {
        let output = ProcessCommand::new("git")
            .args(["ls-files"])
            .output()
            .context("Failed to get all files")?;

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_variable_expansion() {
        let executor = HookExecutor::new("test".to_string());

        let cmd = "echo {staged_files}";
        let expanded = executor.expand_variables(cmd);

        assert!(!expanded.contains("{staged_files}"));
    }

    #[test]
    fn test_executor_creation() {
        let executor = HookExecutor::new("pre-commit".to_string());
        assert_eq!(executor.hook_name, "pre-commit");
    }
}
