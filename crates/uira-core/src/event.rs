use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum HookEvent {
    PreToolUse,
    PostToolUse,
    UserPromptSubmit,
    Stop,
    PreCompact,
    PreCheck,
    PostCheck,
    PreAi,
    PostAi,
    PreFix,
    PostFix,
}

impl HookEvent {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PreToolUse => "PreToolUse",
            Self::PostToolUse => "PostToolUse",
            Self::UserPromptSubmit => "UserPromptSubmit",
            Self::Stop => "Stop",
            Self::PreCompact => "PreCompact",
            Self::PreCheck => "pre-check",
            Self::PostCheck => "post-check",
            Self::PreAi => "pre-ai",
            Self::PostAi => "post-ai",
            Self::PreFix => "pre-fix",
            Self::PostFix => "post-fix",
        }
    }

    pub fn is_claude_event(&self) -> bool {
        matches!(
            self,
            Self::PreToolUse
                | Self::PostToolUse
                | Self::UserPromptSubmit
                | Self::Stop
                | Self::PreCompact
        )
    }

    pub fn is_uira_event(&self) -> bool {
        matches!(
            self,
            Self::PreCheck
                | Self::PostCheck
                | Self::PreAi
                | Self::PostAi
                | Self::PreFix
                | Self::PostFix
        )
    }
}

impl std::fmt::Display for HookEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for HookEvent {
    type Err = HookEventParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pretooluse" | "pre-tool-use" | "pre_tool_use" => Ok(Self::PreToolUse),
            "posttooluse" | "post-tool-use" | "post_tool_use" => Ok(Self::PostToolUse),
            "userpromptsubmit" | "user-prompt-submit" | "user_prompt_submit" => {
                Ok(Self::UserPromptSubmit)
            }
            "stop" => Ok(Self::Stop),
            "precompact" | "pre-compact" | "pre_compact" => Ok(Self::PreCompact),
            "precheck" | "pre-check" | "pre_check" => Ok(Self::PreCheck),
            "postcheck" | "post-check" | "post_check" => Ok(Self::PostCheck),
            "preai" | "pre-ai" | "pre_ai" => Ok(Self::PreAi),
            "postai" | "post-ai" | "post_ai" => Ok(Self::PostAi),
            "prefix" | "pre-fix" | "pre_fix" => Ok(Self::PreFix),
            "postfix" | "post-fix" | "post_fix" => Ok(Self::PostFix),
            _ => Err(HookEventParseError(s.to_string())),
        }
    }
}

#[derive(Debug, Clone)]
pub struct HookEventParseError(pub String);

impl std::fmt::Display for HookEventParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown hook event: {}", self.0)
    }
}

impl std::error::Error for HookEventParseError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_as_str() {
        assert_eq!(HookEvent::PreToolUse.as_str(), "PreToolUse");
        assert_eq!(HookEvent::PreCheck.as_str(), "pre-check");
    }

    #[test]
    fn test_event_parse() {
        assert_eq!(
            "PreToolUse".parse::<HookEvent>().unwrap(),
            HookEvent::PreToolUse
        );
        assert_eq!(
            "pre-check".parse::<HookEvent>().unwrap(),
            HookEvent::PreCheck
        );
        assert!("invalid".parse::<HookEvent>().is_err());
    }

    #[test]
    fn test_is_claude_event() {
        assert!(HookEvent::PreToolUse.is_claude_event());
        assert!(!HookEvent::PreCheck.is_claude_event());
    }
}
