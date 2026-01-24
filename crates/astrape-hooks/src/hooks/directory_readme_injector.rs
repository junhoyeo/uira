use async_trait::async_trait;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::RwLock;

use crate::hook::{Hook, HookContext, HookResult};
use crate::types::{HookEvent, HookInput, HookOutput};

pub const README_FILENAME: &str = "README.md";

/// Tools that trigger README injection.
///
/// Matches oh-my-claudecode/src/hooks/directory-readme-injector/constants.ts
pub const TRACKED_TOOLS: [&str; 4] = ["read", "write", "edit", "multiedit"];

const CHARS_PER_TOKEN: usize = 4;
const DEFAULT_MAX_README_TOKENS: usize = 5000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InjectedPathsData {
    #[serde(rename = "sessionID")]
    pub session_id: String,
    #[serde(rename = "injectedPaths")]
    pub injected_paths: Vec<String>,
    #[serde(rename = "updatedAt")]
    pub updated_at: u64,
}

fn readme_injector_storage_dir() -> Option<PathBuf> {
    // Test-only override to avoid mutating process-wide HOME in parallel test runs.
    if let Ok(dir) = std::env::var("ASTRAPE_README_INJECTOR_STORAGE_DIR") {
        return Some(PathBuf::from(dir));
    }

    dirs::home_dir().map(|h| h.join(".astrape").join("directory-readme"))
}

fn get_storage_path(session_id: &str) -> Option<PathBuf> {
    Some(readme_injector_storage_dir()?.join(format!("{}.json", session_id)))
}

pub fn load_injected_paths(session_id: &str) -> HashSet<String> {
    let Some(file_path) = get_storage_path(session_id) else {
        return HashSet::new();
    };

    if !file_path.exists() {
        return HashSet::new();
    }

    let Ok(content) = fs::read_to_string(file_path) else {
        return HashSet::new();
    };

    let Ok(data) = serde_json::from_str::<InjectedPathsData>(&content) else {
        return HashSet::new();
    };

    data.injected_paths.into_iter().collect()
}

pub fn save_injected_paths(session_id: &str, paths: &HashSet<String>) {
    let Some(storage_dir) = readme_injector_storage_dir() else {
        return;
    };

    let _ = fs::create_dir_all(&storage_dir);

    let Some(file_path) = get_storage_path(session_id) else {
        return;
    };

    let data = InjectedPathsData {
        session_id: session_id.to_string(),
        injected_paths: paths.iter().cloned().collect(),
        updated_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
    };

    if let Ok(content) = serde_json::to_string_pretty(&data) {
        let _ = fs::write(file_path, content);
    }
}

