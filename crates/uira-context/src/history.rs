//! Message history storage

use uira_protocol::Message;

/// Stores conversation history
#[derive(Debug, Default, Clone)]
pub struct MessageHistory {
    messages: Vec<Message>,
}

impl MessageHistory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, message: Message) {
        self.messages.push(message);
    }

    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    pub fn clear(&mut self) {
        self.messages.clear();
    }

    /// Remove the oldest message
    pub fn remove_first(&mut self) -> Option<Message> {
        if self.messages.is_empty() {
            None
        } else {
            Some(self.messages.remove(0))
        }
    }

    /// Estimate total tokens in history
    pub fn estimate_tokens(&self) -> usize {
        self.messages.iter().map(|m| m.estimate_tokens()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_history_operations() {
        let mut history = MessageHistory::new();
        assert!(history.is_empty());

        history.push(Message::user("Hello"));
        history.push(Message::assistant("Hi there!"));

        assert_eq!(history.len(), 2);
        assert!(!history.is_empty());

        let first = history.remove_first().unwrap();
        assert_eq!(first.content.as_text(), Some("Hello"));
        assert_eq!(history.len(), 1);
    }
}
