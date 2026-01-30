use super::WorkflowTask;
use anyhow::Result;

pub struct WorkflowVerifier;

impl WorkflowVerifier {
    pub fn verify(task: WorkflowTask, working_dir: &std::path::Path) -> Result<VerificationResult> {
        match task {
            WorkflowTask::Typos => Self::verify_typos(working_dir),
            WorkflowTask::Diagnostics => Self::verify_diagnostics(working_dir),
            WorkflowTask::Comments => Self::verify_comments(working_dir),
        }
    }

    fn verify_typos(working_dir: &std::path::Path) -> Result<VerificationResult> {
        let output = std::process::Command::new("typos")
            .arg("--format=brief")
            .current_dir(working_dir)
            .output()?;

        if output.status.success() {
            Ok(VerificationResult::Pass)
        } else {
            let remaining = String::from_utf8_lossy(&output.stdout);
            let count = remaining.lines().count();
            Ok(VerificationResult::Fail {
                remaining_issues: count,
                details: remaining.to_string(),
            })
        }
    }

    fn verify_diagnostics(_working_dir: &std::path::Path) -> Result<VerificationResult> {
        Ok(VerificationResult::Pass)
    }

    fn verify_comments(_working_dir: &std::path::Path) -> Result<VerificationResult> {
        Ok(VerificationResult::Pass)
    }
}

#[derive(Debug)]
pub enum VerificationResult {
    Pass,
    Fail {
        remaining_issues: usize,
        details: String,
    },
}
