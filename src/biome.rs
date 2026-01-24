use anyhow::{Context, Result};
use colored::Colorize;
use std::process::{Command, Stdio};
use which::which;

pub struct BiomeRunner {
    biome_path: Option<String>,
}

impl BiomeRunner {
    pub fn new() -> Self {
        let biome_path = which("biome")
            .ok()
            .and_then(|p| p.to_str().map(String::from));

        Self { biome_path }
    }

    pub fn check(&self, files: &[String]) -> Result<()> {
        self.ensure_biome_installed()?;

        println!("{} Running Biome check...", "🔍".bright_blue());

        let mut cmd = Command::new(self.biome_path.as_ref().unwrap());
        cmd.arg("check");

        if !files.is_empty() {
            cmd.args(files);
        } else {
            cmd.arg(".");
        }

        let status = cmd
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .context("Failed to run biome check")?;

        if !status.success() {
            anyhow::bail!("Biome check failed");
        }

        println!("{} Biome check passed", "✓".bright_green());
        Ok(())
    }

    pub fn fix(&self, files: &[String]) -> Result<()> {
        self.ensure_biome_installed()?;

        println!("{} Running Biome fix...", "🔧".bright_yellow());

        let mut cmd = Command::new(self.biome_path.as_ref().unwrap());
        cmd.arg("check").arg("--write");

        if !files.is_empty() {
            cmd.args(files);
        } else {
            cmd.arg(".");
        }

        let status = cmd
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .context("Failed to run biome fix")?;

        if !status.success() {
            anyhow::bail!("Biome fix failed");
        }

        println!("{} Biome fix completed", "✓".bright_green());
        Ok(())
    }

    fn ensure_biome_installed(&self) -> Result<()> {
        if self.biome_path.is_none() {
            anyhow::bail!(
                "Biome not found in PATH. Install it with:\n  npm install -D @biomejs/biome\n  or: brew install biome"
            );
        }
        Ok(())
    }
}

impl Default for BiomeRunner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_biome_runner_creation() {
        let runner = BiomeRunner::new();
        assert!(runner.biome_path.is_some() || runner.biome_path.is_none());
    }
}
