mod executor;

pub use executor::HookExecutor;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Command;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HookEvent {
    PreCheck,
    PostCheck,
    PreAi,
    PostAi,
    PreFix,
    PostFix,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HookMatcher {
    #[serde(default)]
    pub matcher: Option<String>,

    #[serde(default)]
    pub run: Option<String>,

    #[serde(default)]
    pub commands: Vec<HookCommand>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HookCommand {
    #[serde(default)]
    pub name: Option<String>,

    pub run: String,

    #[serde(default)]
    pub on_fail: OnFail,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum OnFail {
    #[default]
    Continue,
    Stop,
    Warn,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct HooksConfig {
    #[serde(default, rename = "pre-check")]
    pub pre_check: Vec<HookMatcher>,

    #[serde(default, rename = "post-check")]
    pub post_check: Vec<HookMatcher>,

    #[serde(default, rename = "pre-ai")]
    pub pre_ai: Vec<HookMatcher>,

    #[serde(default, rename = "post-ai")]
    pub post_ai: Vec<HookMatcher>,

    #[serde(default, rename = "pre-fix")]
    pub pre_fix: Vec<HookMatcher>,

    #[serde(default, rename = "post-fix")]
    pub post_fix: Vec<HookMatcher>,
}

#[allow(dead_code)]
impl HooksConfig {
    pub fn get_hooks(&self, event: HookEvent) -> &[HookMatcher] {
        match event {
            HookEvent::PreCheck => &self.pre_check,
            HookEvent::PostCheck => &self.post_check,
            HookEvent::PreAi => &self.pre_ai,
            HookEvent::PostAi => &self.post_ai,
            HookEvent::PreFix => &self.pre_fix,
            HookEvent::PostFix => &self.post_fix,
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct HookContext {
    pub cwd: String,
    pub env: HashMap<String, String>,
}

impl Default for HookContext {
    fn default() -> Self {
        Self {
            cwd: std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| ".".to_string()),
            env: HashMap::new(),
        }
    }
}

#[allow(dead_code)]
impl HookContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_env(&mut self, key: &str, value: &str) {
        self.env.insert(key.to_string(), value.to_string());
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct HookResult {
    pub should_continue: bool,
    pub message: Option<String>,
    pub output: Option<String>,
}

impl Default for HookResult {
    fn default() -> Self {
        Self {
            should_continue: true,
            message: None,
            output: None,
        }
    }
}

#[allow(dead_code)]
pub struct AiHookExecutor {
    config: HooksConfig,
}

#[allow(dead_code)]
impl AiHookExecutor {
    pub fn new(config: HooksConfig) -> Self {
        Self { config }
    }

    pub fn execute(&self, event: HookEvent, context: &HookContext) -> Result<HookResult> {
        let hooks = self.config.get_hooks(event);

        if hooks.is_empty() {
            return Ok(HookResult::default());
        }

        let mut final_result = HookResult::default();
        let mut outputs = Vec::new();

        for matcher in hooks {
            if let Some(pattern) = &matcher.matcher {
                if !self.matches_context(pattern, context) {
                    continue;
                }
            }

            if let Some(run) = &matcher.run {
                let result = self.execute_command(run, context, &OnFail::Continue)?;
                if !result.should_continue {
                    return Ok(result);
                }
                if let Some(output) = result.output {
                    outputs.push(output);
                }
            }

            for cmd in &matcher.commands {
                let result = self.execute_command(&cmd.run, context, &cmd.on_fail)?;
                if !result.should_continue {
                    return Ok(result);
                }
                if let Some(output) = result.output {
                    outputs.push(output);
                }
            }
        }

        if !outputs.is_empty() {
            final_result.output = Some(outputs.join("\n"));
        }

        Ok(final_result)
    }

    fn matches_context(&self, pattern: &str, context: &HookContext) -> bool {
        if pattern == "*" {
            return true;
        }

        if let Some(file) = context.env.get("FILE") {
            if pattern.contains('*') {
                let regex_pattern = pattern.replace('.', "\\.").replace('*', ".*");
                if let Ok(re) = regex::Regex::new(&regex_pattern) {
                    return re.is_match(file);
                }
            }
            return file.contains(pattern);
        }

        true
    }

    fn execute_command(
        &self,
        cmd: &str,
        context: &HookContext,
        on_fail: &OnFail,
    ) -> Result<HookResult> {
        let expanded_cmd = self.expand_variables(cmd, context);

        let output = Command::new("sh")
            .arg("-c")
            .arg(&expanded_cmd)
            .current_dir(&context.cwd)
            .envs(&context.env)
            .output()
            .context(format!("Failed to execute hook command: {}", cmd))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let mut result = HookResult {
            should_continue: true,
            message: None,
            output: if stdout.is_empty() {
                None
            } else {
                Some(stdout)
            },
        };

        if !output.status.success() {
            match on_fail {
                OnFail::Stop => {
                    result.should_continue = false;
                    result.message = Some(format!("Hook failed: {}", stderr.trim()));
                }
                OnFail::Warn => {
                    result.message = Some(format!("Hook warning: {}", stderr.trim()));
                }
                OnFail::Continue => {}
            }
        }

        Ok(result)
    }

    fn expand_variables(&self, cmd: &str, context: &HookContext) -> String {
        let mut result = cmd.to_string();

        for (key, value) in &context.env {
            result = result.replace(&format!("${}", key), value);
            result = result.replace(&format!("${{{}}}", key), value);
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_context() {
        let mut ctx = HookContext::new();
        ctx.set_env("FILE", "test.rs");
        ctx.set_env("TYPO", "teh");

        assert_eq!(ctx.env.get("FILE"), Some(&"test.rs".to_string()));
        assert_eq!(ctx.env.get("TYPO"), Some(&"teh".to_string()));
    }

    #[test]
    fn test_hook_executor_empty() {
        let config = HooksConfig::default();
        let executor = AiHookExecutor::new(config);
        let context = HookContext::new();

        let result = executor.execute(HookEvent::PreCheck, &context).unwrap();
        assert!(result.should_continue);
    }

    #[test]
    fn test_matches_context_wildcard() {
        let config = HooksConfig::default();
        let executor = AiHookExecutor::new(config);
        let mut context = HookContext::new();
        context.set_env("FILE", "test.rs");

        assert!(executor.matches_context("*", &context));
        assert!(executor.matches_context("*.rs", &context));
        assert!(!executor.matches_context("*.ts", &context));
    }

    #[test]
    fn test_expand_variables() {
        let config = HooksConfig::default();
        let executor = AiHookExecutor::new(config);
        let mut context = HookContext::new();
        context.set_env("FILE", "test.rs");
        context.set_env("TYPO", "teh");

        let cmd = "echo $FILE has typo: ${TYPO}";
        let expanded = executor.expand_variables(cmd, &context);
        assert_eq!(expanded, "echo test.rs has typo: teh");
    }
}
