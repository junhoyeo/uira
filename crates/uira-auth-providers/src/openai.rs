use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use uira_auth::{AuthError, AuthMethod, AuthProvider, OAuthChallenge, OAuthTokens, Result};
use url::Url;

const AUTHORIZE_URL: &str = "https://auth.openai.com/authorize";
const TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const REDIRECT_URI: &str = "http://localhost:8765/callback";

#[derive(Debug, Clone)]
pub struct OpenAIAuth {
    client_id: Option<String>,
    redirect_uri: String,
    scopes: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TokenRequest {
    grant_type: String,
    code: String,
    client_id: String,
    redirect_uri: String,
    code_verifier: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct RefreshTokenRequest {
    grant_type: String,
    refresh_token: String,
    client_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    token_type: String,
}

impl Default for OpenAIAuth {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenAIAuth {
    pub fn new() -> Self {
        Self {
            client_id: None,
            redirect_uri: REDIRECT_URI.to_string(),
            scopes: vec![
                "openid".to_string(),
                "profile".to_string(),
                "email".to_string(),
            ],
        }
    }

    pub fn with_oauth(client_id: String) -> Self {
        Self {
            client_id: Some(client_id),
            redirect_uri: REDIRECT_URI.to_string(),
            scopes: vec![
                "openid".to_string(),
                "profile".to_string(),
                "email".to_string(),
            ],
        }
    }

    async fn exchange_code_impl(&self, code: &str, verifier: &str) -> Result<OAuthTokens> {
        let client_id = self
            .client_id
            .as_ref()
            .ok_or_else(|| AuthError::OAuthFailed("Client ID not configured".to_string()))?;

        let client = Client::new();

        let request = TokenRequest {
            grant_type: "authorization_code".to_string(),
            code: code.to_string(),
            client_id: client_id.clone(),
            redirect_uri: self.redirect_uri.clone(),
            code_verifier: verifier.to_string(),
        };

        let response = client
            .post(TOKEN_URL)
            .json(&request)
            .send()
            .await
            .map_err(|e| AuthError::OAuthFailed(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(AuthError::OAuthFailed(format!("HTTP {}: {}", status, text)));
        }

        let token_response: TokenResponse = response
            .json()
            .await
            .map_err(|e| AuthError::OAuthFailed(e.to_string()))?;

        Ok(OAuthTokens {
            access_token: token_response.access_token,
            refresh_token: token_response.refresh_token,
            expires_at: token_response.expires_in,
            token_type: token_response.token_type,
        })
    }

    async fn refresh_token_impl(&self, refresh_token: &str) -> Result<OAuthTokens> {
        let client_id = self
            .client_id
            .as_ref()
            .ok_or_else(|| AuthError::OAuthFailed("Client ID not configured".to_string()))?;

        let client = Client::new();

        let request = RefreshTokenRequest {
            grant_type: "refresh_token".to_string(),
            refresh_token: refresh_token.to_string(),
            client_id: client_id.clone(),
        };

        let response = client
            .post(TOKEN_URL)
            .json(&request)
            .send()
            .await
            .map_err(|e| AuthError::OAuthFailed(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(AuthError::OAuthFailed(format!("HTTP {}: {}", status, text)));
        }

        let token_response: TokenResponse = response
            .json()
            .await
            .map_err(|e| AuthError::OAuthFailed(e.to_string()))?;

        Ok(OAuthTokens {
            access_token: token_response.access_token,
            refresh_token: token_response.refresh_token,
            expires_at: token_response.expires_in,
            token_type: token_response.token_type,
        })
    }
}

#[async_trait]
impl AuthProvider for OpenAIAuth {
    fn provider_id(&self) -> &str {
        "openai"
    }

    fn auth_methods(&self) -> Vec<AuthMethod> {
        let mut methods = vec![AuthMethod::ApiKey {
            label: "OpenAI API Key".to_string(),
            env_var: "OPENAI_API_KEY".to_string(),
        }];

        if self.client_id.is_some() {
            methods.push(AuthMethod::OAuth {
                label: "OpenAI OAuth".to_string(),
                authorize_url: AUTHORIZE_URL.to_string(),
                token_url: TOKEN_URL.to_string(),
                scopes: self.scopes.clone(),
            });
        }

        methods
    }

    async fn start_oauth(&self, _method_index: usize) -> Result<OAuthChallenge> {
        let client_id = self
            .client_id
            .as_ref()
            .ok_or_else(|| AuthError::OAuthFailed("Client ID not configured".to_string()))?;

        let pkce = uira_auth::generate_pkce();

        let mut auth_url =
            Url::parse(AUTHORIZE_URL).map_err(|e| AuthError::OAuthFailed(e.to_string()))?;

        auth_url
            .query_pairs_mut()
            .append_pair("client_id", client_id)
            .append_pair("redirect_uri", &self.redirect_uri)
            .append_pair("response_type", "code")
            .append_pair("scope", &self.scopes.join(" "))
            .append_pair("code_challenge", &pkce.challenge)
            .append_pair("code_challenge_method", "S256");

        Ok(OAuthChallenge {
            url: auth_url.to_string(),
            verifier: pkce.verifier,
            state: uuid::Uuid::new_v4().to_string(),
        })
    }

    async fn exchange_code(&self, code: &str, verifier: &str) -> Result<OAuthTokens> {
        self.exchange_code_impl(code, verifier).await
    }

    async fn refresh_token(&self, refresh_token: &str) -> Result<OAuthTokens> {
        self.refresh_token_impl(refresh_token).await
    }
}
