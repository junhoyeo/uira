//! macOS Seatbelt (sandbox-exec) implementation

use super::SandboxPolicy;

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

    // Allow access to /dev/null, /dev/tty, /dev/random, /dev/urandom
    rules.push("(allow file-read* file-write* (literal \"/dev/null\"))".to_string());
    rules.push("(allow file-read* file-write* (literal \"/dev/tty\"))".to_string());
    rules.push("(allow file-read* (literal \"/dev/random\"))".to_string());
    rules.push("(allow file-read* (literal \"/dev/urandom\"))".to_string());

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

            // Allow write to user's home directory for cross-project operations
            if let Ok(home) = std::env::var("HOME") {
                if home != workspace_path.as_ref() {
                    rules.push(format!("(allow file-write* (subpath \"{}\"))", home));
                }
            }

            // Deny write to protected paths
            for path in protected_paths {
                let protected = format!("{}/{}", workspace_path, path.to_string_lossy());
                rules.push(format!("(deny file-write* (subpath \"{}\"))", protected));
            }

            // Allow write to temp directories
            rules.push("(allow file-write* (subpath \"/tmp\"))".to_string());
            rules.push("(allow file-write* (subpath \"/var/tmp\"))".to_string());
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
        assert!(policy.contains("(allow file-read* file-write* (literal \"/dev/null\"))"));
        assert!(!policy.contains("(allow file-write*)"));
    }

    #[test]
    fn test_dev_null_allowed_in_all_policy_modes() {
        let policies = vec![
            SandboxPolicy::ReadOnly,
            SandboxPolicy::WorkspaceWrite {
                workspace: PathBuf::from("/workspace"),
                protected_paths: vec![],
            },
            SandboxPolicy::FullAccess,
            SandboxPolicy::Custom {
                readable: vec![],
                writable: vec![],
                executable: vec![],
                network: false,
            },
        ];

        for policy in policies {
            let generated = generate_policy(&policy);
            assert!(generated.contains("(allow file-read* file-write* (literal \"/dev/null\"))"));
        }
    }

    #[test]
    fn test_workspace_write_policy() {
        let policy = generate_policy(&SandboxPolicy::WorkspaceWrite {
            workspace: PathBuf::from("/workspace"),
            protected_paths: vec![],
        });
        assert!(policy.contains("(allow file-write* (subpath \"/workspace\"))"));
        // .git is no longer protected by default
        assert!(!policy.contains("(deny file-write* (subpath \"/workspace/.git\"))"));
    }

    #[test]
    fn test_workspace_write_allows_home_directory() {
        let home = std::env::var("HOME").expect("HOME env var not set");
        let policy = generate_policy(&SandboxPolicy::WorkspaceWrite {
            workspace: PathBuf::from("/workspace"),
            protected_paths: vec![],
        });
        // Should allow writes to home directory for cross-project operations
        assert!(policy.contains(&format!("(allow file-write* (subpath \"{}\"))", home)));
    }

    #[test]
    fn test_workspace_write_home_equals_workspace() {
        // When workspace is the home directory, don't duplicate the rule
        if let Ok(home) = std::env::var("HOME") {
            let policy = generate_policy(&SandboxPolicy::WorkspaceWrite {
                workspace: PathBuf::from(&home),
                protected_paths: vec![],
            });
            // Count occurrences of the home path rule - should be exactly 1
            let count = policy
                .matches(&format!("(allow file-write* (subpath \"{}\"))", home))
                .count();
            assert_eq!(
                count, 1,
                "Home directory rule should appear exactly once when workspace equals home"
            );
        }
    }
}
