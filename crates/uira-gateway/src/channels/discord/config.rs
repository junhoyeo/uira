use std::collections::HashMap;

use uira_core::schema::{DiscordActionConfig, DiscordChannelConfig, DiscordGuildEntry};

#[derive(Debug, Clone)]
pub struct ResolvedDiscordAccount {
    pub account_id: String,
    pub enabled: bool,
    pub name: Option<String>,
    pub token: String,
    pub config: DiscordChannelConfig,
}

pub fn resolve_discord_account(config: &DiscordChannelConfig) -> ResolvedDiscordAccount {
    ResolvedDiscordAccount {
        account_id: config.account_id.clone(),
        enabled: config.enabled,
        name: config.name.clone(),
        token: normalize_discord_token(&config.bot_token),
        config: config.clone(),
    }
}

pub fn normalize_discord_token(token: &str) -> String {
    let trimmed = token.trim();
    trimmed.strip_prefix("Bot ").unwrap_or(trimmed).to_string()
}

pub fn is_action_allowed(actions: &Option<DiscordActionConfig>, key: &str, default: bool) -> bool {
    let Some(actions) = actions else {
        return default;
    };
    match key {
        "reactions" => actions.reactions,
        "stickers" => actions.stickers,
        "polls" => actions.polls,
        "permissions" => actions.permissions,
        "messages" => actions.messages,
        "threads" => actions.threads,
        "pins" => actions.pins,
        "search" => actions.search,
        "member_info" => actions.member_info,
        "role_info" => actions.role_info,
        "roles" => actions.roles,
        "channel_info" => actions.channel_info,
        "events" => actions.events,
        "moderation" => actions.moderation,
        "emoji_uploads" => actions.emoji_uploads,
        "sticker_uploads" => actions.sticker_uploads,
        "channels" => actions.channels,
        "presence" => actions.presence,
        _ => default,
    }
}

pub fn resolve_guild_entry<'a>(
    guilds: &'a HashMap<String, DiscordGuildEntry>,
    guild_id: &str,
) -> Option<&'a DiscordGuildEntry> {
    if let Some(entry) = guilds.get(guild_id) {
        return Some(entry);
    }
    guilds.values().find(|entry| {
        entry
            .slug
            .as_deref()
            .is_some_and(|slug| slug.eq_ignore_ascii_case(guild_id))
    })
}

pub fn is_user_allowed(allowed_users: &[String], user_id: &str, username: Option<&str>) -> bool {
    if allowed_users.is_empty() {
        return true;
    }
    for allowed in allowed_users {
        if allowed == user_id {
            return true;
        }
        if let Some(uname) = username {
            let allowed_trimmed = allowed.strip_prefix('@').unwrap_or(allowed);
            if uname == allowed_trimmed {
                return true;
            }
        }
    }
    false
}

#[derive(Debug, Clone)]
pub struct GuildChannelAccess {
    pub allowed: bool,
    pub require_mention: bool,
}

pub fn check_guild_channel_access(
    config: &DiscordChannelConfig,
    guild_id: &str,
    channel_id: &str,
) -> GuildChannelAccess {
    match config.group_policy.as_str() {
        "disabled" => GuildChannelAccess {
            allowed: false,
            require_mention: false,
        },
        "allowlist" => {
            let Some(guild) = resolve_guild_entry(&config.guilds, guild_id) else {
                return GuildChannelAccess {
                    allowed: false,
                    require_mention: false,
                };
            };
            if let Some(ch_config) = guild.channels.get(channel_id) {
                GuildChannelAccess {
                    allowed: ch_config.enabled && ch_config.allow,
                    require_mention: ch_config.require_mention || guild.require_mention,
                }
            } else {
                GuildChannelAccess {
                    allowed: false,
                    require_mention: guild.require_mention,
                }
            }
        }
        _ => GuildChannelAccess {
            allowed: true,
            require_mention: false,
        },
    }
}

pub fn is_guild_channel_allowed(
    config: &DiscordChannelConfig,
    guild_id: &str,
    channel_id: &str,
) -> bool {
    check_guild_channel_access(config, guild_id, channel_id).allowed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_token_strips_bot_prefix() {
        assert_eq!(normalize_discord_token("Bot abc123"), "abc123");
        assert_eq!(normalize_discord_token("abc123"), "abc123");
        assert_eq!(normalize_discord_token("  Bot xyz  "), "xyz");
    }

    #[test]
    fn test_is_user_allowed_empty_list() {
        assert!(is_user_allowed(&[], "123", None));
    }

    #[test]
    fn test_is_user_allowed_by_id() {
        let allowed = vec!["123".to_string(), "456".to_string()];
        assert!(is_user_allowed(&allowed, "123", None));
        assert!(!is_user_allowed(&allowed, "789", None));
    }

    #[test]
    fn test_is_user_allowed_by_username() {
        let allowed = vec!["@alice".to_string()];
        assert!(is_user_allowed(&allowed, "999", Some("alice")));
        assert!(!is_user_allowed(&allowed, "999", Some("bob")));
    }

    #[test]
    fn test_is_action_allowed_defaults() {
        assert!(is_action_allowed(&None, "reactions", true));
        assert!(!is_action_allowed(&None, "reactions", false));
    }

    #[test]
    fn test_is_action_allowed_from_config() {
        let mut actions = DiscordActionConfig::default();
        actions.presence = true;
        assert!(is_action_allowed(&Some(actions), "presence", false));
    }
}
