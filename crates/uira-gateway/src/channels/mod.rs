//! Multi-channel messaging for Uira.

pub mod channel;
pub mod discord;
pub mod error;
pub mod slack;
pub mod telegram;
pub mod types;

pub use channel::Channel;
pub use discord::DiscordChannel;
pub use error::ChannelError;
pub use slack::SlackChannel;
pub use telegram::TelegramChannel;
pub use types::{ChannelCapabilities, ChannelMessage, ChannelResponse, ChannelType};
