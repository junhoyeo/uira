use async_trait::async_trait;
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::io::IsTerminal;

use crate::hook::{Hook, HookContext, HookResult};
use crate::types::{HookEvent, HookInput, HookOutput};

pub const HOOK_NAME: &str = "non-interactive-env";

/// Order matters for `build_env_prefix`.
pub const NON_INTERACTIVE_ENV: [(&str, &str); 15] = [
    ("CI", "true"),
    ("DEBIAN_FRONTEND", "noninteractive"),
    ("GIT_TERMINAL_PROMPT", "0"),
    ("GCM_INTERACTIVE", "never"),
    ("HOMEBREW_NO_AUTO_UPDATE", "1"),
    // Block interactive editors - git rebase, commit, etc.
    ("GIT_EDITOR", ":"),
    ("EDITOR", ":"),
    ("VISUAL", ""),
    ("GIT_SEQUENCE_EDITOR", ":"),
    ("GIT_MERGE_AUTOEDIT", "no"),
    // Block pagers
    ("GIT_PAGER", "cat"),
    ("PAGER", "cat"),
    // NPM non-interactive
    ("npm_config_yes", "true"),
    // Pip non-interactive
    ("PIP_NO_INPUT", "1"),
    // Yarn non-interactive
    ("YARN_ENABLE_IMMUTABLE_INSTALLS", "false"),
];

/// Shell command guidance for non-interactive environments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellCommandPatterns {
    pub npm: PatternGroup,
    pub apt: PatternGroup,
    pub pip: PatternGroup,
    pub git: PatternGroup,
    pub system: PatternGroup,
    pub banned: Vec<String>,
    pub workarounds: Workarounds,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternGroup {
    pub bad: Vec<String>,
    pub good: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workarounds {
    #[serde(rename = "yesPipe")]
    pub yes_pipe: String,
    pub heredoc: String,
    #[serde(rename = "expectAlternative")]
    pub expect_alternative: String,
}

lazy_static! {
    pub static ref SHELL_COMMAND_PATTERNS: ShellCommandPatterns = ShellCommandPatterns {
        npm: PatternGroup {
            bad: vec!["npm init".to_string(), "npm install (prompts)".to_string()],
            good: vec!["npm init -y".to_string(), "npm install --yes".to_string()],
        },
        apt: PatternGroup {
            bad: vec!["apt-get install pkg".to_string()],
            good: vec![
                "apt-get install -y pkg".to_string(),
                "DEBIAN_FRONTEND=noninteractive apt-get install pkg".to_string(),
            ],
        },
        pip: PatternGroup {
            bad: vec!["pip install pkg (with prompts)".to_string()],
            good: vec![
                "pip install --no-input pkg".to_string(),
                "PIP_NO_INPUT=1 pip install pkg".to_string(),
            ],
        },
        git: PatternGroup {
            bad: vec![
                "git commit".to_string(),
                "git merge branch".to_string(),
                "git add -p".to_string(),
                "git rebase -i".to_string(),
            ],
            good: vec![
                "git commit -m 'msg'".to_string(),
                "git merge --no-edit branch".to_string(),
                "git add .".to_string(),
                "git rebase --no-edit".to_string(),
            ],
        },
        system: PatternGroup {
            bad: vec![
                "rm file (prompts)".to_string(),
                "cp a b (prompts)".to_string(),
                "ssh host".to_string(),
            ],
            good: vec![
                "rm -f file".to_string(),
                "cp -f a b".to_string(),
                "ssh -o BatchMode=yes host".to_string(),
                "unzip -o file.zip".to_string(),
            ],
        },
        banned: vec![
            "vim".to_string(),
            "nano".to_string(),
            "vi".to_string(),
            "emacs".to_string(),
            "less".to_string(),
            "more".to_string(),
            "man".to_string(),
            "python (REPL)".to_string(),
            "node (REPL)".to_string(),
            "git add -p".to_string(),
            "git rebase -i".to_string(),
        ],
        workarounds: Workarounds {
            yes_pipe: "yes | ./script.sh".to_string(),
            heredoc: "./script.sh <<EOF\noption1\noption2\nEOF".to_string(),
            expect_alternative: "Use environment variables or config files instead of expect"
                .to_string(),
        },
    };
    static ref SPECIAL_CHARS: Regex = Regex::new(r"[^a-zA-Z0-9_\-.:/]").unwrap();
    static ref IS_GIT_COMMAND: Regex = Regex::new(r"\bgit\b").unwrap();
}

/// Hook config (currently parity-only; matches TS shape).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NonInteractiveEnvConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeforeCommandResult {
    pub command: String,
    pub warning: Option<String>,
}

