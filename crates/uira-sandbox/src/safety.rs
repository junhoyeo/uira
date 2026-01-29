//! Command safety detection

/// Check if a command is considered dangerous
pub fn is_dangerous_command(cmd: &[String]) -> bool {
    if cmd.is_empty() {
        return false;
    }

    let program = cmd[0].as_str();
    let args: Vec<&str> = cmd[1..].iter().map(|s| s.as_str()).collect();

    match program {
        // Destructive file operations
        "rm" => args
            .iter()
            .any(|a| *a == "-f" || *a == "-rf" || *a == "-fr"),
        "rmdir" => true,
        "shred" => true,

        // Git dangerous operations
        "git" => {
            matches!(
                args.first(),
                Some(&"reset") | Some(&"clean") | Some(&"checkout")
            ) && args.iter().any(|a| *a == "--hard" || *a == "-f")
        }

        // System modification
        "sudo" => {
            !args.is_empty()
                && is_dangerous_command(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>())
        }
        "chmod" => args
            .iter()
            .any(|a| a.starts_with("777") || a.contains("+s")),
        "chown" => true,

        // Package managers (can modify system)
        "apt" | "apt-get" | "yum" | "dnf" | "pacman" => args
            .iter()
            .any(|a| *a == "install" || *a == "remove" || *a == "purge"),

        // Disk operations
        "dd" => true,
        "mkfs" => true,
        "fdisk" => true,

        // Network tools that could be abused
        "nc" | "netcat" => args.iter().any(|a| *a == "-e" || *a == "-c"),
        "curl" | "wget" => args.iter().any(|a| *a == "-o" || *a == "-O"),

        // Shell execution
        "bash" | "sh" | "zsh" => args.contains(&"-c"),
        "eval" => true,

        // Environment manipulation
        "export" => args.iter().any(|a| a.contains("PATH=")),

        _ => false,
    }
}

/// Check if a command is considered safe (read-only, no side effects)
pub fn is_safe_command(cmd: &[String]) -> bool {
    if cmd.is_empty() {
        return true;
    }

    let program = cmd[0].as_str();
    let args: Vec<&str> = cmd[1..].iter().map(|s| s.as_str()).collect();

    match program {
        // File inspection
        "cat" | "head" | "tail" | "less" | "more" => true,
        "ls" | "dir" | "tree" => true,
        "wc" | "stat" | "file" | "du" | "df" => true,

        // Search tools
        "grep" | "rg" | "ag" | "ack" => true,
        "find" => !args
            .iter()
            .any(|a| *a == "-exec" || *a == "-delete" || *a == "-execdir"),
        "fd" => true,
        "locate" | "which" | "whereis" => true,

        // Text processing (read-only)
        "sort" | "uniq" | "cut" | "tr" | "awk" | "sed" => {
            // Safe if no -i (in-place) flag for sed
            !args.iter().any(|a| *a == "-i" || a.starts_with("-i"))
        }
        "jq" | "yq" => true,

        // Version/info commands
        "pwd" | "whoami" | "id" | "hostname" | "uname" => true,
        "env" | "printenv" => true,
        "date" | "uptime" => true,

        // Development tools (safe operations)
        "git" => matches!(
            args.first(),
            Some(&"status")
                | Some(&"log")
                | Some(&"diff")
                | Some(&"show")
                | Some(&"branch")
                | Some(&"remote")
                | Some(&"tag")
        ),
        "cargo" => matches!(
            args.first(),
            Some(&"check") | Some(&"clippy") | Some(&"fmt") | Some(&"doc")
        ),
        "npm" | "yarn" | "pnpm" | "bun" => matches!(
            args.first(),
            Some(&"list") | Some(&"ls") | Some(&"outdated")
        ),
        "rustc" | "rustfmt" => args
            .iter()
            .any(|a| *a == "--check" || *a == "--emit=metadata"),

        // Node/Python introspection
        "node" | "python" | "python3" => args.iter().any(|a| *a == "--version" || *a == "-V"),

        // Echo is generally safe
        "echo" | "printf" => true,

        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(s: &str) -> Vec<String> {
        s.split_whitespace().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_dangerous_commands() {
        assert!(is_dangerous_command(&cmd("rm -rf /")));
        assert!(is_dangerous_command(&cmd("sudo rm -f file")));
        assert!(is_dangerous_command(&cmd("git reset --hard")));
        assert!(is_dangerous_command(&cmd("dd if=/dev/zero of=/dev/sda")));
    }

    #[test]
    fn test_safe_commands() {
        assert!(is_safe_command(&cmd("ls -la")));
        assert!(is_safe_command(&cmd("cat file.txt")));
        assert!(is_safe_command(&cmd("git status")));
        assert!(is_safe_command(&cmd("grep pattern file")));
        assert!(is_safe_command(&cmd("find . -name '*.rs'")));
    }

    #[test]
    fn test_find_with_exec_is_dangerous() {
        assert!(!is_safe_command(&cmd("find . -exec rm {} \\;")));
        assert!(!is_safe_command(&cmd("find . -delete")));
    }
}
