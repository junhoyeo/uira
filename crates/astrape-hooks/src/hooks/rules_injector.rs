//! Rules Injector Hook (ported from TypeScript)
//!
//! Automatically injects relevant rule files when tools access files.
//! Supports project-level rule discovery (.github/instructions, .cursor/rules, .claude/rules)
//! and user-level rules (~/.claude/rules).
//!
//! This is a single-file Rust port of the TypeScript module:
//! `oh-my-claudecode/src/hooks/rules-injector/*`.

use async_trait::async_trait;
use dirs::home_dir;
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use crate::hook::{Hook, HookContext, HookResult};
use crate::types::{HookEvent, HookInput, HookOutput};

// =============================================================================
// Constants
// =============================================================================

pub const PROJECT_MARKERS: &[&str] = &[
    ".git",
    "pyproject.toml",
    "package.json",
    "Cargo.toml",
    "go.mod",
    ".venv",
];

pub const PROJECT_RULE_SUBDIRS: &[(&str, &str)] = &[
    (".github", "instructions"),
    (".cursor", "rules"),
    (".claude", "rules"),
];

pub const PROJECT_RULE_FILES: &[&str] = &[".github/copilot-instructions.md"];

pub const USER_RULE_DIR: &str = ".claude/rules";
pub const RULE_EXTENSIONS: &[&str] = &[".md", ".mdc"];
pub const TRACKED_TOOLS: &[&str] = &["read", "write", "edit", "multiedit"];

lazy_static! {
    static ref GITHUB_INSTRUCTIONS_PATTERN: Regex = Regex::new(r"\.instructions\.md$").unwrap();
}

// =============================================================================
// Types
// =============================================================================

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuleMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub globs: Option<GlobValue>,

    #[serde(rename = "alwaysApply", skip_serializing_if = "Option::is_none")]
    pub always_apply: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GlobValue {
    Single(String),
    Multiple(Vec<String>),
}

