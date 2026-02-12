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
