use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

pub struct GitTracker {
    working_dir: std::path::PathBuf,
    baseline: HashSet<String>,
}

impl GitTracker {
    pub fn new(working_dir: impl AsRef<Path>) -> Self {
        let working_dir = working_dir.as_ref().to_path_buf();
        let baseline = Self::get_changed_files(&working_dir);
        Self {
            working_dir,
            baseline,
        }
    }

    pub fn get_modifications(&self) -> Vec<String> {
        let current = Self::get_changed_files(&self.working_dir);
        current.difference(&self.baseline).cloned().collect()
    }

    fn get_changed_files(working_dir: &Path) -> HashSet<String> {
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