impl GlobValue {
    fn as_vec(&self) -> Vec<String> {
        match self {
            Self::Single(s) => vec![s.clone()],
            Self::Multiple(v) => v.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleFileCandidate {
    pub path: String,
    pub real_path: String,
    pub is_global: bool,
    pub distance: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_single_file: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InjectedRulesData {
    pub session_id: String,
    pub injected_hashes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub injected_real_paths: Option<Vec<String>>,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleToInject {
    pub relative_path: String,
    pub match_reason: String,
    pub content: String,
    pub distance: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchResult {
    pub applies: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleFrontmatterResult {
    pub metadata: RuleMetadata,
    pub body: String,
}

#[derive(Debug, Clone, Default)]
struct SessionCache {
    content_hashes: HashSet<String>,
    real_paths: HashSet<String>,
}

// =============================================================================
// Frontmatter Parser
// =============================================================================

pub fn parse_rule_frontmatter(content: &str) -> RuleFrontmatterResult {
    // TS regex: /^---\r?\n([\s\S]*?)\r?\n---\r?\n?([\s\S]*)$/
    let re = Regex::new(r"(?s)^---\r?\n(.*?)\r?\n---\r?\n?(.*)$").unwrap();
    let Some(caps) = re.captures(content) else {
        return RuleFrontmatterResult {
            metadata: RuleMetadata::default(),
            body: content.to_string(),
        };
    };

    let yaml_content = caps.get(1).map(|m| m.as_str()).unwrap_or("");
    let body = caps.get(2).map(|m| m.as_str()).unwrap_or("");

    // The TS version catches exceptions and falls back to original content.
    // Our parser is non-panicking; treat any failure as empty metadata.
    let metadata = parse_yaml_content(yaml_content);

    RuleFrontmatterResult {
        metadata,
        body: body.to_string(),
    }
}

fn parse_yaml_content(yaml_content: &str) -> RuleMetadata {
    let lines: Vec<&str> = yaml_content.split('\n').collect();
    let mut metadata = RuleMetadata::default();

    let mut i = 0usize;
    while i < lines.len() {
        let line = lines[i];
        let Some(colon_index) = line.find(':') else {
            i += 1;
            continue;
        };

        let key = line[..colon_index].trim();
        let raw_value = line[colon_index + 1..].trim();

        match key {
            "description" => {
                metadata.description = Some(parse_string_value(raw_value));
            }
            "alwaysApply" => {
                metadata.always_apply = Some(raw_value == "true");
            }
            "globs" | "paths" | "applyTo" => {
                let (value, consumed) = parse_array_or_string_value(raw_value, &lines, i);
                metadata.globs = Some(merge_globs(metadata.globs.take(), value));
                i += consumed;
                continue;
            }
            _ => {}
        }

        i += 1;
    }

    metadata
}

fn parse_string_value(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }

    let bytes = value.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0] as char;
        let last = bytes[bytes.len() - 1] as char;
        if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
            return value[1..value.len() - 1].to_string();
        }
    }

    value.to_string()
}

fn parse_array_or_string_value(
    raw_value: &str,
    lines: &[&str],
    current_index: usize,
) -> (GlobValue, usize) {
    // Case 1: Inline array ["a", "b", "c"]
    if raw_value.starts_with('[') {
        return (GlobValue::Multiple(parse_inline_array(raw_value)), 1);
    }

    // Case 2: Multi-line array (value is empty, next lines start with "  - ")
    if raw_value.is_empty() {
        let mut items: Vec<String> = Vec::new();
        let mut consumed = 1usize;

        for j in (current_index + 1)..lines.len() {
            let next_line = lines[j];

            // ^\s+-\s*(.*)$
            if let Some(stripped) = next_line.trim_start().strip_prefix('-') {
                let item_value = parse_string_value(stripped.trim());
                if !item_value.is_empty() {
                    items.push(item_value);
                }
                consumed += 1;
                continue;
            }

            if next_line.trim().is_empty() {
                consumed += 1;
                continue;
            }

            break;
        }

        if !items.is_empty() {
            return (GlobValue::Multiple(items), consumed);
        }
    }

    // Case 3: Comma-separated patterns in single string
    let string_value = parse_string_value(raw_value);
    if string_value.contains(',') {
        let items: Vec<String> = string_value
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        return (GlobValue::Multiple(items), 1);
    }

    // Case 4: Single string value
    (GlobValue::Single(string_value), 1)
}

fn parse_inline_array(value: &str) -> Vec<String> {
    // Remove brackets
    let end = value.rfind(']').unwrap_or(value.len());
    let content = value.get(1..end).unwrap_or("").trim();
    if content.is_empty() {
        return Vec::new();
    }

    let mut items: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    let mut quote_char = '\0';

    for ch in content.chars() {
        if !in_quote && (ch == '"' || ch == '\'') {
            in_quote = true;
            quote_char = ch;
            continue;
        }

        if in_quote && ch == quote_char {
            in_quote = false;
            quote_char = '\0';
            continue;
        }

        if !in_quote && ch == ',' {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                items.push(parse_string_value(trimmed));
            }
            current.clear();
            continue;
        }

        current.push(ch);
    }

    let trimmed = current.trim();
    if !trimmed.is_empty() {
        items.push(parse_string_value(trimmed));
    }

    items
}

fn merge_globs(existing: Option<GlobValue>, new_value: GlobValue) -> GlobValue {
    let Some(existing) = existing else {
        return new_value;
    };

    let mut combined = existing.as_vec();
    combined.extend(new_value.as_vec());
    GlobValue::Multiple(combined)
}

// =============================================================================
// Glob Matcher
// =============================================================================

fn match_glob(pattern: &str, file_path: &str) -> bool {
    // Matches basic glob patterns used by the TS implementation.
    //
    // Important: `**/` should match *zero or more* directories, so `src/**/*.ts`
    // must match both `src/main.ts` and `src/a/b.ts`.
    let pattern = pattern.replace('\\', "/");

    // TS implementation only escapes '.'
    let mut regex_str = pattern.replace('.', "\\.");
    regex_str = regex_str.replace("**/", "<<<GLOBSTAR_DIR>>>");
    regex_str = regex_str.replace("**", "<<<GLOBSTAR>>>");
    regex_str = regex_str.replace('?', ".");
    regex_str = regex_str.replace('*', "[^/]*");
    regex_str = regex_str.replace("<<<GLOBSTAR_DIR>>>", "(.*/)?");
    regex_str = regex_str.replace("<<<GLOBSTAR>>>", ".*");

    let full = format!("^{}$", regex_str);
    let Ok(re) = Regex::new(&full) else {
        return false;
    };

    re.is_match(file_path)
}

pub fn should_apply_rule(
    metadata: &RuleMetadata,
    current_file_path: &str,
    project_root: Option<&str>,
) -> MatchResult {
    if metadata.always_apply == Some(true) {
        return MatchResult {
            applies: true,
            reason: Some("alwaysApply".to_string()),
        };
    }

    let Some(globs) = &metadata.globs else {
        return MatchResult {
            applies: false,
            reason: None,
        };
    };

    let patterns = globs.as_vec();
    if patterns.is_empty() {
        return MatchResult {
            applies: false,
            reason: None,
        };
    }

    let relative_path = if let Some(root) = project_root {
        relative_path_str(root, current_file_path)
    } else {
        current_file_path.to_string()
    };

    // Normalize path separators to forward slashes for matching
    let normalized_path = relative_path.replace('\\', "/");

    for pattern in patterns {
        if match_glob(&pattern, &normalized_path) {
            return MatchResult {
                applies: true,
                reason: Some(format!("glob: {}", pattern)),
            };
        }
    }

    MatchResult {
        applies: false,
        reason: None,
    }
}

pub fn is_duplicate_by_real_path(real_path: &str, cache: &HashSet<String>) -> bool {
    cache.contains(real_path)
}

pub fn is_duplicate_by_content_hash(hash: &str, cache: &HashSet<String>) -> bool {
    cache.contains(hash)
}

pub fn create_content_hash(content: &str) -> String {
    let digest = sha256_digest(content.as_bytes());
    let hex = to_hex(&digest);
    hex.chars().take(16).collect()
}

// =============================================================================
// Finder
// =============================================================================

fn is_github_instructions_dir(dir: &str) -> bool {
    let normalized = dir.replace('\\', "/");
    normalized.contains(".github/instructions") || normalized.ends_with(".github/instructions")
}

fn is_valid_rule_file(file_name: &str, dir: &str) -> bool {
    if is_github_instructions_dir(dir) {
        return GITHUB_INSTRUCTIONS_PATTERN.is_match(file_name);
    }
    RULE_EXTENSIONS.iter().any(|ext| file_name.ends_with(ext))
}

pub fn find_project_root(start_path: &str) -> Option<String> {
    let start = Path::new(start_path);
    let current = match fs::metadata(start) {
        Ok(m) if m.is_dir() => start.to_path_buf(),
        _ => start
            .parent()
            .unwrap_or_else(|| Path::new(start_path))
            .to_path_buf(),
    };

    let mut cur = current;

    loop {
        for marker in PROJECT_MARKERS {
            if cur.join(marker).exists() {
                return Some(cur.to_string_lossy().to_string());
            }
        }

        let parent = match cur.parent() {
            Some(p) => p.to_path_buf(),
            None => return None,
        };

        if parent == cur {
            return None;
        }
        cur = parent;
    }
}

fn find_rule_files_recursive(dir: &Path, results: &mut Vec<PathBuf>) {
    if !dir.exists() {
        return;
    }

    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else {
            continue;
        };

        if ft.is_dir() {
            find_rule_files_recursive(&path, results);
        } else if ft.is_file() {
            let file_name = match path.file_name().and_then(|s| s.to_str()) {
                Some(s) => s,
                None => continue,
            };
            let dir_str = dir.to_string_lossy();
            if is_valid_rule_file(file_name, &dir_str) {
                results.push(path);
            }
        }
    }
}

fn safe_realpath(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub fn calculate_distance(rule_path: &str, current_file: &str, project_root: Option<&str>) -> u32 {
    let Some(root) = project_root else {
        return 9999;
    };

    let root = Path::new(root);
    let rule_dir = Path::new(rule_path)
        .parent()
        .unwrap_or_else(|| Path::new(rule_path));
    let current_dir = Path::new(current_file)
        .parent()
        .unwrap_or_else(|| Path::new(current_file));

    let rule_rel = relative_path_buf(root, rule_dir);
    let current_rel = relative_path_buf(root, current_dir);

    if starts_with_parent(&rule_rel) || starts_with_parent(&current_rel) {
        return 9999;
    }

    let rule_rel_str = rule_rel.to_string_lossy().to_string();
    let current_rel_str = current_rel.to_string_lossy().to_string();

    let rule_parts: Vec<String> = rule_rel_str
        .split(|c| c == '/' || c == '\\')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();
    let current_parts: Vec<String> = current_rel_str
        .split(|c| c == '/' || c == '\\')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    let mut common = 0usize;
    for idx in 0..std::cmp::min(rule_parts.len(), current_parts.len()) {
        if rule_parts[idx] == current_parts[idx] {
            common += 1;
        } else {
            break;
        }
    }

    (current_parts.len().saturating_sub(common)) as u32
}

pub fn find_rule_files(
    project_root: Option<&str>,
    home_dir: &str,
    current_file: &str,
) -> Vec<RuleFileCandidate> {
    let project_root_path = project_root.map(PathBuf::from);
    let home = PathBuf::from(home_dir);
    let current_file_path = PathBuf::from(current_file);

    let mut candidates: Vec<RuleFileCandidate> = Vec::new();
    let mut seen_real_paths: HashSet<String> = HashSet::new();

    // Search from current file's directory up to project root
    let mut current_dir = current_file_path
        .parent()
        .unwrap_or_else(|| current_file_path.as_path())
        .to_path_buf();
    let mut distance: u32 = 0;

    loop {
        for (parent, subdir) in PROJECT_RULE_SUBDIRS {
            let rule_dir = current_dir.join(parent).join(subdir);
            let mut files: Vec<PathBuf> = Vec::new();
            find_rule_files_recursive(&rule_dir, &mut files);

            for file_path in files {
                let real_path = safe_realpath(&file_path);
                let real_str = real_path.to_string_lossy().to_string();
                if seen_real_paths.contains(&real_str) {
                    continue;
                }
                seen_real_paths.insert(real_str.clone());

                candidates.push(RuleFileCandidate {
                    path: file_path.to_string_lossy().to_string(),
                    real_path: real_str,
                    is_global: false,
                    distance,
                    is_single_file: None,
                });
            }
        }

        if let Some(root) = &project_root_path {
            if &current_dir == root {
                break;
            }
        }

        let Some(parent_dir) = current_dir.parent().map(|p| p.to_path_buf()) else {
            break;
        };
        if parent_dir == current_dir {
            break;
        }
        current_dir = parent_dir;
        distance += 1;
    }

    // Single-file rules at project root
    if let Some(root) = &project_root_path {
        for rule_file in PROJECT_RULE_FILES {
            let file_path = root.join(rule_file);
            if !file_path.exists() {
                continue;
            }

            let Ok(meta) = fs::metadata(&file_path) else {
                continue;
            };
            if !meta.is_file() {
                continue;
            }

            let real_path = safe_realpath(&file_path);
            let real_str = real_path.to_string_lossy().to_string();
            if seen_real_paths.contains(&real_str) {
                continue;
            }
            seen_real_paths.insert(real_str.clone());

            candidates.push(RuleFileCandidate {
                path: file_path.to_string_lossy().to_string(),
                real_path: real_str,
                is_global: false,
                distance: 0,
                is_single_file: Some(true),
            });
        }
    }

    // User-level rules (~/.claude/rules)
    let user_rule_dir = home.join(USER_RULE_DIR);
    let mut user_files: Vec<PathBuf> = Vec::new();
    find_rule_files_recursive(&user_rule_dir, &mut user_files);

    for file_path in user_files {
        let real_path = safe_realpath(&file_path);
        let real_str = real_path.to_string_lossy().to_string();
        if seen_real_paths.contains(&real_str) {
            continue;
        }
        seen_real_paths.insert(real_str.clone());

        candidates.push(RuleFileCandidate {
            path: file_path.to_string_lossy().to_string(),
            real_path: real_str,
            is_global: true,
            distance: 9999,
            is_single_file: None,
        });
    }

    // Sort by distance (closest first, then global last)
    candidates.sort_by(|a, b| {
        if a.is_global != b.is_global {
            return if a.is_global {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Less
            };
        }
        a.distance.cmp(&b.distance)
    });

    candidates
}

// =============================================================================
// Storage
// =============================================================================

fn rules_injector_storage_dir_for_home(home: &Path) -> PathBuf {
    home.join(".omc").join("rules-injector")
}

fn rules_injector_storage_dir() -> Option<PathBuf> {
    home_dir().map(|h| rules_injector_storage_dir_for_home(&h))
}

fn storage_path_for_session(home: &Path, session_id: &str) -> PathBuf {
    rules_injector_storage_dir_for_home(home).join(format!("{}.json", session_id))
}

pub fn load_injected_rules(session_id: &str) -> SessionCache {
    let Some(home) = home_dir() else {
        return SessionCache::default();
    };
    load_injected_rules_with_home(session_id, &home)
}

pub fn load_injected_rules_with_home(session_id: &str, home: &Path) -> SessionCache {
    let file_path = storage_path_for_session(home, session_id);
    if !file_path.exists() {
        return SessionCache::default();
    }

    let Ok(content) = fs::read_to_string(&file_path) else {
        return SessionCache::default();
    };
    let Ok(data) = serde_json::from_str::<InjectedRulesData>(&content) else {
        return SessionCache::default();
    };

    SessionCache {
        content_hashes: data.injected_hashes.into_iter().collect(),
        real_paths: data
            .injected_real_paths
            .unwrap_or_default()
            .into_iter()
            .collect(),
    }
}

pub fn save_injected_rules(session_id: &str, cache: &SessionCache) {
    let Some(home) = home_dir() else {
        return;
    };
    save_injected_rules_with_home(session_id, cache, &home);
}

pub fn save_injected_rules_with_home(session_id: &str, cache: &SessionCache, home: &Path) {
    let storage_dir = rules_injector_storage_dir_for_home(home);
    let _ = fs::create_dir_all(&storage_dir);

    let data = InjectedRulesData {
        session_id: session_id.to_string(),
        injected_hashes: cache.content_hashes.iter().cloned().collect(),
        injected_real_paths: Some(cache.real_paths.iter().cloned().collect()),
        updated_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
    };

    let Ok(content) = serde_json::to_string_pretty(&data) else {
        return;
    };

    let _ = fs::write(storage_path_for_session(home, session_id), content);
}

pub fn clear_injected_rules(session_id: &str) {
    let Some(home) = home_dir() else {
        return;
    };
    clear_injected_rules_with_home(session_id, &home);
}

pub fn clear_injected_rules_with_home(session_id: &str, home: &Path) {
    let file_path = storage_path_for_session(home, session_id);
    if file_path.exists() {
        let _ = fs::remove_file(&file_path);
    }
}

// =============================================================================
// Main Hook API
// =============================================================================

pub struct RulesInjectorHook {
    working_directory: PathBuf,
    session_caches: RwLock<HashMap<String, SessionCache>>,
}

impl RulesInjectorHook {
    pub fn new(working_directory: impl Into<PathBuf>) -> Self {
        Self {
            working_directory: working_directory.into(),
            session_caches: RwLock::new(HashMap::new()),
        }
    }

    fn resolve_file_path(&self, path: &str) -> Option<PathBuf> {
        if path.is_empty() {
            return None;
        }
        if path.starts_with('/') {
            return Some(PathBuf::from(path));
        }
        Some(self.working_directory.join(path))
    }

    fn process_file_path_for_rules(&self, file_path: &str, session_id: &str) -> Vec<RuleToInject> {
        let Some(resolved) = self.resolve_file_path(file_path) else {
            return Vec::new();
        };

        let resolved_str = resolved.to_string_lossy().to_string();
        let project_root = find_project_root(&resolved_str);
        let home = home_dir()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_default();

        let candidates = find_rule_files(project_root.as_deref(), &home, &resolved_str);

        let mut to_inject: Vec<RuleToInject> = Vec::new();

        // Ensure cache exists in memory
        let mut caches = self.session_caches.write().unwrap();
        let cache = caches
            .entry(session_id.to_string())
            .or_insert_with(|| load_injected_rules(session_id));

        for candidate in candidates {
            if is_duplicate_by_real_path(&candidate.real_path, &cache.real_paths) {
                continue;
            }

            let Ok(raw_content) = fs::read_to_string(&candidate.path) else {
                continue;
            };
            let RuleFrontmatterResult { metadata, body } = parse_rule_frontmatter(&raw_content);

            let match_reason = if candidate.is_single_file == Some(true) {
                "copilot-instructions (always apply)".to_string()
            } else {
                let match_result =
                    should_apply_rule(&metadata, &resolved_str, project_root.as_deref());
                if !match_result.applies {
                    continue;
                }
                match_result.reason.unwrap_or_else(|| "matched".to_string())
            };

            let content_hash = create_content_hash(&body);
            if is_duplicate_by_content_hash(&content_hash, &cache.content_hashes) {
                continue;
            }

            let relative_path = if let Some(root) = project_root.as_deref() {
                relative_path_str(root, &candidate.path)
            } else {
                candidate.path.clone()
            };

            to_inject.push(RuleToInject {
                relative_path,
                match_reason,
                content: body,
                distance: candidate.distance,
            });

            cache.real_paths.insert(candidate.real_path);
            cache.content_hashes.insert(content_hash);
        }

        if !to_inject.is_empty() {
            to_inject.sort_by_key(|r| r.distance);
            save_injected_rules(session_id, cache);
        }

        to_inject
    }

    fn format_rules_for_injection(rules: &[RuleToInject]) -> String {
        if rules.is_empty() {
            return String::new();
        }

        let mut output = String::new();
        for rule in rules {
            output.push_str("\n\n[Rule: ");
            output.push_str(&rule.relative_path);
            output.push_str("]\n[Match: ");
            output.push_str(&rule.match_reason);
            output.push_str("]\n");
            output.push_str(&rule.content);
        }
        output
    }

    pub fn process_tool_execution(
        &self,
        tool_name: &str,
        file_path: &str,
        session_id: &str,
    ) -> String {
        if !Self::is_tracked_tool(tool_name) {
            return String::new();
        }

        let rules = self.process_file_path_for_rules(file_path, session_id);
        Self::format_rules_for_injection(&rules)
    }

    pub fn get_rules_for_file(&self, file_path: &str) -> Vec<RuleToInject> {
        let Some(resolved) = self.resolve_file_path(file_path) else {
            return Vec::new();
        };

        let resolved_str = resolved.to_string_lossy().to_string();
        let project_root = find_project_root(&resolved_str);
        let home = home_dir()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_default();

        let candidates = find_rule_files(project_root.as_deref(), &home, &resolved_str);
        let mut rules: Vec<RuleToInject> = Vec::new();

        for candidate in candidates {
            let Ok(raw_content) = fs::read_to_string(&candidate.path) else {
                continue;
            };
            let RuleFrontmatterResult { metadata, body } = parse_rule_frontmatter(&raw_content);

            let match_reason = if candidate.is_single_file == Some(true) {
                "copilot-instructions (always apply)".to_string()
            } else {
                let match_result =
                    should_apply_rule(&metadata, &resolved_str, project_root.as_deref());
                if !match_result.applies {
                    continue;
                }
                match_result.reason.unwrap_or_else(|| "matched".to_string())
            };

            let relative_path = if let Some(root) = project_root.as_deref() {
                relative_path_str(root, &candidate.path)
            } else {
                candidate.path.clone()
            };

            rules.push(RuleToInject {
                relative_path,
                match_reason,
                content: body,
                distance: candidate.distance,
            });
        }

        rules.sort_by_key(|r| r.distance);
        rules
    }

    pub fn clear_session(&self, session_id: &str) {
        if let Ok(mut caches) = self.session_caches.write() {
            caches.remove(session_id);
        }
        clear_injected_rules(session_id);
    }

    pub fn is_tracked_tool(tool_name: &str) -> bool {
        TRACKED_TOOLS.iter().any(|t| *t == tool_name.to_lowercase())
    }
}

#[async_trait]
impl Hook for RulesInjectorHook {
    fn name(&self) -> &str {
        "rules-injector"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::SessionStart]
    }

    async fn execute(
        &self,
        _event: HookEvent,
        _input: &HookInput,
        _context: &HookContext,
    ) -> HookResult {
        // Rules injection happens at session start
        Ok(HookOutput::pass())
    }
}

impl Default for RulesInjectorHook {
    fn default() -> Self {
        Self::new(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    }
}

pub fn get_rules_for_path(file_path: &str, working_directory: Option<&str>) -> Vec<RuleToInject> {
    let cwd = working_directory
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let hook = RulesInjectorHook::new(cwd);
    hook.get_rules_for_file(file_path)
}

// =============================================================================
// Path utils (minimal Node.js path.relative equivalent)
// =============================================================================

fn relative_path_buf(from: &Path, to: &Path) -> PathBuf {
    // If `from` or `to` isn't absolute, keep behavior consistent enough for our use:
    // treat as normal path components.
    let from_components: Vec<_> = from.components().collect();
    let to_components: Vec<_> = to.components().collect();

    let mut common = 0usize;
    for idx in 0..std::cmp::min(from_components.len(), to_components.len()) {
        if from_components[idx] == to_components[idx] {
            common += 1;
        } else {
            break;
        }
    }

    let mut out = PathBuf::new();
    for _ in common..from_components.len() {
        out.push("..");
    }
    for comp in &to_components[common..] {
        out.push(comp.as_os_str());
    }

    if out.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        out
    }
}

fn relative_path_str(from: &str, to: &str) -> String {
    let rel = relative_path_buf(Path::new(from), Path::new(to));
    rel.to_string_lossy().to_string()
}

fn starts_with_parent(path: &Path) -> bool {
    path.components()
        .next()
        .map(|c| c.as_os_str() == "..")
        .unwrap_or(false)
}

// =============================================================================
// SHA-256 (no extra dependency; matches Node crypto createHash('sha256'))
// =============================================================================

// Minimal SHA-256 implementation for deterministic content hashing.
// Returns raw 32-byte digest.
fn sha256_digest(input: &[u8]) -> [u8; 32] {
    const H0: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let mut h = H0;

    // Pre-processing: padding
    let bit_len = (input.len() as u64) * 8;
    let mut msg = input.to_vec();
    msg.push(0x80);

    while (msg.len() % 64) != 56 {
        msg.push(0x00);
    }

    msg.extend_from_slice(&bit_len.to_be_bytes());

    // Process each 512-bit chunk
    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, word) in w.iter_mut().take(16).enumerate() {
            let start = i * 4;
            *word = u32::from_be_bytes([
                chunk[start],
                chunk[start + 1],
                chunk[start + 2],
                chunk[start + 3],
            ]);
        }

        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut hh = h[7];

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_parse_rule_frontmatter_no_frontmatter() {
        let content = "Hello";
        let res = parse_rule_frontmatter(content);
        assert_eq!(res.body, "Hello");
        assert!(res.metadata.description.is_none());
        assert!(res.metadata.globs.is_none());
    }

