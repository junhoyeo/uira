use std::sync::Arc;
use tokio::task::JoinHandle;
use uira_core::{BroadcastBus, EventBus, HandlerRegistry, SubscriberRunner};
use uira_memory::MemorySystem;
use uira_orchestration::create_hook_event_adapter;

pub struct EventSystem {
    pub bus: Arc<BroadcastBus>,
    pub registry: Arc<HandlerRegistry>,
    runner_handle: Option<JoinHandle<()>>,
}

impl EventSystem {
    pub fn new(working_directory: String, memory_system: Option<Arc<MemorySystem>>) -> Self {
        let bus = Arc::new(BroadcastBus::new());
        let mut registry = HandlerRegistry::new();

        let hook_adapter = create_hook_event_adapter(working_directory, memory_system);
        registry.register(Arc::new(hook_adapter));

        Self {
            bus,
            registry: Arc::new(registry),
            runner_handle: None,
        }
    }

    pub fn with_capacity(
        working_directory: String,
        capacity: usize,
        memory_system: Option<Arc<MemorySystem>>,
    ) -> Self {
        let bus = Arc::new(BroadcastBus::with_capacity(capacity));
        let mut registry = HandlerRegistry::new();

        let hook_adapter = create_hook_event_adapter(working_directory, memory_system);
        registry.register(Arc::new(hook_adapter));

        Self {
            bus,
            registry: Arc::new(registry),
            runner_handle: None,
        }
    }

    pub fn start(&mut self) {
        if self.runner_handle.is_some() {
            return;
        }

        let receiver = self.bus.subscribe();
        let runner = SubscriberRunner::new(self.registry.clone());
        self.runner_handle = Some(runner.spawn(receiver));
    }

    pub fn stop(&mut self) {
        if let Some(handle) = self.runner_handle.take() {
            handle.abort();
        }
    }

    pub fn bus(&self) -> Arc<dyn EventBus> {
        self.bus.clone()
    }

    pub fn is_running(&self) -> bool {
        self.runner_handle.is_some()
    }
}

impl Drop for EventSystem {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn create_event_system(
    working_directory: String,
    memory_system: Option<Arc<MemorySystem>>,
) -> EventSystem {
    EventSystem::new(working_directory, memory_system)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uira_core::Event;

    #[tokio::test]
    async fn test_event_system_creation() {
        let system = EventSystem::new("/tmp".to_string(), None);
        assert_eq!(system.registry.handler_count(), 1);
        assert!(!system.is_running());
    }

    #[tokio::test]
    async fn test_event_system_start_stop() {
        let mut system = EventSystem::new("/tmp".to_string(), None);
        system.start();
        assert!(system.is_running());

        system.stop();
        assert!(!system.is_running());
    }

    #[tokio::test]
    async fn test_event_system_publish() {
        let mut system = EventSystem::new("/tmp".to_string(), None);
        system.start();

        let event = Event::SessionStarted {
            session_id: "test_123".to_string(),
            parent_id: None,
        };

        system.bus.publish(event);

        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        system.stop();
    }
}
