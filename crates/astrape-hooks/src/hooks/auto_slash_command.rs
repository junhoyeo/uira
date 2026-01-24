//! Auto Slash Command Hook
//!
//! Detects and expands `/astrape:*` slash commands in user prompts.
//!
//! Ported from:
//! - oh-my-claudecode/src/hooks/auto-slash-command/{index,detector,executor,constants,types}.ts

use async_trait::async_trait;
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use crate::hook::{Hook, HookContext, HookResult};
use crate::types::{HookEvent, HookInput, HookOutput, MessagePart};

// --- constants.ts ---

pub const HOOK_NAME: &str = "auto-slash-command";

/// XML tags to mark auto-expanded slash commands
pub const AUTO_SLASH_COMMAND_TAG_OPEN: &str = "<auto-slash-command>";
pub const AUTO_SLASH_COMMAND_TAG_CLOSE: &str = "</auto-slash-command>";

/// Pattern to detect `/astrape:*` slash commands at the start of a message.
///
/// Captures:
/// - group 1: command name (without leading slash)
/// - group 2: argument string (possibly empty)
///
/// NOTE: No lookahead/lookbehind.
pub const SLASH_COMMAND_PATTERN: &str = r"^/(astrape:[a-zA-Z][\w-]*)\s*(.*)";

/// Commands that should NOT be auto-expanded.
///
/// This list mirrors the TypeScript exclusion list semantics.
pub const EXCLUDED_COMMANDS: &[&str] = &[
    // Mode toggles handled by other hooks
    "astrape:ralph",
    "astrape:cancel-ralph",
    // Claude Code built-ins (not matched by our pattern, but kept for completeness)
    "help",
    "clear",
    "history",
    "exit",
    "quit",
];

// --- types.ts ---

/// Input for auto slash command hook
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AutoSlashCommandHookInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
}

/// Output for auto slash command hook
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AutoSlashCommandHookOutput {
    pub parts: Vec<serde_json::Value>,
}

/// Parsed slash command from user input
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedSlashCommand {
    /// The command name without the leading slash
    pub command: String,
    /// Arguments passed to the command
    pub args: String,
    /// Raw matched text
    pub raw: String,
}

/// Result of auto slash command detection
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoSlashCommandResult {
    pub detected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parsed_command: Option<ParsedSlashCommand>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub injected_message: Option<String>,
}

/// Command scope indicating where it was discovered
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommandScope {
    User,
    Project,
    Skill,
}

impl std::fmt::Display for CommandScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::User => write!(f, "user"),
            Self::Project => write!(f, "project"),
            Self::Skill => write!(f, "skill"),
        }
    }
}

/// Command metadata from frontmatter
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommandMetadata {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub argument_hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
}

/// Discovered command information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub metadata: CommandMetadata,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    pub scope: CommandScope,
}

/// Result of executing a slash command
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecuteResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// --- detector.ts ---

lazy_static! {
    static ref CODE_BLOCK_PATTERN: Regex = Regex::new(r"```[\s\S]*?```").unwrap();
    static ref SLASH_COMMAND_RE: Regex = Regex::new(SLASH_COMMAND_PATTERN).unwrap();
    static ref EXCLUDED_SET: HashSet<&'static str> = EXCLUDED_COMMANDS.iter().copied().collect();
}

/// Remove code blocks from text to prevent false positives
pub fn remove_code_blocks(text: &str) -> String {
    CODE_BLOCK_PATTERN.replace_all(text, "").to_string()
}

/// Parse a slash command from text
pub fn parse_slash_command(text: &str) -> Option<ParsedSlashCommand> {
    let trimmed = text.trim();
    if !trimmed.starts_with('/') {
        return None;
    }

    let caps = SLASH_COMMAND_RE.captures(trimmed)?;
    let raw = caps.get(0)?.as_str().to_string();
    let command = caps
        .get(1)
        .map(|m| m.as_str().to_lowercase())
        .unwrap_or_default();
    let args = caps
        .get(2)
        .map(|m| m.as_str().trim().to_string())
        .unwrap_or_default();

    Some(ParsedSlashCommand { command, args, raw })
}

