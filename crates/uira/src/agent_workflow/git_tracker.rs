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

        Command::new("git")
            .arg("add")
            .arg("--")
            .args(files)
            .current_dir(&self.working_dir)
            .status()?;

        Ok(())
    }
}
