use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

use crate::{AuthError, Result};
use crate::types::OAuthTokens;

/// Response from device authorization endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    #[serde(default)]
    pub verification_uri_complete: Option<String>,
    pub expires_in: u64,
    #[serde(default = "default_interval")]
    pub interval: u64,
}

fn default_interval() -> u64 {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RawTokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<i64>,
    pub token_type: String,
}

impl From<RawTokenResponse> for OAuthTokens {
    fn from(raw: RawTokenResponse) -> Self {
        OAuthTokens {
            access_token: raw.access_token,
            refresh_token: raw.refresh_token,
            expires_at: raw.expires_in,
            token_type: raw.token_type,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum DeviceTokenResponse {
    Success(RawTokenResponse),
    Pending(DeviceTokenError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceTokenError {
    pub error: String,
    #[serde(default)]
    pub error_description: Option<String>,
}

/// Device flow configuration
#[derive(Debug, Clone)]
pub struct DeviceFlowConfig {
    pub device_auth_url: String,
    pub token_url: String,
    pub client_id: String,
    pub scope: String,
}

/// Polls the token endpoint until authorization completes or times out
pub async fn poll_for_token(
    config: &DeviceFlowConfig,
    device_code: &str,
    interval: u64,
    expires_in: u64,
) -> Result<OAuthTokens> {
    let client = reqwest::Client::new();
    let mut poll_interval = Duration::from_secs(interval);
    let timeout = Duration::from_secs(expires_in);
    let start = Instant::now();

    loop {
        if start.elapsed() >= timeout {
            return Err(AuthError::DeviceFlowTimeout);
        }

        tokio::time::sleep(poll_interval).await;

        let response = client
            .post(&config.token_url)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("device_code", device_code),
                ("client_id", &config.client_id),
            ])
            .send()
            .await
            .map_err(|e| AuthError::NetworkError(e.to_string()))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| AuthError::NetworkError(e.to_string()))?;

        if !status.is_success() && status.as_u16() != 400 {
            return Err(AuthError::OAuthError(format!(
                "Token endpoint returned {}: {}",
                status, body
            )));
        }

        match serde_json::from_str::<DeviceTokenResponse>(&body) {
            Ok(DeviceTokenResponse::Success(raw_tokens)) => {
                return Ok(raw_tokens.into());
            }
            Ok(DeviceTokenResponse::Pending(error)) => {
                match error.error.as_str() {
                    "authorization_pending" => {
                        // User hasn't authorized yet, continue polling
                        continue;
                    }
                    "slow_down" => {
                        // RFC 8628: permanently increase polling interval by 5 seconds
                        poll_interval += Duration::from_secs(5);
                        continue;
                    }
                    "expired_token" => {
                        return Err(AuthError::DeviceFlowTimeout);
                    }
                    "access_denied" => {
                        return Err(AuthError::OAuthError(
                            "User denied authorization".to_string(),
                        ));
                    }
                    _ => {
                        return Err(AuthError::OAuthError(format!(
                            "Device flow error: {} - {}",
                            error.error,
                            error.error_description.unwrap_or_default()
                        )));
                    }
                }
            }
            Err(e) => {
                return Err(AuthError::OAuthError(format!(
                    "Failed to parse token response: {} - Body: {}",
                    e, body
                )));
            }
        }
    }
}

/// Initiates device authorization flow
pub async fn start_device_flow(config: &DeviceFlowConfig) -> Result<DeviceCodeResponse> {
    let client = reqwest::Client::new();

    let response = client
        .post(&config.device_auth_url)
        .form(&[
            ("client_id", config.client_id.as_str()),
            ("scope", config.scope.as_str()),
        ])
        .send()
        .await
        .map_err(|e| AuthError::NetworkError(e.to_string()))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "Unable to read response body".to_string());
        return Err(AuthError::OAuthError(format!(
            "Device authorization failed with status {}: {}",
            status, body
        )));
    }

    let device_response: DeviceCodeResponse = response
        .json()
        .await
        .map_err(|e| AuthError::OAuthError(format!("Failed to parse device response: {}", e)))?;

    Ok(device_response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_code_response_deserialization() {
        let json = r#"{
            "device_code": "abc123",
            "user_code": "ABCD-EFGH",
            "verification_uri": "https://example.com/device",
            "expires_in": 900,
            "interval": 5
        }"#;

        let response: DeviceCodeResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.device_code, "abc123");
        assert_eq!(response.user_code, "ABCD-EFGH");
        assert_eq!(response.verification_uri, "https://example.com/device");
        assert_eq!(response.expires_in, 900);
        assert_eq!(response.interval, 5);
    }

    #[test]
    fn test_device_code_response_with_complete_uri() {
        let json = r#"{
            "device_code": "abc123",
            "user_code": "ABCD-EFGH",
            "verification_uri": "https://example.com/device",
            "verification_uri_complete": "https://example.com/device?code=ABCD-EFGH",
            "expires_in": 900,
            "interval": 5
        }"#;

        let response: DeviceCodeResponse = serde_json::from_str(json).unwrap();
        assert_eq!(
            response.verification_uri_complete,
            Some("https://example.com/device?code=ABCD-EFGH".to_string())
        );
    }

    #[test]
    fn test_device_token_error_deserialization() {
        let json = r#"{
            "error": "authorization_pending",
            "error_description": "User has not yet authorized"
        }"#;

        let error: DeviceTokenError = serde_json::from_str(json).unwrap();
        assert_eq!(error.error, "authorization_pending");
        assert_eq!(
            error.error_description,
            Some("User has not yet authorized".to_string())
        );
    }
}
