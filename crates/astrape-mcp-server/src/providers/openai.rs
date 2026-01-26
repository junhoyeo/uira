use crate::providers::{retry_with_backoff, Provider};
use eventsource_stream::Eventsource;
use futures::StreamExt;
use reqwest::Client;
use serde_json::json;

pub struct OpenAIProvider {
    token: String,
    client: Client,
}

impl OpenAIProvider {
    pub fn new(token: String) -> Self {
        Self {
            token,
            client: Client::new(),
        }
    }
}

impl Provider for OpenAIProvider {
    async fn query(&self, prompt: &str, model: &str) -> Result<String, String> {
        let prompt = prompt.to_string();
        let model = model.to_string();
        let token = self.token.clone();
        let client = self.client.clone();

        retry_with_backoff(
            || {
                let prompt = prompt.clone();
                let model = model.clone();
                let token = token.clone();
                let client = client.clone();
                async move {
                    let body = json!({
                        "model": model.trim_start_matches("openai/"),
                        "messages": [
                            {"role": "user", "content": prompt}
                        ],
                        "stream": true
                    });

                    let response = client
                        .post("https://api.openai.com/v1/chat/completions")
                        .header("Authorization", format!("Bearer {}", token))
                        .header("Content-Type", "application/json")
                        .json(&body)
                        .send()
                        .await
                        .map_err(|e| format!("OpenAI API request failed: {}", e))?;

                    if !response.status().is_success() {
                        let status = response.status();
                        let is_client_error = status.is_client_error();
                        let error_body = response
                            .text()
                            .await
                            .unwrap_or_else(|_| "Failed to read error body".to_string());
                        let err_msg =
                            format!("OpenAI API returned error {}: {}", status, error_body);
                        if is_client_error {
                            return Err(format!("CLIENT_ERROR:{}", err_msg));
                        }
                        return Err(err_msg);
                    }

                    let mut stream = response.bytes_stream().eventsource();
                    let mut combined_text = String::new();

                    while let Some(event) = stream.next().await {
                        match event {
                            Ok(event) => {
                                if event.data == "[DONE]" {
                                    break;
                                }

                                if let Ok(chunk) =
                                    serde_json::from_str::<serde_json::Value>(&event.data)
                                {
                                    if let Some(content) =
                                        chunk["choices"][0]["delta"]["content"].as_str()
                                    {
                                        combined_text.push_str(content);
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!("SSE stream error: {}", e);
                                break;
                            }
                        }
                    }

                    serde_json::to_string(&json!({"result": combined_text}))
                        .map_err(|e| format!("Failed to serialize result: {}", e))
                }
            },
            3,
        )
        .await
        .map_err(|e| {
            if e.starts_with("CLIENT_ERROR:") {
                e.trim_start_matches("CLIENT_ERROR:").to_string()
            } else {
                e
            }
        })
    }
}
