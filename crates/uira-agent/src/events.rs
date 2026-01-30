//! Event streaming for agent execution

use futures::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc;
use uira_protocol::ThreadEvent;

/// Stream of agent events
pub struct EventStream {
    receiver: mpsc::Receiver<ThreadEvent>,
}

impl EventStream {
    pub fn new(receiver: mpsc::Receiver<ThreadEvent>) -> Self {
        Self { receiver }
    }

    /// Create a channel pair for event streaming
    pub fn channel(buffer: usize) -> (EventSender, Self) {
        let (tx, rx) = mpsc::channel(buffer);
        (EventSender { sender: tx }, Self::new(rx))
    }
}

impl Stream for EventStream {
    type Item = ThreadEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.receiver).poll_recv(cx)
    }
}

/// Sender for agent events
#[derive(Clone)]
pub struct EventSender {
    sender: mpsc::Sender<ThreadEvent>,
}

impl EventSender {
    /// Send an event
    pub async fn send(
        &self,
        event: ThreadEvent,
    ) -> Result<(), mpsc::error::SendError<ThreadEvent>> {
        self.sender.send(event).await
    }

    /// Try to send an event without blocking
    pub fn try_send(
        &self,
        event: ThreadEvent,
    ) -> Result<(), mpsc::error::TrySendError<ThreadEvent>> {
        self.sender.try_send(event)
    }

    /// Check if the receiver has been dropped
    pub fn is_closed(&self) -> bool {
        self.sender.is_closed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[tokio::test]
    async fn test_event_stream() {
        let (sender, mut stream) = EventStream::channel(10);

        sender
            .send(ThreadEvent::ThreadStarted {
                thread_id: "test".to_string(),
            })
            .await
            .unwrap();

        let event = stream.next().await.unwrap();
        assert!(matches!(event, ThreadEvent::ThreadStarted { .. }));
    }
}
