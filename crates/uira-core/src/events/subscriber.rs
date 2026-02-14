use crate::events::{Event, EventCategory};
use async_trait::async_trait;
use std::collections::HashSet;
use tokio::sync::broadcast;

#[derive(Debug, Clone, Default)]
pub struct SubscriptionFilter {
    categories: Option<HashSet<EventCategory>>,
    event_names: Option<HashSet<String>>,
    session_ids: Option<HashSet<String>>,
}

impl SubscriptionFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn categories(mut self, categories: impl IntoIterator<Item = EventCategory>) -> Self {
        self.categories = Some(categories.into_iter().collect());
        self
    }

    pub fn event_names(mut self, names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.event_names = Some(names.into_iter().map(|s| s.into()).collect());
        self
    }

    pub fn session_ids(mut self, ids: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.session_ids = Some(ids.into_iter().map(|s| s.into()).collect());
        self
    }

    pub fn matches(&self, event: &Event) -> bool {
        if let Some(ref cats) = self.categories {
            if !cats.contains(&event.category()) {
                return false;
            }
        }

        if let Some(ref names) = self.event_names {
            if !names.contains(event.event_name()) {
                return false;
            }
        }

        if let Some(ref ids) = self.session_ids {
            if let Some(session_id) = event.session_id() {
                if !ids.contains(session_id) {
                    return false;
                }
            } else {
                return false;
            }
        }

        true
    }

    pub fn is_wildcard(&self) -> bool {
        self.categories.is_none() && self.event_names.is_none() && self.session_ids.is_none()
    }
}

#[async_trait]
pub trait EventHandler: Send + Sync {
    fn name(&self) -> &str;
    fn filter(&self) -> SubscriptionFilter {
        SubscriptionFilter::new()
    }
    async fn handle(&self, event: &Event) -> HandlerResult;
    fn priority(&self) -> i32 {
        0
    }
}

#[derive(Debug, Clone)]
pub struct HandlerResult {
    pub should_continue: bool,
    pub message: Option<String>,
    pub modified_event: Option<Event>,
}

impl HandlerResult {
    pub fn pass() -> Self {
        Self {
            should_continue: true,
            message: None,
            modified_event: None,
        }
    }

    pub fn with_message(message: impl Into<String>) -> Self {
        Self {
            should_continue: true,
            message: Some(message.into()),
            modified_event: None,
        }
    }

    pub fn block(reason: impl Into<String>) -> Self {
        Self {
            should_continue: false,
            message: Some(reason.into()),
            modified_event: None,
        }
    }
}

pub struct Subscriber {
    receiver: broadcast::Receiver<Event>,
    filter: SubscriptionFilter,
}

impl Subscriber {
    pub fn new(receiver: broadcast::Receiver<Event>) -> Self {
        Self {
            receiver,
            filter: SubscriptionFilter::new(),
        }
    }

    pub fn with_filter(mut self, filter: SubscriptionFilter) -> Self {
        self.filter = filter;
        self
    }

    pub async fn recv(&mut self) -> Option<Event> {
        loop {
            match self.receiver.recv().await {
                Ok(event) => {
                    if self.filter.matches(&event) {
                        return Some(event);
                    }
                }
                Err(broadcast::error::RecvError::Closed) => return None,
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("Subscriber lagged, missed {} events", n);
                    continue;
                }
            }
        }
    }

    pub fn try_recv(&mut self) -> Option<Event> {
        loop {
            match self.receiver.try_recv() {
                Ok(event) => {
                    if self.filter.matches(&event) {
                        return Some(event);
                    }
                }
                Err(broadcast::error::TryRecvError::Empty) => return None,
                Err(broadcast::error::TryRecvError::Closed) => return None,
                Err(broadcast::error::TryRecvError::Lagged(n)) => {
                    tracing::warn!("Subscriber lagged, missed {} events", n);
                    continue;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_categories() {
        let filter = SubscriptionFilter::new().categories([EventCategory::Tool]);

        let tool_event = Event::ToolExecutionStarted {
            session_id: "test".to_string(),
            tool_call_id: "tc_1".to_string(),
            tool_name: "bash".to_string(),
            input: serde_json::json!({}),
        };
        assert!(filter.matches(&tool_event));

        let session_event = Event::SessionStarted {
            session_id: "test".to_string(),
            parent_id: None,
        };
        assert!(!filter.matches(&session_event));
    }

    #[test]
    fn test_filter_event_names() {
        let filter = SubscriptionFilter::new().event_names(["session_started", "session_ended"]);

        let start_event = Event::SessionStarted {
            session_id: "test".to_string(),
            parent_id: None,
        };
        assert!(filter.matches(&start_event));

        let turn_event = Event::TurnStarted {
            session_id: "test".to_string(),
            turn_number: 1,
        };
        assert!(!filter.matches(&turn_event));
    }

    #[test]
    fn test_filter_session_ids() {
        let filter = SubscriptionFilter::new().session_ids(["ses_123"]);

        let matching = Event::TurnStarted {
            session_id: "ses_123".to_string(),
            turn_number: 1,
        };
        assert!(filter.matches(&matching));

        let non_matching = Event::TurnStarted {
            session_id: "ses_456".to_string(),
            turn_number: 1,
        };
        assert!(!filter.matches(&non_matching));
    }

    #[test]
    fn test_filter_wildcard() {
        let filter = SubscriptionFilter::new();
        assert!(filter.is_wildcard());

        let filter_with_cat = SubscriptionFilter::new().categories([EventCategory::Tool]);
        assert!(!filter_with_cat.is_wildcard());
    }

    #[test]
    fn test_handler_result() {
        let pass = HandlerResult::pass();
        assert!(pass.should_continue);
        assert!(pass.message.is_none());

        let with_msg = HandlerResult::with_message("hello");
        assert!(with_msg.should_continue);
        assert_eq!(with_msg.message, Some("hello".to_string()));

        let block = HandlerResult::block("blocked");
        assert!(!block.should_continue);
        assert_eq!(block.message, Some("blocked".to_string()));
    }
}
