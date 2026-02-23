use std::collections::HashMap;
use std::sync::Arc;

use super::hook::{Hook, HookContext, HookResult};
use super::hooks::*;
use super::types::{HookEvent, HookInput, HookOutput};

pub struct HookRegistry {
    hooks: HashMap<String, Arc<dyn Hook>>,
    event_hooks: HashMap<HookEvent, Vec<String>>,
}

impl HookRegistry {
    pub fn new() -> Self {
        Self {
            hooks: HashMap::new(),
            event_hooks: HashMap::new(),
        }
    }

    /// Register a hook
    pub fn register(&mut self, hook: Arc<dyn Hook>) {
        let name = hook.name().to_string();
        let events = hook.events().to_vec();

        for event in events {
            self.event_hooks
                .entry(event)
                .or_default()
                .push(name.clone());
        }

        self.hooks.insert(name, hook);
    }

    /// Get a hook by name
    pub fn get(&self, name: &str) -> Option<Arc<dyn Hook>> {
        self.hooks.get(name).cloned()
    }

    /// Get all hooks for a specific event, sorted by priority
    pub fn get_hooks_for_event(&self, event: HookEvent) -> Vec<Arc<dyn Hook>> {
        let hook_names = match self.event_hooks.get(&event) {
            Some(names) => names,
            None => return Vec::new(),
        };

        let mut hooks: Vec<Arc<dyn Hook>> = hook_names
            .iter()
            .filter_map(|name| self.hooks.get(name).cloned())
            .filter(|hook| hook.is_enabled())
            .collect();

        hooks.sort_by_key(|b| std::cmp::Reverse(b.priority()));

        hooks
    }

    /// Execute all hooks for an event
    ///
    /// Hooks are executed in priority order (highest first).
    /// If any hook returns continue=false, execution stops and that output is returned.
    /// Otherwise, messages from all hooks are combined.
    pub async fn execute_hooks(
        &self,
        event: HookEvent,
        input: &HookInput,
        context: &HookContext,
    ) -> HookResult {
        let hooks = self.get_hooks_for_event(event);

        if hooks.is_empty() {
            return Ok(HookOutput::pass());
        }

        let mut combined_messages = Vec::new();

        for hook in hooks {
            match hook.execute(event, input, context).await {
                Ok(output) => {
                    if !output.should_continue {
                        return Ok(output);
                    }

                    if let Some(message) = output.message {
                        combined_messages.push(message);
                    }
                }
                Err(e) => {
                    eprintln!("[hook-registry] Error in hook '{}': {}", hook.name(), e);
                }
            }
        }

        if combined_messages.is_empty() {
            Ok(HookOutput::pass())
        } else {
            Ok(HookOutput::continue_with_message(
                combined_messages.join("\n\n"),
            ))
        }
    }

    /// Get count of registered hooks
    pub fn count(&self) -> usize {
        self.hooks.len()
    }

