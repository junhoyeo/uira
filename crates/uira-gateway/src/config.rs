use serde::{Deserialize, Serialize};

/// Configuration for creating a new gateway session
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionConfig {
    /// Optional model override for this session
    #[serde(default)]
    pub model: Option<String>,

    /// Optional provider override
    #[serde(default)]
    pub provider: Option<String>,

    /// Skills to activate for this session
    #[serde(default)]
    pub skills: Vec<String>,

    /// Pre-resolved skill context injection string (XML blocks from SKILL.md files)
    #[serde(default)]
    pub skill_context: Option<String>,

    /// Working directory for this session
    #[serde(default)]
    pub working_directory: Option<String>,
}

impl SessionConfig {
    /// Strip fields that should not be set by external (WebSocket) clients.
    /// Only internally-created sessions (e.g., channel bridge) may set these.
    pub fn sanitize(&mut self) {
        self.working_directory = None;
        self.skill_context = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_strips_unsafe_fields() {
        let mut config = SessionConfig {
            model: Some("gpt-4".to_string()),
            provider: Some("openai".to_string()),
            skills: vec!["skill1".to_string()],
            skill_context: Some("<dangerous/>".to_string()),
            working_directory: Some("/etc/passwd".to_string()),
        };
        config.sanitize();
        assert_eq!(config.model, Some("gpt-4".to_string())); // preserved
        assert_eq!(config.provider, Some("openai".to_string())); // preserved
        assert_eq!(config.skills, vec!["skill1".to_string()]); // preserved
        assert!(config.skill_context.is_none()); // stripped
        assert!(config.working_directory.is_none()); // stripped
    }
}
