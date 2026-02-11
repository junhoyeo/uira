use std::path::PathBuf;

use serde::Deserialize;

use crate::error::SkillError;

/// Additional metadata for a skill (emoji, requirements, etc.).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct SkillMeta {
    pub emoji: Option<String>,
    pub requirements: Option<Vec<String>>,
}

/// Core metadata parsed from the YAML frontmatter of a SKILL.md file.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct SkillMetadata {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub metadata: Option<SkillMeta>,
}

/// A fully loaded skill with metadata, markdown content, and source path.
#[derive(Debug, Clone)]
pub struct Skill {
    pub metadata: SkillMetadata,
    pub content: String,
    pub source_path: PathBuf,
}

/// Parse a SKILL.md file's content into metadata and markdown body.
///
/// The expected format is:
/// ```text
/// ---
/// name: my-skill
/// description: A useful skill
/// ---
/// # Markdown body here
/// ```
pub fn parse_skill(content: &str) -> Result<(SkillMetadata, String), SkillError> {
    let trimmed = content.trim_start();

    // Must start with `---`
    if !trimmed.starts_with("---") {
        return Err(SkillError::ParseError(
            "Missing YAML frontmatter: file must start with '---'".to_string(),
        ));
    }

    // Find the closing `---` delimiter (skip the first line)
    let after_first = &trimmed[3..];
    let after_first = after_first.trim_start_matches(['\r', '\n']);

    let closing_pos = after_first.find("\n---");
    let closing_pos = match closing_pos {
        Some(pos) => pos,
        None => {
            // Also check for `---` at the very end without trailing newline
            if after_first.ends_with("\n---") || after_first.trim_end() == "---" {
                return Err(SkillError::ParseError(
                    "YAML frontmatter has no content".to_string(),
                ));
            }
            return Err(SkillError::ParseError(
                "Missing closing '---' delimiter for YAML frontmatter".to_string(),
            ));
        }
    };

    let yaml_str = &after_first[..closing_pos];
    let body_start = closing_pos + 4; // skip `\n---`
    let body = if body_start < after_first.len() {
        after_first[body_start..].trim().to_string()
    } else {
        String::new()
    };

    let metadata: SkillMetadata = serde_yaml_ng::from_str(yaml_str)?;

    Ok((metadata, body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_skill() {
        let content = r#"---
name: coding-agent
description: Run coding agents via background process.
metadata:
  emoji: "🧩"
  requirements:
    - claude
    - codex
---

# Coding Agent

Use **bash** for all coding agent work.
"#;
        let (meta, body) = parse_skill(content).unwrap();
        assert_eq!(meta.name, "coding-agent");
        assert_eq!(
            meta.description,
            "Run coding agents via background process."
        );
        let skill_meta = meta.metadata.unwrap();
        assert_eq!(skill_meta.emoji, Some("🧩".to_string()));
        assert_eq!(
            skill_meta.requirements,
            Some(vec!["claude".to_string(), "codex".to_string()])
        );
        assert!(body.contains("# Coding Agent"));
        assert!(body.contains("Use **bash** for all coding agent work."));
    }

    #[test]
    fn test_parse_minimal_skill() {
        let content = r#"---
name: minimal
description: A minimal skill
---
Body content here.
"#;
        let (meta, body) = parse_skill(content).unwrap();
        assert_eq!(meta.name, "minimal");
        assert_eq!(meta.description, "A minimal skill");
        assert!(meta.metadata.is_none());
        assert_eq!(body, "Body content here.");
    }

    #[test]
    fn test_parse_missing_frontmatter() {
        let content = "# Just markdown, no frontmatter\n\nSome text.";
        let result = parse_skill(content);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Missing YAML frontmatter"));
    }

    #[test]
    fn test_parse_empty_body() {
        let content = "---\nname: empty-body\ndescription: No body\n---\n";
        let (meta, body) = parse_skill(content).unwrap();
        assert_eq!(meta.name, "empty-body");
        assert!(body.is_empty());
    }

    #[test]
    fn test_parse_malformed_yaml() {
        let content = "---\nname: [invalid yaml\n  : broken\n---\nBody";
        let result = parse_skill(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_missing_closing_delimiter() {
        let content = "---\nname: broken\ndescription: No closing delimiter\n";
        let result = parse_skill(content);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Missing closing '---'"));
    }
}
