use std::collections::HashSet;
use std::path::Path;
use std::process::Command;
pub struct GitTracker {
    working_dir: std::path::PathBuf,
    /// Files with unstaged (working tree) changes at baseline
    baseline_unstaged: HashSet<String>,
    /// Files with staged changes at baseline (kept for potential future use)
    #[allow(dead_code)]
    baseline_staged: HashSet<String>,
}
impl GitTracker {
    pub fn new(working_dir: impl AsRef<Path>) -> Self {
        let working_dir = working_dir.as_ref().to_path_buf();
        let baseline_unstaged = Self::get_unstaged_files(&working_dir);
        let baseline_staged = Self::get_staged_files(&working_dir);
        Self {
            working_dir,
            baseline_unstaged,
            baseline_staged,
        }
    }

    /// Returns files that have been modified during the workflow.
    /// This detects files with NEW working tree changes (not in baseline_unstaged).
    /// This correctly handles --cached mode where files start staged-only
    /// and gain working tree modifications after the agent edits them.
    pub fn get_modifications(&self) -> Vec<String> {
        let current_unstaged = Self::get_unstaged_files(&self.working_dir);
        // Files with working tree changes now that didn't have them before
        current_unstaged
            .difference(&self.baseline_unstaged)
            .cloned()
            .collect()
    }

    /// Get files with unstaged (working tree vs index) changes
    fn get_unstaged_files(working_dir: &Path) -> HashSet<String> {
        let mut files = HashSet::new();

        if let Ok(output) = Command::new("git")
            .args(["diff", "--name-only"])
            .current_dir(working_dir)
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    if !line.is_empty() {
                        files.insert(line.to_string());
                    }
                }
            }
        }

        files
    }

    /// Get files with staged (index vs HEAD) changes
    fn get_staged_files(working_dir: &Path) -> HashSet<String> {
        let mut files = HashSet::new();

        if let Ok(output) = Command::new("git")
            .args(["diff", "--cached", "--name-only"])
            .current_dir(working_dir)
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    if !line.is_empty() {
                        files.insert(line.to_string());
                    }
                }
            }
        }

        files
    }

    pub fn stage_files(&self, files: &[String]) -> anyhow::Result<()> {
        if files.is_empty() {
            return Ok(());
        }

        let status = Command::new("git")
            .arg("add")
            .arg("--")
            .args(files)
            .current_dir(&self.working_dir)
            .status()?;

        if !status.success() {
            anyhow::bail!("git add failed with exit code: {:?}", status.code());
        }

        Ok(())
    }

    /// Commit staged files with the given message
    pub fn commit(&self, message: &str) -> anyhow::Result<()> {
        let status = Command::new("git")
            .args(["commit", "-m", message])
            .current_dir(&self.working_dir)
            .status()?;

        if !status.success() {
            anyhow::bail!("git commit failed with exit code: {:?}", status.code());
        }

        Ok(())
    }

    /// Generate a conventional commit message based on the task and files modified
    pub fn generate_commit_message(
        task_name: &str,
        files_modified: &[String],
        summary: Option<&str>,
    ) -> String {
        let file_count = files_modified.len();
        let file_hint = if file_count == 1 {
            files_modified[0].clone()
        } else if file_count <= 3 {
            files_modified.join(", ")
        } else {
            format!("{} files", file_count)
        };

        let action = match task_name {
            "typos" => {
                if let Some(s) = summary {
                    if s.contains("typo") || s.contains("spelling") {
                        return format!("fix: {}", s);
                    }
                }
                format!("fix: correct typos in {}", file_hint)
            }
            "diagnostics" => {
                if let Some(s) = summary {
                    return format!("fix: {}", s);
                }
                format!("fix: resolve diagnostics in {}", file_hint)
            }
            "comments" => {
                if let Some(s) = summary {
                    return format!("refactor: {}", s);
                }
                format!("refactor: clean up comments in {}", file_hint)
            }
            _ => {
                if let Some(s) = summary {
                    return format!("fix: {}", s);
                }
                format!("fix: auto-fix issues in {}", file_hint)
            }
        };

        action
    }
}
