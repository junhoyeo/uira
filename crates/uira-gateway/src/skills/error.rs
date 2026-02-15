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

    #[error("Skill file too large: {path} ({size} bytes exceeds maximum {max_size} bytes)")]
    FileTooLarge {
        path: PathBuf,
        size: u64,
        max_size: u64,
    },

    #[error("Skill not found: {0}")]
    NotFound(String),
}
