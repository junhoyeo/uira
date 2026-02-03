mod bus;
mod events;
mod runner;
mod subscriber;

pub use bus::{BroadcastBus, EventBus};
pub use events::{ApprovalDecision, Event, EventCategory, FileChangeType, SessionEndReason};
pub use runner::{HandlerRegistry, SubscriberRunner};
pub use subscriber::{EventHandler, HandlerResult, Subscriber, SubscriptionFilter};

pub mod compat;
