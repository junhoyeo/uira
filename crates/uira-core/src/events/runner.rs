use crate::events::Event;
use crate::events::subscriber::{EventHandler, HandlerResult};
use std::sync::Arc;
use tokio::sync::broadcast;

pub struct HandlerRegistry {
    handlers: Vec<Arc<dyn EventHandler>>,
}

impl HandlerRegistry {
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    pub fn register(&mut self, handler: Arc<dyn EventHandler>) {
        self.handlers.push(handler);
        self.handlers
            .sort_by_key(|h| std::cmp::Reverse(h.priority()));
    }

    pub fn handler_count(&self) -> usize {
        self.handlers.len()
    }

    pub fn list_handlers(&self) -> Vec<&str> {
        self.handlers.iter().map(|h| h.name()).collect()
    }

    pub async fn dispatch(&self, event: &Event) -> Vec<HandlerResult> {
        let mut results = Vec::new();

        for handler in &self.handlers {
            let filter = handler.filter();
            if filter.matches(event) {
                let result = handler.handle(event).await;
                let should_continue = result.should_continue;
                results.push(result);

                if !should_continue {
                    break;
                }
            }
        }

        results
    }
}

impl Default for HandlerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub struct SubscriberRunner {
    registry: Arc<HandlerRegistry>,
}

impl SubscriberRunner {
    pub fn new(registry: Arc<HandlerRegistry>) -> Self {
        Self { registry }
    }

    pub async fn run(&self, mut receiver: broadcast::Receiver<Event>) {
        loop {
            match receiver.recv().await {
                Ok(event) => {
                    let results = self.registry.dispatch(&event).await;
                    for result in results {
                        if !result.should_continue {
                            tracing::debug!("Handler blocked event: {:?}", result.message);
                            break;
                        }
                    }
                }
                Err(broadcast::error::RecvError::Closed) => {
                    tracing::info!("Event bus closed, stopping subscriber runner");
                    break;
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("Subscriber runner lagged, missed {} events", n);
                }
            }
        }
    }

    pub fn spawn(self, receiver: broadcast::Receiver<Event>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            self.run(receiver).await;
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::EventCategory;
    use crate::events::subscriber::SubscriptionFilter;
    use async_trait::async_trait;

    struct TestHandler {
        name: String,
        priority: i32,
        filter: SubscriptionFilter,
        message: Option<String>,
    }

    impl TestHandler {
        fn new(name: &str, priority: i32) -> Self {
            Self {
                name: name.to_string(),
                priority,
                filter: SubscriptionFilter::new(),
                message: None,
            }
        }

        fn with_filter(mut self, filter: SubscriptionFilter) -> Self {
            self.filter = filter;
            self
        }

        fn with_message(mut self, msg: &str) -> Self {
            self.message = Some(msg.to_string());
            self
        }
    }

    #[async_trait]
    impl EventHandler for TestHandler {
        fn name(&self) -> &str {
            &self.name
        }

        fn filter(&self) -> SubscriptionFilter {
            self.filter.clone()
        }

        async fn handle(&self, _event: &Event) -> HandlerResult {
            if let Some(ref msg) = self.message {
                HandlerResult::with_message(msg.clone())
            } else {
                HandlerResult::pass()
            }
        }

        fn priority(&self) -> i32 {
            self.priority
        }
    }

    struct BlockingHandler;

    #[async_trait]
    impl EventHandler for BlockingHandler {
        fn name(&self) -> &str {
            "blocking"
        }

        async fn handle(&self, _event: &Event) -> HandlerResult {
            HandlerResult::block("blocked by test")
        }

        fn priority(&self) -> i32 {
            100
        }
    }

    #[tokio::test]
    async fn test_registry_dispatch() {
        let mut registry = HandlerRegistry::new();
        registry.register(Arc::new(
            TestHandler::new("handler1", 10).with_message("msg1"),
        ));
        registry.register(Arc::new(
            TestHandler::new("handler2", 5).with_message("msg2"),
        ));

        let event = Event::SessionStarted {
            session_id: "test".to_string(),
            parent_id: None,
        };

        let results = registry.dispatch(&event).await;
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].message, Some("msg1".to_string()));
        assert_eq!(results[1].message, Some("msg2".to_string()));
    }

    #[tokio::test]
    async fn test_registry_priority_order() {
        let mut registry = HandlerRegistry::new();
        registry.register(Arc::new(TestHandler::new("low", 1)));
        registry.register(Arc::new(TestHandler::new("high", 100)));
        registry.register(Arc::new(TestHandler::new("medium", 50)));

        let handlers = registry.list_handlers();
        assert_eq!(handlers, vec!["high", "medium", "low"]);
    }

    #[tokio::test]
    async fn test_registry_filter_matching() {
        let mut registry = HandlerRegistry::new();
        registry.register(Arc::new(
            TestHandler::new("session_only", 10)
                .with_filter(SubscriptionFilter::new().categories([EventCategory::Session]))
                .with_message("session"),
        ));
        registry.register(Arc::new(
            TestHandler::new("tool_only", 5)
                .with_filter(SubscriptionFilter::new().categories([EventCategory::Tool]))
                .with_message("tool"),
        ));

        let session_event = Event::SessionStarted {
            session_id: "test".to_string(),
            parent_id: None,
        };

        let results = registry.dispatch(&session_event).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].message, Some("session".to_string()));
    }

    #[tokio::test]
    async fn test_registry_stops_on_block() {
        let mut registry = HandlerRegistry::new();
        registry.register(Arc::new(BlockingHandler));
        registry.register(Arc::new(
            TestHandler::new("after_block", 0).with_message("should not run"),
        ));

        let event = Event::SessionStarted {
            session_id: "test".to_string(),
            parent_id: None,
        };

        let results = registry.dispatch(&event).await;
        assert_eq!(results.len(), 1);
        assert!(!results[0].should_continue);
    }
}
