use crate::schema::AstrapeConfig;
use anyhow::{anyhow, Context, Result};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// Load configuration from file or environment
pub fn load_config(config_path: Option<&Path>) -> Result<AstrapeConfig> {
    let path = config_path
        .map(|p| p.to_path_buf())
        .or_else(find_config_file)
        .ok_or_else(|| anyhow!("No configuration file found"))?;

    load_config_from_file(&path)
}

/// Load configuration from a specific file
pub fn load_config_from_file(path: &Path) -> Result<AstrapeConfig> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file: {}", path.display()))?;

    let config = if path.extension().is_some_and(|ext| ext == "json") {
        serde_json::from_str(&content).context("Failed to parse JSON configuration")?
    } else {
        serde_yaml::from_str(&content).context("Failed to parse YAML configuration")?
    };

    Ok(expand_env_vars(config))
}

/// Find configuration file in standard locations
fn find_config_file() -> Option<PathBuf> {
    let candidates = [
        "astrape.yml",
        "astrape.yaml",
        "astrape.json",
        ".astrape.yml",
        ".astrape.yaml",
        ".astrape.json",
    ];

    for candidate in &candidates {
        let path = PathBuf::from(candidate);
        if path.exists() {
            return Some(path);
        }
    }

    // Check in home directory
    if let Ok(home) = env::var("HOME") {
        for candidate in &candidates {
            let path = PathBuf::from(&home)
                .join(".config")
                .join("astrape")
                .join(candidate);
            if path.exists() {
                return Some(path);
            }
        }
    }

    None
}

/// Expand environment variables in configuration
fn expand_env_vars(config: AstrapeConfig) -> AstrapeConfig {
    AstrapeConfig {
        ai: expand_ai_settings(config.ai),
        mcp: expand_mcp_settings(config.mcp),
        agents: config.agents,
        hooks: config.hooks,
        ai_hooks: config.ai_hooks,
        goals: expand_goals_settings(config.goals),
    }
}

/// Expand environment variables in Goals settings
fn expand_goals_settings(mut goals: crate::schema::GoalsConfig) -> crate::schema::GoalsConfig {
    for goal in goals.goals.iter_mut() {
        goal.command = expand_env_string(&goal.command);
        if let Some(ws) = &goal.workspace {
            goal.workspace = Some(expand_env_string(ws));
        }
    }
    goals
}

/// Expand environment variables in AI settings
fn expand_ai_settings(mut ai: crate::schema::AiSettings) -> crate::schema::AiSettings {
    ai.model = expand_env_string(&ai.model);
    ai.host = expand_env_string(&ai.host);
    ai
}

/// Expand environment variables in MCP settings
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

/// Expand environment variables in a string
/// Supports $VAR and ${VAR} syntax
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
        // This test ensures the space after the variable is preserved (bug fix)
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
ai:
  model: anthropic/claude-sonnet-4-20250514
  temperature: 0.7

pre-commit:
  parallel: true
  commands:
    - name: fmt
      run: cargo fmt --check
"#;
        let config: AstrapeConfig = serde_yaml::from_str(yaml_content).unwrap();
        assert_eq!(config.ai.model, "anthropic/claude-sonnet-4-20250514");
        assert_eq!(config.ai.temperature, 0.7);
    }

    #[test]
    fn test_load_config_from_json() {
        let json_content = r#"{
  "ai": {
    "model": "anthropic/claude-opus-4-1",
    "temperature": 0.5
  }
}"#;
        let config: AstrapeConfig = serde_json::from_str(json_content).unwrap();
        assert_eq!(config.ai.model, "anthropic/claude-opus-4-1");
        assert_eq!(config.ai.temperature, 0.5);
    }
}
