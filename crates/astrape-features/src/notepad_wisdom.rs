use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::Utc;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::astrape_state::NOTEPAD_BASE_PATH;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WisdomEntry {
    pub timestamp: String,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WisdomCategory {
    Learnings,
    Decisions,
    Issues,
    Problems,
}

impl WisdomCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            WisdomCategory::Learnings => "learnings",
            WisdomCategory::Decisions => "decisions",
            WisdomCategory::Issues => "issues",
            WisdomCategory::Problems => "problems",
        }
    }

    fn file_name(self) -> &'static str {
        match self {
            WisdomCategory::Learnings => "learnings.md",
            WisdomCategory::Decisions => "decisions.md",
            WisdomCategory::Issues => "issues.md",
            WisdomCategory::Problems => "problems.md",
        }
    }

    fn title(self) -> &'static str {
        match self {
            WisdomCategory::Learnings => "Learnings",
            WisdomCategory::Decisions => "Decisions",
            WisdomCategory::Issues => "Issues",
            WisdomCategory::Problems => "Problems",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanWisdom {
    pub plan_name: String,
    pub learnings: Vec<WisdomEntry>,
    pub decisions: Vec<WisdomEntry>,
    pub issues: Vec<WisdomEntry>,
    pub problems: Vec<WisdomEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedWisdom {
    pub category: WisdomCategory,
    pub content: String,
}

pub fn extract_wisdom_from_completion(response: &str) -> Vec<ExtractedWisdom> {
    let mut extracted = Vec::new();

    // Accept both `category="learnings"` (escaped quotes) and `category="learnings"`.
    let wisdom_tag_re = Regex::new(r#"(?is)<wisdom\s+category=\\?["'](\w+)\\?["']>(.*?)</wisdom>"#)
        .expect("wisdom regex");
    for caps in wisdom_tag_re.captures_iter(response) {
        let Some(category) = caps.get(1).map(|m| m.as_str().to_ascii_lowercase()) else {
            continue;
        };
        let content = caps.get(2).map(|m| m.as_str().trim()).unwrap_or_default();
        if content.is_empty() {
            continue;
        }

        let category = match category.as_str() {
            "learnings" => Some(WisdomCategory::Learnings),
            "decisions" => Some(WisdomCategory::Decisions),
            "issues" => Some(WisdomCategory::Issues),
            "problems" => Some(WisdomCategory::Problems),
            _ => None,
        };

        if let Some(category) = category {
            extracted.push(ExtractedWisdom {
                category,
                content: content.to_string(),
            });
        }
    }

    for (tag, category) in [
        ("learning", WisdomCategory::Learnings),
        ("decision", WisdomCategory::Decisions),
        ("issue", WisdomCategory::Issues),
        ("problem", WisdomCategory::Problems),
    ] {
        let re = Regex::new(&format!(r"(?is)<{tag}>(.*?)</{tag}>")).expect("tag regex");
        for caps in re.captures_iter(response) {
            let content = caps.get(1).map(|m| m.as_str().trim()).unwrap_or_default();
            if content.is_empty() {
                continue;
            }
            extracted.push(ExtractedWisdom {
                category,
                content: content.to_string(),
            });
        }
    }

    extracted
}

pub fn extract_wisdom_by_category(response: &str, target: WisdomCategory) -> Vec<String> {
    extract_wisdom_from_completion(response)
        .into_iter()
        .filter(|w| w.category == target)
        .map(|w| w.content)
        .collect()
}

pub fn has_wisdom(response: &str) -> bool {
    !extract_wisdom_from_completion(response).is_empty()
}

fn sanitize_plan_name(plan_name: &str) -> String {
    let re = Regex::new(r"[^a-zA-Z0-9_-]").expect("sanitize regex");
    re.replace_all(plan_name, "-").into_owned()
}

fn get_notepad_dir(plan_name: &str, directory: impl AsRef<Path>) -> PathBuf {
    let sanitized = sanitize_plan_name(plan_name);
    directory.as_ref().join(NOTEPAD_BASE_PATH).join(sanitized)
}

fn get_wisdom_file_path(
    plan_name: &str,
    category: WisdomCategory,
    directory: impl AsRef<Path>,
) -> PathBuf {
    get_notepad_dir(plan_name, directory).join(category.file_name())
}

pub fn init_plan_notepad(plan_name: &str, directory: impl AsRef<Path>) -> bool {
    let notepad_dir = get_notepad_dir(plan_name, directory.as_ref());

    if fs::create_dir_all(&notepad_dir).is_err() {
        return false;
    }

    for category in [
        WisdomCategory::Learnings,
        WisdomCategory::Decisions,
        WisdomCategory::Issues,
        WisdomCategory::Problems,
    ] {
        let path = notepad_dir.join(category.file_name());
        if path.exists() {
            continue;
        }

        let header = format!("# {} - {}\n\n", category.title(), plan_name);
        if fs::write(path, header).is_err() {
            return false;
        }
    }

    true
}

fn read_wisdom_category(
    plan_name: &str,
    category: WisdomCategory,
    directory: impl AsRef<Path>,
) -> Vec<WisdomEntry> {
    let file_path = get_wisdom_file_path(plan_name, category, directory);
    let Ok(content) = fs::read_to_string(file_path) else {
        return vec![];
    };

    // Rust's `regex` crate doesn't support look-around, so parse entries manually.
    //
    // File format:
    // - Optional header: `# {Category} - {plan}`
    // - Entries appended as:
    //   `## 2025-01-24 12:34:56` + blank line + content
    let mut entries = Vec::new();
    let mut current_ts: Option<String> = None;
    let mut buf: Vec<&str> = Vec::new();

    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("## ") {
            // New entry begins.
            if let Some(ts) = current_ts.take() {
                let body = buf.join("\n").trim().to_string();
                if !body.is_empty() {
                    entries.push(WisdomEntry {
                        timestamp: ts,
                        content: body,
                    });
                }
                buf.clear();
            }

            current_ts = Some(rest.trim().to_string());
            continue;
        }

        if current_ts.is_some() {
            buf.push(line);
        }
    }

    if let Some(ts) = current_ts.take() {
        let body = buf.join("\n").trim().to_string();
        if !body.is_empty() {
            entries.push(WisdomEntry {
                timestamp: ts,
                content: body,
            });
        }
    }

    entries
}

pub fn read_plan_wisdom(plan_name: &str, directory: impl AsRef<Path>) -> PlanWisdom {
    PlanWisdom {
        plan_name: plan_name.to_string(),
        learnings: read_wisdom_category(plan_name, WisdomCategory::Learnings, directory.as_ref()),
        decisions: read_wisdom_category(plan_name, WisdomCategory::Decisions, directory.as_ref()),
        issues: read_wisdom_category(plan_name, WisdomCategory::Issues, directory.as_ref()),
        problems: read_wisdom_category(plan_name, WisdomCategory::Problems, directory),
    }
}

fn add_wisdom_entry(
    plan_name: &str,
    category: WisdomCategory,
    content: &str,
    directory: impl AsRef<Path>,
) -> bool {
    let file_path = get_wisdom_file_path(plan_name, category, directory.as_ref());
    let parent = match file_path.parent() {
        Some(p) => p,
        None => return false,
    };

    if !parent.exists() {
        if !init_plan_notepad(plan_name, directory.as_ref()) {
            return false;
        }
    }

    let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let entry = format!("\n## {timestamp}\n\n{content}\n");

    let mut file = match fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(file_path)
    {
        Ok(f) => f,
        Err(_) => return false,
    };

    file.write_all(entry.as_bytes()).is_ok()
}

pub fn add_learning(plan_name: &str, content: &str, directory: impl AsRef<Path>) -> bool {
    add_wisdom_entry(plan_name, WisdomCategory::Learnings, content, directory)
}

pub fn add_decision(plan_name: &str, content: &str, directory: impl AsRef<Path>) -> bool {
    add_wisdom_entry(plan_name, WisdomCategory::Decisions, content, directory)
}

pub fn add_issue(plan_name: &str, content: &str, directory: impl AsRef<Path>) -> bool {
    add_wisdom_entry(plan_name, WisdomCategory::Issues, content, directory)
}

pub fn add_problem(plan_name: &str, content: &str, directory: impl AsRef<Path>) -> bool {
    add_wisdom_entry(plan_name, WisdomCategory::Problems, content, directory)
}

pub fn get_wisdom_summary(plan_name: &str, directory: impl AsRef<Path>) -> String {
    let wisdom = read_plan_wisdom(plan_name, directory);
    let mut sections = Vec::new();

    if !wisdom.learnings.is_empty() {
        let items = wisdom
            .learnings
            .iter()
            .map(|e| format!("- [{}] {}", e.timestamp, e.content))
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!("# Learnings\n\n{items}"));
    }

    if !wisdom.decisions.is_empty() {
        let items = wisdom
            .decisions
            .iter()
            .map(|e| format!("- [{}] {}", e.timestamp, e.content))
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!("# Decisions\n\n{items}"));
    }

    if !wisdom.issues.is_empty() {
        let items = wisdom
            .issues
            .iter()
            .map(|e| format!("- [{}] {}", e.timestamp, e.content))
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!("# Issues\n\n{items}"));
    }

    if !wisdom.problems.is_empty() {
        let items = wisdom
            .problems
            .iter()
            .map(|e| format!("- [{}] {}", e.timestamp, e.content))
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!("# Problems\n\n{items}"));
    }

    sections.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn extracts_wisdom_from_tags() {
        let input = r#"\n<wisdom category=\"learnings\">one</wisdom>\n<decision>two</decision>\n"#;

        let extracted = extract_wisdom_from_completion(input);
        assert_eq!(extracted.len(), 2);
        assert_eq!(extracted[0].category, WisdomCategory::Learnings);
        assert_eq!(extracted[0].content, "one");
        assert_eq!(extracted[1].category, WisdomCategory::Decisions);
        assert_eq!(extracted[1].content, "two");

        assert!(has_wisdom(input));
        assert_eq!(
            extract_wisdom_by_category(input, WisdomCategory::Decisions),
            vec!["two".to_string()]
        );
    }

    #[test]
    fn sanitize_plan_name_removes_separators() {
        assert_eq!(sanitize_plan_name("../../evil"), "------evil");
        assert_eq!(sanitize_plan_name("ok_name-123"), "ok_name-123");
    }

    #[test]
    fn init_add_and_read_wisdom() {
        let dir = TempDir::new().unwrap();
        let plan = "my-plan";

        assert!(init_plan_notepad(plan, dir.path()));
        assert!(add_learning(plan, "hello", dir.path()));

        let wisdom = read_plan_wisdom(plan, dir.path());
        assert_eq!(wisdom.learnings.len(), 1);
        assert_eq!(wisdom.learnings[0].content, "hello");

        let summary = get_wisdom_summary(plan, dir.path());
        assert!(summary.contains("# Learnings"));
        assert!(summary.contains("hello"));
    }
}