    /// List all registered hook names
    pub fn list_hooks(&self) -> Vec<String> {
        self.hooks.keys().cloned().collect()
    }
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub fn default_hooks() -> HookRegistry {
    let mut registry = HookRegistry::new();

    registry.register(Arc::new(AgentUsageReminderHook));
    registry.register(Arc::new(AutoSlashCommandHook));
    registry.register(Arc::new(AutopilotHook::new()));
    registry.register(Arc::new(BackgroundNotificationHook::new()));
    registry.register(Arc::new(DirectoryReadmeInjectorHook::new(
        std::env::current_dir().unwrap_or_default(),
    )));
    registry.register(Arc::new(DirectoryAgentsInjectorHook::new(
        std::env::current_dir().unwrap_or_default(),
    )));
    registry.register(Arc::new(EmptyMessageSanitizerHook::new()));
    registry.register(Arc::new(KeywordDetectorHook::new()));
    registry.register(Arc::new(LearnerHook::new()));
    registry.register(Arc::new(NonInteractiveEnvHook));
    registry.register(Arc::new(NotepadHook::new()));
    registry.register(Arc::new(UiraOrchestratorHook));
    registry.register(Arc::new(PersistentModeHook));
    registry.register(Arc::new(PreemptiveCompactionHook::new(None)));
    registry.register(Arc::new(RalphHook::new()));
    registry.register(Arc::new(RecoveryHook::new()));
    registry.register(Arc::new(RulesInjectorHook::new(
        std::env::current_dir().unwrap_or_default(),
    )));
    registry.register(Arc::new(ThinkModeHook::new()));
    registry.register(Arc::new(ThinkingBlockValidatorHook));
    registry.register(Arc::new(TodoContinuationHook::new()));
    registry.register(Arc::new(UltrapilotHook::new()));
    registry.register(Arc::new(UltraQAHook::new()));
    registry.register(Arc::new(UltraworkHook::new()));
    registry.register(Arc::new(CommentCheckerHook::new()));
    registry.register(Arc::new(DelegationEnforcerHook::new()));
    registry.register(Arc::new(MemoryRecallAdapter::new()));
    registry.register(Arc::new(MemoryCaptureAdapter::new()));

    registry
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct TestHook {
        name: String,
        priority: i32,
        message: String,
    }

    #[async_trait]
    impl Hook for TestHook {
        fn name(&self) -> &str {
            &self.name
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
            Ok(HookOutput::continue_with_message(&self.message))
        }

        fn priority(&self) -> i32 {
            self.priority
        }
    }

    #[tokio::test]
    async fn test_registry_register_and_get() {
        let mut registry = HookRegistry::new();
        let hook = Arc::new(TestHook {
            name: "test".to_string(),
            priority: 0,
            message: "test message".to_string(),
        });

        registry.register(hook.clone());

        assert_eq!(registry.count(), 1);
        assert!(registry.get("test").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    #[tokio::test]
    async fn test_registry_get_hooks_for_event() {
        let mut registry = HookRegistry::new();

        let hook1 = Arc::new(TestHook {
            name: "hook1".to_string(),
            priority: 10,
            message: "first".to_string(),
        });
        let hook2 = Arc::new(TestHook {
            name: "hook2".to_string(),
            priority: 5,
            message: "second".to_string(),
        });

        registry.register(hook1);
        registry.register(hook2);

        let hooks = registry.get_hooks_for_event(HookEvent::UserPromptSubmit);
        assert_eq!(hooks.len(), 2);
        assert_eq!(hooks[0].name(), "hook1");
        assert_eq!(hooks[1].name(), "hook2");
    }

    #[tokio::test]
    async fn test_registry_execute_hooks_combines_messages() {
        let mut registry = HookRegistry::new();

        let hook1 = Arc::new(TestHook {
            name: "hook1".to_string(),
            priority: 10,
            message: "Message 1".to_string(),
        });
        let hook2 = Arc::new(TestHook {
            name: "hook2".to_string(),
            priority: 5,
            message: "Message 2".to_string(),
        });

        registry.register(hook1);
        registry.register(hook2);

        let input = HookInput {
            session_id: None,
            prompt: Some("test".to_string()),
            message: None,
            parts: None,
            tool_name: None,
            tool_input: None,
            tool_output: None,
            directory: None,
            stop_reason: None,
            user_requested: None,
            transcript_path: None,
            extra: HashMap::new(),
        };
        let context = HookContext::new(None, "/tmp".to_string(), None);

        let result = registry
            .execute_hooks(HookEvent::UserPromptSubmit, &input, &context)
            .await
            .unwrap();

        assert!(result.should_continue);
        assert_eq!(result.message, Some("Message 1\n\nMessage 2".to_string()));
    }

    struct BlockingHook;

    #[async_trait]
    impl Hook for BlockingHook {
        fn name(&self) -> &str {
            "blocking"
        }

        fn events(&self) -> &[HookEvent] {
            &[HookEvent::Stop]
        }

        async fn execute(
            &self,
            _event: HookEvent,
            _input: &HookInput,
            _context: &HookContext,
        ) -> HookResult {
            Ok(HookOutput::block_with_reason("Blocked by test"))
        }
    }

    #[tokio::test]
    async fn test_registry_execute_hooks_stops_on_block() {
        let mut registry = HookRegistry::new();
        registry.register(Arc::new(BlockingHook));

        let input = HookInput {
            session_id: None,
            prompt: None,
            message: None,
            parts: None,
            tool_name: None,
            tool_input: None,
            tool_output: None,
            directory: None,
            stop_reason: None,
            user_requested: None,
            transcript_path: None,
            extra: HashMap::new(),
        };
        let context = HookContext::new(None, "/tmp".to_string(), None);

        let result = registry
            .execute_hooks(HookEvent::Stop, &input, &context)
            .await
            .unwrap();

        assert!(!result.should_continue);
        assert_eq!(result.reason, Some("Blocked by test".to_string()));
    }
}
