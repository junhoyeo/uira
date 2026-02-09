use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthChallenge {
    pub url: String,
    pub verifier: String,
    pub state: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct OAuthTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>,
    pub token_type: String,
}

pub fn expires_at_from_now(expires_in_seconds: Option<i64>) -> Option<i64> {
    let expires_in = expires_in_seconds?;
    if expires_in <= 0 {
        return None;
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?;
    Some(now.as_secs() as i64 + expires_in)
}

impl std::fmt::Debug for OAuthTokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OAuthTokens")
            .field("access_token", &"[REDACTED]")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field("expires_at", &self.expires_at)
            .field("token_type", &self.token_type)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub enum AuthMethod {
    OAuth {
        label: String,
        authorize_url: String,
        token_url: String,
        scopes: Vec<String>,
    },
    ApiKey {
        label: String,
        env_var: String,
    },
    DeviceCode {
        label: String,
        device_url: String,
        token_url: String,
    },
}

mod secret_string_serde {
    use super::*;

    pub fn serialize<S>(secret: &SecretString, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        secret.expose_secret().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<SecretString, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer).map(SecretString::from)
    }
}

mod option_secret_string_serde {
    use super::*;

    pub fn serialize<S>(secret: &Option<SecretString>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match secret {
            Some(s) => serializer.serialize_some(s.expose_secret()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<SecretString>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<String>::deserialize(deserializer).map(|opt| opt.map(SecretString::from))
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum StoredCredential {
    #[serde(rename = "oauth")]
    OAuth {
        #[serde(with = "secret_string_serde")]
        access_token: SecretString,
        #[serde(with = "option_secret_string_serde")]
        refresh_token: Option<SecretString>,
        expires_at: Option<i64>,
    },
    #[serde(rename = "api")]
    ApiKey {
        #[serde(with = "secret_string_serde")]
        key: SecretString,
    },
}

#[derive(Debug)]
pub struct OAuthCallback {
    pub code: String,
    pub state: String,
}
