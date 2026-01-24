use anyhow::{Context, Result};
use astrape_core::HookEvent;
use std::fs;
use std::path::PathBuf;

use crate::settings::update_claude_settings;
use crate::shell::{generate_keyword_detector_script, generate_stop_continuation_script};

pub fn get_claude_hooks_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not determine home directory")?;
    Ok(home.join(".claude").join("hooks"))
}

pub fn install_hooks(events: &[HookEvent]) -> Result<Vec<String>> {
    let hooks_dir = get_claude_hooks_dir()?;
    fs::create_dir_all(&hooks_dir)?;

    let mut installed = Vec::new();

    if events.contains(&HookEvent::UserPromptSubmit) {
        let script = generate_keyword_detector_script();
        let path = hooks_dir.join("keyword-detector.sh");
        fs::write(&path, script)?;
        make_executable(&path)?;
        installed.push("keyword-detector.sh".to_string());
    }

    if events.contains(&HookEvent::Stop) {
        let script = generate_stop_continuation_script();
        let path = hooks_dir.join("stop-continuation.sh");
        fs::write(&path, script)?;
        make_executable(&path)?;
        installed.push("stop-continuation.sh".to_string());
    }

    update_claude_settings(events)?;

    Ok(installed)
}

pub fn uninstall_hooks() -> Result<Vec<String>> {
    let hooks_dir = get_claude_hooks_dir()?;
    let mut removed = Vec::new();

    let hook_files = ["keyword-detector.sh", "stop-continuation.sh"];

    for file in hook_files {
        let path = hooks_dir.join(file);
        if path.exists() {
            fs::remove_file(&path)?;
            removed.push(file.to_string());
        }
    }

    Ok(removed)
}

pub fn list_installed_hooks() -> Result<Vec<String>> {
    let hooks_dir = get_claude_hooks_dir()?;
    let mut hooks = Vec::new();

    if hooks_dir.exists() {
        for entry in fs::read_dir(&hooks_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map(|e| e == "sh").unwrap_or(false) {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    hooks.push(name.to_string());
                }
            }
        }
    }

    Ok(hooks)
}

#[cfg(unix)]
fn make_executable(path: &PathBuf) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_path: &PathBuf) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_hooks_dir() {
        let dir = get_claude_hooks_dir();
        assert!(dir.is_ok());
        let path = dir.unwrap();
        assert!(path.ends_with(".claude/hooks"));
    }
}