pub fn is_non_interactive() -> bool {
    let ci = std::env::var("CI").ok();
    if matches!(ci.as_deref(), Some("true") | Some("1")) {
        return true;
    }

    if matches!(
        std::env::var("CLAUDE_CODE_RUN").ok().as_deref(),
        Some("true")
    ) || matches!(
        std::env::var("CLAUDE_CODE_NON_INTERACTIVE").ok().as_deref(),
        Some("true")
    ) {
        return true;
    }

    if matches!(
        std::env::var("GITHUB_ACTIONS").ok().as_deref(),
        Some("true")
    ) {
        return true;
    }

    if !std::io::stdout().is_terminal() {
        return true;
    }

    false
}

/// Shell-escape a value for use in VAR=value prefix.
/// Wraps in single quotes if contains special chars.
fn shell_escape(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }

    if SPECIAL_CHARS.is_match(value) {
        let escaped = value.replace("'", "'\\''");
        return format!("'{}'", escaped);
    }

    value.to_string()
}

/// Build export statement for environment variables.
/// Uses `export VAR1=val1 VAR2=val2;` format.
fn build_env_prefix(env: &[(&str, &str)]) -> String {
    let exports = env
        .iter()
        .map(|(k, v)| format!("{}={}", k, shell_escape(v)))
        .collect::<Vec<_>>()
        .join(" ");
    format!("export {};", exports)
}

lazy_static! {
    // NOTE: This intentionally reproduces a subtle bug in the TypeScript implementation:
    // the regex list is built from a filtered `banned` list, but the reported command
    // is indexed from the unfiltered list.
    static ref BANNED_COMMAND_PATTERNS: Vec<Regex> = SHELL_COMMAND_PATTERNS
        .banned
        .iter()
        .filter(|cmd| !cmd.contains('('))
        .map(|cmd| Regex::new(&format!(r"\b{}\b", cmd)).unwrap())
        .collect();
}

fn detect_banned_command(command: &str) -> Option<String> {
    for (i, re) in BANNED_COMMAND_PATTERNS.iter().enumerate() {
        if re.is_match(command) {
            return SHELL_COMMAND_PATTERNS.banned.get(i).cloned();
        }
    }
    None
}

pub struct NonInteractiveEnvHook;

impl NonInteractiveEnvHook {
    pub fn new() -> Self {
        Self
    }

    pub async fn before_command(&self, command: &str) -> BeforeCommandResult {
        let banned_cmd = detect_banned_command(command);
        let warning = banned_cmd.map(|cmd| {
            format!(
                "Warning: '{}' is an interactive command that may hang in non-interactive environments.",
                cmd
            )
        });

        if !IS_GIT_COMMAND.is_match(command) {
            return BeforeCommandResult {
                command: command.to_string(),
                warning,
            };
        }

        let env_prefix = build_env_prefix(&NON_INTERACTIVE_ENV);
        let modified = format!("{} {}", env_prefix, command);

        BeforeCommandResult {
            command: modified,
            warning,
        }
    }
}

impl Default for NonInteractiveEnvHook {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Hook for NonInteractiveEnvHook {
    fn name(&self) -> &str {
        HOOK_NAME
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::SessionStart]
    }

    async fn execute(
        &self,
        _event: HookEvent,
        _input: &HookInput,
        _context: &HookContext,
    ) -> HookResult {
        // Check if running in non-interactive environment
        if is_non_interactive() {
            let message = format!(
                "Non-interactive environment detected. Git commands will be automatically prefixed with: {}",
                build_env_prefix(&NON_INTERACTIVE_ENV)
            );
            Ok(HookOutput::continue_with_message(message))
        } else {
            Ok(HookOutput::pass())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_escape() {
        assert_eq!(shell_escape(""), "''");
        assert_eq!(shell_escape("abc"), "abc");
        assert_eq!(shell_escape("a b"), "'a b'");
        assert_eq!(shell_escape("a'b"), "'a'\\''b'");
    }

    #[test]
    fn test_build_env_prefix_format() {
        let prefix = build_env_prefix(&[("A", "1"), ("B", "a b"), ("C", "")]);
        assert_eq!(prefix, "export A=1 B='a b' C='';");
    }

    #[test]
    fn test_detect_banned_command_ts_indexing_bug() {
        // Regex list filters out entries containing "(", but indexing still reads
        // from the original list.
        let banned = detect_banned_command("git add -p file").unwrap();
        assert_eq!(banned, "python (REPL)");

        let banned = detect_banned_command("git rebase -i HEAD~1").unwrap();
        assert_eq!(banned, "node (REPL)");
    }

    #[tokio::test]
    async fn test_before_command_prepends_env_for_git_only() {
        let hook = NonInteractiveEnvHook::new();

        let res = hook.before_command("echo hi").await;
        assert_eq!(res.command, "echo hi");

        let res = hook.before_command("git status").await;
        assert!(res.command.starts_with("export "));
        assert!(res.command.contains("git status"));
    }

    #[test]
    fn test_is_non_interactive_env_var() {
        std::env::set_var("CI", "true");
        assert!(is_non_interactive());
        std::env::remove_var("CI");
    }
}
