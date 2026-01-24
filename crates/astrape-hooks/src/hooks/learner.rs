//! Learner Hook (ported from TypeScript)
//!
//! Extracts reusable skills from sessions and injects matching skills into context.
//! This is a single-file Rust port of:
//! `oh-my-claudecode/src/hooks/learner/*`.
//!
//! Key requirements:
//! - Skill file parsing/writing with YAML frontmatter (custom parser; no YAML libs)
//! - Storage:
//!   - Config: ~/.claude/omc/learner.json
//!   - User skills: ~/.claude/skills/omc-learned/
//!   - Project skills: .omc/skills/
//!
//! Notes:
//! - This module intentionally implements a *minimal* YAML frontmatter parser supporting:
//!   - string scalars
//!   - integer scalars
//!   - string arrays (inline: ["a", "b"], or multi-line with `- item`)
//! - No external YAML libraries are used.

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
// Constants (ported from constants.ts)
// =============================================================================

/// Project-level skills subdirectory.
pub const PROJECT_SKILLS_SUBDIR: &str = ".omc/skills";

/// Valid skill file extension.
pub const SKILL_EXTENSION: &str = ".md";

/// Feature flag key for enabling/disabling.
pub const FEATURE_FLAG_KEY: &str = "learner.enabled";

/// Default feature flag value.
pub const FEATURE_FLAG_DEFAULT: bool = true;

/// Maximum skill content length (characters).
pub const MAX_SKILL_CONTENT_LENGTH: usize = 4000;

/// Minimum quality score for auto-injection.
pub const MIN_QUALITY_SCORE: i32 = 50;

/// Required metadata fields.
pub const REQUIRED_METADATA_FIELDS: &[&str] = &["id", "name", "description", "triggers", "source"];

/// Maximum skills to inject per session.
pub const MAX_SKILLS_PER_SESSION: usize = 10;

fn debug_enabled() -> bool {
    std::env::var("OMC_DEBUG").ok().as_deref() == Some("1")
}

/// Default config path: `~/.claude/omc/learner.json`.
/// Test override: `OMC_LEARNER_CONFIG_PATH=/tmp/.../learner.json`.
fn config_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("OMC_LEARNER_CONFIG_PATH") {
        return Some(PathBuf::from(p));
    }
    home_dir().map(|h| h.join(".claude").join("omc").join("learner.json"))
}

/// Default user skills dir: `~/.claude/skills/omc-learned`.
/// Test override: `OMC_LEARNER_USER_SKILLS_DIR=/tmp/.../omc-learned`.
fn user_skills_dir() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("OMC_LEARNER_USER_SKILLS_DIR") {
        return Some(PathBuf::from(p));
    }
    home_dir().map(|h| h.join(".claude").join("skills").join("omc-learned"))
}

