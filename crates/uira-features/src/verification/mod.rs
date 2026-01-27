pub mod types;

pub use types::*;

use std::collections::HashMap;
use std::process::Command;
use std::time::{Duration, SystemTime};

/// Standard verification checks used across workflows
pub fn standard_checks() -> HashMap<&'static str, VerificationCheck> {
    let mut checks = HashMap::new();

    checks.insert(
        "build",
        VerificationCheck {
            id: "build".to_string(),
            name: "Build Success".to_string(),
            description: "Code compiles without errors".to_string(),
            evidence_type: VerificationEvidenceType::BuildSuccess,
            required: true,
            command: Some("npm run build".to_string()),
            completed: false,
            evidence: None,
        },
    );

    checks.insert(
        "test",
        VerificationCheck {
            id: "test".to_string(),
            name: "Tests Pass".to_string(),
            description: "All tests pass without errors".to_string(),
            evidence_type: VerificationEvidenceType::TestPass,
            required: true,
            command: Some("npm test".to_string()),
            completed: false,
            evidence: None,
        },
    );

    checks.insert(
        "lint",
        VerificationCheck {
            id: "lint".to_string(),
            name: "Lint Clean".to_string(),
            description: "No linting errors".to_string(),
            evidence_type: VerificationEvidenceType::LintClean,
            required: true,
            command: Some("npm run lint".to_string()),
            completed: false,
            evidence: None,
        },
    );

    checks.insert(
        "functionality",
        VerificationCheck {
            id: "functionality".to_string(),
            name: "Functionality Verified".to_string(),
            description: "All requested features work as described".to_string(),
            evidence_type: VerificationEvidenceType::FunctionalityVerified,
            required: true,
            command: None,
            completed: false,
            evidence: None,
        },
    );

    checks.insert(
        "architect",
        VerificationCheck {
            id: "architect".to_string(),
            name: "Architect Approval".to_string(),
            description: "Architect has reviewed and approved the implementation".to_string(),
            evidence_type: VerificationEvidenceType::ArchitectApproval,
            required: true,
            command: None,
            completed: false,
            evidence: None,
        },
    );

    checks.insert(
        "todo",
        VerificationCheck {
            id: "todo".to_string(),
            name: "TODO Complete".to_string(),
            description: "Zero pending or in_progress tasks".to_string(),
            evidence_type: VerificationEvidenceType::TodoComplete,
            required: true,
            command: None,
            completed: false,
            evidence: None,
        },
    );

    checks.insert(
        "error_free",
        VerificationCheck {
            id: "error_free".to_string(),
            name: "Error Free".to_string(),
            description: "Zero unaddressed errors".to_string(),
            evidence_type: VerificationEvidenceType::ErrorFree,
            required: true,
            command: None,
            completed: false,
            evidence: None,
        },
    );

    checks
}

/// Create a verification protocol
pub fn create_protocol(
    name: String,
    description: String,
    checks: Vec<VerificationCheck>,
    strict_mode: bool,
) -> VerificationProtocol {
    VerificationProtocol {
        name,
        description,
        checks,
        strict_mode,
        custom_validator: None,
    }
}

/// Create a verification checklist from a protocol
pub fn create_checklist(protocol: VerificationProtocol) -> VerificationChecklist {
    VerificationChecklist {
        checks: protocol.checks.clone(),
        protocol,
        started_at: SystemTime::now(),
        completed_at: None,
        status: VerificationStatus::Pending,
        summary: None,
    }
}

/// Run a single verification check
async fn run_single_check(
    check: &VerificationCheck,
    options: &VerificationOptions,
) -> VerificationEvidence {
    // If check has a command, run it
    // Note: timeout from options is not currently enforced (see run_command_without_timeout docs)
    if let Some(cmd) = &check.command {
        match run_command_without_timeout(cmd, options.cwd.as_deref()) {
            Ok((stdout, stderr)) => VerificationEvidence {
                evidence_type: check.evidence_type,
                passed: true,
                command: Some(cmd.clone()),
                output: Some(if !stdout.is_empty() { stdout } else { stderr }),
                error: None,
                timestamp: SystemTime::now(),
                metadata: None,
            },
            Err(err) => VerificationEvidence {
                evidence_type: check.evidence_type,
                passed: false,
                command: Some(cmd.clone()),
                output: None,
                error: Some(err),
                timestamp: SystemTime::now(),
                metadata: None,
            },
        }
    } else {
        // Manual verification checks (no command)
        let mut metadata = HashMap::new();
        metadata.insert(
            "requiresManualVerification".to_string(),
            serde_json::Value::Bool(true),
        );

        VerificationEvidence {
            evidence_type: check.evidence_type,
            passed: false,
            command: None,
            output: None,
            error: None,
            timestamp: SystemTime::now(),
            metadata: Some(metadata),
        }
    }
}

