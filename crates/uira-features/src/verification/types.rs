use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

/// Types of verification evidence
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerificationEvidenceType {
    BuildSuccess,
    TestPass,
    LintClean,
    FunctionalityVerified,
    ArchitectApproval,
    TodoComplete,
    ErrorFree,
}

/// Proof of verification for a specific check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationEvidence {
    /// Type of evidence
    pub evidence_type: VerificationEvidenceType,
    /// Whether the check passed
    pub passed: bool,
    /// Command that was run to verify (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Output from the verification command
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// Error message if check failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Timestamp when evidence was collected
    pub timestamp: SystemTime,
    /// Additional metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// A single verification check requirement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationCheck {
    /// Unique identifier for this check
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Description of what this check verifies
    pub description: String,
    /// Type of evidence this check produces
    pub evidence_type: VerificationEvidenceType,
    /// Whether this check is required for completion
    pub required: bool,
    /// Command to run for verification (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Whether this check has been completed
    pub completed: bool,
    /// Evidence collected for this check
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<VerificationEvidence>,
}

/// Complete verification protocol definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationProtocol {
    /// Protocol name (e.g., "ralph", "autopilot", "ultrawork")
    pub name: String,
    /// Description of what this protocol verifies
    pub description: String,
    /// List of verification checks to perform
    pub checks: Vec<VerificationCheck>,
    /// Whether all required checks must pass
    pub strict_mode: bool,
    /// Optional custom validation function (not serializable)
    #[serde(skip)]
    pub custom_validator: Option<fn(&VerificationChecklist) -> ValidationResult>,
}

/// Verification status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerificationStatus {
    Pending,
    InProgress,
    Complete,
    Failed,
}

/// Verdict for verification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict {
    Approved,
    Rejected,
    Incomplete,
}

/// Current state of verification checks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationChecklist {
    /// Protocol being followed
    pub protocol: VerificationProtocol,
    /// Timestamp when verification started
    pub started_at: SystemTime,
    /// Timestamp when verification completed (if finished)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<SystemTime>,
    /// All checks with their current status
    pub checks: Vec<VerificationCheck>,
    /// Overall completion status
    pub status: VerificationStatus,
    /// Summary of results
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<VerificationSummary>,
}

/// Summary of verification results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationSummary {
    /// Total number of checks
    pub total: usize,
    /// Number of checks passed
    pub passed: usize,
    /// Number of checks failed
    pub failed: usize,
    /// Number of checks skipped (non-required)
    pub skipped: usize,
    /// Whether all required checks passed
    pub all_required_passed: bool,
    /// List of failed check IDs
    pub failed_checks: Vec<String>,
    /// Overall verdict
    pub verdict: Verdict,
}

/// Result of validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    /// Whether validation passed
    pub valid: bool,
    /// Validation message
    pub message: String,
    /// List of issues found
    pub issues: Vec<String>,
    /// Recommendations for fixing issues
    pub recommendations: Vec<String>,
}

/// Options for running verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationOptions {
    /// Whether to run checks in parallel
    #[serde(default)]
    pub parallel: bool,
    /// Timeout per check
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<Duration>,
    /// Whether to stop on first failure
    #[serde(default)]
    pub fail_fast: bool,
    /// Whether to skip non-required checks
    #[serde(default)]
    pub skip_optional: bool,
    /// Custom working directory
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

impl Default for VerificationOptions {
    fn default() -> Self {
        Self {
            parallel: true,
            timeout: Some(Duration::from_secs(60)),
            fail_fast: false,
            skip_optional: false,
            cwd: None,
        }
    }
}

/// Report format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReportFormat {
    Text,
    Markdown,
    Json,
}

/// Report format options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportOptions {
    /// Include detailed evidence in report
    #[serde(default = "default_true")]
    pub include_evidence: bool,
    /// Include command output in report
    #[serde(default)]
    pub include_output: bool,
    /// Format for report
    #[serde(default = "default_markdown")]
    pub format: ReportFormat,
    /// Whether to colorize output (for terminal)
    #[serde(default)]
    pub colorize: bool,
}

fn default_true() -> bool {
    true
}

fn default_markdown() -> ReportFormat {
    ReportFormat::Markdown
}

impl Default for ReportOptions {
    fn default() -> Self {
        Self {
            include_evidence: true,
            include_output: false,
            format: ReportFormat::Markdown,
            colorize: false,
        }
    }
}
