use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscordTargetKind {
    User,
    Channel,
}

#[derive(Debug, Clone)]
pub struct DiscordTarget {
    pub kind: DiscordTargetKind,
    pub id: String,
    pub raw: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordGuildSummary {
    pub id: String,
    pub name: String,
    pub slug: String,
}

pub fn normalize_slug(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

pub fn parse_discord_target(raw: &str) -> Option<DiscordTarget> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Discord mention: <@123456> or <@!123456>
    if let Some(caps) = trimmed.strip_prefix("<@") {
        let inner = caps.strip_suffix('>')?;
        let id = inner.strip_prefix('!').unwrap_or(inner);
        if id.chars().all(|c| c.is_ascii_digit()) && !id.is_empty() {
            return Some(DiscordTarget {
                kind: DiscordTargetKind::User,
                id: id.to_string(),
                raw: trimmed.to_string(),
            });
        }
    }

    // Prefixed: user:123, channel:123, discord:123
    if let Some(rest) = trimmed.strip_prefix("user:") {
        return Some(DiscordTarget {
            kind: DiscordTargetKind::User,
            id: rest.trim().to_string(),
            raw: trimmed.to_string(),
        });
    }
    if let Some(rest) = trimmed.strip_prefix("channel:") {
        return Some(DiscordTarget {
            kind: DiscordTargetKind::Channel,
            id: rest.trim().to_string(),
            raw: trimmed.to_string(),
        });
    }
    if let Some(rest) = trimmed.strip_prefix("discord:") {
        return Some(DiscordTarget {
            kind: DiscordTargetKind::User,
            id: rest.trim().to_string(),
            raw: trimmed.to_string(),
        });
    }

    // @-prefixed → user
    if let Some(rest) = trimmed.strip_prefix('@') {
        let candidate = rest.trim();
        if candidate.chars().all(|c| c.is_ascii_digit()) && !candidate.is_empty() {
            return Some(DiscordTarget {
                kind: DiscordTargetKind::User,
                id: candidate.to_string(),
                raw: trimmed.to_string(),
            });
        }
    }

    // Pure numeric → ambiguous, default to channel
    if trimmed.chars().all(|c| c.is_ascii_digit()) && !trimmed.is_empty() {
        return Some(DiscordTarget {
            kind: DiscordTargetKind::Channel,
            id: trimmed.to_string(),
            raw: trimmed.to_string(),
        });
    }

    // Fallback: treat as channel name/id
    Some(DiscordTarget {
        kind: DiscordTargetKind::Channel,
        id: trimmed.to_string(),
        raw: trimmed.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mention() {
        let target = parse_discord_target("<@123456>").unwrap();
        assert_eq!(target.kind, DiscordTargetKind::User);
        assert_eq!(target.id, "123456");
    }

    #[test]
    fn test_parse_mention_with_exclamation() {
        let target = parse_discord_target("<@!789>").unwrap();
        assert_eq!(target.kind, DiscordTargetKind::User);
        assert_eq!(target.id, "789");
    }

    #[test]
    fn test_parse_user_prefix() {
        let target = parse_discord_target("user:42").unwrap();
        assert_eq!(target.kind, DiscordTargetKind::User);
        assert_eq!(target.id, "42");
    }

    #[test]
    fn test_parse_channel_prefix() {
        let target = parse_discord_target("channel:99").unwrap();
        assert_eq!(target.kind, DiscordTargetKind::Channel);
        assert_eq!(target.id, "99");
    }

    #[test]
    fn test_parse_at_prefix() {
        let target = parse_discord_target("@555").unwrap();
        assert_eq!(target.kind, DiscordTargetKind::User);
        assert_eq!(target.id, "555");
    }

    #[test]
    fn test_parse_numeric_defaults_to_channel() {
        let target = parse_discord_target("12345").unwrap();
        assert_eq!(target.kind, DiscordTargetKind::Channel);
        assert_eq!(target.id, "12345");
    }

    #[test]
    fn test_parse_empty() {
        assert!(parse_discord_target("").is_none());
        assert!(parse_discord_target("   ").is_none());
    }

    #[test]
    fn test_normalize_slug() {
        assert_eq!(normalize_slug("My Cool Server!"), "my-cool-server");
        assert_eq!(normalize_slug("test---server"), "test-server");
    }
}
