//! Notepad Support
//!
//! Implements compaction-resilient memory persistence using notepad.md format.
//! Provides a three-tier memory system:
//! 1. Priority Context - Always loaded, critical discoveries (max 500 chars)
//! 2. Working Memory - Session notes, auto-pruned after 7 days
//! 3. MANUAL - User content, never auto-pruned

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uira_core::UIRA_DIR;

use crate::hook::{Hook, HookContext, HookResult};
use crate::types::{HookEvent, HookInput, HookOutput};

pub const NOTEPAD_FILENAME: &str = "notepad.md";
pub const PRIORITY_HEADER: &str = "## Priority Context";
pub const WORKING_MEMORY_HEADER: &str = "## Working Memory";
pub const MANUAL_HEADER: &str = "## MANUAL";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotepadConfig {
    pub priority_max_chars: usize,
    pub working_memory_days: i64,
    pub max_total_size: usize,
}

pub const DEFAULT_NOTEPAD_CONFIG: NotepadConfig = NotepadConfig {
    priority_max_chars: 500,
    working_memory_days: 7,
    max_total_size: 8192, // 8KB
};

impl Default for NotepadConfig {
    fn default() -> Self {
        DEFAULT_NOTEPAD_CONFIG
    }
}

#[derive(Debug, Clone, Default)]
pub struct NotepadStats {
    pub exists: bool,
    pub total_size: usize,
    pub priority_size: usize,
    pub working_memory_entries: usize,
    pub oldest_entry: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PriorityContextResult {
    pub success: bool,
    pub warning: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PruneResult {
    pub pruned: usize,
    pub remaining: usize,
}

pub struct NotepadHook;

impl NotepadHook {
    pub fn new() -> Self {
        Self
    }

    pub fn get_notepad_path(directory: &str) -> PathBuf {
        Path::new(directory).join(UIRA_DIR).join(NOTEPAD_FILENAME)
    }

    fn ensure_uira_dir(directory: &str) -> std::io::Result<()> {
        let uira_dir = Path::new(directory).join(UIRA_DIR);
        if !uira_dir.exists() {
            fs::create_dir_all(&uira_dir)?;
        }
        Ok(())
    }

    pub fn init_notepad(directory: &str) -> bool {
        if Self::ensure_uira_dir(directory).is_err() {
            return false;
        }

        let notepad_path = Self::get_notepad_path(directory);
        if notepad_path.exists() {
            return true;
        }

        let content = format!(
            r#"# Notepad
<!-- Auto-managed by Uira. Manual edits preserved in MANUAL section. -->

{PRIORITY_HEADER}
<!-- ALWAYS loaded. Keep under 500 chars. Critical discoveries only. -->

{WORKING_MEMORY_HEADER}
<!-- Session notes. Auto-pruned after 7 days. -->

{MANUAL_HEADER}
<!-- User content. Never auto-pruned. -->

"#
        );

        fs::write(&notepad_path, content).is_ok()
    }

    pub fn read_notepad(directory: &str) -> Option<String> {
        let notepad_path = Self::get_notepad_path(directory);
        if !notepad_path.exists() {
            return None;
        }
        fs::read_to_string(&notepad_path).ok()
    }

    fn extract_section(content: &str, header: &str) -> Option<String> {
        let header_pos = content.find(header)?;
        let after_header = &content[header_pos + header.len()..];

        let section_end = after_header.find("\n## ").unwrap_or(after_header.len());

        let section = &after_header[..section_end];

        let comment_regex = Regex::new(r"<!--[\s\S]*?-->").unwrap();
        let cleaned = comment_regex.replace_all(section, "");
        let cleaned = cleaned.trim().to_string();

        if cleaned.is_empty() {
            None
        } else {
            Some(cleaned)
        }
    }

    fn replace_section(content: &str, header: &str, new_content: &str) -> String {
        let header_pos = match content.find(header) {
            Some(pos) => pos,
            None => return content.to_string(),
        };

        let after_header_start = header_pos + header.len();
        let after_header = &content[after_header_start..];

        let section_end = after_header
            .find("\n## ")
            .map(|pos| after_header_start + pos)
            .unwrap_or(content.len());

        let comment_pattern = format!(r"{}[\r\n]+(<!--[\s\S]*?-->)", regex::escape(header));
        let comment_regex = Regex::new(&comment_pattern).unwrap();
        let comment = comment_regex
            .captures(content)
            .and_then(|c| c.get(1))
            .map(|m| format!("{}\n", m.as_str()))
            .unwrap_or_default();

        let before = &content[..header_pos + header.len()];
        let after = &content[section_end..];

        format!("{}\n{}{}\n{}", before, comment, new_content, after)
    }

    pub fn get_priority_context(directory: &str) -> Option<String> {
        let content = Self::read_notepad(directory)?;
        Self::extract_section(&content, PRIORITY_HEADER)
    }

    pub fn get_working_memory(directory: &str) -> Option<String> {
        let content = Self::read_notepad(directory)?;
        Self::extract_section(&content, WORKING_MEMORY_HEADER)
    }

    pub fn get_manual_section(directory: &str) -> Option<String> {
        let content = Self::read_notepad(directory)?;
        Self::extract_section(&content, MANUAL_HEADER)
    }

    pub fn set_priority_context(
        directory: &str,
        content: &str,
        config: Option<&NotepadConfig>,
    ) -> PriorityContextResult {
        let cfg = config.unwrap_or(&DEFAULT_NOTEPAD_CONFIG);

        if !Self::get_notepad_path(directory).exists() && !Self::init_notepad(directory) {
            return PriorityContextResult {
                success: false,
                warning: None,
            };
        }

        let notepad_path = Self::get_notepad_path(directory);
        let notepad_content = match fs::read_to_string(&notepad_path) {
            Ok(c) => c,
            Err(_) => {
                return PriorityContextResult {
                    success: false,
                    warning: None,
                }
            }
        };

        let warning = if content.len() > cfg.priority_max_chars {
            Some(format!(
                "Priority Context exceeds {} chars ({} chars). Consider condensing.",
                cfg.priority_max_chars,
                content.len()
            ))
        } else {
            None
        };

        let updated = Self::replace_section(&notepad_content, PRIORITY_HEADER, content);

        match fs::write(&notepad_path, updated) {
            Ok(_) => PriorityContextResult {
                success: true,
                warning,
            },
            Err(_) => PriorityContextResult {
                success: false,
                warning: None,
            },
        }
    }

    pub fn add_working_memory_entry(directory: &str, content: &str) -> bool {
        if !Self::get_notepad_path(directory).exists() && !Self::init_notepad(directory) {
            return false;
        }

        let notepad_path = Self::get_notepad_path(directory);
        let notepad_content = match fs::read_to_string(&notepad_path) {
            Ok(c) => c,
            Err(_) => return false,
        };

        let current_memory =
            Self::extract_section(&notepad_content, WORKING_MEMORY_HEADER).unwrap_or_default();

        // Format timestamp: YYYY-MM-DD HH:MM
        let now: DateTime<Utc> = Utc::now();
        let timestamp = now.format("%Y-%m-%d %H:%M").to_string();

        let new_entry = format!("### {}\n{}\n", timestamp, content);
        let updated_memory = if current_memory.is_empty() {
            new_entry
        } else {
            format!("{}\n{}", current_memory, new_entry)
        };

        let updated =
            Self::replace_section(&notepad_content, WORKING_MEMORY_HEADER, &updated_memory);
        fs::write(&notepad_path, updated).is_ok()
    }

    pub fn add_manual_entry(directory: &str, content: &str) -> bool {
        if !Self::get_notepad_path(directory).exists() && !Self::init_notepad(directory) {
            return false;
        }

        let notepad_path = Self::get_notepad_path(directory);
        let notepad_content = match fs::read_to_string(&notepad_path) {
            Ok(c) => c,
            Err(_) => return false,
        };

        let current_manual =
            Self::extract_section(&notepad_content, MANUAL_HEADER).unwrap_or_default();

        let now: DateTime<Utc> = Utc::now();
        let timestamp = now.format("%Y-%m-%d %H:%M").to_string();

        let new_entry = format!("### {}\n{}\n", timestamp, content);
        let updated_manual = if current_manual.is_empty() {
            new_entry
        } else {
            format!("{}\n{}", current_manual, new_entry)
        };

        let updated = Self::replace_section(&notepad_content, MANUAL_HEADER, &updated_manual);
        fs::write(&notepad_path, updated).is_ok()
    }

    pub fn prune_old_entries(directory: &str, days_old: Option<i64>) -> PruneResult {
        let days = days_old.unwrap_or(DEFAULT_NOTEPAD_CONFIG.working_memory_days);

        let notepad_path = Self::get_notepad_path(directory);
        if !notepad_path.exists() {
            return PruneResult {
                pruned: 0,
                remaining: 0,
            };
        }

        let notepad_content = match fs::read_to_string(&notepad_path) {
            Ok(c) => c,
            Err(_) => {
                return PruneResult {
                    pruned: 0,
                    remaining: 0,
                }
            }
        };

        let working_memory = match Self::extract_section(&notepad_content, WORKING_MEMORY_HEADER) {
            Some(m) => m,
            None => {
                return PruneResult {
                    pruned: 0,
                    remaining: 0,
                }
            }
        };

        // Parse entries: ### YYYY-MM-DD HH:MM
        let entry_regex = Regex::new(r"### (\d{4}-\d{2}-\d{2} \d{2}:\d{2})\n([\s\S]*?)").unwrap();

        let mut entries: Vec<(String, String)> = Vec::new();
        for cap in entry_regex.captures_iter(&working_memory) {
            let timestamp = cap.get(1).map(|m| m.as_str().to_string()).unwrap();
            let content = cap
                .get(2)
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_default();
            entries.push((timestamp, content));
        }

        let cutoff = Utc::now() - Duration::days(days);
        let original_count = entries.len();

        let kept: Vec<_> = entries
            .into_iter()
            .filter(|(ts, _)| {
                // Parse timestamp: YYYY-MM-DD HH:MM
                if let Ok(entry_date) = chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M")
                {
                    let entry_utc = entry_date.and_utc();
                    entry_utc >= cutoff
                } else {
                    true // Keep entries with unparsable timestamps
                }
            })
            .collect();

        let pruned = original_count - kept.len();

        let new_content = kept
            .iter()
            .map(|(ts, content)| format!("### {}\n{}", ts, content))
            .collect::<Vec<_>>()
            .join("\n\n");

        let updated = Self::replace_section(&notepad_content, WORKING_MEMORY_HEADER, &new_content);

        match fs::write(&notepad_path, updated) {
            Ok(_) => PruneResult {
                pruned,
                remaining: kept.len(),
            },
            Err(_) => PruneResult {
                pruned: 0,
                remaining: original_count,
            },
        }
    }

    pub fn get_notepad_stats(directory: &str) -> NotepadStats {
        let notepad_path = Self::get_notepad_path(directory);

        if !notepad_path.exists() {
            return NotepadStats::default();
        }

        let content = match fs::read_to_string(&notepad_path) {
            Ok(c) => c,
            Err(_) => return NotepadStats::default(),
        };

        let priority_context = Self::extract_section(&content, PRIORITY_HEADER).unwrap_or_default();
        let working_memory =
            Self::extract_section(&content, WORKING_MEMORY_HEADER).unwrap_or_default();

        // Count entries
        let entry_regex = Regex::new(r"### \d{4}-\d{2}-\d{2} \d{2}:\d{2}").unwrap();
        let entries: Vec<_> = entry_regex.find_iter(&working_memory).collect();
        let entry_count = entries.len();

        // Find oldest entry
        let oldest_entry = if !entries.is_empty() {
            let mut timestamps: Vec<_> = entries
                .iter()
                .map(|m| m.as_str().replace("### ", ""))
                .collect();
            timestamps.sort();
            timestamps.first().cloned()
        } else {
            None
        };

        NotepadStats {
            exists: true,
            total_size: content.len(),
            priority_size: priority_context.len(),
            working_memory_entries: entry_count,
            oldest_entry,
        }
    }

    pub fn format_notepad_context(directory: &str) -> Option<String> {
        let notepad_path = Self::get_notepad_path(directory);
        if !notepad_path.exists() {
            return None;
        }

        let priority_context = Self::get_priority_context(directory)?;

        Some(format!(
            r#"<notepad-priority>

## Priority Context

{}

</notepad-priority>
"#,
            priority_context
        ))
    }

    pub fn format_full_notepad(directory: &str) -> Option<String> {
        Self::read_notepad(directory)
    }
}

#[async_trait]
impl Hook for NotepadHook {
    fn name(&self) -> &str {
        "notepad"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::UserPromptSubmit]
    }

    async fn execute(
        &self,
        _event: HookEvent,
        _input: &HookInput,
        _context: &HookContext,
    ) -> HookResult {
        // Process notepad commands in prompt
        Ok(HookOutput::pass())
    }
}

impl Default for NotepadHook {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_init_notepad() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path().to_str().unwrap();

