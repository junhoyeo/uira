use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeFile {
    pub name: String,
    pub colors: ThemeColors,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeColors {
    pub bg: String,
    pub fg: String,
    pub accent: String,
    pub error: String,
    pub warning: String,
    pub success: String,
}

#[derive(Debug, Clone, Default)]
pub struct ThemeLoadResult {
    pub themes: Vec<ThemeFile>,
    pub warnings: Vec<String>,
    pub fingerprint: u64,
}

pub fn load_external_themes() -> ThemeLoadResult {
    let mut result = ThemeLoadResult::default();
    let theme_dir = theme_directory();

    let Ok(entries) = fs::read_dir(&theme_dir) else {
        result.fingerprint = fingerprint_for_dir(&theme_dir);
        return result;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }

        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) => {
                result.warnings.push(format!(
                    "Failed to read theme file '{}': {}",
                    path.display(),
                    error
                ));
                continue;
            }
        };

        let parsed = match serde_json::from_str::<ThemeFile>(&content) {
            Ok(parsed) => parsed,
            Err(error) => {
                result.warnings.push(format!(
                    "Failed to parse theme file '{}': {}",
                    path.display(),
                    error
                ));
                continue;
            }
        };

        if let Err(validation_error) = validate_theme(&parsed) {
            result.warnings.push(format!(
                "Invalid theme file '{}': {}",
                path.display(),
                validation_error
            ));
            continue;
        }

        result.themes.push(parsed);
    }

    result.themes.sort_by(|a, b| a.name.cmp(&b.name));
    result.fingerprint = fingerprint_for_dir(&theme_dir);
    result
}

pub fn external_theme_fingerprint() -> u64 {
    fingerprint_for_dir(&theme_directory())
}

fn theme_directory() -> PathBuf {
    let mut path = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push(crate::UIRA_DIR);
    path.push("themes");
    path
}

fn validate_theme(theme: &ThemeFile) -> Result<(), String> {
    if theme.name.trim().is_empty() {
        return Err("theme name must not be empty".to_string());
    }

    for (field, value) in [
        ("bg", theme.colors.bg.as_str()),
        ("fg", theme.colors.fg.as_str()),
        ("accent", theme.colors.accent.as_str()),
        ("error", theme.colors.error.as_str()),
        ("warning", theme.colors.warning.as_str()),
        ("success", theme.colors.success.as_str()),
    ] {
        validate_hex_color(value)
            .map_err(|error| format!("invalid '{}' color '{}': {}", field, value, error))?;
    }

    Ok(())
}

fn validate_hex_color(raw: &str) -> Result<(), &'static str> {
    let value = raw.strip_prefix('#').unwrap_or(raw);
    if value.len() != 6 {
        return Err("expected 6 hex digits");
    }
    if !value.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err("expected hexadecimal characters");
    }
    Ok(())
}

fn fingerprint_for_dir(dir: &PathBuf) -> u64 {
    let mut hasher = DefaultHasher::new();
    dir.hash(&mut hasher);

    let Ok(entries) = fs::read_dir(dir) else {
        return hasher.finish();
    };

    let mut metadata_entries: Vec<(String, u64, u64)> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                return None;
            }

            let metadata = entry.metadata().ok()?;
            let modified = metadata.modified().ok()?;
            let modified_secs = modified
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_secs();
            let file_name = path.file_name()?.to_string_lossy().to_string();
            Some((file_name, modified_secs, metadata.len()))
        })
        .collect();

    metadata_entries.sort_by(|a, b| a.0.cmp(&b.0));
    metadata_entries.hash(&mut hasher);
    hasher.finish()
}
