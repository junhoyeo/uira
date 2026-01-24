use anyhow::{Context, Result};
use astrape_core::HookEvent;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClaudeSettings {
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hooks: Option<ClaudeHooksConfig>,

    #[serde(flatten)]
    pub other: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClaudeHooksConfig {
    #[serde(rename = "UserPromptSubmit", skip_serializing_if = "Option::is_none")]
    pub user_prompt_submit: Option<Vec<HookEntry>>,

    #[serde(rename = "Stop", skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<HookEntry>>,

    #[serde(rename = "PreToolUse", skip_serializing_if = "Option::is_none")]
    pub pre_tool_use: Option<Vec<HookEntry>>,

    #[serde(rename = "PostToolUse", skip_serializing_if = "Option::is_none")]
    pub post_tool_use: Option<Vec<HookEntry>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matcher: Option<String>,

    pub hooks: Vec<HookCommand>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookCommand {
    #[serde(rename = "type")]
    pub command_type: String,
    pub command: String,
}

fn get_settings_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not determine home directory")?;
    Ok(home.join(".claude").join("settings.json"))
}

pub fn read_settings() -> Result<ClaudeSettings> {
    let path = get_settings_path()?;
    if !path.exists() {
        return Ok(ClaudeSettings::default());
    }
    let content = fs::read_to_string(&path)?;
    let settings: ClaudeSettings = serde_json::from_str(&content)?;
    Ok(settings)
}

pub fn write_settings(settings: &ClaudeSettings) -> Result<()> {
    let path = get_settings_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(settings)?;
    fs::write(&path, content)?;
    Ok(())
}

pub fn update_claude_settings(events: &[HookEvent]) -> Result<()> {
    let mut settings = read_settings()?;

    let mut hooks_config = settings.hooks.unwrap_or_default();

    if events.contains(&HookEvent::UserPromptSubmit) {
        let hook_entry = HookEntry {
            matcher: None,
            hooks: vec![HookCommand {
                command_type: "command".to_string(),
                command: "bash $HOME/.claude/hooks/keyword-detector.sh".to_string(),
            }],
        };

        let current = hooks_config.user_prompt_submit.get_or_insert_with(Vec::new);
        if !current.iter().any(|h| {
            h.hooks
                .iter()
                .any(|c| c.command.contains("keyword-detector"))
        }) {
            current.push(hook_entry);
        }
    }

    if events.contains(&HookEvent::Stop) {
        let hook_entry = HookEntry {
            matcher: None,
            hooks: vec![HookCommand {
                command_type: "command".to_string(),
                command: "bash $HOME/.claude/hooks/stop-continuation.sh".to_string(),
            }],
        };

        let current = hooks_config.stop.get_or_insert_with(Vec::new);
        if !current.iter().any(|h| {
            h.hooks
                .iter()
                .any(|c| c.command.contains("stop-continuation"))
        }) {
            current.push(hook_entry);
        }
    }

    settings.hooks = Some(hooks_config);
    write_settings(&settings)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_settings() {
        let settings = ClaudeSettings {
            schema: Some("https://json.schemastore.org/claude-code-settings.json".to_string()),
            hooks: Some(ClaudeHooksConfig {
                user_prompt_submit: Some(vec![HookEntry {
                    matcher: None,
                    hooks: vec![HookCommand {
                        command_type: "command".to_string(),
                        command: "bash test.sh".to_string(),
                    }],
                }]),
                ..Default::default()
            }),
            other: HashMap::new(),
        };

        let json = serde_json::to_string_pretty(&settings).unwrap();
        assert!(json.contains("UserPromptSubmit"));
        assert!(json.contains("bash test.sh"));
    }
}
