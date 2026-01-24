pub mod types;

pub use types::*;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Parse YAML-like frontmatter from markdown file
fn parse_frontmatter(content: &str) -> (HashMap<String, String>, String) {
    let frontmatter_regex =
        regex::Regex::new(r"^---\r?\n([\s\S]*?)\r?\n---\r?\n?([\s\S]*)$").expect("Invalid regex");

    if let Some(captures) = frontmatter_regex.captures(content) {
        let yaml_content = captures.get(1).map(|m| m.as_str()).unwrap_or("");
        let body = captures.get(2).map(|m| m.as_str()).unwrap_or("");

        let mut data = HashMap::new();

        for line in yaml_content.lines() {
            if let Some(colon_index) = line.find(':') {
                let key = line[..colon_index].trim().to_string();
                let mut value = line[colon_index + 1..].trim().to_string();

                // Remove surrounding quotes
                if (value.starts_with('"') && value.ends_with('"'))
                    || (value.starts_with('\'') && value.ends_with('\''))
                {
                    value = value[1..value.len() - 1].to_string();
                }

                data.insert(key, value);
            }
        }

        (data, body.to_string())
    } else {
        (HashMap::new(), content.to_string())
    }
}

/// Load a single skill from a SKILL.md file
fn load_skill_from_file(skill_path: &Path, skill_name: &str) -> Option<BuiltinSkill> {
    let content = fs::read_to_string(skill_path).ok()?;
    let (data, body) = parse_frontmatter(&content);

    Some(BuiltinSkill {
        name: data
            .get("name")
            .cloned()
            .unwrap_or_else(|| skill_name.to_string()),
        description: data.get("description").cloned().unwrap_or_default(),
        template: body.trim().to_string(),
        license: None,
        compatibility: None,
        metadata: None,
        allowed_tools: None,
        agent: data.get("agent").cloned(),
        model: data.get("model").cloned(),
        subtask: None,
        argument_hint: data.get("argument-hint").cloned(),
        mcp_config: None,
    })
}

/// Load all skills from the skills/ directory
fn load_skills_from_directory(skills_dir: &Path) -> Vec<BuiltinSkill> {
    if !skills_dir.exists() {
        return vec![];
    }

    let mut skills = vec![];

    if let Ok(entries) = fs::read_dir(skills_dir) {
        for entry in entries.flatten() {
            if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                let skill_path = entry.path().join("SKILL.md");
                if skill_path.exists() {
                    if let Some(skill_name) = entry.file_name().to_str() {
                        if let Some(skill) = load_skill_from_file(&skill_path, skill_name) {
                            skills.push(skill);
                        }
                    }
                }
            }
        }
    }

    skills
}

/// Get the skills directory path
fn get_skills_dir() -> PathBuf {
    // Try to find skills directory relative to workspace root
    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    // Check common locations
    let candidates = [
        current_dir.join("skills"),
        current_dir.join("../skills"),
        current_dir.join("../../skills"),
        current_dir.join("../../../skills"),
    ];

    for candidate in &candidates {
        if candidate.exists() {
            return candidate.clone();
        }
    }

    // Default to current_dir/skills
    current_dir.join("skills")
}

use std::sync::OnceLock;

static CACHED_SKILLS: OnceLock<Vec<BuiltinSkill>> = OnceLock::new();

/// Get all builtin skills
///
/// Skills are loaded from bundled SKILL.md files in the skills/ directory.
/// Results are cached after first load.
pub fn create_builtin_skills() -> Vec<BuiltinSkill> {
    CACHED_SKILLS
        .get_or_init(|| {
            let skills_dir = get_skills_dir();
            load_skills_from_directory(&skills_dir)
        })
        .clone()
}

/// Get a skill by name
pub fn get_builtin_skill(name: &str) -> Option<BuiltinSkill> {
    let skills = create_builtin_skills();
    skills
        .into_iter()
        .find(|s| s.name.to_lowercase() == name.to_lowercase())
}

/// List all builtin skill names
pub fn list_builtin_skill_names() -> Vec<String> {
    create_builtin_skills()
        .into_iter()
        .map(|s| s.name)
        .collect()
}

/// Clear the skills cache.
///
/// NOTE: This is a no-op because the cache uses `OnceLock` which cannot be cleared
/// in stable Rust. The cache is initialized once and persists for the process lifetime.
/// If cache clearing is required, consider using `RwLock<Option<...>>` instead.
pub fn clear_skills_cache() {
    // OnceLock does not support clearing - this is intentionally a no-op
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_frontmatter() {
        let content = r#"---
name: test-skill
description: A test skill
model: claude-3-opus
---
This is the template content"#;

        let (data, body) = parse_frontmatter(content);
        assert_eq!(data.get("name"), Some(&"test-skill".to_string()));
        assert_eq!(data.get("description"), Some(&"A test skill".to_string()));
        assert_eq!(data.get("model"), Some(&"claude-3-opus".to_string()));
        assert_eq!(body.trim(), "This is the template content");
    }

    #[test]
    fn test_parse_frontmatter_no_frontmatter() {
        let content = "Just plain content";
        let (data, body) = parse_frontmatter(content);
        assert!(data.is_empty());
        assert_eq!(body, content);
    }

    #[test]
    fn test_list_builtin_skill_names_does_not_panic() {
        // Verify the function returns without panicking
        let names = list_builtin_skill_names();
        // All returned names should be non-empty strings
        for name in &names {
            assert!(!name.is_empty(), "Skill names should not be empty");
        }
    }
}