    #[test]
    fn test_parse_rule_frontmatter_inline_array_and_merge_paths() {
        let content = r#"---
description: 'Test'
globs: ["src/**/*.ts", 'lib/**/*.js']
paths: "tests/**/*.rs"
---
BODY"#;

        let res = parse_rule_frontmatter(content);
        assert_eq!(res.body.trim(), "BODY");
        assert_eq!(res.metadata.description, Some("Test".to_string()));

        let globs = res.metadata.globs.unwrap().as_vec();
        assert_eq!(globs.len(), 3);
        assert_eq!(globs[0], "src/**/*.ts");
        assert_eq!(globs[1], "lib/**/*.js");
        assert_eq!(globs[2], "tests/**/*.rs");
    }

    #[test]
    fn test_parse_rule_frontmatter_multiline_array() {
        let content = r#"---
globs:
  - "src/**/*.ts"
  - 'lib/**/*.js'
---
X"#;
        let res = parse_rule_frontmatter(content);
        let globs = res.metadata.globs.unwrap().as_vec();
        assert_eq!(
            globs,
            vec!["src/**/*.ts".to_string(), "lib/**/*.js".to_string()]
        );
    }

    #[test]
    fn test_should_apply_rule_glob_match() {
        let metadata = RuleMetadata {
            globs: Some(GlobValue::Single("src/**/*.ts".to_string())),
            ..Default::default()
        };

        let project = "/project";
        // NOTE: This matches the TypeScript implementation's naive glob->regex conversion.
        // Pattern "src/**/*.ts" does NOT match "src/main.ts" (requires an extra "/").
        let current = "/project/src/foo/main.ts";

        let rel = super::relative_path_str(project, current);
        assert_eq!(rel, "src/foo/main.ts");
        assert!(super::match_glob("src/**/*.ts", &rel));

        let res = should_apply_rule(&metadata, current, Some(project));
        assert!(res.applies);
        assert_eq!(res.reason, Some("glob: src/**/*.ts".to_string()));
    }

