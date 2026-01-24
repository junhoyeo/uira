use anyhow::{Context, Result};
use astrape_core::{HookContext, HookEvent, HookMatcher, HookResult, OnFail};
use std::process::Command;

pub struct HookExecutor {
    matchers: Vec<HookMatcher>,
}

impl Default for HookExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl HookExecutor {
    pub fn new() -> Self {
        Self {
            matchers: Vec::new(),
        }
    }

    pub fn with_matchers(matchers: Vec<HookMatcher>) -> Self {
        Self { matchers }
    }

    pub fn add_matcher(&mut self, matcher: HookMatcher) {
        self.matchers.push(matcher);
    }

    pub fn execute(&self, _event: HookEvent, context: &HookContext) -> Result<HookResult> {
        if self.matchers.is_empty() {
            return Ok(HookResult::default());
        }

        let mut final_result = HookResult::default();
        let mut outputs = Vec::new();

        for matcher in &self.matchers {
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
            success: output.status.success(),
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
    fn test_executor_empty() {
        let executor = HookExecutor::new();
        let context = HookContext::new();

        let result = executor.execute(HookEvent::PreCheck, &context).unwrap();
        assert!(result.success);
        assert!(result.should_continue);
    }

    #[test]
    fn test_matches_context_wildcard() {
        let executor = HookExecutor::new();
        let context = HookContext::new().with_env("FILE", "test.rs");

        assert!(executor.matches_context("*", &context));
        assert!(executor.matches_context("*.rs", &context));
        assert!(!executor.matches_context("*.ts", &context));
    }

    #[test]
    fn test_expand_variables() {
        let executor = HookExecutor::new();
        let context = HookContext::new()
            .with_env("FILE", "test.rs")
            .with_env("TYPO", "teh");

        let cmd = "echo $FILE has typo: ${TYPO}";
        let expanded = executor.expand_variables(cmd, &context);
        assert_eq!(expanded, "echo test.rs has typo: teh");
    }

    #[test]
    fn test_execute_simple_command() {
        let mut executor = HookExecutor::new();
        executor.add_matcher(HookMatcher {
            matcher: None,
            run: Some("echo hello".to_string()),
            commands: vec![],
        });

        let context = HookContext::new();
        let result = executor.execute(HookEvent::PreCheck, &context).unwrap();

        assert!(result.success);
        assert!(result.output.is_some());
        assert!(result.output.unwrap().contains("hello"));
    }
}