/// Check if a command should be excluded from auto-expansion
pub fn is_excluded_command(command: &str) -> bool {
    let lower = command.to_lowercase();
    EXCLUDED_SET.contains(lower.as_str())
}

/// Detect a slash command in user input text.
/// Returns `None` if no command detected or if command is excluded.
pub fn detect_slash_command(text: &str) -> Option<ParsedSlashCommand> {
    let text_without_code = remove_code_blocks(text);
    let trimmed = text_without_code.trim();

    if !trimmed.starts_with('/') {
        return None;
    }

    let parsed = parse_slash_command(trimmed)?;
    if is_excluded_command(&parsed.command) {
        return None;
    }

    Some(parsed)
}

/// Extract text content from message parts array
pub fn extract_prompt_text(parts: &[MessagePart]) -> String {
    parts
        .iter()
        .filter(|p| p.part_type == "text")
        .filter_map(|p| p.text.as_ref())
        .cloned()
        .collect::<Vec<_>>()
        .join(" ")
}

// --- executor.ts ---

fn claude_config_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".claude"))
}

fn parse_frontmatter(content: &str) -> (HashMap<String, String>, String) {
    // Port of TypeScript regex: /^---\r?\n([\s\S]*?)\r?\n---\r?\n?([\s\S]*)$/
    // NOTE: no lookahead/lookbehind.
    let re = Regex::new(r"(?s)^---\r?\n(.*?)\r?\n---\r?\n?(.*)$").unwrap();
    let Some(caps) = re.captures(content) else {
        return (HashMap::new(), content.to_string());
    };

    let yaml_content = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
    let body = caps.get(2).map(|m| m.as_str()).unwrap_or_default();

    let mut data = HashMap::new();
    for line in yaml_content.lines() {
        let Some(colon_idx) = line.find(':') else {
            continue;
        };

        let key = line[..colon_idx].trim();
        let mut value = line[colon_idx + 1..].trim().to_string();

        if (value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\''))
        {
            value = value[1..value.len().saturating_sub(1)].to_string();
        }

        data.insert(key.to_string(), value);
    }

    (data, body.to_string())
}

fn discover_commands_from_dir(commands_dir: &Path, scope: CommandScope) -> Vec<CommandInfo> {
    let Ok(entries) = fs::read_dir(commands_dir) else {
        return Vec::new();
    };

    let mut commands = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }

        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let command_name = stem.to_string();

        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        let (data, body) = parse_frontmatter(&content);

        let metadata = CommandMetadata {
            name: command_name.clone(),
            description: data.get("description").cloned().unwrap_or_default(),
            argument_hint: data.get("argument-hint").cloned(),
            model: data.get("model").cloned(),
            agent: data.get("agent").cloned(),
        };

        commands.push(CommandInfo {
            name: command_name,
            path: Some(path.to_string_lossy().to_string()),
            metadata,
            content: Some(body),
            scope,
        });
    }

    commands
}

/// Discover all available commands from multiple sources
pub fn discover_all_commands(working_directory: &Path) -> Vec<CommandInfo> {
    let Some(claude_dir) = claude_config_dir() else {
        return Vec::new();
    };

    let user_commands_dir = claude_dir.join("commands");
    let project_commands_dir = working_directory.join(".claude").join("commands");
    let skills_dir = claude_dir.join("skills");

    let user_commands = discover_commands_from_dir(&user_commands_dir, CommandScope::User);
    let project_commands = discover_commands_from_dir(&project_commands_dir, CommandScope::Project);

    // Discover skills (each skill directory may have a SKILL.md)
    let mut skill_commands = Vec::new();
    let Ok(skill_entries) = fs::read_dir(skills_dir) else {
        // Priority: project > user > skills
        return project_commands
            .into_iter()
            .chain(user_commands)
            .collect();
    };

    for entry in skill_entries.flatten() {
        let dir_path = entry.path();
        if !dir_path.is_dir() {
            continue;
        }

        let skill_md = dir_path.join("SKILL.md");
        if !skill_md.exists() {
            continue;
        }

        let Ok(content) = fs::read_to_string(&skill_md) else {
            continue;
        };
        let (data, body) = parse_frontmatter(&content);

        let raw_name = data
            .get("name")
            .cloned()
            .unwrap_or_else(|| dir_path.file_name().unwrap_or_default().to_string_lossy().to_string());

        let metadata = CommandMetadata {
            name: raw_name.clone(),
            description: data.get("description").cloned().unwrap_or_default(),
            argument_hint: data.get("argument-hint").cloned(),
            model: data.get("model").cloned(),
            agent: data.get("agent").cloned(),
        };

        skill_commands.push(CommandInfo {
            name: raw_name,
            path: Some(skill_md.to_string_lossy().to_string()),
            metadata,
            content: Some(body),
            scope: CommandScope::Skill,
        });
    }

    // Priority: project > user > skills
    project_commands
        .into_iter()
        .chain(user_commands)
        .chain(skill_commands)
        .collect()
}

