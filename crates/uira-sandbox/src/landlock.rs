//! Linux Landlock implementation

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::SandboxPolicy;

/// Landlock access rights for files
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AccessRights {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

impl Default for AccessRights {
    fn default() -> Self {
        Self {
            read: true,
            write: false,
            execute: false,
        }
    }
}

impl AccessRights {
    pub fn read_only() -> Self {
        Self::default()
    }

    pub fn read_write() -> Self {
        Self {
            read: true,
            write: true,
            execute: false,
        }
    }

    pub fn full() -> Self {
        Self {
            read: true,
            write: true,
            execute: true,
        }
    }
}

/// Rule for Landlock path restrictions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LandlockRule {
    pub path: String,
    pub access: AccessRights,
}

/// Generate Landlock rules from a sandbox policy
pub fn generate_rules(policy: &SandboxPolicy) -> Vec<LandlockRule> {
    let mut rules = Vec::new();

    match policy {
        SandboxPolicy::ReadOnly => {
            // Read-only access to everything
            rules.push(LandlockRule {
                path: "/".to_string(),
                access: AccessRights::read_only(),
            });
        }
        SandboxPolicy::WorkspaceWrite {
            workspace,
            protected_paths,
        } => {
            // Read-only root
            rules.push(LandlockRule {
                path: "/".to_string(),
                access: AccessRights::read_only(),
            });

            // Read-write workspace
            rules.push(LandlockRule {
                path: workspace.to_string_lossy().to_string(),
                access: AccessRights::read_write(),
            });

            // Temp directories
            rules.push(LandlockRule {
                path: "/tmp".to_string(),
                access: AccessRights::read_write(),
            });
            rules.push(LandlockRule {
                path: "/var/tmp".to_string(),
                access: AccessRights::read_write(),
            });

            // Protected paths would need special handling
            // (Landlock doesn't directly support deny rules, so we'd need to
            // structure the rules to exclude protected paths)
        }
        SandboxPolicy::FullAccess => {
            rules.push(LandlockRule {
                path: "/".to_string(),
                access: AccessRights::full(),
            });
        }
        SandboxPolicy::Custom {
            readable,
            writable,
            executable,
            ..
        } => {
            for path in readable {
                rules.push(LandlockRule {
                    path: path.to_string_lossy().to_string(),
                    access: AccessRights::read_only(),
                });
            }
            for path in writable {
                rules.push(LandlockRule {
                    path: path.to_string_lossy().to_string(),
                    access: AccessRights::read_write(),
                });
            }
            for path in executable {
                rules.push(LandlockRule {
                    path: path.to_string_lossy().to_string(),
                    access: AccessRights::full(),
                });
            }
        }
    }

    rules
}

/// Apply Landlock rules to the current process
#[cfg(target_os = "linux")]
pub fn apply_landlock(rules: &[LandlockRule]) -> Result<(), crate::SandboxError> {
    use landlock::{
        Access, AccessFs, PathBeneath, PathFd, Ruleset, RulesetAttr, RulesetCreatedAttr, ABI,
    };
    use std::os::unix::io::AsFd;

    // Use the latest available ABI
    let abi = ABI::V5;

    // Define the access rights we want to handle
    let access_fs_read = AccessFs::Execute
        | AccessFs::ReadFile
        | AccessFs::ReadDir
        | AccessFs::MakeChar
        | AccessFs::MakeDir
        | AccessFs::MakeReg
        | AccessFs::MakeSock
        | AccessFs::MakeFifo
        | AccessFs::MakeBlock
        | AccessFs::MakeSym
        | AccessFs::Refer
        | AccessFs::Truncate
        | AccessFs::IoctlDev;

    let access_fs_write = AccessFs::WriteFile | AccessFs::RemoveDir | AccessFs::RemoveFile;

    let access_fs_all = access_fs_read | access_fs_write;

    // Create the ruleset
    let mut ruleset = Ruleset::default()
        .handle_access(access_fs_all)
        .map_err(|e| crate::SandboxError::PolicyError(e.to_string()))?
        .create()
        .map_err(|e| crate::SandboxError::PolicyError(e.to_string()))?;

    // Add rules for each path
    for rule in rules {
        let path = Path::new(&rule.path);
        if !path.exists() {
            continue; // Skip non-existent paths
        }

        let path_fd = PathFd::new(path).map_err(|e| {
            crate::SandboxError::PolicyError(format!("Failed to open path {}: {}", rule.path, e))
        })?;

        let mut access = AccessFs::empty();

        if rule.access.read {
            access |= AccessFs::ReadFile | AccessFs::ReadDir | AccessFs::Execute;
        }
        if rule.access.write {
            access |= AccessFs::WriteFile
                | AccessFs::RemoveDir
                | AccessFs::RemoveFile
                | AccessFs::MakeChar
                | AccessFs::MakeDir
                | AccessFs::MakeReg
                | AccessFs::MakeSock
                | AccessFs::MakeFifo
                | AccessFs::MakeBlock
                | AccessFs::MakeSym
                | AccessFs::Truncate;
        }
        if rule.access.execute {
            access |= AccessFs::Execute;
        }

        if !access.is_empty() {
            ruleset = ruleset
                .add_rule(PathBeneath::new(path_fd, access))
                .map_err(|e| crate::SandboxError::PolicyError(e.to_string()))?;
        }
    }

    // Restrict the current thread
    ruleset
        .restrict_self()
        .map_err(|e| crate::SandboxError::PolicyError(format!("Failed to restrict self: {}", e)))?;

    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn apply_landlock(_rules: &[LandlockRule]) -> Result<(), crate::SandboxError> {
    Err(crate::SandboxError::NotAvailable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_generate_read_only_rules() {
        let rules = generate_rules(&SandboxPolicy::ReadOnly);
        assert_eq!(rules.len(), 1);
        assert!(rules[0].access.read);
        assert!(!rules[0].access.write);
    }

    #[test]
    fn test_generate_workspace_rules() {
        let rules = generate_rules(&SandboxPolicy::WorkspaceWrite {
            workspace: PathBuf::from("/home/user/project"),
            protected_paths: vec![],
        });
        assert!(rules.len() >= 3); // root + workspace + tmp dirs
    }
}
