use std::sync::Arc;

use chrono::Utc;
use serenity::all::{Context, EventHandler, GatewayIntents, Guild, Interaction, Message, Ready};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};
use uira_core::schema::DiscordChannelConfig;

use super::components::{
    self, parse_component_custom_id, parse_modal_custom_id, ComponentRegistry,
};
use super::config;
use crate::channels::types::{ChannelMessage, ChannelType};

pub struct DiscordHandler {
    pub config: DiscordChannelConfig,
    pub message_tx: mpsc::Sender<ChannelMessage>,
    pub component_registry: Arc<ComponentRegistry>,
    pub bot_user_id: Arc<RwLock<Option<u64>>>,
}

enum AccessResult {
    Allowed,
    AllowedRequireMention,
    Denied,
}

impl DiscordHandler {
    async fn check_access(
        &self,
        user_id: u64,
        username: &str,
        guild_id: Option<u64>,
        channel_id: u64,
    ) -> AccessResult {
        let is_dm = guild_id.is_none();

        if is_dm {
            if let Some(ref dm_config) = self.config.dm {
                if !dm_config.enabled {
                    return AccessResult::Denied;
                }
                if !dm_config.allow_from.is_empty() {
                    let uid = user_id.to_string();
                    if !config::is_user_allowed(&dm_config.allow_from, &uid, Some(username)) {
                        return AccessResult::Denied;
                    }
                }
            }
        } else if let Some(gid) = guild_id {
            let guild_id_str = gid.to_string();
            let channel_id_str = channel_id.to_string();
            let access =
                config::check_guild_channel_access(&self.config, &guild_id_str, &channel_id_str);
            if !access.allowed {
                return AccessResult::Denied;
            }

            if !self.config.allowed_users.is_empty() {
                let uid = user_id.to_string();
                if !config::is_user_allowed(&self.config.allowed_users, &uid, Some(username)) {
                    return AccessResult::Denied;
                }
            }

            if access.require_mention {
                return AccessResult::AllowedRequireMention;
            }
        }

        AccessResult::Allowed
    }
}

