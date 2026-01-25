use std::fs;
use std::path::PathBuf;

/// How prompts should be resolved.
#[derive(Debug, Clone)]
pub enum PromptSource {
    /// Load prompts from the filesystem at runtime (useful during development).
    FileSystem { root_dir: PathBuf },
    /// Load prompts from embedded string content.
    ///
    /// This is primarily intended for `include_str!()` based embedding.
    Embedded { name: String, content: &'static str },

    /// Load prompts from an embedded name->content mapping.
    ///
    /// This is the recommended form for embedding multiple prompts with `include_str!()`.
    EmbeddedMap {
        prompts: &'static [(&'static str, &'static str)],
    },
}

/// Loads agent prompts and strips YAML frontmatter (if present).
#[derive(Debug, Clone)]
pub struct PromptLoader {
    source: PromptSource,
}

impl PromptLoader {
    pub fn from_fs(root_dir: impl Into<PathBuf>) -> Self {
        Self {
            source: PromptSource::FileSystem {
                root_dir: root_dir.into(),
            },
        }
    }

    pub fn from_embedded(name: impl Into<String>, content: &'static str) -> Self {
        Self {
            source: PromptSource::Embedded {
                name: name.into(),
                content,
            },
        }
    }

    pub fn from_embedded_map(prompts: &'static [(&'static str, &'static str)]) -> Self {
        Self {
            source: PromptSource::EmbeddedMap { prompts },
        }
    }

    pub fn load(&self, agent_name: &str) -> String {
        match &self.source {
            PromptSource::FileSystem { root_dir } => {
                let path = root_dir.join(format!("{agent_name}.md"));
                match fs::read_to_string(&path) {
                    Ok(content) => strip_yaml_frontmatter(&content),
                    Err(_) => fallback_prompt(agent_name),
                }
            }
            PromptSource::Embedded { name, content } => {
                if name == agent_name {
                    strip_yaml_frontmatter(content)
                } else {
                    // For embedded prompts we need a fixed content at compile time.
                    // This is a deliberate failure mode to avoid silently returning
                    // the wrong prompt.
                    fallback_prompt(agent_name)
                }
            }
            PromptSource::EmbeddedMap { prompts } => prompts
                .iter()
                .find(|(name, _)| *name == agent_name)
                .map(|(_, content)| strip_yaml_frontmatter(content))
                .unwrap_or_else(|| fallback_prompt(agent_name)),
        }
    }
}

/// Convenience macro for embedding prompts with `include_str!()`.
///
/// Example:
/// ```ignore
/// use astrape_agents::{PromptLoader, include_agent_prompts};
///
/// static PROMPTS: &[(&str, &str)] = include_agent_prompts!(
///   "architect" => "../../packages/astrape/claude-plugin/agents/architect.md",
///   "explore" => "../../packages/astrape/claude-plugin/agents/explore.md",
/// );
///
/// let loader = PromptLoader::from_embedded_map(PROMPTS);
/// ```
#[macro_export]
macro_rules! include_agent_prompts {
    ($($name:literal => $path:literal),+ $(,)?) => {
        &[
            $(($name, include_str!($path))),+
        ]
    };
}

pub fn default_agents_dir() -> PathBuf {
    // crates/astrape-agents -> packages/astrape/claude-plugin/agents
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("packages/astrape/claude-plugin/agents")
}

pub fn strip_yaml_frontmatter(content: &str) -> String {
    // Match the same semantics as the TS implementation:
    // If content starts with `---`, remove the first YAML frontmatter block.
    let s = content.trim();
    if !s.starts_with("---") {
        return s.to_string();
    }

    // Find second `---` delimiter on its own line.
    // We accept both `---\n` and `---\r\n` line endings.
    let mut lines = s.lines();

    // Consume the opening delimiter.
    let first = lines.next().unwrap_or_default();
    if first.trim() != "---" {
        return s.to_string();
    }

    // Skip until closing delimiter.
    for line in &mut lines {
        if line.trim() == "---" {
            let rest: String = lines.collect::<Vec<_>>().join("\n");
            return rest.trim().to_string();
        }
    }

    // Unclosed frontmatter; return as-is.
    s.to_string()
}

pub fn fallback_prompt(agent_name: &str) -> String {
    format!(
        "Agent: {agent_name}\n\nPrompt file not found. Please ensure packages/astrape/claude-plugin/agents/{agent_name}.md exists.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_frontmatter_when_present() {
        let md = "---\nname: test\n---\n\nHello\nWorld\n";
        assert_eq!(strip_yaml_frontmatter(md), "Hello\nWorld");
    }

    #[test]
    fn strip_frontmatter_is_noop_without_frontmatter() {
        let md = "Hello\nWorld\n";
        assert_eq!(strip_yaml_frontmatter(md), "Hello\nWorld");
    }

    #[test]
    fn strip_frontmatter_is_noop_when_unclosed() {
        let md = "---\nname: test\nHello\n";
        assert_eq!(strip_yaml_frontmatter(md), "---\nname: test\nHello");
    }

    #[test]
    fn fs_loader_falls_back_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let loader = PromptLoader::from_fs(tmp.path());
        let prompt = loader.load("missing");
        assert!(prompt.contains("Prompt file not found"));
    }
}