/// Run command with timeout
///
/// NOTE: The timeout parameter is currently not enforced. The function will block
/// until the command completes. For actual timeout support, use an async runtime
/// with `tokio::time::timeout` wrapping the command execution.
fn run_command_without_timeout(cmd: &str, cwd: Option<&str>) -> Result<(String, String), String> {
    let mut command = if cfg!(target_os = "windows") {
        let mut c = Command::new("cmd");
        c.args(["/C", cmd]);
        c
    } else {
        let mut c = Command::new("sh");
        c.args(["-c", cmd]);
        c
    };

    if let Some(dir) = cwd {
        command.current_dir(dir);
    }

    // Note: Rust's std::process::Command doesn't have built-in timeout
    // For timeout support, wrap this in tokio::time::timeout when using async
    match command.output() {
        Ok(output) => {
            if output.status.success() {
                Ok((
                    String::from_utf8_lossy(&output.stdout).to_string(),
                    String::from_utf8_lossy(&output.stderr).to_string(),
                ))
            } else {
                Err(format!(
                    "Command failed with exit code: {:?}",
                    output.status.code()
                ))
            }
        }
        Err(e) => Err(format!("Failed to execute command: {}", e)),
    }
}

/// Execute all verification checks
pub async fn run_verification(
    mut checklist: VerificationChecklist,
    options: VerificationOptions,
) -> VerificationChecklist {
    checklist.status = VerificationStatus::InProgress;

    let check_ids_to_run: Vec<String> = if options.skip_optional {
        checklist
            .checks
            .iter()
            .filter(|c| c.required)
            .map(|c| c.id.clone())
            .collect()
    } else {
        checklist.checks.iter().map(|c| c.id.clone()).collect()
    };

    if options.parallel && !options.fail_fast {
        // Run all checks in parallel
        let mut handles = vec![];
        for check_id in &check_ids_to_run {
            if let Some(check) = checklist.checks.iter().find(|c| &c.id == check_id) {
                let check_clone = check.clone();
                let options_clone = options.clone();
                handles.push(tokio::spawn(async move {
                    (
                        check_clone.id.clone(),
                        run_single_check(&check_clone, &options_clone).await,
                    )
                }));
            }
        }

        // Collect results
        let results = futures::future::join_all(handles).await;

        // Update checklist with results
        for (check_id, evidence) in results.into_iter().flatten() {
            if let Some(check) = checklist.checks.iter_mut().find(|c| c.id == check_id) {
                check.evidence = Some(evidence);
                check.completed = true;
            }
        }
    } else {
        // Run checks sequentially
        for check_id in check_ids_to_run {
            if let Some(check_idx) = checklist.checks.iter().position(|c| c.id == check_id) {
                let check_clone = checklist.checks[check_idx].clone();
                let evidence = run_single_check(&check_clone, &options).await;

                checklist.checks[check_idx].evidence = Some(evidence.clone());
                checklist.checks[check_idx].completed = true;

                // Stop on first failure if failFast is enabled
                if options.fail_fast && !evidence.passed {
                    break;
                }
            }
        }
    }

    // Generate summary
    checklist.summary = Some(generate_summary(&checklist));
    checklist.completed_at = Some(SystemTime::now());
    checklist.status = if checklist.summary.as_ref().unwrap().all_required_passed {
        VerificationStatus::Complete
    } else {
        VerificationStatus::Failed
    };

    checklist
}

