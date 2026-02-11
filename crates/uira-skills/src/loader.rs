use std::path::Path;

use crate::discovery::{discover_skills, SkillInfo};
use crate::error::SkillError;
use crate::parser::{parse_skill, Skill};

/// Loads and manages discovered skills.
pub struct SkillLoader {
    pub discovered: Vec<SkillInfo>,
}

impl SkillLoader {
    pub fn new(paths: &[impl AsRef<Path>]) -> Result<Self, SkillError> {
        let discovered = discover_skills(paths)?;
        Ok(Self { discovered })
    }

    /// Load full skill content for each name in `active`.
    /// Returns an error if any active skill name is not found.
    pub fn load_active_skills(&self, active: &[String]) -> Result<Vec<Skill>, SkillError> {
        let mut skills = Vec::new();

        for name in active {
            let info = self
                .discovered
                .iter()
                .find(|s| s.name == *name)
                .ok_or_else(|| SkillError::NotFound(name.clone()))?;

            let content = std::fs::read_to_string(&info.path).map_err(|e| SkillError::IoError {
                path: info.path.clone(),
                source: e,
            })?;

            let (metadata, body) = parse_skill(&content)?;

            skills.push(Skill {
                metadata,
                content: body,
                source_path: info.path.clone(),
            });
        }

        Ok(skills)
    }
}

/// Format loaded skills as XML-tagged blocks for context injection.
///
/// Each skill becomes:
/// ```text
/// <skill name="my-skill">
/// markdown content
/// </skill>
/// ```
pub fn get_context_injection(skills: &[Skill]) -> String {
    if skills.is_empty() {
        return String::new();
    }

    skills
        .iter()
        .map(|skill| {
            format!(
                "<skill name=\"{}\">\n{}\n</skill>",
                skill.metadata.name, skill.content
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::parser::{SkillMeta, SkillMetadata};

    fn make_skill_md(name: &str, desc: &str) -> String {
        format!(
            "---\nname: {name}\ndescription: {desc}\n---\n\n# {name}\n\nSkill body for {name}.\n"
        )
    }

    #[test]
    fn test_context_injection_format() {
        let skills = vec![
            Skill {
                metadata: SkillMetadata {
                    name: "alpha".to_string(),
                    description: "Alpha skill".to_string(),
                    metadata: None,
                },
                content: "# Alpha\n\nAlpha content.".to_string(),
                source_path: PathBuf::from("/fake/alpha/SKILL.md"),
            },
            Skill {
                metadata: SkillMetadata {
                    name: "beta".to_string(),
                    description: "Beta skill".to_string(),
                    metadata: Some(SkillMeta {
                        emoji: Some("🔥".to_string()),
                        requirements: None,
                    }),
                },
                content: "# Beta\n\nBeta content.".to_string(),
                source_path: PathBuf::from("/fake/beta/SKILL.md"),
            },
        ];

        let result = get_context_injection(&skills);
        assert!(result.contains("<skill name=\"alpha\">"));
        assert!(result.contains("# Alpha\n\nAlpha content."));
        assert!(result.contains("</skill>"));
        assert!(result.contains("<skill name=\"beta\">"));
        assert!(result.contains("# Beta\n\nBeta content."));

        let skill_tags: Vec<&str> = result.matches("<skill name=").collect();
        assert_eq!(skill_tags.len(), 2);
    }

    #[test]
    fn test_context_injection_empty() {
        let result = get_context_injection(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_load_active_skills() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_a = tmp.path().join("alpha");
        let skill_b = tmp.path().join("beta");
        std::fs::create_dir_all(&skill_a).unwrap();
        std::fs::create_dir_all(&skill_b).unwrap();

        std::fs::write(skill_a.join("SKILL.md"), make_skill_md("alpha", "Alpha")).unwrap();
        std::fs::write(skill_b.join("SKILL.md"), make_skill_md("beta", "Beta")).unwrap();

        let loader = SkillLoader::new(&[tmp.path().to_str().unwrap()]).unwrap();
        assert_eq!(loader.discovered.len(), 2);

        let active = loader.load_active_skills(&["alpha".to_string()]).unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].metadata.name, "alpha");
        assert!(active[0].content.contains("Skill body for alpha."));
    }

    #[test]
    fn test_load_missing_skill() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("only-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            make_skill_md("only-skill", "Only one"),
        )
        .unwrap();

        let loader = SkillLoader::new(&[tmp.path().to_str().unwrap()]).unwrap();
        let result = loader.load_active_skills(&["nonexistent".to_string()]);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Skill not found: nonexistent"));
    }
}