/// Find a specific command by name
pub fn find_command(command_name: &str, working_directory: &Path) -> Option<CommandInfo> {
    discover_all_commands(working_directory)
        .into_iter()
        .find(|cmd| cmd.name.eq_ignore_ascii_case(command_name))
}

fn resolve_arguments(content: &str, args: &str) -> String {
    content.replace(
        "$ARGUMENTS",
        if args.is_empty() {
            "(no arguments provided)"
        } else {
            args
        },
    )
}

fn format_command_template(cmd: &CommandInfo, args: &str) -> String {
    let mut sections = Vec::<String>::new();

    sections.push(format!("<command-name>/{}</command-name>\n", cmd.name));

    if !cmd.metadata.description.is_empty() {
        sections.push(format!("**Description**: {}\n", cmd.metadata.description));
    }

    if !args.is_empty() {
        sections.push(format!("**Arguments**: {}\n", args));
    }

    if let Some(model) = &cmd.metadata.model {
        if !model.is_empty() {
            sections.push(format!("**Model**: {}\n", model));
        }
    }

    if let Some(agent) = &cmd.metadata.agent {
        if !agent.is_empty() {
            sections.push(format!("**Agent**: {}\n", agent));
        }
    }

    sections.push(format!("**Scope**: {}\n", cmd.scope));
    sections.push("---\n".to_string());

    let content = cmd.content.as_deref().unwrap_or_default();
    let resolved = resolve_arguments(content, args);
    sections.push(resolved.trim().to_string());

    if !args.is_empty() && !content.contains("$ARGUMENTS") {
        sections.push("\n\n---\n".to_string());
        sections.push("## User Request\n".to_string());
        sections.push(args.to_string());
    }

    sections.join("\n")
}

/// Execute a slash command and return replacement text
pub fn execute_slash_command(parsed: &ParsedSlashCommand, working_directory: &Path) -> ExecuteResult {
    let Some(command) = find_command(&parsed.command, working_directory) else {
        return ExecuteResult {
            success: false,
            replacement_text: None,
            error: Some(format!(
                "Command \"/{}\" not found. Available commands are in ~/.claude/commands/ or .claude/commands/",
                parsed.command
            )),
        };
    };

    let template = format_command_template(&command, &parsed.args);
    ExecuteResult {
        success: true,
        replacement_text: Some(template),
        error: None,
    }
}

/// List all available commands
pub fn list_available_commands(working_directory: &Path) -> Vec<(String, String, CommandScope)> {
    discover_all_commands(working_directory)
        .into_iter()
        .map(|cmd| (cmd.name, cmd.metadata.description, cmd.scope))
        .collect()
}

// --- index.ts ---

lazy_static! {
    /// Track processed commands to avoid duplicate expansion.
    static ref SESSION_PROCESSED_COMMANDS: RwLock<HashSet<String>> = RwLock::new(HashSet::new());
}

pub struct AutoSlashCommandHook;

impl AutoSlashCommandHook {
    pub fn new() -> Self {
        Self
    }

    /// Clear processed commands cache for a session.
    pub fn clear_session(session_id: &str) {
        let Ok(mut set) = SESSION_PROCESSED_COMMANDS.write() else {
            return;
        };
        set.retain(|k| !k.starts_with(&format!("{}:", session_id)));
    }

