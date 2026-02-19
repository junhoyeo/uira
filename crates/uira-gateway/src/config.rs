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

    /// Primary agent/orchestrator personality for this session.
    ///
    /// Overrides `gateway.default_agent`. Valid values:
    /// - "balanced": Delegates heavily, asks before acting (default)
    /// - "autonomous": Deep worker, completes tasks without asking
    /// - "orchestrator": Conductor, never writes code, only delegates
    #[serde(default)]
    pub agent: Option<String>,

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
    ///
    /// Also warns if the `agent` field contains an unrecognized personality name.
    /// Invalid values are preserved (session_manager handles the fallback to
    /// Balanced), but the warning helps operators catch typos early.
    pub fn sanitize(&mut self) {
        self.working_directory = None;
        self.skill_context = None;

        if let Some(agent) = &self.agent {
            if uira_orchestration::OrchestratorPersonality::parse(agent).is_none() {
                let valid: Vec<&str> = uira_orchestration::OrchestratorPersonality::all()
                    .iter()
                    .map(|p| p.as_str())
                    .collect();
                tracing::warn!(
                    agent = %agent,
                    valid = ?valid,
                    "SessionConfig contains unrecognized agent personality; \
                     will fall back to 'balanced'"
                );
            }
        }
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
            agent: Some("autonomous".to_string()),
            skills: vec!["skill1".to_string()],
            skill_context: Some("<dangerous/>".to_string()),
            working_directory: Some("/etc/passwd".to_string()),
        };
        config.sanitize();
        assert_eq!(config.model, Some("gpt-4".to_string())); // preserved
        assert_eq!(config.provider, Some("openai".to_string())); // preserved
        assert_eq!(config.agent, Some("autonomous".to_string())); // preserved — user choice
        assert_eq!(config.skills, vec!["skill1".to_string()]); // preserved
        assert!(config.skill_context.is_none()); // stripped
        assert!(config.working_directory.is_none()); // stripped
    }

    #[test]
    fn test_sanitize_preserves_invalid_agent_for_fallback() {
        // Invalid agent names are preserved by sanitize() — session_manager
        // handles the fallback to Balanced. sanitize() only emits a warning.
        let mut config = SessionConfig {
            agent: Some("malicious-agent".to_string()),
            ..SessionConfig::default()
        };
        config.sanitize();
        // The invalid value is still there (session_manager will fall back)
        assert_eq!(config.agent, Some("malicious-agent".to_string()));
    }

    #[test]
    fn test_sanitize_accepts_valid_agent_names() {
        for valid_name in [
            "balanced",
            "autonomous",
            "orchestrator",
            "sisyphus",
            "atlas",
        ] {
            let mut config = SessionConfig {
                agent: Some(valid_name.to_string()),
                ..SessionConfig::default()
            };
            config.sanitize();
            assert_eq!(config.agent, Some(valid_name.to_string()));
        }
    }

    #[test]
    fn test_sanitize_handles_no_agent() {
        let mut config = SessionConfig::default();
        config.sanitize();
        assert!(config.agent.is_none());
    }
}
