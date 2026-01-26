use crate::auth::{get_access_token, load_opencode_auth, model_to_provider};
use crate::providers::{GeminiProvider, OpenAIProvider, Provider};
use reqwest::Client;
use serde::Deserialize;

#[derive(Deserialize)]
#[allow(dead_code)]
struct Session {
    id: String,
}

pub async fn query(prompt: &str, model: &str, opencode_port: u16) -> Result<String, String> {
    let auth_store = load_opencode_auth()
        .await
        .map_err(|e| format!("Failed to load OpenCode auth: {}", e))?;

    let client = Client::new();
    let _session: Session = client
        .post(format!("http://localhost:{}/session", opencode_port))
        .send()
        .await
        .map_err(|e| format!("Failed to create session: {}", e))?
        .json()
        .await
        .map_err(|e| format!("Failed to parse session response: {}", e))?;

    let provider_name = model_to_provider(model);
    let token = get_access_token(&auth_store, provider_name)
        .map_err(|e| format!("Failed to get access token: {}", e))?;

    match provider_name {
        "openai" => {
            let provider = OpenAIProvider::new(token);
            provider.query(prompt, model).await
        }
        "google" => {
            let provider = GeminiProvider::new(token);
            provider.query(prompt, model).await
        }
        _ => Err(format!("Provider '{}' not yet implemented", provider_name)),
    }
}
