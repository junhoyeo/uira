use std::path::{Path, PathBuf};

use super::error::SkillError;
use super::parser::{parse_skill, SkillMetadata};

/// Information about a discovered skill before full content loading.
#[derive(Debug, Clone)]
pub struct SkillInfo {
    pub name: String,
    pub path: PathBuf,
    pub metadata: SkillMetadata,
}

/// Expand `~` at the start of a path to the user's home directory.
pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    } else if path == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }
    PathBuf::from(path)
}

/// Discover skills from a list of directory paths.
///
/// Each path is checked for:
/// 1. A `SKILL.md` directly inside (flat layout)
/// 2. Subdirectories containing `SKILL.md` (nested layout)
///
/// Non-existent paths are silently skipped. Results are sorted by name.
pub fn discover_skills(paths: &[impl AsRef<Path>]) -> Result<Vec<SkillInfo>, SkillError> {
    let mut skills = Vec::new();

    for path in paths {
        let expanded = expand_tilde(&path.as_ref().to_string_lossy());

        if !expanded.exists() || !expanded.is_dir() {
            continue;
        }

        // Check flat layout: path itself contains SKILL.md
        let flat_skill = expanded.join("SKILL.md");
        if flat_skill.is_file() {
            if let Some(info) = try_load_skill_info(&flat_skill, &expanded)? {
                skills.push(info);
            }
        }

        // Check nested layout: subdirectories containing SKILL.md
        let entries = std::fs::read_dir(&expanded).map_err(|e| SkillError::IoError {
            path: expanded.clone(),
            source: e,
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| SkillError::IoError {
                path: expanded.clone(),
                source: e,
            })?;

            let entry_path = entry.path();
            if entry_path.is_dir() {
                let nested_skill = entry_path.join("SKILL.md");
                if nested_skill.is_file() {
                    if let Some(info) = try_load_skill_info(&nested_skill, &entry_path)? {
                        skills.push(info);
                    }
                }
            }
        }
    }

    skills.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(skills)
}

fn try_load_skill_info(
    skill_path: &Path,
    dir_path: &Path,
) -> Result<Option<SkillInfo>, SkillError> {
    let content = std::fs::read_to_string(skill_path).map_err(|e| SkillError::IoError {
        path: skill_path.to_path_buf(),
        source: e,
    })?;

    let (metadata, _body) = parse_skill(&content)?;

    let name = dir_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| metadata.name.clone());

    Ok(Some(SkillInfo {
        name,
        path: skill_path.to_path_buf(),
        metadata,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_skill_md(name: &str, desc: &str) -> String {
        format!("---\nname: {name}\ndescription: {desc}\n---\n\n# {name}\n\nSkill body.\n")
    }

    #[test]
    fn test_discover_nested_skills() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_a = tmp.path().join("alpha");
        let skill_b = tmp.path().join("beta");
        std::fs::create_dir_all(&skill_a).unwrap();
        std::fs::create_dir_all(&skill_b).unwrap();

        std::fs::write(
            skill_a.join("SKILL.md"),
            make_skill_md("alpha", "Alpha skill"),
        )
        .unwrap();
        std::fs::write(
            skill_b.join("SKILL.md"),
            make_skill_md("beta", "Beta skill"),
        )
        .unwrap();

        let skills = discover_skills(&[tmp.path().to_str().unwrap()]).unwrap();
        assert_eq!(skills.len(), 2);
        assert_eq!(skills[0].name, "alpha");
        assert_eq!(skills[1].name, "beta");
        assert_eq!(skills[0].metadata.description, "Alpha skill");
    }

    #[test]
    fn test_discover_flat_skill() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("SKILL.md"),
            make_skill_md("flat-skill", "A flat skill"),
        )
        .unwrap();

        let skills = discover_skills(&[tmp.path().to_str().unwrap()]).unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].metadata.name, "flat-skill");
    }

    #[test]
    fn test_discover_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let skills = discover_skills(&[tmp.path().to_str().unwrap()]).unwrap();
        assert!(skills.is_empty());
    }

    #[test]
    fn test_discover_nonexistent_path() {
        let skills = discover_skills(&["/nonexistent/path/that/does/not/exist"]).unwrap();
        assert!(skills.is_empty());
    }

    #[test]
    fn test_expand_tilde() {
        let expanded = expand_tilde("~/some/path");
        if let Some(home) = dirs::home_dir() {
            assert_eq!(expanded, home.join("some/path"));
        }

        let plain = expand_tilde("/absolute/path");
        assert_eq!(plain, PathBuf::from("/absolute/path"));

        let relative = expand_tilde("relative/path");
        assert_eq!(relative, PathBuf::from("relative/path"));
    }

    #[test]
    fn test_discover_sorted_by_name() {
        let tmp = tempfile::tempdir().unwrap();
        for name in &["zebra", "apple", "mango"] {
            let dir = tmp.path().join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("SKILL.md"),
                make_skill_md(name, &format!("{name} skill")),
            )
            .unwrap();
        }

        let skills = discover_skills(&[tmp.path().to_str().unwrap()]).unwrap();
        let names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["apple", "mango", "zebra"]);
    }
}
