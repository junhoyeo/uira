pub mod typos;
pub mod typos_config;

pub use typos_config::TyposConfig;

use std::path::PathBuf;

/// Budget for rendering prompts (controls token usage)
pub struct RenderBudget {
    pub max_issues: usize,
    pub include_context: bool,
}

/// A detected issue (typo, diagnostic, comment, etc.)
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields reserved for future Diagnostics/Comments detectors
pub struct Issue {
    pub id: String, // stable fingerprint
    pub path: PathBuf,
    pub line: usize,
    pub col: usize,
    pub byte_offset: usize,
    pub message: String,          // e.g., "mispelled"
    pub suggestions: Vec<String>, // e.g., ["misspelled"]
    pub context: Option<String>,  // surrounding line text
}

/// Scope of files to check
pub struct Scope {
    pub working_dir: PathBuf,
    pub paths: Vec<PathBuf>,
}

impl Scope {
    pub fn from_files(working_dir: PathBuf, files: Vec<String>) -> Self {
        let paths = files.into_iter().map(PathBuf::from).collect();
        Self { working_dir, paths }
    }

    pub fn from_staged(working_dir: &std::path::Path) -> anyhow::Result<Self> {
        let output = std::process::Command::new("git")
            .arg("diff")
            .arg("--cached")
            .arg("--name-only")
            .arg("--diff-filter=ACM")
            .current_dir(working_dir)
            .output()?;

        if !output.status.success() {
            anyhow::bail!("git diff failed");
        }

        let paths = String::from_utf8(output.stdout)?
            .lines()
            .map(|line| working_dir.join(line))
            .collect();

        Ok(Self {
            working_dir: working_dir.to_path_buf(),
            paths,
        })
    }

    pub fn from_repo(working_dir: &std::path::Path) -> anyhow::Result<Self> {
        let output = std::process::Command::new("git")
            .arg("ls-files")
            .current_dir(working_dir)
            .output()?;

        if !output.status.success() {
            anyhow::bail!("git ls-files failed");
        }

        let paths = String::from_utf8(output.stdout)?
            .lines()
            .map(|line| working_dir.join(line))
            .collect();

        Ok(Self {
            working_dir: working_dir.to_path_buf(),
            paths,
        })
    }
}

/// Trait for detecting issues in code
pub trait Detector: Send + Sync {
    #[allow(dead_code)] // Reserved for logging/debugging
    fn name(&self) -> &'static str;
    fn detect(&self, scope: &Scope) -> anyhow::Result<Vec<Issue>>;
    fn render_prompt(&self, issues: &[Issue], budget: &RenderBudget) -> String;
}