    #[test]
    fn test_find_project_root_marker() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "").unwrap();

        let nested = dir.path().join("src").join("lib");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("file.rs"), "").unwrap();

        let root = find_project_root(nested.join("file.rs").to_str().unwrap());
        assert_eq!(root, Some(dir.path().to_string_lossy().to_string()));
    }

    #[test]
    fn test_find_rule_files_project_and_user() {
        let project = tempdir().unwrap();
        fs::write(project.path().join("package.json"), "{}").unwrap();
        let current_file = project.path().join("src").join("main.ts");
        fs::create_dir_all(current_file.parent().unwrap()).unwrap();
        fs::write(&current_file, "").unwrap();

        // Project rule: .claude/rules
        let proj_rule_dir = project.path().join(".claude").join("rules");
        fs::create_dir_all(&proj_rule_dir).unwrap();
        fs::write(
            proj_rule_dir.join("a.mdc"),
            "---\nalwaysApply: true\n---\nA",
        )
        .unwrap();

        // GitHub instructions
        let gh_dir = project.path().join(".github").join("instructions");
        fs::create_dir_all(&gh_dir).unwrap();
        fs::write(gh_dir.join("b.instructions.md"), "---\n---\nB").unwrap();

        // User rules
        let home = tempdir().unwrap();
        let user_rule_dir = home.path().join(".claude").join("rules");
        fs::create_dir_all(&user_rule_dir).unwrap();
        fs::write(user_rule_dir.join("c.md"), "---\n---\nC").unwrap();

        let root = find_project_root(current_file.to_str().unwrap());
        let candidates = find_rule_files(
            root.as_deref(),
            home.path().to_str().unwrap(),
            current_file.to_str().unwrap(),
        );

        assert!(candidates.iter().any(|c| c.path.ends_with("a.mdc")));
        assert!(candidates
            .iter()
            .any(|c| c.path.ends_with("b.instructions.md")));
        assert!(candidates
            .iter()
            .any(|c| c.path.ends_with("c.md") && c.is_global));
    }

    #[test]
    fn test_storage_roundtrip_with_home_override() {
        let home = tempdir().unwrap();
        let session_id = "s1";

        let cache = SessionCache {
            content_hashes: ["abcd".to_string()].into_iter().collect(),
            real_paths: ["/x".to_string()].into_iter().collect(),
        };

        save_injected_rules_with_home(session_id, &cache, home.path());
        let loaded = load_injected_rules_with_home(session_id, home.path());
        assert!(loaded.content_hashes.contains("abcd"));
        assert!(loaded.real_paths.contains("/x"));

        clear_injected_rules_with_home(session_id, home.path());
        let loaded2 = load_injected_rules_with_home(session_id, home.path());
        assert!(loaded2.content_hashes.is_empty());
        assert!(loaded2.real_paths.is_empty());
    }

    #[test]
    fn test_create_content_hash_matches_expected_prefix() {
        // Deterministic, lowercase hex.
        let h = create_content_hash("hello");
        assert_eq!(h.len(), 16);
        assert!(h
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }
}
