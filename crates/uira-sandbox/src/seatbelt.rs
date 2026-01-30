//! macOS Seatbelt (sandbox-exec) implementation

use crate::SandboxPolicy;

/// Generate a Seatbelt policy string for the given sandbox policy
#[allow(dead_code)] // Used when sandbox is applied
pub fn generate_policy(policy: &SandboxPolicy) -> String {
    let mut rules = vec![
        "(version 1)".to_string(),
        "(deny default)".to_string(),
        // Always allow basic process operations
        "(allow process-exec process-fork)".to_string(),
        "(allow signal)".to_string(),
        // Allow mach operations for IPC
        "(allow mach-lookup)".to_string(),
        // Allow sysctl for system info
        "(allow sysctl-read)".to_string(),
    ];

    match policy {
        SandboxPolicy::ReadOnly => {
            rules.push("(allow file-read*)".to_string());
        }
        SandboxPolicy::WorkspaceWrite {
            workspace,
            protected_paths,
        } => {
            rules.push("(allow file-read*)".to_string());

            // Allow write to workspace
            let workspace_path = workspace.to_string_lossy();
            rules.push(format!(
                "(allow file-write* (subpath \"{}\"))",
                workspace_path
            ));

            // Deny write to protected paths
            for path in protected_paths {
                let protected = format!("{}/{}", workspace_path, path.to_string_lossy());
                rules.push(format!("(deny file-write* (subpath \"{}\"))", protected));
            }

            // Allow write to temp directories
            rules.push("(allow file-write* (subpath \"/tmp\"))".to_string());
            rules.push("(allow file-write* (subpath \"/var/tmp\"))".to_string());
            // Note: Would add ~/Library/Caches with dirs crate if available
        }
        SandboxPolicy::FullAccess => {
            rules.push("(allow file-read*)".to_string());
            rules.push("(allow file-write*)".to_string());
            rules.push("(allow network*)".to_string());
        }
        SandboxPolicy::Custom {
            readable,
            writable,
            executable,
            network,
        } => {
            for path in readable {
                rules.push(format!(
                    "(allow file-read* (subpath \"{}\"))",
                    path.to_string_lossy()
                ));
            }
            for path in writable {
                rules.push(format!(
                    "(allow file-write* (subpath \"{}\"))",
                    path.to_string_lossy()
                ));
            }
            for path in executable {
                rules.push(format!(
                    "(allow process-exec (subpath \"{}\"))",
                    path.to_string_lossy()
                ));
            }
            if *network {
                rules.push("(allow network*)".to_string());
            }
        }
    }

    rules.join("\n")
}

/// Create the command-line arguments for sandbox-exec
#[allow(dead_code)] // Used when sandbox is applied
pub fn sandbox_exec_args(policy: &str) -> Vec<String> {
    vec![
        "/usr/bin/sandbox-exec".to_string(),
        "-p".to_string(),
        policy.to_string(),
        "--".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_read_only_policy() {
        let policy = generate_policy(&SandboxPolicy::ReadOnly);
        assert!(policy.contains("(deny default)"));
        assert!(policy.contains("(allow file-read*)"));
        assert!(!policy.contains("(allow file-write*)"));
    }

    #[test]
    fn test_workspace_write_policy() {
        let policy = generate_policy(&SandboxPolicy::WorkspaceWrite {
            workspace: PathBuf::from("/workspace"),
            protected_paths: vec![PathBuf::from(".git")],
        });
        assert!(policy.contains("(allow file-write* (subpath \"/workspace\"))"));
        assert!(policy.contains("(deny file-write* (subpath \"/workspace/.git\"))"));
    }
}
