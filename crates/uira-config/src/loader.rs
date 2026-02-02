use crate::schema::UiraConfig;
use anyhow::{anyhow, Context, Result};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFormat {
    Jsonc,
    Json,
    Yaml,
}

impl ConfigFormat {
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?;

        match ext {
            "jsonc" => Some(Self::Jsonc),
            "json" => Some(Self::Json),
            "yml" | "yaml" => Some(Self::Yaml),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub config: UiraConfig,
    pub path: PathBuf,
    pub format: ConfigFormat,
}

pub fn load_config(config_path: Option<&Path>) -> Result<UiraConfig> {
    resolve_config(config_path).map(|r| r.config)
}

pub fn resolve_config(config_path: Option<&Path>) -> Result<ResolvedConfig> {
    let path = config_path
        .map(|p| p.to_path_buf())
        .or_else(find_config_file)
        .ok_or_else(|| anyhow!("No configuration file found"))?;

    load_config_from_file(&path)
}

pub fn load_config_from_file(path: &Path) -> Result<ResolvedConfig> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file: {}", path.display()))?;

    let format = ConfigFormat::from_path(path)
        .ok_or_else(|| anyhow!("Unknown config format for: {}", path.display()))?;

    let config = parse_config_content(&content, format)?;

    Ok(ResolvedConfig {
        config: expand_env_vars(config),
        path: path.to_path_buf(),
        format,
    })
}

fn parse_config_content(content: &str, format: ConfigFormat) -> Result<UiraConfig> {
    match format {
        ConfigFormat::Jsonc => json5::from_str(content).context("Failed to parse JSONC"),
        ConfigFormat::Json => serde_json::from_str(content).context("Failed to parse JSON"),
        ConfigFormat::Yaml => serde_yaml_ng::from_str(content).context("Failed to parse YAML"),
    }
}

const CONFIG_CANDIDATES: &[&str] = &[
    "uira.jsonc",
    "uira.json",
    "uira.yml",
    "uira.yaml",
    ".uira.jsonc",
    ".uira.json",
    ".uira.yml",
    ".uira.yaml",
];

fn find_config_file() -> Option<PathBuf> {
    for candidate in CONFIG_CANDIDATES {
        let path = PathBuf::from(candidate);
        if path.exists() {
            return Some(path);
        }
    }

    if let Ok(home) = env::var("HOME") {
        for candidate in CONFIG_CANDIDATES {
            let path = PathBuf::from(&home)
                .join(".config")
                .join("uira")
                .join(candidate);
            if path.exists() {
                return Some(path);
            }
        }
    }

    None
}

pub fn find_all_config_files() -> Vec<PathBuf> {
    let mut found = Vec::new();

    for candidate in CONFIG_CANDIDATES {
        let path = PathBuf::from(candidate);
        if path.exists() {
            found.push(path);
        }
    }

    if let Ok(home) = env::var("HOME") {
        for candidate in CONFIG_CANDIDATES {
            let path = PathBuf::from(&home)
                .join(".config")
                .join("uira")
                .join(candidate);
            if path.exists() {
                found.push(path);
            }
        }
    }

    found
}

fn expand_env_vars(config: UiraConfig) -> UiraConfig {
    UiraConfig {
        typos: expand_typos_settings(config.typos),
        diagnostics: expand_diagnostics_settings(config.diagnostics),
        comments: expand_comments_settings(config.comments),
        opencode: expand_opencode_settings(config.opencode),
        mcp: expand_mcp_settings(config.mcp),
        agents: config.agents,
        hooks: config.hooks,
        ai_hooks: config.ai_hooks,
        goals: expand_goals_settings(config.goals),
    }
}

fn expand_opencode_settings(
    mut opencode: crate::schema::OpencodeSettings,
) -> crate::schema::OpencodeSettings {
    opencode.host = expand_env_string(&opencode.host);
    opencode
}

fn expand_typos_settings(mut typos: crate::schema::TyposSettings) -> crate::schema::TyposSettings {
    typos.ai.model = expand_env_string(&typos.ai.model);
    typos
}

fn expand_diagnostics_settings(
    mut diagnostics: crate::schema::DiagnosticsSettings,
) -> crate::schema::DiagnosticsSettings {
    diagnostics.ai.model = expand_env_string(&diagnostics.ai.model);
    diagnostics
}

fn expand_comments_settings(
    mut comments: crate::schema::CommentsSettings,
) -> crate::schema::CommentsSettings {
    comments.ai.model = expand_env_string(&comments.ai.model);
    comments
}