/// Validate evidence for a specific check
pub fn check_evidence(
    check: &VerificationCheck,
    evidence: &VerificationEvidence,
) -> ValidationResult {
    let mut issues = vec![];
    let mut recommendations = vec![];

    // Check evidence type matches
    if evidence.evidence_type != check.evidence_type {
        issues.push(format!(
            "Evidence type mismatch: expected {:?}, got {:?}",
            check.evidence_type, evidence.evidence_type
        ));
    }

    // Check if passed
    if !evidence.passed {
        issues.push(format!("Check failed: {}", check.name));
        if let Some(error) = &evidence.error {
            issues.push(format!("Error: {}", error));
        }
        if let Some(cmd) = &check.command {
            recommendations.push(format!("Review command output: {}", cmd));
        }
        recommendations.push("Fix the issue and re-run verification".to_string());
    }

    // Check for stale evidence (older than 5 minutes)
    let five_minutes_ago = SystemTime::now() - Duration::from_secs(5 * 60);
    if evidence.timestamp < five_minutes_ago {
        issues.push("Evidence is stale (older than 5 minutes)".to_string());
        recommendations.push("Re-run verification to get fresh evidence".to_string());
    }

    ValidationResult {
        valid: issues.is_empty(),
        message: if issues.is_empty() {
            format!("{} verified successfully", check.name)
        } else {
            format!("{} verification failed", check.name)
        },
        issues,
        recommendations,
    }
}

/// Generate summary of verification results
fn generate_summary(checklist: &VerificationChecklist) -> VerificationSummary {
    let total = checklist.checks.len();
    let passed = checklist
        .checks
        .iter()
        .filter(|c| c.evidence.as_ref().map(|e| e.passed).unwrap_or(false))
        .count();
    let failed = checklist
        .checks
        .iter()
        .filter(|c| c.completed && !c.evidence.as_ref().map(|e| e.passed).unwrap_or(false))
        .count();
    let skipped = checklist.checks.iter().filter(|c| !c.completed).count();

    let required_checks: Vec<_> = checklist.checks.iter().filter(|c| c.required).collect();
    let all_required_passed = required_checks
        .iter()
        .all(|c| c.evidence.as_ref().map(|e| e.passed).unwrap_or(false));

    let failed_checks: Vec<String> = checklist
        .checks
        .iter()
        .filter(|c| c.completed && !c.evidence.as_ref().map(|e| e.passed).unwrap_or(false))
        .map(|c| c.id.clone())
        .collect();

    let verdict = if skipped > 0 {
        Verdict::Incomplete
    } else if checklist.protocol.strict_mode && failed > 0 {
        Verdict::Rejected
    } else if all_required_passed {
        Verdict::Approved
    } else {
        Verdict::Rejected
    };

    VerificationSummary {
        total,
        passed,
        failed,
        skipped,
        all_required_passed,
        failed_checks,
        verdict,
    }
}

/// Format verification report
pub fn format_report(checklist: &VerificationChecklist, options: &ReportOptions) -> String {
    if options.format == ReportFormat::Json {
        return serde_json::to_string_pretty(checklist).unwrap_or_default();
    }

    let mut lines = vec![];

    // Header
    if options.format == ReportFormat::Markdown {
        lines.push(format!(
            "# Verification Report: {}",
            checklist.protocol.name
        ));
        lines.push(String::new());
        lines.push(format!("**Status:** {:?}", checklist.status));
        lines.push(format!("**Started:** {:?}", checklist.started_at));
        if let Some(completed) = checklist.completed_at {
            lines.push(format!("**Completed:** {:?}", completed));
        }
        lines.push(String::new());
    } else {
        lines.push(format!("Verification Report: {}", checklist.protocol.name));
        lines.push(format!("Status: {:?}", checklist.status));
        lines.push(format!("Started: {:?}", checklist.started_at));
        if let Some(completed) = checklist.completed_at {
            lines.push(format!("Completed: {:?}", completed));
        }
        lines.push(String::new());
    }

    // Summary
    if let Some(summary) = &checklist.summary {
        if options.format == ReportFormat::Markdown {
            lines.push("## Summary".to_string());
            lines.push(String::new());
            lines.push(format!("- **Total Checks:** {}", summary.total));
            lines.push(format!("- **Passed:** {}", summary.passed));
            lines.push(format!("- **Failed:** {}", summary.failed));
            lines.push(format!("- **Skipped:** {}", summary.skipped));
            lines.push(format!("- **Verdict:** {:?}", summary.verdict));
            lines.push(String::new());
        } else {
            lines.push("Summary:".to_string());
            lines.push(format!("  Total Checks: {}", summary.total));
            lines.push(format!("  Passed: {}", summary.passed));
            lines.push(format!("  Failed: {}", summary.failed));
            lines.push(format!("  Skipped: {}", summary.skipped));
            lines.push(format!("  Verdict: {:?}", summary.verdict));
            lines.push(String::new());
        }
    }

    // Checks
    if options.format == ReportFormat::Markdown {
        lines.push("## Checks".to_string());
        lines.push(String::new());
    } else {
        lines.push("Checks:".to_string());
    }

    for check in &checklist.checks {
        let status = if check.evidence.as_ref().map(|e| e.passed).unwrap_or(false) {
            "✓"
        } else if check.completed {
            "✗"
        } else {
            "○"
        };
        let required = if check.required {
            "(required)"
        } else {
            "(optional)"
        };

        if options.format == ReportFormat::Markdown {
            lines.push(format!("### {} {} {}", status, check.name, required));
            lines.push(String::new());
            lines.push(check.description.clone());
            lines.push(String::new());
        } else {
            lines.push(format!("  {} {} {}", status, check.name, required));
            lines.push(format!("     {}", check.description));
        }

        if options.include_evidence {
            if let Some(evidence) = &check.evidence {
                if options.format == ReportFormat::Markdown {
                    lines.push("**Evidence:**".to_string());
                    lines.push(format!("- Passed: {}", evidence.passed));
                    lines.push(format!("- Timestamp: {:?}", evidence.timestamp));
                    if let Some(cmd) = &evidence.command {
                        lines.push(format!("- Command: `{}`", cmd));
                    }
                    if let Some(error) = &evidence.error {
                        lines.push(format!("- Error: {}", error));
                    }
                } else {
                    lines.push(format!(
                        "     Evidence: {}",
                        if evidence.passed { "PASSED" } else { "FAILED" }
                    ));
                    if let Some(error) = &evidence.error {
                        lines.push(format!("     Error: {}", error));
                    }
                }

                if options.include_output {
                    if let Some(output) = &evidence.output {
                        if options.format == ReportFormat::Markdown {
                            lines.push(String::new());
                            lines.push("**Output:**".to_string());
                            lines.push("```".to_string());
                            lines.push(output.trim().to_string());
                            lines.push("```".to_string());
                        } else {
                            let truncated = if output.len() > 100 {
                                format!("{}...", &output[..100])
                            } else {
                                output.clone()
                            };
                            lines.push(format!("     Output: {}", truncated));
                        }
                    }
                }

                lines.push(String::new());
            }
        }
    }

    lines.join("\n")
}