    fn should_skip_already_processed(prompt_text: &str) -> bool {
        prompt_text.contains(AUTO_SLASH_COMMAND_TAG_OPEN)
            || prompt_text.contains(AUTO_SLASH_COMMAND_TAG_CLOSE)
    }

    fn get_message_id_from_input(input: &HookInput) -> Option<String> {
        // Common conventions in event payloads.
        let keys = ["messageId", "message_id", "message-id"];
        for key in keys {
            if let Some(v) = input.extra.get(key) {
                if let Some(s) = v.as_str() {
                    if !s.is_empty() {
                        return Some(s.to_string());
                    }
                }
            }
        }
        None
    }

    fn should_dedup(input: &HookInput) -> Option<(String, String)> {
        let session_id = input.session_id.clone()?;
        let message_id = Self::get_message_id_from_input(input)?;
        Some((session_id, message_id))
    }
}

impl Default for AutoSlashCommandHook {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Hook for AutoSlashCommandHook {
    fn name(&self) -> &str {
        HOOK_NAME
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::UserPromptSubmit]
    }

    async fn execute(
        &self,
        _event: HookEvent,
        input: &HookInput,
        context: &HookContext,
    ) -> HookResult {
        let prompt_text = input.get_prompt_text();
        if prompt_text.is_empty() {
            return Ok(HookOutput::pass());
        }

        if Self::should_skip_already_processed(&prompt_text) {
            return Ok(HookOutput::pass());
        }

        let Some(parsed) = detect_slash_command(&prompt_text) else {
            return Ok(HookOutput::pass());
        };

        // Deduplicate within session+message (only when both IDs are available).
        if let Some((session_id, message_id)) = Self::should_dedup(input) {
            let key = format!("{}:{}:{}", session_id, message_id, parsed.command);
            if let Ok(mut set) = SESSION_PROCESSED_COMMANDS.write() {
                if set.contains(&key) {
                    return Ok(HookOutput::pass());
                }
                set.insert(key);
            }
        }

        let working_dir = Path::new(&context.directory);
        let result = execute_slash_command(&parsed, working_dir);

        if result.success {
            if let Some(text) = result.replacement_text {
                let tagged = format!(
                    "{}\n{}\n{}",
                    AUTO_SLASH_COMMAND_TAG_OPEN, text, AUTO_SLASH_COMMAND_TAG_CLOSE
                );
                return Ok(HookOutput::continue_with_message(tagged));
            }
        }

        let err = result.error.unwrap_or_else(|| "Unknown error".to_string());
        let error_message = format!(
            "{}\n[AUTO-SLASH-COMMAND ERROR]\n{}\n\nOriginal input: {}\n{}",
            AUTO_SLASH_COMMAND_TAG_OPEN, err, parsed.raw, AUTO_SLASH_COMMAND_TAG_CLOSE
        );
        Ok(HookOutput::continue_with_message(error_message))
    }

    fn priority(&self) -> i32 {
        90
    }
}