fn expand_goals_settings(mut goals: crate::schema::GoalsConfig) -> crate::schema::GoalsConfig {
    for goal in goals.goals.iter_mut() {
        goal.command = expand_env_string(&goal.command);
        if let Some(ws) = &goal.workspace {
            goal.workspace = Some(expand_env_string(ws));
        }
    }
    goals
}

fn expand_mcp_settings(mut mcp: crate::schema::McpSettings) -> crate::schema::McpSettings {
    for server in mcp.servers.values_mut() {
        server.command = expand_env_string(&server.command);
        server.args = server
            .args
            .iter()
            .map(|arg| expand_env_string(arg))
            .collect();
        for value in server.env.values_mut() {
            *value = expand_env_string(value);
        }
    }
    mcp
}
fn expand_env_string(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '$' {
            if chars.peek() == Some(&'{') {
                // ${VAR} syntax
                chars.next(); // consume '{'
                let var_name: String = chars.by_ref().take_while(|&c| c != '}').collect();
                if let Ok(value) = env::var(&var_name) {
                    result.push_str(&value);
                } else {
                    result.push('$');
                    result.push('{');
                    result.push_str(&var_name);
                    result.push('}');
                }
            } else {
                // $VAR syntax - use peek() to avoid consuming the delimiter
                let mut var_name = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_alphanumeric() || c == '_' {
                        var_name.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if !var_name.is_empty() {
                    if let Ok(value) = env::var(&var_name) {
                        result.push_str(&value);
                    } else {
                        result.push('$');
                        result.push_str(&var_name);
                    }
                } else {
                    result.push('$');
                }
            }
        } else {
            result.push(ch);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_expand_env_string_with_braces() {
        env::set_var("TEST_VAR", "test_value");
        let result = expand_env_string("prefix_${TEST_VAR}_suffix");
        assert_eq!(result, "prefix_test_value_suffix");
    }

    #[test]
    fn test_expand_env_string_without_braces() {
        env::set_var("TEST_VAR", "test_value");
        let result = expand_env_string("prefix_$TEST_VAR");
        assert_eq!(result, "prefix_test_value");
    }

    #[test]
    fn test_expand_env_string_preserves_space_after_var() {
        env::set_var("TEST_VAR2", "value");
        let result = expand_env_string("hello $TEST_VAR2 world");
        assert_eq!(result, "hello value world");
    }

    #[test]
    fn test_expand_env_string_missing_var() {
        let result = expand_env_string("prefix_${NONEXISTENT_VAR}_suffix");
        assert_eq!(result, "prefix_${NONEXISTENT_VAR}_suffix");
    }

    #[test]
    fn test_expand_env_string_no_vars() {
        let result = expand_env_string("no_variables_here");
        assert_eq!(result, "no_variables_here");
    }

    #[test]
    fn test_load_config_from_yaml() {
        let yaml_content = r#"
opencode:
  port: 4096

pre-commit:
  parallel: true
  commands:
    - name: fmt
      run: cargo fmt --check
"#;
        let config: UiraConfig = serde_yaml_ng::from_str(yaml_content).unwrap();
        assert_eq!(config.opencode.port, 4096);
    }

    #[test]
    fn test_load_config_from_json() {
        let json_content = r#"{
  "opencode": {
    "port": 8080
  }
}"#;
        let config: UiraConfig = serde_json::from_str(json_content).unwrap();
        assert_eq!(config.opencode.port, 8080);
    }

    #[test]
    fn test_config_format_from_path() {
        assert_eq!(
            ConfigFormat::from_path(Path::new("uira.jsonc")),
            Some(ConfigFormat::Jsonc)
        );
        assert_eq!(
            ConfigFormat::from_path(Path::new("uira.json")),
            Some(ConfigFormat::Json)
        );
        assert_eq!(
            ConfigFormat::from_path(Path::new("uira.yml")),
            Some(ConfigFormat::Yaml)
        );
        assert_eq!(
            ConfigFormat::from_path(Path::new("uira.yaml")),
            Some(ConfigFormat::Yaml)
        );
        assert_eq!(ConfigFormat::from_path(Path::new("uira.txt")), None);
    }

    #[test]
    fn test_json_files_ending_with_c_are_not_jsonc() {
        assert_eq!(
            ConfigFormat::from_path(Path::new("music.json")),
            Some(ConfigFormat::Json)
        );
        assert_eq!(
            ConfigFormat::from_path(Path::new("epic.json")),
            Some(ConfigFormat::Json)
        );
        assert_eq!(
            ConfigFormat::from_path(Path::new("basic.json")),
            Some(ConfigFormat::Json)
        );
    }

    #[test]
    fn test_load_jsonc_with_comments() {
        let jsonc_content = r#"{
  // This is a comment
  "opencode": {
    "port": 9000 // inline comment
  }
  /* block comment */
}"#;
        let config: UiraConfig = json5::from_str(jsonc_content).unwrap();
        assert_eq!(config.opencode.port, 9000);
    }

    #[test]
    fn test_resolve_config_from_jsonc_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("uira.jsonc");

        let content = r#"{
  // Configuration with comments
  "opencode": { "port": 5000 }
}"#;
        fs::write(&path, content).unwrap();

        let resolved = load_config_from_file(&path).unwrap();
        assert_eq!(resolved.format, ConfigFormat::Jsonc);
        assert_eq!(resolved.config.opencode.port, 5000);
    }

    #[test]
    fn test_resolve_config_from_json_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("uira.json");

        let content = r#"{"opencode": {"port": 6000}}"#;
        fs::write(&path, content).unwrap();

        let resolved = load_config_from_file(&path).unwrap();
        assert_eq!(resolved.format, ConfigFormat::Json);
        assert_eq!(resolved.config.opencode.port, 6000);
    }

    #[test]
    fn test_resolve_config_from_yaml_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("uira.yml");

        let content = r#"
