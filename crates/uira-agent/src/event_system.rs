use std::sync::Arc;
use tokio::task::JoinHandle;
use uira_events::{BroadcastBus, EventBus, HandlerRegistry, SubscriberRunner};
use uira_hooks::create_hook_event_adapter;

pub struct EventSystem {
    pub bus: Arc<BroadcastBus>,
    pub registry: Arc<HandlerRegistry>,
    runner_handle: Option<JoinHandle<()>>,
}

impl EventSystem {
    pub fn new(working_directory: String) -> Self {
        let bus = Arc::new(BroadcastBus::new());
        let mut registry = HandlerRegistry::new();

        let legacy_adapter = create_hook_event_adapter(working_directory);
        registry.register(Arc::new(legacy_adapter));

        Self {
            bus,
            registry: Arc::new(registry),
            runner_handle: None,
        }
    }

    pub fn with_capacity(working_directory: String, capacity: usize) -> Self {
        let bus = Arc::new(BroadcastBus::with_capacity(capacity));
        let mut registry = HandlerRegistry::new();

        let legacy_adapter = create_hook_event_adapter(working_directory);
        registry.register(Arc::new(legacy_adapter));

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

pub fn create_event_system(working_directory: String) -> EventSystem {
    EventSystem::new(working_directory)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uira_events::Event;

    #[tokio::test]
    async fn test_event_system_creation() {
        let system = EventSystem::new("/tmp".to_string());
        assert_eq!(system.registry.handler_count(), 1);
        assert!(!system.is_running());
    }

    #[tokio::test]
    async fn test_event_system_start_stop() {
        let mut system = EventSystem::new("/tmp".to_string());
        system.start();
        assert!(system.is_running());

        system.stop();
        assert!(!system.is_running());
    }

    #[tokio::test]
    async fn test_event_system_publish() {
        let mut system = EventSystem::new("/tmp".to_string());
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