#[serenity::async_trait]
impl EventHandler for DiscordHandler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        info!(
            bot_name = %ready.user.name,
            bot_id = %ready.user.id,
            guild_count = ready.guilds.len(),
            "Discord bot connected"
        );

        {
            let mut id = self.bot_user_id.write().await;
            *id = Some(ready.user.id.get());
        }

        if let Some(ref activity_text) = self.config.activity {
            use serenity::all::ActivityData;
            let activity_type = self.config.activity_type.unwrap_or(4);
            let activity = match activity_type {
                0 => ActivityData::playing(activity_text),
                1 => ActivityData::streaming(
                    activity_text,
                    self.config.activity_url.as_deref().unwrap_or(""),
                )
                .unwrap_or_else(|_| ActivityData::playing(activity_text)),
                2 => ActivityData::listening(activity_text),
                3 => ActivityData::watching(activity_text),
                5 => ActivityData::competing(activity_text),
                _ => ActivityData::custom(activity_text),
            };
            ctx.set_activity(Some(activity));
            debug!(activity = %activity_text, "Set bot activity");
        }
    }

    async fn message(&self, _ctx: Context, msg: Message) {
        if msg.author.bot && !self.config.allow_bots {
            return;
        }

        let bot_id_val = {
            let bot_id = self.bot_user_id.read().await;
            *bot_id
        };
        if let Some(id) = bot_id_val {
            if msg.author.id.get() == id {
                return;
            }
        }

        let is_dm = msg.guild_id.is_none();

        match self
            .check_access(
                msg.author.id.get(),
                &msg.author.name,
                msg.guild_id.map(|g| g.get()),
                msg.channel_id.get(),
            )
            .await
        {
            AccessResult::Denied => return,
            AccessResult::AllowedRequireMention => {
                if let Some(id) = bot_id_val {
                    if !msg.mentions.iter().any(|u| u.id.get() == id) {
                        debug!(
                            channel = %msg.channel_id,
                            "Message ignored: require_mention is set and bot was not mentioned"
                        );
                        return;
                    }
                }
            }
            AccessResult::Allowed => {}
        }

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("user_id".to_string(), msg.author.id.get().to_string());
        metadata.insert("username".to_string(), msg.author.name.clone());
        if let Some(guild_id) = msg.guild_id {
            metadata.insert("guild_id".to_string(), guild_id.get().to_string());
        }
        metadata.insert("channel_id".to_string(), msg.channel_id.get().to_string());
        metadata.insert("message_id".to_string(), msg.id.get().to_string());
        if is_dm {
            metadata.insert("is_dm".to_string(), "true".to_string());
        }

        let channel_msg = ChannelMessage {
            sender: msg.author.id.get().to_string(),
            content: msg.content.clone(),
            channel_type: ChannelType::Discord,
            channel_id: msg.channel_id.get().to_string(),
            timestamp: Utc::now(),
            metadata,
        };

        if let Err(e) = self.message_tx.send(channel_msg).await {
            error!(error = %e, "Failed to forward Discord message");
        }
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        match interaction {
            Interaction::Command(cmd) => {
                if matches!(
                    self.check_access(
                        cmd.user.id.get(),
                        &cmd.user.name,
                        cmd.guild_id.map(|g| g.get()),
                        cmd.channel_id.get(),
                    )
                    .await,
                    AccessResult::Denied
                ) {
                    debug!(
                        command = %cmd.data.name,
                        user = %cmd.user.name,
                        "Slash command blocked by access controls"
                    );
                    let _ = cmd.defer(&ctx.http).await;
                    return;
                }

                debug!(
                    command = %cmd.data.name,
                    user = %cmd.user.name,
                    "Slash command received"
                );

                let mut metadata = std::collections::HashMap::new();
                metadata.insert("interaction_type".to_string(), "command".to_string());
                metadata.insert("command_name".to_string(), cmd.data.name.clone());
                metadata.insert("user_id".to_string(), cmd.user.id.get().to_string());
                metadata.insert("username".to_string(), cmd.user.name.clone());
                if let Some(guild_id) = cmd.guild_id {
                    metadata.insert("guild_id".to_string(), guild_id.get().to_string());
                }
                metadata.insert("channel_id".to_string(), cmd.channel_id.get().to_string());
                metadata.insert("interaction_id".to_string(), cmd.id.get().to_string());
                metadata.insert("interaction_token".to_string(), cmd.token.clone());

                let options_text: Vec<String> = cmd
                    .data
                    .options
                    .iter()
                    .map(|opt| format!("{}={:?}", opt.name, opt.value))
                    .collect();

                let content = if options_text.is_empty() {
                    format!("/{}", cmd.data.name)
                } else {
                    format!("/{} {}", cmd.data.name, options_text.join(" "))
                };

                let channel_msg = ChannelMessage {
                    sender: cmd.user.id.get().to_string(),
                    content,
                    channel_type: ChannelType::Discord,
                    channel_id: cmd.channel_id.get().to_string(),
                    timestamp: Utc::now(),
                    metadata,
                };

                if let Err(e) = self.message_tx.send(channel_msg).await {
                    error!(error = %e, "Failed to forward slash command");
                }

                if let Err(e) = cmd.defer(&ctx.http).await {
                    warn!(error = %e, "Failed to defer command response");
                }
            }

            Interaction::Component(component) => {
                let custom_id = &component.data.custom_id;
                debug!(custom_id = %custom_id, "Component interaction received");

                if let Some((component_id, _modal_id)) = parse_component_custom_id(custom_id) {
                    if let Some(entry) = self
                        .component_registry
                        .resolve_component(&component_id, true)
                    {
                        if !entry.allowed_users.is_empty() {
                            let user_id = component.user.id.get().to_string();
                            if !entry.allowed_users.contains(&user_id)
                                && !entry.allowed_users.contains(&component.user.name)
                            {
                                let _ = component.defer(&ctx.http).await;
                                return;
                            }
                        }

                        let values: Vec<String> = match &component.data.kind {
                            serenity::all::ComponentInteractionDataKind::StringSelect {
                                values,
                            } => values.clone(),
                            serenity::all::ComponentInteractionDataKind::UserSelect { values } => {
                                values.iter().map(|v| v.get().to_string()).collect()
                            }
                            serenity::all::ComponentInteractionDataKind::RoleSelect { values } => {
                                values.iter().map(|v| v.get().to_string()).collect()
                            }
                            serenity::all::ComponentInteractionDataKind::ChannelSelect {
                                values,
                            } => values.iter().map(|v| v.get().to_string()).collect(),
                            serenity::all::ComponentInteractionDataKind::MentionableSelect {
                                values,
                            } => values.iter().map(|v| v.get().to_string()).collect(),
                            _ => Vec::new(),
                        };

                        let event_text = components::format_component_event_text(
                            entry.kind,
                            &entry.label,
                            &values,
                        );

                        let mut metadata = std::collections::HashMap::new();
                        metadata.insert("interaction_type".to_string(), "component".to_string());
                        metadata.insert("component_id".to_string(), component_id);
                        metadata.insert("user_id".to_string(), component.user.id.get().to_string());
                        metadata.insert("username".to_string(), component.user.name.clone());
                        if let Some(session_key) = &entry.session_key {
                            metadata.insert("session_key".to_string(), session_key.clone());
                        }
                        if !values.is_empty() {
                            metadata.insert("values".to_string(), values.join(","));
                        }
                        metadata
                            .insert("interaction_id".to_string(), component.id.get().to_string());
                        metadata.insert("interaction_token".to_string(), component.token.clone());

                        let channel_msg = ChannelMessage {
                            sender: component.user.id.get().to_string(),
                            content: event_text,
                            channel_type: ChannelType::Discord,
                            channel_id: component.channel_id.get().to_string(),
                            timestamp: Utc::now(),
                            metadata,
                        };

                        if let Err(e) = self.message_tx.send(channel_msg).await {
                            error!(error = %e, "Failed to forward component interaction");
                        }
                    }
                }

                if let Err(e) = component.defer(&ctx.http).await {
                    warn!(error = %e, "Failed to defer component response");
                }
            }

            Interaction::Modal(modal) => {
                let custom_id = &modal.data.custom_id;
                debug!(custom_id = %custom_id, "Modal submission received");

                if let Some(modal_id) = parse_modal_custom_id(custom_id) {
                    if let Some(modal_entry) =
                        self.component_registry.resolve_modal(&modal_id, true)
                    {
                        let mut field_values = std::collections::HashMap::new();
                        for row in &modal.data.components {
                            for component in &row.components {
                                if let serenity::all::ActionRowComponent::InputText(input) =
                                    component
                                {
                                    let field_name = modal_entry
                                        .fields
                                        .iter()
                                        .find(|f| f.id == input.custom_id)
                                        .map(|f| f.name.clone())
                                        .unwrap_or_else(|| input.custom_id.clone());
                                    if let Some(ref value) = input.value {
                                        field_values.insert(field_name, value.clone());
                                    }
                                }
                            }
                        }

                        let fields_text: Vec<String> = field_values
                            .iter()
                            .map(|(k, v)| format!("{k}: {v}"))
                            .collect();

                        let mut metadata = std::collections::HashMap::new();
                        metadata.insert("interaction_type".to_string(), "modal".to_string());
                        metadata.insert("modal_id".to_string(), modal_id);
                        metadata.insert("user_id".to_string(), modal.user.id.get().to_string());
                        metadata.insert("username".to_string(), modal.user.name.clone());
                        if let Some(session_key) = &modal_entry.session_key {
                            metadata.insert("session_key".to_string(), session_key.clone());
                        }
                        for (k, v) in &field_values {
                            metadata.insert(format!("field_{k}"), v.clone());
                        }
                        metadata.insert("interaction_id".to_string(), modal.id.get().to_string());
                        metadata.insert("interaction_token".to_string(), modal.token.clone());

                        let content = format!(
                            "Form submitted: {}",
                            if fields_text.is_empty() {
                                "(empty)".to_string()
                            } else {
                                fields_text.join("; ")
                            }
                        );

                        let channel_msg = ChannelMessage {
                            sender: modal.user.id.get().to_string(),
                            content,
                            channel_type: ChannelType::Discord,
                            channel_id: modal.channel_id.get().to_string(),
                            timestamp: Utc::now(),
                            metadata,
                        };

                        if let Err(e) = self.message_tx.send(channel_msg).await {
                            error!(error = %e, "Failed to forward modal submission");
                        }
                    }
                }

                if let Err(e) = modal.defer(&ctx.http).await {
                    warn!(error = %e, "Failed to defer modal response");
                }
            }

            _ => {}
        }
    }

    async fn guild_create(&self, _ctx: Context, guild: Guild, is_new: Option<bool>) {
        if is_new == Some(true) {
            info!(
                guild_name = %guild.name,
                guild_id = %guild.id,
                member_count = guild.member_count,
                "Joined new Discord guild"
            );
        } else {
            debug!(
                guild_name = %guild.name,
                guild_id = %guild.id,
                "Guild available"
            );
        }
    }
}

pub fn build_gateway_intents(config: &DiscordChannelConfig) -> GatewayIntents {
    let mut intents = GatewayIntents::GUILDS
        | GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::DIRECT_MESSAGES;

    if let Some(ref intents_config) = config.intents {
        if intents_config.presence {
            intents |= GatewayIntents::GUILD_PRESENCES;
        }
        if intents_config.guild_members {
            intents |= GatewayIntents::GUILD_MEMBERS;
        }
    }

    intents
}