opencode:
  port: 7000
"#;
        fs::write(&path, content).unwrap();

        let resolved = load_config_from_file(&path).unwrap();
        assert_eq!(resolved.format, ConfigFormat::Yaml);
        assert_eq!(resolved.config.opencode.port, 7000);
    }

    #[test]
    fn test_config_priority_order_documented() {
        assert_eq!(CONFIG_CANDIDATES[0], "uira.jsonc");
        assert_eq!(CONFIG_CANDIDATES[1], "uira.json");
        assert_eq!(CONFIG_CANDIDATES[2], "uira.yml");
        assert_eq!(CONFIG_CANDIDATES[3], "uira.yaml");
        assert_eq!(CONFIG_CANDIDATES[4], ".uira.jsonc");
        assert_eq!(CONFIG_CANDIDATES[5], ".uira.json");
        assert_eq!(CONFIG_CANDIDATES[6], ".uira.yml");
        assert_eq!(CONFIG_CANDIDATES[7], ".uira.yaml");
    }

    #[test]
    fn test_load_jsonc_takes_priority_over_json() {
        let dir = TempDir::new().unwrap();

        let jsonc_path = dir.path().join("config.jsonc");
        let json_path = dir.path().join("config.json");

        fs::write(&jsonc_path, r#"{"opencode": {"port": 1111}}"#).unwrap();
        fs::write(&json_path, r#"{"opencode": {"port": 2222}}"#).unwrap();

        let jsonc_result = load_config_from_file(&jsonc_path).unwrap();
        let json_result = load_config_from_file(&json_path).unwrap();

        assert_eq!(jsonc_result.format, ConfigFormat::Jsonc);
        assert_eq!(json_result.format, ConfigFormat::Json);
        assert_eq!(jsonc_result.config.opencode.port, 1111);
        assert_eq!(json_result.config.opencode.port, 2222);
    }

    #[test]
    fn test_load_json_takes_priority_over_yml() {
        let dir = TempDir::new().unwrap();

        let json_path = dir.path().join("config.json");
        let yml_path = dir.path().join("config.yml");

        fs::write(&json_path, r#"{"opencode": {"port": 3333}}"#).unwrap();
        fs::write(&yml_path, "opencode:\n  port: 4444").unwrap();

        let json_result = load_config_from_file(&json_path).unwrap();
        let yml_result = load_config_from_file(&yml_path).unwrap();

        assert_eq!(json_result.format, ConfigFormat::Json);
        assert_eq!(yml_result.format, ConfigFormat::Yaml);
        assert_eq!(json_result.config.opencode.port, 3333);
        assert_eq!(yml_result.config.opencode.port, 4444);
    }

    #[test]
    fn test_all_formats_supported() {
        let dir = TempDir::new().unwrap();

        let jsonc = dir.path().join("test.jsonc");
        let json = dir.path().join("test.json");
        let yml = dir.path().join("test.yml");
        let yaml = dir.path().join("test.yaml");

        fs::write(&jsonc, r#"{"opencode": {"port": 1}}"#).unwrap();
        fs::write(&json, r#"{"opencode": {"port": 2}}"#).unwrap();
        fs::write(&yml, "opencode:\n  port: 3").unwrap();
        fs::write(&yaml, "opencode:\n  port: 4").unwrap();

        assert!(load_config_from_file(&jsonc).is_ok());
        assert!(load_config_from_file(&json).is_ok());
        assert!(load_config_from_file(&yml).is_ok());
        assert!(load_config_from_file(&yaml).is_ok());
    }
}
