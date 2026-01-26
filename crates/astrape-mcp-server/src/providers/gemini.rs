use crate::providers::Provider;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use reqwest::Client;
use serde_json::json;

pub struct GeminiProvider {
    token: String,
    client: Client,
}

impl GeminiProvider {
    pub fn new(token: String) -> Self {
        Self {
            token,
            client: Client::new(),
        }
    }
}

impl Provider for GeminiProvider {
    async fn query(&self, prompt: &str, model: &str) -> Result<String, String> {
        // Extract model name (remove "google/" or "gemini/" prefix)
        let model_name = model
            .trim_start_matches("google/")
            .trim_start_matches("gemini/");

        // Build request body (Gemini format)
        let body = json!({
            "contents": [{
                "parts": [{"text": prompt}]
            }]
        });

        // Build URL with API key as query parameter and alt=sse for streaming
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?alt=sse&key={}",
            model_name, self.token
        );

        // Send request to Gemini API
        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Gemini API request failed: {}", e))?;

        // Check response status
        if !response.status().is_success() {
            let status = response.status();
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Failed to read error body".to_string());
            return Err(format!(
                "Gemini API returned error {}: {}",
                status, error_body
            ));
        }

        // Parse SSE stream
        let mut stream = response.bytes_stream().eventsource();
        let mut combined_text = String::new();

        while let Some(event) = stream.next().await {
            match event {
                Ok(event) => {
                    // Parse Gemini response format
                    if let Ok(chunk) = serde_json::from_str::<serde_json::Value>(&event.data) {
                        if let Some(candidates) = chunk["candidates"].as_array() {
                            if let Some(candidate) = candidates.first() {
                                if let Some(parts) = candidate["content"]["parts"].as_array() {
                                    for part in parts {
                                        if let Some(text) = part["text"].as_str() {
                                            combined_text.push_str(text);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("SSE stream error: {}", e);
                    break;
                }
            }
        }

        // Return JSON format (same as OpenAI provider)
        serde_json::to_string(&json!({"result": combined_text}))
            .map_err(|e| format!("Failed to serialize result: {}", e))
    }
}
