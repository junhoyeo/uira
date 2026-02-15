//! Sandbox policy definitions

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Type of sandbox to use
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxType {
    /// No sandboxing
    None,
    /// Platform-native sandbox (Seatbelt on macOS, Landlock on Linux)
    #[default]
    Native,
    /// Container-based isolation
    Container,
}

/// Policy for sandbox restrictions
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxPolicy {
    /// Read anywhere, write nowhere
    ReadOnly,
    /// Read anywhere, write only to workspace
    WorkspaceWrite {
        workspace: PathBuf,
        #[serde(default)]
        protected_paths: Vec<PathBuf>,
    },
    /// Full access (no restrictions)
    FullAccess,
    /// Custom policy with explicit allow/deny lists
    Custom {
        readable: Vec<PathBuf>,
        writable: Vec<PathBuf>,
        executable: Vec<PathBuf>,
        network: bool,
    },
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        Self::WorkspaceWrite {
            workspace: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            protected_paths: vec![],
        }
    }
}

impl SandboxPolicy {
    pub fn read_only() -> Self {
        Self::ReadOnly
    }

    pub fn workspace_write(workspace: impl Into<PathBuf>) -> Self {
        Self::WorkspaceWrite {
            workspace: workspace.into(),
            protected_paths: vec![],
        }
    }

    pub fn full_access() -> Self {
        Self::FullAccess
    }

    pub fn is_restrictive(&self) -> bool {
        !matches!(self, Self::FullAccess)
    }

    pub fn allows_network(&self) -> bool {
        match self {
            Self::Custom { network, .. } => *network,
            Self::FullAccess => true,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_policy() {
        let policy = SandboxPolicy::default();
        assert!(policy.is_restrictive());
    }

    #[test]
    fn test_full_access_not_restrictive() {
        let policy = SandboxPolicy::full_access();
        assert!(!policy.is_restrictive());
    }
}
