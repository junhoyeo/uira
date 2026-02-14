use crate::{
    generate_pkce, AuthError, AuthMethod, AuthProvider, OAuthChallenge, OAuthTokens, Result,
};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use url::Url;

const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const AUTHORIZE_URL: &str = "https://claude.ai/oauth/authorize";
const TOKEN_URL: &str = "https://console.anthropic.com/v1/oauth/token";
const REDIRECT_URI: &str = "https://console.anthropic.com/oauth/code/callback";

#[derive(Debug, Clone)]
pub struct AnthropicAuth {
    client_id: String,
    redirect_uri: String,
    scopes: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TokenRequest {
    grant_type: String,
    code: String,
    state: String,
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

impl Default for AnthropicAuth {
    fn default() -> Self {
        Self::new()
    }
}

impl AnthropicAuth {
    pub fn new() -> Self {
        Self {
            client_id: CLIENT_ID.to_string(),
            redirect_uri: REDIRECT_URI.to_string(),
            scopes: vec![
                "org:create_api_key".to_string(),
                "user:profile".to_string(),
                "user:inference".to_string(),
            ],
        }
    }

    async fn exchange_code_impl(&self, code: &str, verifier: &str) -> Result<OAuthTokens> {
        let client = Client::new();

        // Code comes as "code#state" format from Anthropic's code-copy flow
        let (auth_code, state) = if let Some(pos) = code.find('#') {
            (&code[..pos], &code[pos + 1..])
        } else {
            (code, verifier)
        };

        let request = TokenRequest {
            grant_type: "authorization_code".to_string(),
            code: auth_code.to_string(),
            state: state.to_string(),
            client_id: self.client_id.clone(),
            redirect_uri: self.redirect_uri.clone(),
            code_verifier: verifier.to_string(),
        };

        let response = client
            .post(TOKEN_URL)
            .header("Content-Type", "application/json")
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
        let client = Client::new();

        let request = RefreshTokenRequest {
            grant_type: "refresh_token".to_string(),
            refresh_token: refresh_token.to_string(),
            client_id: self.client_id.clone(),
        };

        let response = client
            .post(TOKEN_URL)
            .header("Content-Type", "application/json")
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
impl AuthProvider for AnthropicAuth {
    fn provider_id(&self) -> &str {
        "anthropic"
    }

    fn auth_methods(&self) -> Vec<AuthMethod> {
        vec![AuthMethod::OAuth {
            label: "Anthropic OAuth".to_string(),
            authorize_url: AUTHORIZE_URL.to_string(),
            token_url: TOKEN_URL.to_string(),
            scopes: self.scopes.clone(),
        }]
    }

    async fn start_oauth(&self, _method_index: usize) -> Result<OAuthChallenge> {
        let pkce = generate_pkce();

        let mut auth_url =
            Url::parse(AUTHORIZE_URL).map_err(|e| AuthError::OAuthFailed(e.to_string()))?;

        // Use verifier as state (matching opencode-anthropic-auth approach)
        auth_url
            .query_pairs_mut()
            .append_pair("code", "true")
            .append_pair("client_id", &self.client_id)
            .append_pair("redirect_uri", &self.redirect_uri)
            .append_pair("response_type", "code")
            .append_pair("scope", &self.scopes.join(" "))
            .append_pair("code_challenge", &pkce.challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", &pkce.verifier);

        Ok(OAuthChallenge {
            url: auth_url.to_string(),
            verifier: pkce.verifier.clone(),
            state: pkce.verifier,
        })
    }

    async fn exchange_code(&self, code: &str, verifier: &str) -> Result<OAuthTokens> {
        self.exchange_code_impl(code, verifier).await
    }

    async fn refresh_token(&self, refresh_token: &str) -> Result<OAuthTokens> {
        self.refresh_token_impl(refresh_token).await
    }
}
