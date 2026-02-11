use async_trait::async_trait;

use crate::{AuthMethod, OAuthChallenge, OAuthTokens, Result};

#[async_trait]
pub trait AuthProvider: Send + Sync {
    fn provider_id(&self) -> &str;

    fn auth_methods(&self) -> Vec<AuthMethod>;

    async fn start_oauth(&self, method_index: usize) -> Result<OAuthChallenge>;

    async fn exchange_code(&self, code: &str, verifier: &str) -> Result<OAuthTokens>;

    async fn refresh_token(&self, refresh_token: &str) -> Result<OAuthTokens>;
}