/// Validate entire checklist
pub async fn validate_checklist(checklist: &VerificationChecklist) -> ValidationResult {
    let mut issues = vec![];
    let mut recommendations = vec![];

    // Check if verification is complete
    if checklist.status != VerificationStatus::Complete
        && checklist.status != VerificationStatus::Failed
    {
        issues.push("Verification is not complete".to_string());
        recommendations.push("Run verification to completion before validating".to_string());
        return ValidationResult {
            valid: false,
            message: "Incomplete verification".to_string(),
            issues,
            recommendations,
        };
    }

    // Validate each check
    for check in &checklist.checks {
        if let Some(evidence) = &check.evidence {
            let validation = check_evidence(check, evidence);
            if !validation.valid && check.required {
                issues.extend(validation.issues);
                recommendations.extend(validation.recommendations);
            }
        } else if check.required {
            issues.push(format!(
                "Missing evidence for required check: {}",
                check.name
            ));
            recommendations.push(format!("Run verification check: {}", check.name));
        }
    }

    ValidationResult {
        valid: issues.is_empty(),
        message: if issues.is_empty() {
            "All verifications passed".to_string()
        } else {
            "Some verifications failed".to_string()
        },
        issues,
        recommendations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_protocol() {
        let checks = standard_checks();
        let protocol = create_protocol(
            "test".to_string(),
            "Test protocol".to_string(),
            vec![checks["build"].clone()],
            true,
        );

        assert_eq!(protocol.name, "test");
        assert_eq!(protocol.checks.len(), 1);
        assert!(protocol.strict_mode);
    }

    #[test]
    fn test_create_checklist() {
        let checks = standard_checks();
        let protocol = create_protocol(
            "test".to_string(),
            "Test protocol".to_string(),
            vec![checks["build"].clone()],
            true,
        );

        let checklist = create_checklist(protocol);
        assert_eq!(checklist.status, VerificationStatus::Pending);
        assert_eq!(checklist.checks.len(), 1);
    }

    #[test]
    fn test_generate_summary() {
        let checks = standard_checks();
        let protocol = create_protocol(
            "test".to_string(),
            "Test protocol".to_string(),
            vec![checks["build"].clone()],
            true,
        );

        let checklist = create_checklist(protocol);
        let summary = generate_summary(&checklist);

        assert_eq!(summary.total, 1);
        assert_eq!(summary.passed, 0);
        assert_eq!(summary.verdict, Verdict::Incomplete);
    }
}
