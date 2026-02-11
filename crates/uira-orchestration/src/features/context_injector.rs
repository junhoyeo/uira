use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

/// Separator used between context entries and between context and original content.
const CONTEXT_SEPARATOR: &str = "\n\n---\n\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContextSourceType {
    #[serde(rename = "keyword-detector")]
    KeywordDetector,
    #[serde(rename = "rules-injector")]
    RulesInjector,
    #[serde(rename = "directory-agents")]
    DirectoryAgents,
    #[serde(rename = "directory-readme")]
    DirectoryReadme,
    #[serde(rename = "boulder-state")]
    BoulderState,
    #[serde(rename = "session-context")]
    SessionContext,
    #[serde(rename = "learner")]
    Learner,
    #[serde(rename = "custom")]
    Custom,
}

impl ContextSourceType {
    pub fn as_str(self) -> &'static str {
        match self {
            ContextSourceType::KeywordDetector => "keyword-detector",
            ContextSourceType::RulesInjector => "rules-injector",
            ContextSourceType::DirectoryAgents => "directory-agents",
            ContextSourceType::DirectoryReadme => "directory-readme",
            ContextSourceType::BoulderState => "boulder-state",
            ContextSourceType::SessionContext => "session-context",
            ContextSourceType::Learner => "learner",
            ContextSourceType::Custom => "custom",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContextPriority {
    Critical,
    High,
    Normal,
    Low,
}

impl ContextPriority {
    fn order(self) -> u8 {
        match self {
            ContextPriority::Critical => 0,
            ContextPriority::High => 1,
            ContextPriority::Normal => 2,
            ContextPriority::Low => 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextEntry {
    pub id: String,
    pub source: ContextSourceType,
    pub content: String,
    pub priority: ContextPriority,
    pub timestamp: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RegisterContextOptions {
    pub id: String,
    pub source: ContextSourceType,
    pub content: String,
    pub priority: Option<ContextPriority>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PendingContext {
    pub merged: String,
    pub entries: Vec<ContextEntry>,
    pub has_content: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputPart {
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InjectionStrategy {
    Prepend,
    Append,
    Wrap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct InjectionResult {
    pub injected: bool,
    pub context_length: usize,
    pub entry_count: usize,
}

#[derive(Debug, Default)]
pub struct ContextCollector {
    sessions: Mutex<HashMap<String, HashMap<String, ContextEntry>>>,
}

impl ContextCollector {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }

    pub fn register(&self, session_id: &str, options: RegisterContextOptions) {
        let mut sessions = self.sessions.lock().expect("lock");
        let session_map = sessions.entry(session_id.to_string()).or_default();
        let key = format!("{}:{}", options.source.as_str(), options.id);

        let entry = ContextEntry {
            id: options.id,
            source: options.source,
            content: options.content,
            priority: options.priority.unwrap_or(ContextPriority::Normal),
            timestamp: now_ms(),
            metadata: options.metadata,
        };
        session_map.insert(key, entry);
    }

    pub fn get_pending(&self, session_id: &str) -> PendingContext {
        let sessions = self.sessions.lock().expect("lock");
        let Some(map) = sessions.get(session_id) else {
            return PendingContext {
                merged: String::new(),
                entries: vec![],
                has_content: false,
            };
        };

        if map.is_empty() {
            return PendingContext {
                merged: String::new(),
                entries: vec![],
                has_content: false,
            };
        }

        let mut entries = map.values().cloned().collect::<Vec<_>>();
        entries.sort_by(|a, b| {
            let p = a.priority.order().cmp(&b.priority.order());
            if p != std::cmp::Ordering::Equal {
                return p;
            }
            a.timestamp.cmp(&b.timestamp)
        });

        let merged = entries
            .iter()
            .map(|e| e.content.as_str())
            .collect::<Vec<_>>()
            .join(CONTEXT_SEPARATOR);

        PendingContext {
            merged,
            has_content: !entries.is_empty(),
            entries,
        }
    }

    pub fn consume(&self, session_id: &str) -> PendingContext {
        let pending = self.get_pending(session_id);
        self.clear(session_id);
        pending
    }

    pub fn clear(&self, session_id: &str) {
        self.sessions.lock().expect("lock").remove(session_id);
    }

    pub fn has_pending(&self, session_id: &str) -> bool {
        self.sessions
            .lock()
            .expect("lock")
            .get(session_id)
            .is_some_and(|m| !m.is_empty())
    }
}

pub fn inject_pending_context(
    collector: &ContextCollector,
    session_id: &str,
    parts: &mut [OutputPart],
    strategy: InjectionStrategy,
) -> InjectionResult {
    if !collector.has_pending(session_id) {
        return InjectionResult {
            injected: false,
            context_length: 0,
            entry_count: 0,
        };
    }

    let idx = parts
        .iter()
        .position(|p| p.type_ == "text" && p.text.is_some());
    let Some(text_part_index) = idx else {
        return InjectionResult {
            injected: false,
            context_length: 0,
            entry_count: 0,
        };
    };

    let pending = collector.consume(session_id);
    let original = parts[text_part_index].text.clone().unwrap_or_default();

    let updated = match strategy {
        InjectionStrategy::Prepend => {
            format!("{}{}{}", pending.merged, CONTEXT_SEPARATOR, original)
        }
        InjectionStrategy::Append => format!("{}{}{}", original, CONTEXT_SEPARATOR, pending.merged),
        InjectionStrategy::Wrap => format!(
            "<injected-context>\n{}\n</injected-context>{}{}",
            pending.merged, CONTEXT_SEPARATOR, original
        ),
    };
    parts[text_part_index].text = Some(updated);

    InjectionResult {
        injected: true,
        context_length: pending.merged.len(),
        entry_count: pending.entries.len(),
    }
}

pub fn inject_context_into_text(
    collector: &ContextCollector,
    session_id: &str,
    text: &str,
    strategy: InjectionStrategy,
) -> (String, InjectionResult) {
    if !collector.has_pending(session_id) {
        return (
            text.to_string(),
            InjectionResult {
                injected: false,
                context_length: 0,
                entry_count: 0,
            },
        );
    }

    let pending = collector.consume(session_id);
    let result = match strategy {
        InjectionStrategy::Prepend => format!("{}{}{}", pending.merged, CONTEXT_SEPARATOR, text),
        InjectionStrategy::Append => format!("{}{}{}", text, CONTEXT_SEPARATOR, pending.merged),
        InjectionStrategy::Wrap => format!(
            "<injected-context>\n{}\n</injected-context>{}{}",
            pending.merged, CONTEXT_SEPARATOR, text
        ),
    };

    (
        result,
        InjectionResult {
            injected: true,
            context_length: pending.merged.len(),
            entry_count: pending.entries.len(),
        },
    )
}

pub struct ContextInjectorHook<'a> {
    collector: &'a ContextCollector,
}

impl<'a> ContextInjectorHook<'a> {
    pub fn process_user_message(&self, session_id: &str, message: &str) -> (String, bool) {
        if !self.collector.has_pending(session_id) {
            return (message.to_string(), false);
        }
        let (result, _) = inject_context_into_text(
            self.collector,
            session_id,
            message,
            InjectionStrategy::Prepend,
        );
        (result, true)
    }

    pub fn register_context(&self, session_id: &str, options: RegisterContextOptions) {
        self.collector.register(session_id, options);
    }

    pub fn has_pending(&self, session_id: &str) -> bool {
        self.collector.has_pending(session_id)
    }
}

pub fn create_context_injector_hook<'a>(
    collector: &'a ContextCollector,
) -> ContextInjectorHook<'a> {
    ContextInjectorHook { collector }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collector_sorts_and_deduplicates() {
        let collector = ContextCollector::new();
        collector.register(
            "s1",
            RegisterContextOptions {
                id: "a".to_string(),
                source: ContextSourceType::Custom,
                content: "low".to_string(),
                priority: Some(ContextPriority::Low),
                metadata: None,
            },
        );
        collector.register(
            "s1",
            RegisterContextOptions {
                id: "a".to_string(),
                source: ContextSourceType::Custom,
                content: "replaced".to_string(),
                priority: Some(ContextPriority::Critical),
                metadata: None,
            },
        );
        collector.register(
            "s1",
            RegisterContextOptions {
                id: "b".to_string(),
                source: ContextSourceType::Learner,
                content: "second".to_string(),
                priority: Some(ContextPriority::High),
                metadata: None,
            },
        );

        let pending = collector.get_pending("s1");
        assert!(pending.has_content);
        assert_eq!(pending.entries.len(), 2);
        assert!(pending.merged.starts_with("replaced"));
        assert!(pending.merged.contains(CONTEXT_SEPARATOR));
    }

    #[test]
    fn injects_into_text_part_and_consumes() {
        let collector = ContextCollector::new();
        collector.register(
            "s1",
            RegisterContextOptions {
                id: "a".to_string(),
                source: ContextSourceType::Custom,
                content: "ctx".to_string(),
                priority: None,
                metadata: None,
            },
        );

        let mut parts = vec![OutputPart {
            type_: "text".to_string(),
            text: Some("hello".to_string()),
            extra: HashMap::new(),
        }];
        let result =
            inject_pending_context(&collector, "s1", &mut parts, InjectionStrategy::Prepend);

        assert!(result.injected);
        assert_eq!(result.entry_count, 1);
        assert!(parts[0].text.as_ref().unwrap().starts_with("ctx"));
        assert!(!collector.has_pending("s1"));
    }

    #[test]
    fn hook_processes_user_message() {
        let collector = ContextCollector::new();
        let hook = create_context_injector_hook(&collector);
        hook.register_context(
            "s1",
            RegisterContextOptions {
                id: "a".to_string(),
                source: ContextSourceType::Custom,
                content: "ctx".to_string(),
                priority: None,
                metadata: None,
            },
        );

        let (msg, injected) = hook.process_user_message("s1", "hi");
        assert!(injected);
        assert!(msg.contains("ctx"));
        assert!(!hook.has_pending("s1"));
    }
}