        assert!(NotepadHook::init_notepad(dir_path));

        let notepad_path = NotepadHook::get_notepad_path(dir_path);
        assert!(notepad_path.exists());

        let content = fs::read_to_string(&notepad_path).unwrap();
        assert!(content.contains(PRIORITY_HEADER));
        assert!(content.contains(WORKING_MEMORY_HEADER));
        assert!(content.contains(MANUAL_HEADER));
    }

    #[test]
    fn test_set_priority_context() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path().to_str().unwrap();

        let result = NotepadHook::set_priority_context(dir_path, "Test priority content", None);
        assert!(result.success);
        assert!(result.warning.is_none());

        let priority = NotepadHook::get_priority_context(dir_path);
        assert_eq!(priority, Some("Test priority content".to_string()));
    }

    #[test]
    fn test_priority_context_warning() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path().to_str().unwrap();

        let long_content = "x".repeat(600);
        let result = NotepadHook::set_priority_context(dir_path, &long_content, None);
        assert!(result.success);
        assert!(result.warning.is_some());
        assert!(result.warning.unwrap().contains("exceeds"));
    }

    #[test]
    fn test_add_working_memory_entry() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path().to_str().unwrap();

        assert!(NotepadHook::add_working_memory_entry(
            dir_path,
            "First entry"
        ));
        assert!(NotepadHook::add_working_memory_entry(
            dir_path,
            "Second entry"
        ));

        let memory = NotepadHook::get_working_memory(dir_path).unwrap();
        assert!(memory.contains("First entry"));
        assert!(memory.contains("Second entry"));
    }

    #[test]
    fn test_add_manual_entry() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path().to_str().unwrap();

        assert!(NotepadHook::add_manual_entry(dir_path, "Manual note"));

        let manual = NotepadHook::get_manual_section(dir_path).unwrap();
        assert!(manual.contains("Manual note"));
    }

    #[test]
    fn test_get_notepad_stats() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path().to_str().unwrap();

        // Before init
        let stats = NotepadHook::get_notepad_stats(dir_path);
        assert!(!stats.exists);

        // After init and adding content
        NotepadHook::set_priority_context(dir_path, "Test content", None);
        NotepadHook::add_working_memory_entry(dir_path, "Entry 1");
        NotepadHook::add_working_memory_entry(dir_path, "Entry 2");

        let stats = NotepadHook::get_notepad_stats(dir_path);
        assert!(stats.exists);
        assert!(stats.total_size > 0);
        assert_eq!(stats.working_memory_entries, 2);
    }

    #[test]
    fn test_format_notepad_context() {
        let dir = tempdir().unwrap();
        let dir_path = dir.path().to_str().unwrap();

        // No notepad yet
        assert!(NotepadHook::format_notepad_context(dir_path).is_none());

        // Add priority context
        NotepadHook::set_priority_context(dir_path, "Critical discovery", None);

        let formatted = NotepadHook::format_notepad_context(dir_path).unwrap();
        assert!(formatted.contains("<notepad-priority>"));
        assert!(formatted.contains("Critical discovery"));
        assert!(formatted.contains("</notepad-priority>"));
    }

    #[test]
    fn test_extract_section() {
        let content = r#"# Notepad

## Priority Context
<!-- Comment -->
Priority content here

## Working Memory
Memory content

## MANUAL
Manual content
"#;
        let priority = NotepadHook::extract_section(content, PRIORITY_HEADER);
        assert_eq!(priority, Some("Priority content here".to_string()));

        let memory = NotepadHook::extract_section(content, WORKING_MEMORY_HEADER);
        assert_eq!(memory, Some("Memory content".to_string()));

        let manual = NotepadHook::extract_section(content, MANUAL_HEADER);
        assert_eq!(manual, Some("Manual content".to_string()));
    }
}
