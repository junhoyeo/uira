use std::time::Instant;

use reqwest::Client;
use serde::Deserialize;
use tracing::debug;

const DISCORD_API_BASE: &str = "https://discord.com/api/v10";

const FLAG_GATEWAY_PRESENCE: u64 = 1 << 12;
const FLAG_GATEWAY_PRESENCE_LIMITED: u64 = 1 << 13;
const FLAG_GATEWAY_GUILD_MEMBERS: u64 = 1 << 14;
const FLAG_GATEWAY_GUILD_MEMBERS_LIMITED: u64 = 1 << 15;
const FLAG_GATEWAY_MESSAGE_CONTENT: u64 = 1 << 18;
const FLAG_GATEWAY_MESSAGE_CONTENT_LIMITED: u64 = 1 << 19;

#[derive(Debug, Clone)]
pub struct DiscordProbeResult {
    pub ok: bool,
    pub status: Option<u16>,
    pub error: Option<String>,
    pub elapsed_ms: u64,
    pub bot_id: Option<String>,
    pub bot_username: Option<String>,
    pub application: Option<ApplicationSummary>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivilegedIntentStatus {
    Enabled,
    Limited,
    Disabled,
}

#[derive(Debug, Clone)]
pub struct PrivilegedIntentsSummary {
    pub message_content: PrivilegedIntentStatus,
    pub guild_members: PrivilegedIntentStatus,
    pub presence: PrivilegedIntentStatus,
}

#[derive(Debug, Clone)]
pub struct ApplicationSummary {
    pub id: Option<String>,
    pub flags: Option<u64>,
    pub intents: Option<PrivilegedIntentsSummary>,
}

#[derive(Deserialize)]
struct UserMeResponse {
    id: Option<String>,
    username: Option<String>,
}

#[derive(Deserialize)]
struct ApplicationMeResponse {
    id: Option<String>,
    flags: Option<u64>,
}

pub fn resolve_privileged_intents(flags: u64) -> PrivilegedIntentsSummary {
    let resolve = |enabled_bit: u64, limited_bit: u64| -> PrivilegedIntentStatus {
        if flags & enabled_bit != 0 {
            PrivilegedIntentStatus::Enabled
        } else if flags & limited_bit != 0 {
            PrivilegedIntentStatus::Limited
        } else {
            PrivilegedIntentStatus::Disabled
        }
    };
    PrivilegedIntentsSummary {
        presence: resolve(FLAG_GATEWAY_PRESENCE, FLAG_GATEWAY_PRESENCE_LIMITED),
        guild_members: resolve(
            FLAG_GATEWAY_GUILD_MEMBERS,
            FLAG_GATEWAY_GUILD_MEMBERS_LIMITED,
        ),
        message_content: resolve(
            FLAG_GATEWAY_MESSAGE_CONTENT,
            FLAG_GATEWAY_MESSAGE_CONTENT_LIMITED,
        ),
    }
}

pub async fn probe_discord(
    token: &str,
    timeout: std::time::Duration,
    include_application: bool,
) -> DiscordProbeResult {
    let started = Instant::now();
    let client = match Client::builder().timeout(timeout).build() {
        Ok(c) => c,
        Err(e) => {
            return DiscordProbeResult {
                ok: false,
                status: None,
                error: Some(format!("HTTP client error: {e}")),
                elapsed_ms: started.elapsed().as_millis() as u64,
                bot_id: None,
                bot_username: None,
                application: None,
            };
        }
    };

    let auth = format!("Bot {token}");

    let resp = match client
        .get(format!("{DISCORD_API_BASE}/users/@me"))
        .header("Authorization", &auth)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return DiscordProbeResult {
                ok: false,
                status: None,
                error: Some(e.to_string()),
                elapsed_ms: started.elapsed().as_millis() as u64,
                bot_id: None,
                bot_username: None,
                application: None,
            };
        }
    };

    let status = resp.status().as_u16();
    if !resp.status().is_success() {
        return DiscordProbeResult {
            ok: false,
            status: Some(status),
            error: Some(format!("getMe failed ({status})")),
            elapsed_ms: started.elapsed().as_millis() as u64,
            bot_id: None,
            bot_username: None,
            application: None,
        };
    }

    let user: UserMeResponse = match resp.json().await {
        Ok(u) => u,
        Err(e) => {
            return DiscordProbeResult {
                ok: false,
                status: Some(status),
                error: Some(format!("Failed to parse response: {e}")),
                elapsed_ms: started.elapsed().as_millis() as u64,
                bot_id: None,
                bot_username: None,
                application: None,
            };
        }
    };

    let application = if include_application {
        fetch_application_summary(&client, &auth, timeout).await
    } else {
        None
    };

    debug!(
        bot_id = ?user.id,
        bot_username = ?user.username,
        "Discord probe successful"
    );

    DiscordProbeResult {
        ok: true,
        status: Some(200),
        error: None,
        elapsed_ms: started.elapsed().as_millis() as u64,
        bot_id: user.id,
        bot_username: user.username,
        application,
    }
}

async fn fetch_application_summary(
    client: &Client,
    auth: &str,
    _timeout: std::time::Duration,
) -> Option<ApplicationSummary> {
    let resp = client
        .get(format!("{DISCORD_API_BASE}/oauth2/applications/@me"))
        .header("Authorization", auth)
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let app: ApplicationMeResponse = resp.json().await.ok()?;
    let intents = app.flags.map(resolve_privileged_intents);

    Some(ApplicationSummary {
        id: app.id,
        flags: app.flags,
        intents,
    })
}

pub async fn fetch_application_id(token: &str, timeout: std::time::Duration) -> Option<String> {
    let client = Client::builder().timeout(timeout).build().ok()?;
    let auth = format!("Bot {token}");
    let resp = client
        .get(format!("{DISCORD_API_BASE}/oauth2/applications/@me"))
        .header("Authorization", &auth)
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let app: ApplicationMeResponse = resp.json().await.ok()?;
    app.id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_privileged_intents_all_enabled() {
        let flags =
            FLAG_GATEWAY_PRESENCE | FLAG_GATEWAY_GUILD_MEMBERS | FLAG_GATEWAY_MESSAGE_CONTENT;
        let summary = resolve_privileged_intents(flags);
        assert_eq!(summary.presence, PrivilegedIntentStatus::Enabled);
        assert_eq!(summary.guild_members, PrivilegedIntentStatus::Enabled);
        assert_eq!(summary.message_content, PrivilegedIntentStatus::Enabled);
    }

    #[test]
    fn test_resolve_privileged_intents_limited() {
        let flags = FLAG_GATEWAY_PRESENCE_LIMITED | FLAG_GATEWAY_MESSAGE_CONTENT_LIMITED;
        let summary = resolve_privileged_intents(flags);
        assert_eq!(summary.presence, PrivilegedIntentStatus::Limited);
        assert_eq!(summary.guild_members, PrivilegedIntentStatus::Disabled);
        assert_eq!(summary.message_content, PrivilegedIntentStatus::Limited);
    }

    #[test]
    fn test_resolve_privileged_intents_none() {
        let summary = resolve_privileged_intents(0);
        assert_eq!(summary.presence, PrivilegedIntentStatus::Disabled);
        assert_eq!(summary.guild_members, PrivilegedIntentStatus::Disabled);
        assert_eq!(summary.message_content, PrivilegedIntentStatus::Disabled);
    }
}