pub fn clear_injected_paths(session_id: &str) {
    let Some(file_path) = get_storage_path(session_id) else {
        return;
    };
    if file_path.exists() {
        let _ = fs::remove_file(file_path);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TruncationResult {
    pub result: String,
    pub truncated: bool,
}

fn truncate_content(content: &str, max_tokens: Option<usize>) -> TruncationResult {
    let max_tokens = max_tokens.unwrap_or(DEFAULT_MAX_README_TOKENS);
    let estimated_tokens = content.len().div_ceil(CHARS_PER_TOKEN);

    if estimated_tokens <= max_tokens {
        return TruncationResult {
            result: content.to_string(),
            truncated: false,
        };
    }

    let max_chars = max_tokens * CHARS_PER_TOKEN;
    TruncationResult {
        result: content.chars().take(max_chars).collect(),
        truncated: true,
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();

    for comp in path.components() {
        match comp {
            Component::Prefix(p) => out.push(p.as_os_str()),
            Component::RootDir => out.push(Path::new("/")),
            Component::CurDir => {}
            Component::ParentDir => {
                let popped = out.pop();
                if !popped {
                    out.push("..");
                }
            }
            Component::Normal(s) => out.push(s),
        }
    }

    out
}

pub struct DirectoryReadmeInjectorHook {
    working_directory: PathBuf,
    session_caches: RwLock<HashMap<String, HashSet<String>>>,
}

impl DirectoryReadmeInjectorHook {
    pub fn new(working_directory: impl Into<PathBuf>) -> Self {
        Self {
            working_directory: normalize_path(&working_directory.into()),
            session_caches: RwLock::new(HashMap::new()),
        }
    }

    fn get_session_cache(&self, session_id: &str) -> HashSet<String> {
        if let Ok(state) = self.session_caches.read() {
            if let Some(existing) = state.get(session_id) {
                return existing.clone();
            }
        }

        let loaded = load_injected_paths(session_id);
        if let Ok(mut state) = self.session_caches.write() {
            state.insert(session_id.to_string(), loaded.clone());
        }

        loaded
    }

    fn set_session_cache(&self, session_id: &str, cache: HashSet<String>) {
        if let Ok(mut state) = self.session_caches.write() {
            state.insert(session_id.to_string(), cache);
        }
    }

    fn resolve_file_path(&self, path: &str) -> Option<PathBuf> {
        if path.is_empty() {
            return None;
        }

        if path.starts_with('/') {
            return Some(normalize_path(Path::new(path)));
        }

        Some(normalize_path(&self.working_directory.join(path)))
    }

    fn find_readme_md_up(&self, start_dir: &Path) -> Vec<PathBuf> {
        let mut found = Vec::new();
        let mut current = normalize_path(start_dir);

        loop {
            let readme_path = current.join(README_FILENAME);
            if readme_path.exists() {
                found.push(readme_path);
            }

            if current == self.working_directory {
                break;
            }

            let Some(parent) = current.parent() else {
                break;
            };
            let parent = parent.to_path_buf();
            if parent == current {
                break;
            }
            if !parent.starts_with(&self.working_directory) {
                break;
            }

            current = parent;
        }

        found.reverse();
        found
    }

    fn process_file_path_for_readmes(&self, file_path: &str, session_id: &str) -> String {
        let Some(resolved) = self.resolve_file_path(file_path) else {
            return String::new();
        };

        let Some(dir) = resolved.parent() else {
            return String::new();
        };

        let mut cache = self.get_session_cache(session_id);
        let readme_paths = self.find_readme_md_up(dir);

        let mut output = String::new();

        for readme_path in readme_paths {
            let readme_dir = readme_path
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();

            if cache.contains(&readme_dir) {
                continue;
            }

            let Ok(content) = fs::read_to_string(&readme_path) else {
                continue;
            };

            let trunc = truncate_content(&content, None);
            let readme_path_str = readme_path.to_string_lossy();

            let trunc_notice = if trunc.truncated {
                format!(
                    "\n\n[Note: Content was truncated to save context window space. For full context, please read the file directly: {}]",
                    readme_path_str
                )
            } else {
                String::new()
            };

            output.push_str(&format!(
                "\n\n[Project README: {}]\n{}{}",
                readme_path_str, trunc.result, trunc_notice
            ));

            cache.insert(readme_dir);
        }

        if !output.is_empty() {
            save_injected_paths(session_id, &cache);
            self.set_session_cache(session_id, cache);
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

        self.process_file_path_for_readmes(file_path, session_id)
    }

    pub fn get_readmes_for_file(&self, file_path: &str) -> Vec<String> {
        let Some(resolved) = self.resolve_file_path(file_path) else {
            return Vec::new();
        };
        let Some(dir) = resolved.parent() else {
            return Vec::new();
        };

        self.find_readme_md_up(dir)
            .into_iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect()
    }

    pub fn clear_session(&self, session_id: &str) {
        if let Ok(mut state) = self.session_caches.write() {
            state.remove(session_id);
        }
        clear_injected_paths(session_id);
    }

    pub fn is_tracked_tool(tool_name: &str) -> bool {
        let lower = tool_name.to_lowercase();
        TRACKED_TOOLS.iter().any(|t| *t == lower)
    }
}

#[async_trait]
impl Hook for DirectoryReadmeInjectorHook {
    fn name(&self) -> &str {
        "directory-readme-injector"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PreToolUse]
    }

    async fn execute(
        &self,
        _event: HookEvent,
        input: &HookInput,
        context: &HookContext,
    ) -> HookResult {
        // Extract tool name and file_path from tool_input
        let tool_name = input.tool_name.as_deref().unwrap_or("");

        if !Self::is_tracked_tool(tool_name) {
            return Ok(HookOutput::pass());
        }

        // Try to extract file_path from tool_input
        let file_path = input
            .tool_input
            .as_ref()
            .and_then(|v| v.get("file_path"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if file_path.is_empty() {
            return Ok(HookOutput::pass());
        }

        // Get session ID from context
        let session_id = context.session_id.as_deref().unwrap_or("default-session");

        // Process and get README injection content
        let readme_content = self.process_tool_execution(tool_name, file_path, session_id);

        if readme_content.is_empty() {
            Ok(HookOutput::pass())
        } else {
            Ok(HookOutput::continue_with_message(readme_content))
        }
    }

    fn priority(&self) -> i32 {
        10 // Higher priority to inject README before other hooks
    }
}

pub fn get_readmes_for_path(file_path: &str, working_directory: Option<&str>) -> Vec<String> {
    let cwd = working_directory
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let hook = DirectoryReadmeInjectorHook::new(cwd);
    hook.get_readmes_for_file(file_path)
}

// Keep regex usage to match crate conventions (and avoid warnings about unused dependency).
fn _tracked_tool_regex() -> Regex {
    Regex::new(r"(?i)^(read|write|edit|multiedit)$").unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazy_static::lazy_static;
    use std::sync::Mutex;
    use tempfile::tempdir;

    // Mutex to serialize tests that modify environment variables
    lazy_static! {
        static ref ENV_TEST_LOCK: Mutex<()> = Mutex::new(());
    }

    #[test]
    fn test_truncate_content() {
        let content = "a".repeat((DEFAULT_MAX_README_TOKENS * CHARS_PER_TOKEN) + 1);
        let trunc = truncate_content(&content, None);
        assert!(trunc.truncated);
        assert_eq!(
            trunc.result.len(),
            DEFAULT_MAX_README_TOKENS * CHARS_PER_TOKEN
        );
    }

    #[test]
    fn test_find_readme_md_up_order_and_bounds() {
        let _lock = ENV_TEST_LOCK.lock().unwrap();

        let storage = tempdir().unwrap();
        std::env::set_var("ASTRAPE_README_INJECTOR_STORAGE_DIR", storage.path());

        let wd = tempdir().unwrap();
        let wd_path = wd.path();

        let root_readme = wd_path.join(README_FILENAME);
        fs::write(&root_readme, "root").unwrap();

        let sub = wd_path.join("sub");
        fs::create_dir_all(&sub).unwrap();
        let sub_readme = sub.join(README_FILENAME);
        fs::write(&sub_readme, "sub").unwrap();

        let deep = sub.join("deep");
        fs::create_dir_all(&deep).unwrap();

        let hook = DirectoryReadmeInjectorHook::new(wd_path);
        let readmes = hook.find_readme_md_up(&deep);

        assert_eq!(readmes.len(), 2);
        assert_eq!(readmes[0], root_readme);
        assert_eq!(readmes[1], sub_readme);

        std::env::remove_var("ASTRAPE_README_INJECTOR_STORAGE_DIR");
    }

    #[test]
    fn test_process_tool_execution_caches_per_session_and_persists() {
        let _lock = ENV_TEST_LOCK.lock().unwrap();

        let storage = tempdir().unwrap();
        std::env::set_var("ASTRAPE_README_INJECTOR_STORAGE_DIR", storage.path());

        let wd = tempdir().unwrap();
        let wd_path = wd.path();

        fs::write(wd_path.join(README_FILENAME), "root readme").unwrap();

        let sub = wd_path.join("sub");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join(README_FILENAME), "sub readme").unwrap();

        let file_path = sub.join("file.txt");
        fs::write(&file_path, "content").unwrap();

        let session_id = "sess-123";
        let hook = DirectoryReadmeInjectorHook::new(wd_path);

        let first = hook.process_tool_execution("read", file_path.to_str().unwrap(), session_id);
        assert!(first.contains("[Project README:"));
        assert!(first.contains("root readme"));
        assert!(first.contains("sub readme"));

        let second = hook.process_tool_execution("read", file_path.to_str().unwrap(), session_id);
        assert!(second.is_empty());

        // New instance should load from persisted storage and still not inject.
        let hook2 = DirectoryReadmeInjectorHook::new(wd_path);
        let third = hook2.process_tool_execution("read", file_path.to_str().unwrap(), session_id);
        assert!(third.is_empty());

        hook2.clear_session(session_id);
        let hook3 = DirectoryReadmeInjectorHook::new(wd_path);
        let after_clear =
            hook3.process_tool_execution("read", file_path.to_str().unwrap(), session_id);
        assert!(!after_clear.is_empty());

        std::env::remove_var("ASTRAPE_README_INJECTOR_STORAGE_DIR");
    }
}
