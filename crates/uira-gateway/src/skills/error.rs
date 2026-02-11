use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SkillError {
    #[error("Failed to parse SKILL.md: {0}")]
    ParseError(String),

    #[error("YAML frontmatter error: {0}")]
    YamlError(#[from] serde_yaml_ng::Error),

    #[error("IO error reading {path}: {source}")]
    IoError {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Skill not found: {0}")]
    NotFound(String),
}