// =============================================================================
// Types (ported from types.ts)
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkillSource {
    Extracted,
    Promoted,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMetadata {
    pub id: String,
    pub name: String,
    pub description: String,
    pub triggers: Vec<String>,
    #[serde(rename = "createdAt")]
    pub created_at: String,

    pub source: SkillSource,

    #[serde(rename = "sessionId", skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<i32>,

    #[serde(rename = "usageCount", skip_serializing_if = "Option::is_none")]
    pub usage_count: Option<i32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillScope {
    User,
    Project,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnedSkill {
    pub path: String,
    #[serde(rename = "relativePath")]
    pub relative_path: String,
    pub scope: SkillScope,
    pub metadata: SkillMetadata,
    pub content: String,
    #[serde(rename = "contentHash")]
    pub content_hash: String,
    pub priority: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFileCandidate {
    pub path: String,
    #[serde(rename = "realPath")]
    pub real_path: String,
    pub scope: SkillScope,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityValidation {
    pub valid: bool,
    #[serde(rename = "missingFields")]
    pub missing_fields: Vec<String>,
    pub warnings: Vec<String>,
    pub score: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillExtractionRequest {
    pub problem: String,
    pub solution: String,
    pub triggers: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(rename = "targetScope")]
    pub target_scope: SkillScope,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InjectedSkillsData {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "injectedHashes")]
    pub injected_hashes: Vec<String>,
    #[serde(rename = "updatedAt")]
    pub updated_at: u64,
}

// =============================================================================
// Config (ported from config.ts)
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnerConfig {
    pub enabled: bool,
    pub detection: DetectionConfig,
    pub quality: QualityConfig,
    pub storage: StorageConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionConfig {
    pub enabled: bool,
    #[serde(rename = "promptThreshold")]
    pub prompt_threshold: i32,
    #[serde(rename = "promptCooldown")]
    pub prompt_cooldown: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityConfig {
    #[serde(rename = "minScore")]
    pub min_score: i32,
    #[serde(rename = "minProblemLength")]
    pub min_problem_length: usize,
    #[serde(rename = "minSolutionLength")]
    pub min_solution_length: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    #[serde(rename = "maxSkillsPerScope")]
    pub max_skills_per_scope: usize,
    #[serde(rename = "autoPrune")]
    pub auto_prune: bool,
    #[serde(rename = "pruneDays")]
    pub prune_days: i32,
}

impl Default for LearnerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            detection: DetectionConfig {
                enabled: true,
                prompt_threshold: 60,
                prompt_cooldown: 5,
            },
            quality: QualityConfig {
                min_score: 50,
                min_problem_length: 10,
                min_solution_length: 20,
            },
            storage: StorageConfig {
                max_skills_per_scope: 100,
                auto_prune: false,
                prune_days: 90,
            },
        }
    }
}

/// Load configuration from disk.
pub fn load_config() -> LearnerConfig {
    let Some(path) = config_path() else {
        return LearnerConfig::default();
    };
    if !path.exists() {
        return LearnerConfig::default();
    }

    let Ok(content) = fs::read_to_string(&path) else {
        return LearnerConfig::default();
    };

    let Ok(loaded) = serde_json::from_str::<serde_json::Value>(&content) else {
        if debug_enabled() {
            eprintln!("[learner] Error parsing config JSON");
        }
        return LearnerConfig::default();
    };

    merge_config(LearnerConfig::default(), loaded)
}

/// Save configuration to disk. Accepts partial updates.
pub fn save_config(partial: serde_json::Value) -> bool {
    let Some(path) = config_path() else {
        return false;
    };

    let merged = merge_config(LearnerConfig::default(), partial);
    let Some(parent) = path.parent() else {
        return false;
    };
    if fs::create_dir_all(parent).is_err() {
        return false;
    }

    match serde_json::to_string_pretty(&merged) {
        Ok(content) => fs::write(path, content).is_ok(),
        Err(_) => false,
    }
}

fn merge_config(defaults: LearnerConfig, loaded: serde_json::Value) -> LearnerConfig {
    // Mirrors TS mergeConfig() behavior.
    let enabled = loaded
        .get("enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(defaults.enabled);

    let detection = {
        let d = loaded.get("detection");
        DetectionConfig {
            enabled: d
                .and_then(|v| v.get("enabled"))
                .and_then(|v| v.as_bool())
                .unwrap_or(defaults.detection.enabled),
            prompt_threshold: d
                .and_then(|v| v.get("promptThreshold"))
                .and_then(|v| v.as_i64())
                .map(|n| n as i32)
                .unwrap_or(defaults.detection.prompt_threshold),
            prompt_cooldown: d
                .and_then(|v| v.get("promptCooldown"))
                .and_then(|v| v.as_i64())
                .map(|n| n as i32)
                .unwrap_or(defaults.detection.prompt_cooldown),
        }
    };

    let quality = {
        let q = loaded.get("quality");
        QualityConfig {
            min_score: q
                .and_then(|v| v.get("minScore"))
                .and_then(|v| v.as_i64())
                .map(|n| n as i32)
                .unwrap_or(defaults.quality.min_score),
            min_problem_length: q
                .and_then(|v| v.get("minProblemLength"))
                .and_then(|v| v.as_i64())
                .map(|n| n as usize)
                .unwrap_or(defaults.quality.min_problem_length),
            min_solution_length: q
                .and_then(|v| v.get("minSolutionLength"))
                .and_then(|v| v.as_i64())
                .map(|n| n as usize)
                .unwrap_or(defaults.quality.min_solution_length),
        }
    };

    let storage = {
        let s = loaded.get("storage");
        StorageConfig {
            max_skills_per_scope: s
                .and_then(|v| v.get("maxSkillsPerScope"))
                .and_then(|v| v.as_i64())
                .map(|n| n as usize)
                .unwrap_or(defaults.storage.max_skills_per_scope),
            auto_prune: s
                .and_then(|v| v.get("autoPrune"))
                .and_then(|v| v.as_bool())
                .unwrap_or(defaults.storage.auto_prune),
            prune_days: s
                .and_then(|v| v.get("pruneDays"))
                .and_then(|v| v.as_i64())
                .map(|n| n as i32)
                .unwrap_or(defaults.storage.prune_days),
        }
    };

    LearnerConfig {
        enabled,
        detection,
        quality,
        storage,
    }
}

/// Convenience: check if learner is enabled (from config).
pub fn is_learner_enabled() -> bool {
    load_config().enabled
}

// =============================================================================
// Parser (ported from parser.ts)
// =============================================================================

#[derive(Debug, Clone, Default)]
pub struct SkillParseResult {
    pub metadata: PartialSkillMetadata,
    pub content: String,
    pub valid: bool,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PartialSkillMetadata {
    pub id: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub triggers: Option<Vec<String>>,
    pub created_at: Option<String>,
    pub source: Option<SkillSource>,
    pub session_id: Option<String>,
    pub quality: Option<i32>,
    pub usage_count: Option<i32>,
    pub tags: Option<Vec<String>>,
}

lazy_static! {
    // TS regex: /^---\r?\n([\s\S]*?)\r?\n---\r?\n?([\s\S]*)$/
    static ref FRONTMATTER_RE: Regex = Regex::new(r"(?s)^---\r?\n(.*?)\r?\n---\r?\n?(.*)$").unwrap();
}

pub fn parse_skill_file(raw_content: &str) -> SkillParseResult {
    let Some(caps) = FRONTMATTER_RE.captures(raw_content) else {
        return SkillParseResult {
            metadata: PartialSkillMetadata::default(),
            content: raw_content.to_string(),
            valid: false,
            errors: vec!["Missing YAML frontmatter".to_string()],
        };
    };

    let yaml_content = caps.get(1).map(|m| m.as_str()).unwrap_or("");
    let content = caps
        .get(2)
        .map(|m| m.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    let mut errors: Vec<String> = Vec::new();
    let metadata = parse_yaml_metadata(yaml_content);

    if metadata.id.as_deref().unwrap_or("").is_empty() {
        errors.push("Missing required field: id".to_string());
    }
    if metadata.name.as_deref().unwrap_or("").is_empty() {
        errors.push("Missing required field: name".to_string());
    }
    if metadata.description.as_deref().unwrap_or("").is_empty() {
        errors.push("Missing required field: description".to_string());
    }
    if metadata
        .triggers
        .as_ref()
        .map(|t| t.is_empty())
        .unwrap_or(true)
    {
        errors.push("Missing required field: triggers".to_string());
    }
    if metadata.source.is_none() {
        errors.push("Missing required field: source".to_string());
    }

    SkillParseResult {
        metadata,
        content,
        valid: errors.is_empty(),
        errors,
    }
}

fn parse_yaml_metadata(yaml_content: &str) -> PartialSkillMetadata {
    let lines: Vec<&str> = yaml_content.split('\n').collect();
    let mut metadata = PartialSkillMetadata::default();

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
            "id" => metadata.id = Some(parse_string_value(raw_value)),
            "name" => metadata.name = Some(parse_string_value(raw_value)),
            "description" => metadata.description = Some(parse_string_value(raw_value)),
            "source" => {
                let v = parse_string_value(raw_value);
                metadata.source = match v.as_str() {
                    "extracted" => Some(SkillSource::Extracted),
                    "promoted" => Some(SkillSource::Promoted),
                    "manual" => Some(SkillSource::Manual),
                    _ => None,
                };
            }
            "createdAt" => metadata.created_at = Some(parse_string_value(raw_value)),
            "sessionId" => metadata.session_id = Some(parse_string_value(raw_value)),
            "quality" => metadata.quality = raw_value.parse::<i32>().ok(),
            "usageCount" => {
                metadata.usage_count = raw_value.parse::<i32>().ok().or(Some(0));
            }
            "triggers" => {
                let (arr, consumed) = parse_string_array_value(raw_value, &lines, i);
                metadata.triggers = Some(arr);
                i += consumed;
                continue;
            }
            "tags" => {
                let (arr, consumed) = parse_string_array_value(raw_value, &lines, i);
                metadata.tags = Some(arr);
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

fn parse_string_array_value(
    raw_value: &str,
    lines: &[&str],
    current_index: usize,
) -> (Vec<String>, usize) {
    // Inline array: ["a", "b"]
    if raw_value.starts_with('[') {
        return (parse_inline_array(raw_value), 1);
    }

    // Multi-line array (raw is empty, next lines start with `- `)
    if raw_value.is_empty() {
        let mut items: Vec<String> = Vec::new();
        let mut consumed = 1usize;

        for next_line in lines.iter().skip(current_index + 1) {
            if let Some(stripped) = next_line.trim_start().strip_prefix('-') {
                let item = parse_string_value(stripped.trim());
                if !item.is_empty() {
                    items.push(item);
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
            return (items, consumed);
        }
    }

    // Single value -> wrap
    let v = parse_string_value(raw_value);
    if v.is_empty() {
        (Vec::new(), 1)
    } else {
        (vec![v], 1)
    }
}

fn parse_inline_array(value: &str) -> Vec<String> {
    // Remove [ ... ] (best-effort)
    let end = value.rfind(']').unwrap_or(value.len());
    let content = value.get(1..end).unwrap_or("").trim();
    if content.is_empty() {
        return Vec::new();
    }

    // Split on commas, but respect quotes.
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

    items.into_iter().filter(|s| !s.is_empty()).collect()
}

pub fn generate_skill_frontmatter(metadata: &SkillMetadata) -> String {
    let mut lines: Vec<String> = Vec::new();

    lines.push("---".to_string());
    lines.push(format!("id: \"{}\"", metadata.id));
    lines.push(format!("name: \"{}\"", metadata.name));
    lines.push(format!("description: \"{}\"", metadata.description));
    lines.push(format!(
        "source: {}",
        match metadata.source {
            SkillSource::Extracted => "extracted",
            SkillSource::Promoted => "promoted",
            SkillSource::Manual => "manual",
        }
    ));
    lines.push(format!("createdAt: \"{}\"", metadata.created_at));

    if let Some(session_id) = &metadata.session_id {
        lines.push(format!("sessionId: \"{}\"", session_id));
    }
    if let Some(q) = metadata.quality {
        lines.push(format!("quality: {}", q));
    }
    if let Some(uc) = metadata.usage_count {
        lines.push(format!("usageCount: {}", uc));
    }

    lines.push("triggers:".to_string());
    for trigger in &metadata.triggers {
        lines.push(format!("  - \"{}\"", trigger));
    }

    if let Some(tags) = &metadata.tags {
        if !tags.is_empty() {
            lines.push("tags:".to_string());
            for tag in tags {
                lines.push(format!("  - \"{}\"", tag));
            }
        }
    }

    lines.push("---".to_string());
    lines.join("\n")
}

// =============================================================================
// Finder (ported from finder.ts)
// =============================================================================

fn find_skill_files_recursive(dir: &Path, results: &mut Vec<PathBuf>) {
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
            find_skill_files_recursive(&path, results);
        } else if ft.is_file() {
            let Some(file_name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if file_name.ends_with(SKILL_EXTENSION) {
                results.push(path);
            }
        }
    }
}

fn safe_realpath(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub fn find_skill_files(project_root: Option<&str>) -> Vec<SkillFileCandidate> {
    let mut candidates: Vec<SkillFileCandidate> = Vec::new();
    let mut seen_real_paths: HashSet<String> = HashSet::new();

    // 1. Project scope (higher priority)
    if let Some(root) = project_root {
        let project_skills_dir = Path::new(root).join(PROJECT_SKILLS_SUBDIR);
        let mut project_files: Vec<PathBuf> = Vec::new();
        find_skill_files_recursive(&project_skills_dir, &mut project_files);
        for file_path in project_files {
            let real = safe_realpath(&file_path);
            let real_str = real.to_string_lossy().to_string();
            if seen_real_paths.contains(&real_str) {
                continue;
            }
            seen_real_paths.insert(real_str.clone());
            candidates.push(SkillFileCandidate {
                path: file_path.to_string_lossy().to_string(),
                real_path: real_str,
                scope: SkillScope::Project,
            });
        }
    }

    // 2. User scope (lower priority)
    let Some(user_dir) = user_skills_dir() else {
        return candidates;
    };
    let mut user_files: Vec<PathBuf> = Vec::new();
    find_skill_files_recursive(&user_dir, &mut user_files);
    for file_path in user_files {
        let real = safe_realpath(&file_path);
        let real_str = real.to_string_lossy().to_string();
        if seen_real_paths.contains(&real_str) {
            continue;
        }
        seen_real_paths.insert(real_str.clone());
        candidates.push(SkillFileCandidate {
            path: file_path.to_string_lossy().to_string(),
            real_path: real_str,
            scope: SkillScope::User,
        });
    }

    candidates
}

pub fn get_skills_dir(scope: SkillScope, project_root: Option<&str>) -> Result<PathBuf, String> {
    match scope {
        SkillScope::User => {
            user_skills_dir().ok_or_else(|| "Failed to resolve home dir".to_string())
        }
        SkillScope::Project => {
            let Some(root) = project_root else {
                return Err("Project root required for project scope".to_string());
            };
            Ok(Path::new(root).join(PROJECT_SKILLS_SUBDIR))
        }
    }
}

pub fn ensure_skills_dir(scope: SkillScope, project_root: Option<&str>) -> bool {
    let Ok(dir) = get_skills_dir(scope, project_root) else {
        return false;
    };
    if dir.exists() {
        return true;
    }
    if let Err(e) = fs::create_dir_all(&dir) {
        if debug_enabled() {
            eprintln!("[learner] Error creating skills directory: {}", e);
        }
        return false;
    }
    true
}

// =============================================================================
// Loader (ported from loader.ts)
// =============================================================================

#[derive(Debug, Clone, Default)]
struct LoaderCache {
    // key: project_root string, empty => None
    skills_by_root: HashMap<String, Vec<LearnedSkill>>,
}

lazy_static! {
    static ref LOADER_CACHE: RwLock<LoaderCache> = RwLock::new(LoaderCache::default());
}

pub fn clear_loader_cache() {
    if let Ok(mut c) = LOADER_CACHE.write() {
        c.skills_by_root.clear();
    }
}

pub fn load_all_skills(project_root: Option<&str>) -> Vec<LearnedSkill> {
    load_all_skills_cached(project_root, false)
}

pub fn load_all_skills_cached(project_root: Option<&str>, force_reload: bool) -> Vec<LearnedSkill> {
    let key = project_root.unwrap_or("").to_string();

    if !force_reload {
        if let Ok(cache) = LOADER_CACHE.read() {
            if let Some(existing) = cache.skills_by_root.get(&key) {
                return existing.clone();
            }
        }
    }

    let candidates = find_skill_files(project_root);
    let mut seen_ids: HashMap<String, LearnedSkill> = HashMap::new();

    for candidate in candidates {
        let raw = match fs::read_to_string(&candidate.path) {
            Ok(c) => c,
            Err(e) => {
                if debug_enabled() {
                    eprintln!("[learner] Error reading skill {}: {}", candidate.path, e);
                }
                continue;
            }
        };

        let parsed = parse_skill_file(&raw);
        if !parsed.valid {
            if debug_enabled() {
                eprintln!(
                    "[learner] Invalid skill file {}: {}",
                    candidate.path,
                    parsed.errors.join(", ")
                );
            }
            continue;
        }

        let Some(raw_skill_id) = parsed.metadata.id.clone() else {
            continue;
        };

        // Normalize to ensure project/user duplicates reliably collide (handles stray whitespace/CR).
        let skill_id = raw_skill_id
            .trim()
            .trim_end_matches('\r')
            .trim_matches(|c| c == '"' || c == '\'')
            .trim()
            .to_string();
        if skill_id.is_empty() {
            continue;
        }

        let Ok(skills_dir) = get_skills_dir(candidate.scope.clone(), project_root) else {
            continue;
        };

        let relative_path = relative_path_str(&skills_dir, Path::new(&candidate.path));

        let Some(mut skill_metadata) = partial_to_skill_metadata(&parsed.metadata) else {
            continue;
        };
        skill_metadata.id = skill_id.clone();

        let skill = LearnedSkill {
            path: candidate.path.clone(),
            relative_path,
            scope: candidate.scope.clone(),
            metadata: skill_metadata,
            content: parsed.content.clone(),
            content_hash: create_content_hash(&parsed.content),
            priority: match candidate.scope {
                SkillScope::Project => 1,
                SkillScope::User => 0,
            },
        };

        let existing = seen_ids.get(&skill_id).cloned();
        if existing.is_none() || skill.priority > existing.unwrap().priority {
            seen_ids.insert(skill_id, skill);
        }
    }

    let mut out: Vec<LearnedSkill> = seen_ids.into_values().collect();
    out.sort_by(|a, b| b.priority.cmp(&a.priority));

    if let Ok(mut cache) = LOADER_CACHE.write() {
        cache.skills_by_root.insert(key, out.clone());
    }

    out
}

pub fn load_skill_by_id(skill_id: &str, project_root: Option<&str>) -> Option<LearnedSkill> {
    load_all_skills(project_root)
        .into_iter()
        .find(|s| s.metadata.id == skill_id)
}

pub fn find_matching_skills(
    message: &str,
    project_root: Option<&str>,
    limit: usize,
) -> Vec<LearnedSkill> {
    let skills = load_all_skills(project_root);
    let message_lower = message.to_lowercase();

    let mut scored: Vec<(LearnedSkill, i32)> = Vec::new();
    for skill in skills {
        let mut score: i32 = 0;
        let mut has_match = false;

        for trigger in &skill.metadata.triggers {
            if message_lower.contains(&trigger.to_lowercase()) {
                score += 10;
                has_match = true;
            }
        }

        if let Some(tags) = &skill.metadata.tags {
            for tag in tags {
                if message_lower.contains(&tag.to_lowercase()) {
                    score += 5;
                    has_match = true;
                }
            }
        }

        if has_match {
            if let Some(q) = skill.metadata.quality {
                score += q / 20;
            }
            if let Some(uc) = skill.metadata.usage_count {
                score += std::cmp::min(uc, 10);
            }
        }

        scored.push((skill, score));
    }

    scored.sort_by(|a, b| b.1.cmp(&a.1));
    scored
        .into_iter()
        .filter(|(_, score)| *score > 0)
        .take(limit)
        .map(|(skill, _)| skill)
        .collect()
}

fn partial_to_skill_metadata(p: &PartialSkillMetadata) -> Option<SkillMetadata> {
    Some(SkillMetadata {
        id: p.id.clone()?,
        name: p.name.clone()?,
        description: p.description.clone()?,
        triggers: p.triggers.clone()?,
        created_at: p.created_at.clone().unwrap_or_default(),
        source: p.source.clone()?,
        session_id: p.session_id.clone(),
        quality: p.quality,
        usage_count: p.usage_count,
        tags: p.tags.clone(),
    })
}

// =============================================================================
// Validator (ported from validator.ts)
// =============================================================================

pub fn validate_extraction_request(request: &SkillExtractionRequest) -> QualityValidation {
    let mut missing_fields: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut score: i32 = 100;

    if request.problem.trim().len() < 10 {
        missing_fields.push("problem (minimum 10 characters)".to_string());
        score -= 30;
    }
    if request.solution.trim().len() < 20 {
        missing_fields.push("solution (minimum 20 characters)".to_string());
        score -= 30;
    }
    if request.triggers.is_empty() {
        missing_fields.push("triggers (at least one required)".to_string());
        score -= 20;
    }

    let total_len = request.problem.len() + request.solution.len();
    if total_len > MAX_SKILL_CONTENT_LENGTH {
        warnings.push(format!(
            "Content exceeds {} chars ({}). Consider condensing.",
            MAX_SKILL_CONTENT_LENGTH, total_len
        ));
        score -= 10;
    }

    let short_triggers: Vec<String> = request
        .triggers
        .iter()
        .filter(|t| t.len() < 3)
        .cloned()
        .collect();
    if !short_triggers.is_empty() {
        warnings.push(format!(
            "Short triggers may cause false matches: {}",
            short_triggers.join(", ")
        ));
        score -= 5;
    }

    let generic: [&str; 8] = ["the", "a", "an", "this", "that", "it", "is", "are"];
    let found_generic: Vec<String> = request
        .triggers
        .iter()
        .filter(|t| generic.iter().any(|g| g.eq_ignore_ascii_case(t)))
        .cloned()
        .collect();
    if !found_generic.is_empty() {
        warnings.push(format!(
            "Generic triggers should be avoided: {}",
            found_generic.join(", ")
        ));
        score -= 10;
    }

    score = std::cmp::max(0, score);
    QualityValidation {
        valid: missing_fields.is_empty() && score >= MIN_QUALITY_SCORE,
        missing_fields,
        warnings,
        score,
    }
}

pub fn validate_skill_metadata(metadata: &PartialSkillMetadata) -> QualityValidation {
    let mut missing_fields: Vec<String> = Vec::new();
    let warnings: Vec<String> = Vec::new();
    let mut score: i32 = 100;

    for field in REQUIRED_METADATA_FIELDS {
        let present = match *field {
            "id" => metadata.id.as_ref().map(|s| !s.is_empty()).unwrap_or(false),
            "name" => metadata
                .name
                .as_ref()
                .map(|s| !s.is_empty())
                .unwrap_or(false),
            "description" => metadata
                .description
                .as_ref()
                .map(|s| !s.is_empty())
                .unwrap_or(false),
            "triggers" => metadata
                .triggers
                .as_ref()
                .map(|v| !v.is_empty())
                .unwrap_or(false),
            "source" => metadata.source.is_some(),
            _ => false,
        };
        if !present {
            missing_fields.push((*field).to_string());
            score -= 15;
        }
    }

    // Note: Empty triggers are already penalized as "missing" in the loop above.
    // We don't apply a separate penalty here to avoid double-counting.

    // Source already validated during parsing; keep this for parity.
    score = std::cmp::max(0, score);
    QualityValidation {
        valid: missing_fields.is_empty() && score >= MIN_QUALITY_SCORE,
        missing_fields,
        warnings,
        score,
    }
}

// =============================================================================
// Writer (ported from writer.ts)
// =============================================================================

#[derive(Debug, Clone)]
pub struct WriteSkillResult {
    pub success: bool,
    pub path: Option<String>,
    pub error: Option<String>,
    pub validation: QualityValidation,
}

fn base36(mut n: u64) -> String {
    const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if n == 0 {
        return "0".to_string();
    }
    let mut out: Vec<u8> = Vec::new();
    while n > 0 {
        let rem = (n % 36) as usize;
        out.push(DIGITS[rem]);
        n /= 36;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_else(|_| "0".to_string())
}

fn generate_skill_id() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let ts = now.as_millis() as u64;
    let salt = now.subsec_nanos() as u64;
    format!("skill-{}-{}", base36(ts), base36(salt % (36_u64.pow(4))))
}

fn sanitize_filename(name: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;

    for ch in name.chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            out.push(lower);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
        if out.len() >= 50 {
            break;
        }
    }

    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "skill".to_string()
    } else {
        trimmed
    }
}

pub fn write_skill(
    request: &SkillExtractionRequest,
    project_root: Option<&str>,
    skill_name: &str,
) -> WriteSkillResult {
    let validation = validate_extraction_request(request);
    if !validation.valid {
        return WriteSkillResult {
            success: false,
            path: None,
            error: Some(format!(
                "Quality validation failed: {}",
                validation.missing_fields.join(", ")
            )),
            validation,
        };
    }

    if !ensure_skills_dir(request.target_scope.clone(), project_root) {
        return WriteSkillResult {
            success: false,
            path: None,
            error: Some(format!(
                "Failed to create skills directory for scope: {:?}",
                request.target_scope
            )),
            validation,
        };
    }

    let now_iso = chrono::Utc::now().to_rfc3339();
    let metadata = SkillMetadata {
        id: generate_skill_id(),
        name: skill_name.to_string(),
        description: request.problem.chars().take(200).collect(),
        source: SkillSource::Extracted,
        created_at: now_iso,
        triggers: request.triggers.clone(),
        tags: request.tags.clone(),
        quality: Some(validation.score),
        usage_count: Some(0),
        session_id: None,
    };

    let frontmatter = generate_skill_frontmatter(&metadata);
    let content = format!(
        "{}\n\n# Problem\n\n{}\n\n# Solution\n\n{}\n",
        frontmatter, request.problem, request.solution
    );

    let filename = format!("{}.md", sanitize_filename(skill_name));
    let Ok(skills_dir) = get_skills_dir(request.target_scope.clone(), project_root) else {
        return WriteSkillResult {
            success: false,
            path: None,
            error: Some("Failed to resolve skills dir".to_string()),
            validation,
        };
    };
    let file_path = skills_dir.join(filename);
    if file_path.exists() {
        return WriteSkillResult {
            success: false,
            path: None,
            error: Some(format!(
                "Skill file already exists: {}",
                file_path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("<unknown>")
            )),
            validation,
        };
    }

    match fs::write(&file_path, content) {
        Ok(_) => {
            // Skill list changed.
            clear_loader_cache();
            WriteSkillResult {
                success: true,
                path: Some(file_path.to_string_lossy().to_string()),
                error: None,
                validation,
            }
        }
        Err(e) => {
            if debug_enabled() {
                eprintln!("[learner] Error writing skill file: {}", e);
            }
            WriteSkillResult {
                success: false,
                path: None,
                error: Some(format!("Failed to write skill file: {}", e)),
                validation,
            }
        }
    }
}

pub fn check_duplicate_triggers(
    triggers: &[String],
    project_root: Option<&str>,
) -> (bool, Option<String>) {
    let skills = load_all_skills(project_root);
    let normalized: HashSet<String> = triggers.iter().map(|t| t.to_lowercase()).collect();
    if normalized.is_empty() {
        return (false, None);
    }

    for skill in skills {
        let skill_triggers: Vec<String> = skill
            .metadata
            .triggers
            .iter()
            .map(|t| t.to_lowercase())
            .collect();
        let overlap = skill_triggers
            .iter()
            .filter(|t| normalized.contains(*t))
            .count();
        if overlap * 2 >= normalized.len() {
            return (true, Some(skill.metadata.id));
        }
    }

    (false, None)
}

// =============================================================================
// Detector (ported from detector.ts)
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PatternType {
    #[serde(rename = "problem-solution")]
    ProblemSolution,
    Technique,
    Workaround,
    Optimization,
    #[serde(rename = "best-practice")]
    BestPractice,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionResult {
    pub detected: bool,
    pub confidence: i32,
    #[serde(rename = "patternType")]
    pub pattern_type: PatternType,
    #[serde(rename = "suggestedTriggers")]
    pub suggested_triggers: Vec<String>,
    pub reason: String,
}

struct PatternGroup {
    pattern_type: PatternType,
    patterns: Vec<Regex>,
    confidence: i32,
}

lazy_static! {
    static ref DETECTION_GROUPS: Vec<PatternGroup> = vec![
        PatternGroup {
            pattern_type: PatternType::ProblemSolution,
            patterns: vec![
                Regex::new(r"(?i)the (?:issue|problem|bug|error) was (?:caused by|due to|because)").unwrap(),
                // Allow a short subject between "the/this" and the method marker (e.g. "fixed the bug by ...").
                Regex::new(r"(?i)(?:fixed|resolved|solved)\s+(?:the|this)\s+\w+\s+(?:by|with|using)").unwrap(),
                Regex::new(r"(?i)the (?:solution|fix|answer) (?:is|was) to").unwrap(),
                Regex::new(r"(?i)(?:here's|here is) (?:how|what) (?:to|you need to)").unwrap(),
            ],
            confidence: 80,
        },
        PatternGroup {
            pattern_type: PatternType::Technique,
            patterns: vec![
                Regex::new(r"(?i)(?:a|the) (?:better|good|proper|correct) (?:way|approach|method) (?:is|to)").unwrap(),
                Regex::new(r"(?i)(?:you should|we should|it's better to) (?:always|never|usually)").unwrap(),
                Regex::new(r"(?i)(?:the trick|the key|the secret) (?:is|here is)").unwrap(),
            ],
            confidence: 70,
        },
        PatternGroup {
            pattern_type: PatternType::Workaround,
            patterns: vec![
                Regex::new(r"(?i)(?:as a|for a) workaround").unwrap(),
                Regex::new(r"(?i)(?:temporarily|for now|until).*(?:you can|we can)").unwrap(),
                Regex::new(r"(?i)(?:hack|trick) (?:to|for|that)").unwrap(),
            ],
            confidence: 60,
        },
        PatternGroup {
            pattern_type: PatternType::Optimization,
            patterns: vec![
                Regex::new(r"(?i)(?:to|for) (?:better|improved|faster) performance").unwrap(),
                Regex::new(r"(?i)(?:optimize|optimizing|optimization) (?:by|with|using)").unwrap(),
                Regex::new(r"(?i)(?:more efficient|efficiently) (?:by|to|if)").unwrap(),
            ],
            confidence: 65,
        },
        PatternGroup {
            pattern_type: PatternType::BestPractice,
            patterns: vec![
                Regex::new(r"(?i)(?:best practice|best practices) (?:is|are|include)").unwrap(),
                Regex::new(r"(?i)(?:recommended|standard|common) (?:approach|pattern|practice)").unwrap(),
                Regex::new(r"(?i)(?:you should always|always make sure to)").unwrap(),
            ],
            confidence: 75,
        },
    ];

    static ref TRIGGER_KEYWORDS: Vec<&'static str> = vec![
        "react",
        "typescript",
        "javascript",
        "python",
        "rust",
        "go",
        "node",
        "api",
        "database",
        "sql",
        "graphql",
        "rest",
        "authentication",
        "authorization",
        "testing",
        "debugging",
        "deployment",
        "docker",
        "kubernetes",
        "ci/cd",
        "git",
        "webpack",
        "vite",
        "eslint",
        "prettier",
        "error handling",
        "state management",
        "performance",
        "optimization",
        "refactoring",
        "migration",
        "integration",
        "configuration",
        "pattern",
        "architecture",
        "design",
        "structure",
        "convention",
    ];
}

pub fn detect_extractable_moment(
    assistant_message: &str,
    user_message: Option<&str>,
) -> DetectionResult {
    let combined = format!("{} {}", user_message.unwrap_or(""), assistant_message).to_lowercase();

    let mut best: Option<(PatternType, i32, String)> = None;

    for group in DETECTION_GROUPS.iter() {
        for pattern in &group.patterns {
            if pattern.is_match(assistant_message)
                && best
                    .as_ref()
                    .map(|(_, c, _)| group.confidence > *c)
                    .unwrap_or(true)
            {
                best = Some((
                    group.pattern_type.clone(),
                    group.confidence,
                    format!("Detected {:?} pattern", group.pattern_type),
                ));
            }
        }
    }

    let Some((pattern_type, base_confidence, reason)) = best else {
        return DetectionResult {
            detected: false,
            confidence: 0,
            pattern_type: PatternType::ProblemSolution,
            suggested_triggers: Vec::new(),
            reason: "No extractable pattern detected".to_string(),
        };
    };

    let mut suggested: Vec<String> = Vec::new();
    for kw in TRIGGER_KEYWORDS.iter() {
        if combined.contains(&kw.to_lowercase()) {
            suggested.push((*kw).to_string());
        }
    }

    let trigger_boost = std::cmp::min((suggested.len() as i32) * 5, 15);
    let final_confidence = std::cmp::min(base_confidence + trigger_boost, 100);

    DetectionResult {
        detected: true,
        confidence: final_confidence,
        pattern_type,
        suggested_triggers: suggested.into_iter().take(5).collect(),
        reason,
    }
}

pub fn should_prompt_extraction(detection: &DetectionResult, threshold: i32) -> bool {
    detection.detected && detection.confidence >= threshold
}

pub fn generate_extraction_prompt(detection: &DetectionResult) -> String {
    let type_desc = match detection.pattern_type {
        PatternType::ProblemSolution => "a problem and its solution",
        PatternType::Technique => "a useful technique",
        PatternType::Workaround => "a workaround for a limitation",
        PatternType::Optimization => "an optimization approach",
        PatternType::BestPractice => "a best practice",
    };

    format!(
        "I noticed this conversation contains {} that might be worth saving as a reusable skill.\n\n**Confidence:** {}%\n**Suggested triggers:** {}\n\nWould you like me to extract this as a learned skill? Type `/oh-my-claudecode:learner` to save it, or continue with your current task.",
        type_desc,
        detection.confidence,
        if detection.suggested_triggers.is_empty() {
            "None detected".to_string()
        } else {
            detection.suggested_triggers.join(", ")
        }
    )
}

// =============================================================================
// Detection Hook integration (ported from detection-hook.ts)
// =============================================================================

#[derive(Debug, Clone)]
pub struct DetectionStats {
    pub messages_since_prompt: i32,
    pub prompted_count: i32,
    pub last_detection: Option<DetectionResult>,
}

#[derive(Debug, Clone, Default)]
struct SessionDetectionState {
    messages_since_prompt: i32,
    last_detection: Option<DetectionResult>,
    prompted_count: i32,
}

lazy_static! {
    static ref DETECTION_SESSION_STATES: RwLock<HashMap<String, SessionDetectionState>> =
        RwLock::new(HashMap::new());
}

fn get_detection_session_state(session_id: &str) -> SessionDetectionState {
    if let Ok(state) = DETECTION_SESSION_STATES.read() {
        if let Some(existing) = state.get(session_id) {
            return existing.clone();
        }
    }

    let fresh = SessionDetectionState::default();
    if let Ok(mut state) = DETECTION_SESSION_STATES.write() {
        state.insert(session_id.to_string(), fresh.clone());
    }
    fresh
}

fn set_detection_session_state(session_id: &str, st: SessionDetectionState) {
    if let Ok(mut state) = DETECTION_SESSION_STATES.write() {
        state.insert(session_id.to_string(), st);
    }
}

pub fn process_response_for_detection(
    assistant_message: &str,
    user_message: Option<&str>,
    session_id: &str,
    config_override: Option<&DetectionConfig>,
) -> Option<String> {
    let cfg = config_override
        .cloned()
        .unwrap_or_else(|| load_config().detection);
    if !cfg.enabled || !is_learner_enabled() {
        return None;
    }

    let mut state = get_detection_session_state(session_id);
    state.messages_since_prompt += 1;

    if state.messages_since_prompt < cfg.prompt_cooldown {
        set_detection_session_state(session_id, state);
        return None;
    }

    let detection = detect_extractable_moment(assistant_message, user_message);
    state.last_detection = Some(detection.clone());

    if should_prompt_extraction(&detection, cfg.prompt_threshold) {
        state.messages_since_prompt = 0;
        state.prompted_count += 1;
        set_detection_session_state(session_id, state);
        return Some(generate_extraction_prompt(&detection));
    }

    set_detection_session_state(session_id, state);
    None
}

pub fn get_last_detection(session_id: &str) -> Option<DetectionResult> {
    DETECTION_SESSION_STATES
        .read()
        .ok()
        .and_then(|m| m.get(session_id).and_then(|s| s.last_detection.clone()))
}

pub fn clear_detection_state(session_id: &str) {
    if let Ok(mut m) = DETECTION_SESSION_STATES.write() {
        m.remove(session_id);
    }
}

pub fn get_detection_stats(session_id: &str) -> DetectionStats {
    let st = DETECTION_SESSION_STATES
        .read()
        .ok()
        .and_then(|m| m.get(session_id).cloned())
        .unwrap_or_default();
    DetectionStats {
        messages_since_prompt: st.messages_since_prompt,
        prompted_count: st.prompted_count,
        last_detection: st.last_detection,
    }
}

// =============================================================================
// Promotion (ported from promotion.ts)
// =============================================================================

#[derive(Debug, Clone)]
pub struct PromotionCandidate {
    pub learning: String,
    pub story_id: String,
    pub timestamp: String,
    pub suggested_triggers: Vec<String>,
}

// Minimal port of `hooks/ralph/progress.ts` just for promotion needs.

#[derive(Debug, Clone, Default)]
struct ProgressEntry {
    timestamp: String,
    story_id: String,
    learnings: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct ProgressLog {
    entries: Vec<ProgressEntry>,
}

const PROGRESS_FILENAME: &str = "progress.txt";
const ENTRY_SEPARATOR: &str = "---";

fn get_progress_paths(directory: &str) -> (PathBuf, PathBuf) {
    let dir = Path::new(directory);
    (
        dir.join(PROGRESS_FILENAME),
        dir.join(".omc").join(PROGRESS_FILENAME),
    )
}

fn read_progress_raw(directory: &str) -> Option<String> {
    let (root, omc) = get_progress_paths(directory);
    if root.exists() {
        return fs::read_to_string(root).ok();
    }
    if omc.exists() {
        return fs::read_to_string(omc).ok();
    }
    None
}

fn parse_progress(content: &str) -> ProgressLog {
    let lines: Vec<&str> = content.split('\n').collect();
    let mut entries: Vec<ProgressEntry> = Vec::new();

    let mut current: Option<ProgressEntry> = None;
    let mut current_section = String::new();
    let entry_re = Regex::new(r"^##\s*\[(.+?)\]\s*-\s*(.+)$").unwrap();

    for line in lines {
        let trimmed = line.trim();
        if trimmed == ENTRY_SEPARATOR {
            if let Some(e) = current.take() {
                if !e.story_id.is_empty() {
                    entries.push(e);
                }
            }
            current_section.clear();
            continue;
        }

        // ## [Date] - [Story]
        if let Some(caps) = entry_re.captures(trimmed) {
            if let Some(e) = current.take() {
                if !e.story_id.is_empty() {
                    entries.push(e);
                }
            }
            current = Some(ProgressEntry {
                timestamp: caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string(),
                story_id: caps.get(2).map(|m| m.as_str()).unwrap_or("").to_string(),
                learnings: Vec::new(),
            });
            current_section.clear();
            continue;
        }

        if let Some(ref mut e) = current {
            let lower = trimmed.to_lowercase();
            if lower.contains("learnings") {
                current_section = "learnings".to_string();
                continue;
            }
            if trimmed.starts_with('-') || trimmed.starts_with('*') {
                let item = trimmed[1..].trim();
                if item.is_empty() {
                    continue;
                }
                if current_section == "learnings" {
                    e.learnings.push(item.to_string());
                }
            }
        }
    }

    if let Some(e) = current {
        if !e.story_id.is_empty() {
            entries.push(e);
        }
    }

    ProgressLog { entries }
}

fn read_progress(directory: &str) -> Option<ProgressLog> {
    let content = read_progress_raw(directory)?;
    Some(parse_progress(&content))
}

fn extract_triggers(text: &str) -> Vec<String> {
    let technical_keywords = [
        "react",
        "typescript",
        "javascript",
        "python",
        "api",
        "database",
        "testing",
        "debugging",
        "performance",
        "async",
        "state",
        "component",
        "error",
        "validation",
        "authentication",
        "cache",
        "query",
        "mutation",
    ];

    let lower = text.to_lowercase();
    technical_keywords
        .iter()
        .filter(|kw| lower.contains(*kw))
        .map(|kw| (*kw).to_string())
        .collect()
}

pub fn get_promotion_candidates(directory: &str, limit: usize) -> Vec<PromotionCandidate> {
    let Some(progress) = read_progress(directory) else {
        return Vec::new();
    };

    let recent_entries = if progress.entries.len() > limit {
        progress.entries[progress.entries.len() - limit..].to_vec()
    } else {
        progress.entries
    };

    let mut candidates: Vec<PromotionCandidate> = Vec::new();
    for entry in recent_entries {
        for learning in entry.learnings {
            if learning.len() < 20 {
                continue;
            }
            candidates.push(PromotionCandidate {
                learning: learning.clone(),
                story_id: entry.story_id.clone(),
                timestamp: entry.timestamp.clone(),
                suggested_triggers: extract_triggers(&learning),
            });
        }
    }

    candidates.sort_by(|a, b| b.suggested_triggers.len().cmp(&a.suggested_triggers.len()));
    candidates
}

pub fn promote_learning(
    candidate: &PromotionCandidate,
    skill_name: &str,
    additional_triggers: &[String],
    target_scope: SkillScope,
    project_root: Option<&str>,
) -> WriteSkillResult {
    let mut triggers: Vec<String> = Vec::new();
    triggers.extend(candidate.suggested_triggers.clone());
    triggers.extend(additional_triggers.to_vec());
    let unique: HashSet<String> = triggers.into_iter().collect();

    let request = SkillExtractionRequest {
        problem: format!(
            "Learning from {}: {}...",
            candidate.story_id,
            candidate.learning.chars().take(100).collect::<String>()
        ),
        solution: candidate.learning.clone(),
        triggers: unique.into_iter().collect(),
        tags: None,
        target_scope,
    };

    write_skill(&request, project_root, skill_name)
}

pub fn list_promotable_learnings(directory: &str) -> String {
    let candidates = get_promotion_candidates(directory, 10);
    if candidates.is_empty() {
        return "No promotion candidates found in ralph-progress learnings.".to_string();
    }

    let mut lines: Vec<String> = vec![
        "# Promotion Candidates".to_string(),
        "".to_string(),
        "The following learnings from ralph-progress could be promoted to skills:".to_string(),
        "".to_string(),
    ];

    for (idx, c) in candidates.iter().enumerate() {
        lines.push(format!(
            "## {}. From {} ({})",
            idx + 1,
            c.story_id,
            c.timestamp
        ));
        lines.push("".to_string());
        lines.push(c.learning.clone());
        lines.push("".to_string());
        if !c.suggested_triggers.is_empty() {
            lines.push(format!(
                "**Suggested triggers:** {}",
                c.suggested_triggers.join(", ")
            ));
        }
        lines.push("".to_string());
        lines.push("---".to_string());
        lines.push("".to_string());
    }

    lines.join("\n")
}

// =============================================================================
// Public API / Hook Integration (ported from index.ts)
// =============================================================================

fn format_skills_for_context(skills: &[LearnedSkill]) -> String {
    if skills.is_empty() {
        return String::new();
    }

    let mut lines: Vec<String> = vec![
        "<learner>".to_string(),
        "".to_string(),
        "## Relevant Learned Skills".to_string(),
        "".to_string(),
        "The following skills have been learned from previous sessions and may be helpful:"
            .to_string(),
        "".to_string(),
    ];

    for skill in skills {
        lines.push(format!("### {}", skill.metadata.name));
        lines.push(format!(
            "**Triggers:** {}",
            skill.metadata.triggers.join(", ")
        ));
        if let Some(tags) = &skill.metadata.tags {
            if !tags.is_empty() {
                lines.push(format!("**Tags:** {}", tags.join(", ")));
            }
        }
        lines.push("".to_string());
        lines.push(skill.content.clone());
        lines.push("".to_string());
        lines.push("---".to_string());
        lines.push("".to_string());
    }

    lines.push("</learner>".to_string());
    lines.join("\n")
}

#[derive(Debug, Clone)]
pub struct SkillInjectionResult {
    pub injected: usize,
    pub skills: Vec<LearnedSkill>,
    /// Rendered `<learner>` block for injection.
    pub content: String,
}

pub struct LearnerHook {
    session_caches: RwLock<HashMap<String, HashSet<String>>>,
}

impl LearnerHook {
    pub fn new() -> Self {
        Self {
            session_caches: RwLock::new(HashMap::new()),
        }
    }

    fn get_or_create_session_cache(&self, session_id: &str) -> HashSet<String> {
        if let Ok(caches) = self.session_caches.read() {
            if let Some(existing) = caches.get(session_id) {
                return existing.clone();
            }
        }

        let fresh: HashSet<String> = HashSet::new();
        if let Ok(mut caches) = self.session_caches.write() {
            caches.insert(session_id.to_string(), fresh.clone());
        }
        fresh
    }

    fn set_session_cache(&self, session_id: &str, cache: HashSet<String>) {
        if let Ok(mut caches) = self.session_caches.write() {
            caches.insert(session_id.to_string(), cache);
        }
    }

    pub fn process_message_for_skills(
        &self,
        message: &str,
        session_id: &str,
        project_root: Option<&str>,
    ) -> SkillInjectionResult {
        if !is_learner_enabled() {
            return SkillInjectionResult {
                injected: 0,
                skills: Vec::new(),
                content: String::new(),
            };
        }

        let mut injected_hashes = self.get_or_create_session_cache(session_id);
        let matching = find_matching_skills(message, project_root, MAX_SKILLS_PER_SESSION);
        let new_skills: Vec<LearnedSkill> = matching
            .into_iter()
            .filter(|s| !injected_hashes.contains(&s.content_hash))
            .collect();

        if new_skills.is_empty() {
            return SkillInjectionResult {
                injected: 0,
                skills: Vec::new(),
                content: String::new(),
            };
        }

        for s in &new_skills {
            injected_hashes.insert(s.content_hash.clone());
        }
        self.set_session_cache(session_id, injected_hashes);

        let content = format_skills_for_context(&new_skills);
        SkillInjectionResult {
            injected: new_skills.len(),
            skills: new_skills,
            content,
        }
    }

    pub fn clear_skill_session(&self, session_id: &str) {
        if let Ok(mut caches) = self.session_caches.write() {
            caches.remove(session_id);
        }
    }

    pub fn get_all_skills(&self, project_root: Option<&str>) -> Vec<LearnedSkill> {
        load_all_skills(project_root)
    }
}

impl Default for LearnerHook {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Path utils (minimal `path.relative` equivalent)
// =============================================================================

fn relative_path_buf(from: &Path, to: &Path) -> PathBuf {
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

fn relative_path_str(from: &Path, to: &Path) -> String {
    if let Ok(stripped) = to.strip_prefix(from) {
        return stripped.to_string_lossy().to_string();
    }
    relative_path_buf(from, to).to_string_lossy().to_string()
}

// =============================================================================
// SHA-256 (no extra dependency; matches Node crypto createHash('sha256'))
// =============================================================================

pub fn create_content_hash(content: &str) -> String {
    let digest = sha256_digest(content.as_bytes());
    let hex = to_hex(&digest);
    hex.chars().take(16).collect()
}

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

    let bit_len = (input.len() as u64) * 8;
    let mut msg = input.to_vec();
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0x00);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

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
// Hook trait implementation
// =============================================================================

#[async_trait]
impl Hook for LearnerHook {
    fn name(&self) -> &str {
        "learner"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::UserPromptSubmit]
    }

    async fn execute(
        &self,
        _event: HookEvent,
        input: &HookInput,
        _context: &HookContext,
    ) -> HookResult {
        // Only process if learner is enabled
        if !is_learner_enabled() {
            return Ok(HookOutput::pass());
        }

        // Get prompt
        let prompt = match &input.prompt {
            Some(p) => p,
            None => return Ok(HookOutput::pass()),
        };

        // Check for learner-related keywords
        let keywords = ["extract skill", "/learner", "learn skill"];
        let is_learner_request = keywords.iter().any(|k| prompt.to_lowercase().contains(k));

        if is_learner_request {
            // Return message indicating learner is active
            return Ok(HookOutput::continue_with_message(
                "Learner hook detected a skill extraction request.",
            ));
        }

        Ok(HookOutput::pass())
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_parse_skill_file_requires_frontmatter() {
        let res = parse_skill_file("hello");
        assert!(!res.valid);
        assert!(res.errors.iter().any(|e| e.contains("frontmatter")));
    }

    #[test]
    fn test_parse_skill_file_inline_and_multiline_arrays() {
        let raw = r#"---
id: "skill-1"
name: "Test"
description: "Desc"
source: extracted
createdAt: "2025-01-01T00:00:00Z"
triggers: ["a", 'b']
tags:
  - "t1"
  - 't2'
---

Body"#;

        let res = parse_skill_file(raw);
        assert!(res.valid);
        assert_eq!(res.metadata.id.as_deref(), Some("skill-1"));
        assert_eq!(res.metadata.triggers.clone().unwrap(), vec!["a", "b"]);
        assert_eq!(res.metadata.tags.clone().unwrap(), vec!["t1", "t2"]);
        assert_eq!(res.content, "Body");
    }

    #[test]
    fn test_generate_frontmatter_roundtrip_like() {
        let meta = SkillMetadata {
            id: "skill-x".to_string(),
            name: "Name".to_string(),
            description: "Desc".to_string(),
            triggers: vec!["a".to_string(), "b".to_string()],
            created_at: "2025-01-01T00:00:00Z".to_string(),
            source: SkillSource::Manual,
            session_id: Some("s".to_string()),
            quality: Some(80),
            usage_count: Some(2),
            tags: Some(vec!["t".to_string()]),
        };

        let fm = generate_skill_frontmatter(&meta);
        assert!(fm.contains("id:"));
        assert!(fm.contains("triggers:"));
        assert!(fm.contains("tags:"));
    }

    #[test]
    fn test_validate_extraction_request() {
        let req = SkillExtractionRequest {
            problem: "short".to_string(),
            solution: "also short".to_string(),
            triggers: vec![],
            tags: None,
            target_scope: SkillScope::User,
        };
        let v = validate_extraction_request(&req);
        assert!(!v.valid);
        assert!(v.score < 50);
    }

    #[test]
    fn test_find_and_load_skills_project_overrides_user() {
        let project = tempdir().unwrap();
        let user = tempdir().unwrap();

        std::env::set_var("OMC_LEARNER_USER_SKILLS_DIR", user.path());

        let project_skills_dir = project.path().join(PROJECT_SKILLS_SUBDIR);
        fs::create_dir_all(&project_skills_dir).unwrap();

        // Same id in both scopes; project should win.
        let user_skill = user.path().join("x.md");
        let project_skill = project_skills_dir.join("x.md");

        let user_raw = r#"---
id: "dup"
name: "User"
description: "D"
source: manual
createdAt: "2025-01-01"
triggers:
  - "hello"
---

User content"#;
        let project_raw = r#"---
id: "dup"
name: "Project"
description: "D"
source: manual
createdAt: "2025-01-01"
triggers:
  - "hello"
---

Project content"#;

        fs::write(&user_skill, user_raw).unwrap();
        fs::write(&project_skill, project_raw).unwrap();

        clear_loader_cache();
        let skills = load_all_skills(Some(project.path().to_str().unwrap()));
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].metadata.name, "Project");

        std::env::remove_var("OMC_LEARNER_USER_SKILLS_DIR");
    }

    #[test]
    fn test_find_matching_skills_scoring() {
        let project = tempdir().unwrap();
        let user = tempdir().unwrap();
        std::env::set_var("OMC_LEARNER_USER_SKILLS_DIR", user.path());

        let s1 = user.path().join("s1.md");
        let raw = r#"---
id: "s1"
name: "S1"
description: "D"
source: manual
createdAt: "2025-01-01"
quality: 80
usageCount: 3
triggers:
  - "rust"
tags:
  - "api"
---

X"#;
        fs::write(&s1, raw).unwrap();

        clear_loader_cache();
        let matches = find_matching_skills(
            "Need help with Rust API",
            Some(project.path().to_str().unwrap()),
            5,
        );
        assert_eq!(matches.len(), 1);

        std::env::remove_var("OMC_LEARNER_USER_SKILLS_DIR");
    }

    #[test]
    fn test_write_skill_and_reload() {
        let project = tempdir().unwrap();
        let user = tempdir().unwrap();
        std::env::set_var("OMC_LEARNER_USER_SKILLS_DIR", user.path());

        let req = SkillExtractionRequest {
            problem: "This is a real problem description".to_string(),
            solution: "This is a sufficiently long solution for the problem".to_string(),
            triggers: vec!["rust".to_string()],
            tags: None,
            target_scope: SkillScope::User,
        };

        let res = write_skill(&req, Some(project.path().to_str().unwrap()), "My Skill");
        assert!(res.success);
        let path = res.path.unwrap();
        assert!(Path::new(&path).exists());

        let skills = load_all_skills(Some(project.path().to_str().unwrap()));
        assert_eq!(skills.len(), 1);

        std::env::remove_var("OMC_LEARNER_USER_SKILLS_DIR");
    }

    #[test]
    fn test_detection_hook_cooldown() {
        let cfg_dir = tempfile::tempdir().unwrap();
        std::env::set_var(
            "OMC_LEARNER_CONFIG_PATH",
            cfg_dir.path().join("learner.json"),
        );

        clear_detection_state("s");
        // Force enabled without touching config on disk.
        let cfg = DetectionConfig {
            enabled: true,
            prompt_threshold: 0,
            prompt_cooldown: 2,
        };

        // First message: increment, but still in cooldown.
        let r1 = process_response_for_detection("fixed this by using X", None, "s", Some(&cfg));
        assert!(r1.is_none());

        // Second message: should prompt.
        let r2 = process_response_for_detection("fixed this by using X", None, "s", Some(&cfg));
        assert!(r2.is_some());

        std::env::remove_var("OMC_LEARNER_CONFIG_PATH");
    }

    #[test]
    fn test_promotion_candidates_from_progress() {
        let dir = tempdir().unwrap();
        let progress = dir.path().join(".omc").join(PROGRESS_FILENAME);
        fs::create_dir_all(progress.parent().unwrap()).unwrap();
        fs::write(
            &progress,
            r#"# Ralph Progress Log
Started: 2025-01-01

## Codebase Patterns
- Something

---

## [2025-01-01 12:00] - US-001

 **Learnings for future iterations:**
 - Use TypeScript for validation

---
"#,
        )
        .unwrap();

        let candidates = get_promotion_candidates(dir.path().to_str().unwrap(), 10);
        assert_eq!(candidates.len(), 1);
        assert!(candidates[0]
            .suggested_triggers
            .contains(&"typescript".to_string()));
    }
}
