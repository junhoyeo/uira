//! OpenCode authentication integration.
//!
//! OpenCode stores provider credentials under the user's local data directory
//! (typically `~/.local/share/opencode/auth.json` on Linux and
//! `~/Library/Application Support/opencode/auth.json` on macOS).
//!
//! This module loads that store and provides access tokens for OpenCode session API calls.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};

/// A credential entry in OpenCode's `auth.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AuthCredential {
    /// OAuth credentials (used by some providers).
    #[serde(rename = "oauth")]
    OAuth {
        /// Access token.
        access: String,
        /// Refresh token.
        refresh: String,
        /// Epoch millis when the access token expires.
        expires: u64,
    },

    /// Raw API key.
    #[serde(rename = "api")]
    ApiKey {
        /// Provider API key.
        key: String,
    },
}

/// The decoded contents of OpenCode's `auth.json`.
pub type AuthStore = HashMap<String, AuthCredential>;

/// Returns the default path to OpenCode's `auth.json`.
pub fn opencode_auth_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("opencode")
        .join("auth.json")
}

/// Load OpenCode's authentication store.
pub async fn load_opencode_auth() -> Result<AuthStore> {
    let path = opencode_auth_path();
    let content = tokio::fs::read_to_string(&path).await.with_context(|| {
        format!(
            "Failed to read {}. Run 'opencode auth login' first.",
            path.display()
        )
    })?;

    serde_json::from_str(&content).with_context(|| "Failed to parse OpenCode auth.json")
}

/// Get an access token for a provider.
///
/// For OAuth entries, this fails if the token is expired (with a 5 minute
/// buffer).
pub fn get_access_token(store: &AuthStore, provider: &str) -> Result<String> {
    let cred = store.get(provider).with_context(|| {
        format!(
            "No credentials for provider '{}'. Run 'opencode auth login {}'",
            provider, provider
        )
    })?;

    match cred {
        AuthCredential::OAuth {
            access, expires, ..
        } => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .context("system clock before UNIX_EPOCH")?
                .as_millis() as u64;

            // Fail if token is expired (with 5 minute buffer).
            if *expires > 0 && now + 300_000 > *expires {
                anyhow::bail!(
                    "OAuth token for '{}' has expired. Run 'opencode auth login {}' to refresh.",
                    provider,
                    provider
                );
            }

            Ok(access.clone())
        }
        AuthCredential::ApiKey { key } => Ok(key.clone()),
    }
}