/// Process a prompt for slash command expansion (simple utility function)
pub fn process_slash_command(prompt: &str, working_directory: &Path) -> AutoSlashCommandResult {
    if prompt.contains(AUTO_SLASH_COMMAND_TAG_OPEN) || prompt.contains(AUTO_SLASH_COMMAND_TAG_CLOSE) {
        return AutoSlashCommandResult {
            detected: false,
            parsed_command: None,
            injected_message: None,
        };
    }

    let Some(parsed) = detect_slash_command(prompt) else {
        return AutoSlashCommandResult {
            detected: false,
            parsed_command: None,
            injected_message: None,
        };
    };

    let result = execute_slash_command(&parsed, working_directory);
    if result.success {
        if let Some(text) = result.replacement_text {
            return AutoSlashCommandResult {
                detected: true,
                parsed_command: Some(parsed),
                injected_message: Some(format!(
                    "{}\n{}\n{}",
                    AUTO_SLASH_COMMAND_TAG_OPEN, text, AUTO_SLASH_COMMAND_TAG_CLOSE
                )),
            };
        }
    }

    AutoSlashCommandResult {
        detected: true,
        parsed_command: Some(parsed.clone()),
        injected_message: Some(format!(
            "{}\n[AUTO-SLASH-COMMAND ERROR]\n{}\n\nOriginal input: {}\n{}",
            AUTO_SLASH_COMMAND_TAG_OPEN,
            result
                .error
                .unwrap_or_else(|| "Unknown error".to_string()),
            parsed.raw,
            AUTO_SLASH_COMMAND_TAG_CLOSE
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_remove_code_blocks() {
        let text = "Some text ```code block``` more text";
        let cleaned = remove_code_blocks(text);
        assert_eq!(cleaned, "Some text  more text");
    }

    #[test]
    fn test_parse_slash_command() {
        let parsed = parse_slash_command("/astrape:test hello world").unwrap();
        assert_eq!(parsed.command, "astrape:test");
        assert_eq!(parsed.args, "hello world");
        assert_eq!(parsed.raw, "/astrape:test hello world");

        assert!(parse_slash_command("not a command").is_none());
        assert!(parse_slash_command("/help").is_none()); // pattern is astrape:* only
    }

    #[test]
    fn test_is_excluded_command() {
        assert!(is_excluded_command("astrape:ralph"));
        assert!(is_excluded_command("ASTRAPE:CANCEL-RALPH"));
        assert!(!is_excluded_command("astrape:some-command"));
    }

    #[test]
    fn test_detect_slash_command_ignores_code_blocks() {
        let text = "```\n/astrape:test foo\n```\n";
        assert!(detect_slash_command(text).is_none());
    }

    #[test]
    fn test_parse_frontmatter_basic() {
        let content = "---\ndescription: Hello\nmodel: test\n---\nBody";
        let (data, body) = parse_frontmatter(content);
        assert_eq!(data.get("description").unwrap(), "Hello");
        assert_eq!(data.get("model").unwrap(), "test");
        assert_eq!(body, "Body");
    }

    #[test]
    fn test_execute_slash_command_from_project_commands() {
        let wd = tempdir().unwrap();
        let commands_dir = wd.path().join(".claude").join("commands");
        fs::create_dir_all(&commands_dir).unwrap();

        let cmd_path = commands_dir.join("astrape:test.md");
        fs::write(
            &cmd_path,
            "---\ndescription: Test command\n---\nHello $ARGUMENTS",
        )
        .unwrap();

        let parsed = ParsedSlashCommand {
            command: "astrape:test".to_string(),
            args: "world".to_string(),
            raw: "/astrape:test world".to_string(),
        };

        let result = execute_slash_command(&parsed, wd.path());
        assert!(result.success);
        let text = result.replacement_text.unwrap();
        assert!(text.contains("Hello world"));
        assert!(text.contains("<command-name>/astrape:test</command-name>"));
    }

    #[tokio::test]
    async fn test_hook_integration_injects_tagged_message() {
        let wd = tempdir().unwrap();
        let commands_dir = wd.path().join(".claude").join("commands");
        fs::create_dir_all(&commands_dir).unwrap();
        fs::write(commands_dir.join("astrape:test.md"), "Hi $ARGUMENTS").unwrap();

        let hook = AutoSlashCommandHook::new();
        let mut extra = HashMap::new();
        extra.insert("messageId".to_string(), serde_json::json!("msg-1"));
        let input = HookInput {
            session_id: Some("sess-1".to_string()),
            prompt: Some("/astrape:test there".to_string()),
            message: None,
            parts: None,
            tool_name: None,
            tool_input: None,
            tool_output: None,
            directory: None,
            stop_reason: None,
            user_requested: None,
            extra,
        };

        let context = HookContext::new(
            Some("sess-1".to_string()),
            wd.path().to_string_lossy().to_string(),
        );
        let out = hook
            .execute(HookEvent::UserPromptSubmit, &input, &context)
            .await
            .unwrap();

        assert!(out.should_continue);
        let message = out.message.unwrap();
        assert!(message.contains(AUTO_SLASH_COMMAND_TAG_OPEN));
        assert!(message.contains("Hi there"));
        assert!(message.contains(AUTO_SLASH_COMMAND_TAG_CLOSE));

        // Dedup: same session+message should not emit again.
        let out2 = hook
            .execute(HookEvent::UserPromptSubmit, &input, &context)
            .await
            .unwrap();
        assert!(out2.message.is_none());
    }
}
